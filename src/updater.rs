//! Native release checking, authenticated asset downloads and verified staging.
use semver::Version;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[path = "update_installer.rs"]
mod installer;
#[allow(
    dead_code,
    reason = "The native capture example shares the app without running its installer entrypoint"
)]
pub fn apply_update_cli() -> Option<Result<()>> {
    installer::apply_update_cli()
}

pub const RELEASES_URL: &str = "https://github.com/bugs1111/LevelEditor/releases";
const API: &str = "https://api.github.com/repos/bugs1111/LevelEditor/releases/latest";
const ASSETS: &str = "https://api.github.com/repos/bugs1111/LevelEditor/releases/assets/";
const MANIFEST: &str = "level-editor-updates.json";
const MAX_DOWNLOAD: u64 = 1024 * 1024 * 1024;
const MAX_UNPACKED: u64 = 4 * 1024 * 1024 * 1024;
const PRODUCT: &str = "level-editor";
const LEGACY_PRODUCT: &str = concat!("ldtk", "-rs");
const LEGACY_BUNDLE_NAME: &str = concat!("LDtk", " Rust.app");
const UPDATE_DIR_PREFIX: &str = "level-editor-update-";
const BUNDLE_NAMES: &[&str] = if cfg!(target_os = "macos") {
    &["Level Editor.app", LEGACY_BUNDLE_NAME]
} else {
    &[PRODUCT, LEGACY_PRODUCT]
};
type Result<T> = std::result::Result<T, String>;

pub fn target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        _ => None,
    }
}
pub fn environment_token() -> Option<String> {
    ["GH_TOKEN", "GITHUB_TOKEN"].into_iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty())
    })
}
pub fn prior_install_failure() -> Option<String> {
    installer::prior_failure()
}
#[derive(Clone)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    asset: String,
    url: String,
    sha256: String,
    size: u64,
}
#[derive(Clone)]
pub struct Prepared {
    pub version: String,
    work: PathBuf,
    root: PathBuf,
}
#[derive(Clone)]
pub enum State {
    Idle,
    Checking,
    Current(String),
    Available(Release),
    Downloading {
        version: String,
        done: u64,
        total: u64,
    },
    Ready(Prepared),
    Installing,
    Restarting,
    Cancelled,
    Failed(String),
}
enum Event {
    Checked(Result<Option<Release>>),
    Progress(u64, u64),
    Prepared(Result<Prepared>),
    InstallReady(Result<()>),
}
pub struct Updater {
    pub state: State,
    receiver: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    #[cfg(test)]
    test_install: bool,
    #[cfg(test)]
    test_check: bool,
    #[cfg(test)]
    pub(crate) test_install_requests: usize,
}
impl Default for Updater {
    fn default() -> Self {
        Self {
            state: State::Idle,
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            test_install: false,
            #[cfg(test)]
            test_check: false,
            #[cfg(test)]
            test_install_requests: 0,
        }
    }
}
impl Drop for Updater {
    fn drop(&mut self) {
        self.shutdown();
    }
}
impl Updater {
    pub fn shutdown(&mut self) {
        if let State::Ready(prepared) = &self.state {
            cleanup_work(&prepared.work);
            self.state = State::Cancelled;
        } else if matches!(self.state, State::Checking | State::Downloading { .. }) {
            self.cancel();
        }
    }
    #[cfg(test)]
    pub(crate) fn ready_fixture() -> Self {
        let mut updater = Self::default();
        updater.state = State::Ready(Prepared {
            version: "1.1.0".into(),
            work: PathBuf::new(),
            root: PathBuf::new(),
        });
        updater.test_install = true;
        updater
    }
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    #[cfg(test)]
    pub(crate) fn failed_fixture(error: &str) -> Self {
        let mut updater = Self::default();
        updater.state = State::Failed(error.into());
        updater.test_check = true;
        updater
    }
    pub fn check(&mut self, token: Option<String>, wake: impl Fn() + Send + 'static) {
        #[cfg(test)]
        if self.test_check {
            let (sender, receiver) = mpsc::channel();
            self.state = State::Checking;
            self.receiver = Some(receiver);
            sender.send(Event::Checked(Ok(None))).unwrap();
            wake();
            return;
        }
        self.check_feed(Feed::github(), token, wake);
    }
    fn check_feed(&mut self, feed: Feed, token: Option<String>, wake: impl Fn() + Send + 'static) {
        if self.busy() {
            return;
        }
        let old_work = match &self.state {
            State::Ready(prepared) => Some(prepared.work.clone()),
            _ => None,
        };
        self.state = State::Checking;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            if let Some(work) = old_work {
                cleanup_work(&work);
            }
            cleanup_pending_work();
            let result = check_release(&feed, env!("CARGO_PKG_VERSION"), token, &cancel);
            let _ = sender.send(Event::Checked(result));
            wake();
        });
    }
    pub fn download(&mut self, token: Option<String>, wake: impl Fn() + Send + 'static) {
        if self.busy() {
            return;
        }
        let State::Available(release) = self.state.clone() else {
            return;
        };
        self.state = State::Downloading {
            version: release.version.clone(),
            done: 0,
            total: release.size,
        };
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            let result = stage_release(&Feed::github(), &release, token, &cancel, |done, total| {
                let _ = sender.send(Event::Progress(done, total));
                wake();
            });
            let _ = sender.send(Event::Prepared(result));
            wake();
        });
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn install(&mut self, reopen: Option<PathBuf>, wake: impl Fn() + Send + 'static) {
        if self.busy() {
            return;
        }
        let State::Ready(prepared) = self.state.clone() else {
            return;
        };
        #[cfg(test)]
        if self.test_install {
            self.test_install_requests += 1;
            self.state = State::Installing;
            return;
        }
        self.state = State::Installing;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        std::thread::spawn(move || {
            let result = installer::launch(&prepared, reopen);
            let _ = sender.send(Event::InstallReady(result));
            wake();
        });
    }
    pub fn poll(&mut self) {
        let events = self
            .receiver
            .as_ref()
            .map(|r| r.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Progress(done, total) => {
                    if let State::Downloading {
                        done: current,
                        total: size,
                        ..
                    } = &mut self.state
                    {
                        *current = done;
                        *size = total;
                    }
                }
                Event::Checked(result) => {
                    self.receiver = None;
                    self.state = if self.cancel.load(Ordering::Relaxed) {
                        State::Cancelled
                    } else {
                        match result {
                            Ok(Some(release)) => State::Available(release),
                            Ok(None) => State::Current(env!("CARGO_PKG_VERSION").into()),
                            Err(error) => State::Failed(error),
                        }
                    };
                }
                Event::Prepared(result) => {
                    self.receiver = None;
                    self.state = match result {
                        Ok(prepared) if !self.cancel.load(Ordering::Relaxed) => {
                            State::Ready(prepared)
                        }
                        Ok(prepared) => {
                            let _ = fs::remove_dir_all(prepared.work);
                            State::Cancelled
                        }
                        Err(_) if self.cancel.load(Ordering::Relaxed) => State::Cancelled,
                        Err(error) => State::Failed(error),
                    };
                }
                Event::InstallReady(result) => {
                    self.receiver = None;
                    self.state = match result {
                        Ok(()) => State::Restarting,
                        Err(error) => State::Failed(error),
                    };
                }
            }
        }
    }
}
struct Feed {
    latest: String,
    assets: String,
    auth_origin: String,
    https: bool,
}
impl Feed {
    fn github() -> Self {
        Self {
            latest: API.into(),
            assets: ASSETS.into(),
            auth_origin: "https://api.github.com".into(),
            https: true,
        }
    }
}
struct Http {
    agent: ureq::Agent,
    token: Option<String>,
    auth_origin: String,
    https: bool,
}
fn origin(url: &str) -> Result<String> {
    let uri = url
        .parse::<ureq::http::Uri>()
        .map_err(|_| "Invalid release URL".to_owned())?;
    let scheme = uri.scheme_str().ok_or("Release URL has no scheme")?;
    let authority = uri.authority().ok_or("Release URL has no host")?;
    if !matches!(scheme, "http" | "https") || authority.as_str().contains('@') {
        return Err("Invalid release URL".into());
    }
    Ok(format!("{scheme}://{authority}"))
}
impl Http {
    fn new(feed: &Feed, token: Option<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(feed.https)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(15)))
            .timeout_per_read(Some(Duration::from_secs(3)))
            .timeout_global(Some(Duration::from_secs(600)))
            .build();
        Self {
            agent: config.into(),
            token,
            auth_origin: feed.auth_origin.clone(),
            https: feed.https,
        }
    }
    fn get(
        &self,
        url: &str,
        binary: bool,
        cancel: &AtomicBool,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let mut url = url.to_owned();
        for _ in 0..=5 {
            cancelled(cancel)?;
            let request_origin = origin(&url)?;
            if self.https && !request_origin.starts_with("https://") {
                return Err("Update redirect attempted an insecure connection".into());
            }
            let mut request = self
                .agent
                .get(&url)
                .header(
                    "User-Agent",
                    concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
                )
                .header(
                    "Accept",
                    if binary {
                        "application/octet-stream"
                    } else {
                        "application/vnd.github+json"
                    },
                )
                .header("X-GitHub-Api-Version", "2022-11-28");
            // Redirects are manual: credentials are sent only to the original API origin.
            if request_origin == self.auth_origin
                && let Some(token) = &self.token
            {
                request = request.header("Authorization", format!("Bearer {token}"));
            }
            let response=request.call().map_err(|_|"Network or TLS request failed while contacting the release service. Check your connection and retry.".to_owned())?;
            let status = response.status().as_u16();
            if (300..400).contains(&status) {
                url = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or("Update redirect has no valid Location")?
                    .to_owned();
                continue;
            }
            return match status {
                200 => Ok(response),
                401 | 403 | 404 => Err(format!(
                    "GitHub returned HTTP {status}. No published release may exist, or this private repository needs a token with Contents: read access. Enter a session token or set GH_TOKEN/GITHUB_TOKEN. Draft releases are not update candidates."
                )),
                _ => Err(format!(
                    "Release service returned HTTP {status}. Retry or open the Releases page."
                )),
            };
        }
        Err("Too many update redirects".into())
    }
    fn json(&self, url: &str, binary: bool, cancel: &AtomicBool) -> Result<Value> {
        let mut response = self.get(url, binary, cancel)?;
        let mut bytes = Vec::new();
        let mut reader = response.body_mut().as_reader();
        let mut buffer = [0; 8192];
        loop {
            cancelled(cancel)?;
            let count = reader
                .read(&mut buffer)
                .map_err(|_| "Failed to read the release metadata".to_owned())?;
            if count == 0 {
                break;
            }
            if bytes.len() + count > 2 * 1024 * 1024 {
                return Err("Release metadata is too large".into());
            }
            bytes.extend_from_slice(&buffer[..count]);
        }
        cancelled(cancel)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Release metadata is too large".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "Release metadata is not valid JSON".into())
    }
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("Update cancelled".into())
    } else {
        Ok(())
    }
}
fn check_release(
    feed: &Feed,
    current: &str,
    token: Option<String>,
    cancel: &AtomicBool,
) -> Result<Option<Release>> {
    let http = Http::new(feed, token);
    let value = http.json(&feed.latest, false, cancel)?;
    if value["draft"] == true || value["prerelease"] == true {
        return Err("Only published stable Rust releases can be installed".into());
    }
    let version = value["tag_name"]
        .as_str()
        .ok_or("Release has no version tag")?
        .trim_start_matches('v');
    let version =
        Version::parse(version).map_err(|_| "Release tag is not a semantic version".to_owned())?;
    let current =
        Version::parse(current).map_err(|_| "Current application version is invalid".to_owned())?;
    if !version.pre.is_empty() {
        return Err("Prerelease versions do not belong to the stable update channel".into());
    }
    if !version.cmp_precedence(&current).is_gt() {
        return Ok(None);
    }
    let assets = value["assets"]
        .as_array()
        .ok_or("Release has no Rust update assets")?;
    let asset = |name: &str| -> Result<&Value> {
        assets.iter().find(|a|a["name"]==name).ok_or_else(||format!("This release is missing {name}. Open Releases or try again after the build finishes."))
    };
    let url = |asset: &Value| -> Result<String> {
        let url = asset["url"]
            .as_str()
            .ok_or("Release asset has no API URL")?;
        if !url.starts_with(&feed.assets) {
            return Err("Release asset is outside this repository's API".into());
        }
        Ok(url.into())
    };
    let manifest_asset = asset(MANIFEST)
        .or_else(|missing| asset(&format!("{LEGACY_PRODUCT}-updates.json")).or(Err(missing)))?;
    let manifest = http.json(&url(manifest_asset)?, true, cancel)?;
    if manifest["version"].as_str() != Some(version.to_string().as_str()) {
        return Err("Update manifest version does not match the release tag".into());
    }
    let target = target().ok_or("No Rust update package is available for this OS/architecture")?;
    let selected = &manifest["targets"][target];
    let expected = format!("level-editor-v{version}-{target}.zip");
    let legacy = format!("{LEGACY_PRODUCT}-v{version}-{target}.zip");
    let name = selected["asset"].as_str().unwrap_or_default();
    if name != expected && name != legacy {
        return Err(format!(
            "No compatible Rust package for {target} in this release"
        ));
    }
    let sha256 = selected["sha256"]
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("Update manifest has no valid SHA-256")?
        .to_ascii_lowercase();
    let size = selected["size"]
        .as_u64()
        .filter(|&n| n > 0 && n <= MAX_DOWNLOAD)
        .ok_or("Update manifest has an invalid archive size")?;
    let asset = asset(name)?;
    if asset["size"].as_u64() != Some(size) {
        return Err("GitHub asset size does not match the update manifest".into());
    }
    let page = value["html_url"]
        .as_str()
        .filter(|url| url.starts_with(RELEASES_URL))
        .unwrap_or(RELEASES_URL);
    Ok(Some(Release {
        version: version.to_string(),
        notes: value["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(16_000)
            .collect(),
        page: page.into(),
        asset: name.into(),
        url: url(asset)?,
        sha256,
        size,
    }))
}
fn unique_work() -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("{UPDATE_DIR_PREFIX}{}-{stamp}", std::process::id()));
    fs::create_dir(&path).map_err(|e| format!("Cannot create update staging directory: {e}"))?;
    Ok(path)
}
fn is_update_work_name(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|name| {
        name.starts_with(UPDATE_DIR_PREFIX)
            || name
                .strip_prefix(LEGACY_PRODUCT)
                .is_some_and(|suffix| suffix.starts_with("-update-"))
    })
}
fn safe_work(work: &Path) -> bool {
    work.parent() == Some(std::env::temp_dir().as_path())
        && work.file_name().is_some_and(is_update_work_name)
        && work
            .symlink_metadata()
            .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}
fn cleanup_work(work: &Path) {
    if safe_work(work) {
        let _ = fs::remove_dir_all(work);
    }
}
fn cleanup_pending_work() {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let work = entry.path();
        if !safe_work(&work) || !work.join("cleanup-pending").is_file() {
            continue;
        }
        let mut retained = false;
        if let Ok(children) = fs::read_dir(&work) {
            for child in children.flatten() {
                if child.file_name() == "cleanup-pending" {
                    continue;
                }
                let result = if child.file_type().is_ok_and(|kind| kind.is_dir()) {
                    fs::remove_dir_all(child.path())
                } else {
                    fs::remove_file(child.path())
                };
                retained |= result.is_err();
            }
        }
        if !retained {
            let _ = fs::remove_file(work.join("cleanup-pending"));
            let _ = fs::remove_dir(&work);
        }
    }
}
static STARTED: AtomicBool = AtomicBool::new(false);
fn send_ready() -> Result<()> {
    let Some(address) = std::env::var_os("LDTK_UPDATE_READY_ADDR") else {
        return Ok(());
    };
    let address = address
        .to_str()
        .ok_or("Invalid update readiness address")?
        .parse::<std::net::SocketAddr>()
        .map_err(|_| "Invalid update readiness address")?;
    if !address.ip().is_loopback() {
        return Err("Update readiness handshake must be local".into());
    }
    let nonce =
        std::env::var("LDTK_UPDATE_READY_NONCE").map_err(|_| "Missing update readiness token")?;
    if nonce.len() != 64 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid update readiness token".into());
    }
    let mut socket = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    socket
        .write_all(nonce.as_bytes())
        .map_err(|e| e.to_string())
}
/// Called only after the new editor has completed its first successful UI frame.
pub fn mark_started() {
    if std::env::var_os("LDTK_UPDATE_READY_ADDR").is_none() || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    std::thread::spawn(|| {
        if send_ready().is_ok() {
            // The helper writes cleanup-pending after observing our ACK. On Windows
            // its own EXE remains locked until it exits, so retry from the new app.
            for _ in 0..20 {
                cleanup_pending_work();
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    });
}
fn stage_release(
    feed: &Feed,
    release: &Release,
    token: Option<String>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<Prepared> {
    let work = unique_work()?;
    let result = (|| {
        let http = Http::new(feed, token);
        let mut response = http.get(&release.url, true, cancel)?;
        let archive = work.join(&release.asset);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&archive)
            .map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut done = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        let mut last = std::time::Instant::now();
        let mut reader = response.body_mut().as_reader();
        progress(0, release.size);
        loop {
            cancelled(cancel)?;
            let count = reader
                .read(&mut buffer)
                .map_err(|_| "Failed to read the update archive; retry the download".to_owned())?;
            if count == 0 {
                break;
            }
            done += count as u64;
            if done > release.size {
                return Err("Downloaded update exceeds its declared size".into());
            }
            file.write_all(&buffer[..count])
                .map_err(|e| e.to_string())?;
            hash.update(&buffer[..count]);
            if last.elapsed() >= Duration::from_millis(40) {
                progress(done, release.size);
                last = std::time::Instant::now();
            }
        }
        file.sync_all().map_err(|e| e.to_string())?;
        if done != release.size || format!("{:x}", hash.finalize()) != release.sha256 {
            return Err("Update SHA-256/size verification failed. Nothing was installed.".into());
        }
        progress(done, release.size);
        let root = extract(&archive, &work, cancel)?;
        fs::remove_file(&archive).map_err(|e| e.to_string())?;
        Ok(Prepared {
            version: release.version.clone(),
            work: work.clone(),
            root,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&work);
    }
    result
}
fn marker_path(root: &Path, product: &str) -> PathBuf {
    let marker = format!("{product}-install.json");
    if cfg!(target_os = "macos") {
        root.join("Contents/Resources").join(marker)
    } else {
        root.join(marker)
    }
}
fn executable(root: &Path, product: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        root.join("Contents/MacOS").join(product)
    } else if cfg!(windows) {
        root.join(format!("{product}.exe"))
    } else {
        root.join(product)
    }
}
fn validate_root(root: &Path) -> Result<&'static str> {
    if root
        .symlink_metadata()
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Update installation root cannot be a symbolic link".into());
    }
    let (bytes, product) = match fs::read(marker_path(root, PRODUCT)) {
        Ok(bytes) => (bytes, PRODUCT),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let bytes = fs::read(marker_path(root, LEGACY_PRODUCT))
                .map_err(|_| "Install a Level Editor release package before using automatic updates. Download it from the Releases page.".to_owned())?;
            (bytes, LEGACY_PRODUCT)
        }
        Err(error) => return Err(error.to_string()),
    };
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Invalid Level Editor installation marker".to_owned())?;
    if value["product"] != product || value["layout"] != 1 || !executable(root, product).is_file() {
        return Err("Invalid Level Editor installation layout".into());
    }
    Ok(product)
}
fn extract(archive: &Path, work: &Path, cancel: &AtomicBool) -> Result<PathBuf> {
    let mut archive = zip::ZipArchive::new(File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Invalid update ZIP: {e}"))?;
    if archive.len() > 100_000 {
        return Err("Update ZIP contains too many files".into());
    }
    let mut total = 0_u64;
    let mut root_name = None;
    for index in 0..archive.len() {
        cancelled(cancel)?;
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let enclosed = entry.enclosed_name().ok_or("Unsafe path in update ZIP")?;
        let name = BUNDLE_NAMES
            .iter()
            .copied()
            .find(|name| enclosed.starts_with(name))
            .ok_or("Update ZIP contains an unexpected path")?;
        if entry.name().contains(['\\', ':'])
            || enclosed
                .components()
                .any(|part| part == Component::ParentDir)
            || root_name.is_some_and(|root| root != name)
        {
            return Err("Update ZIP contains an unexpected path".into());
        }
        root_name = Some(name);
        let mode = entry.unix_mode().unwrap_or(0);
        if mode & 0o170000 == 0o120000 {
            return Err("Symbolic links are not allowed in update ZIPs".into());
        }
        total = total
            .checked_add(entry.size())
            .filter(|&n| n <= MAX_UNPACKED)
            .ok_or("Update ZIP expands beyond the size limit")?;
        let path = work.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let mut buffer = [0; 64 * 1024];
        loop {
            cancelled(cancel)?;
            let count = entry.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            out.write_all(&buffer[..count]).map_err(|e| e.to_string())?;
        }
        out.sync_all().map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if mode == 0 { 0o644 } else { mode & 0o777 }),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    let root = work.join(root_name.ok_or("Update ZIP is empty")?);
    validate_root(&root)?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, io::Cursor, net::TcpListener, sync::Mutex};
    #[derive(Clone)]
    struct Reply {
        status: u16,
        body: Vec<u8>,
        location: Option<String>,
    }
    struct Server {
        base: String,
        routes: Arc<Mutex<HashMap<String, Reply>>>,
        requests: Arc<Mutex<Vec<(String, bool)>>>,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Server {
        fn new() -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            listener.set_nonblocking(true).unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let routes = Arc::new(Mutex::new(HashMap::<String, Reply>::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let (r, q, s) = (routes.clone(), requests.clone(), stop.clone());
            let thread = std::thread::spawn(move || {
                while !s.load(Ordering::Relaxed) {
                    let Ok((mut stream, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut one = [0];
                    while bytes.len() < 16_384 && stream.read(&mut one).unwrap_or(0) > 0 {
                        bytes.push(one[0]);
                        if bytes.ends_with(b"\r\n\r\n") {
                            break;
                        }
                    }
                    let request = String::from_utf8(bytes).unwrap();
                    let Some(path) = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    q.lock().unwrap().push((
                        path.clone(),
                        request
                            .to_ascii_lowercase()
                            .contains("authorization: bearer fixture-session-token"),
                    ));
                    let reply = r.lock().unwrap().get(&path).cloned().unwrap_or(Reply {
                        status: 404,
                        body: b"missing".to_vec(),
                        location: None,
                    });
                    let location = reply
                        .location
                        .map(|url| format!("Location: {url}\r\n"))
                        .unwrap_or_default();
                    let header = format!(
                        "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n{location}\r\n",
                        reply.status,
                        reply.body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&reply.body);
                }
            });
            Self {
                base,
                routes,
                requests,
                stop,
                thread: Some(thread),
            }
        }
        fn reply(&self, path: &str, body: Vec<u8>) {
            self.routes.lock().unwrap().insert(
                path.into(),
                Reply {
                    status: 200,
                    body,
                    location: None,
                },
            );
        }
        fn json(&self, path: &str, value: Value) {
            self.reply(path, serde_json::to_vec(&value).unwrap());
        }
        fn feed(&self) -> Feed {
            Feed {
                latest: format!("{}/latest", self.base),
                assets: format!("{}/assets/", self.base),
                auth_origin: self.base.clone(),
                https: false,
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ = self.thread.take().unwrap().join();
        }
    }
    fn archive() -> Vec<u8> {
        archive_with_root(BUNDLE_NAMES[0], PRODUCT)
    }
    fn archive_with_root(root: &str, product: &str) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let marker = marker_path(Path::new(root), product)
            .to_string_lossy()
            .replace('\\', "/");
        let exe = executable(Path::new(root), product)
            .to_string_lossy()
            .replace('\\', "/");
        writer
            .start_file(marker, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(&serde_json::to_vec(&json!({"product": product, "layout": 1})).unwrap())
            .unwrap();
        writer
            .start_file(
                exe,
                zip::write::SimpleFileOptions::default().unix_permissions(0o755),
            )
            .unwrap();
        writer.write_all(b"fixture-new-executable").unwrap();
        writer.finish().unwrap().into_inner()
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_updates_accept_both_bundle_names_in_separate_archives() {
        let cancel = AtomicBool::new(false);
        for (name, product) in [
            ("Level Editor.app", PRODUCT),
            (LEGACY_BUNDLE_NAME, LEGACY_PRODUCT),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("update.zip");
            fs::write(&path, archive_with_root(name, product)).unwrap();
            assert_eq!(
                extract(&path, directory.path(), &cancel).unwrap(),
                directory.path().join(name)
            );
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mixed.zip");
        let mut zip = zip::ZipWriter::new_append(Cursor::new(archive())).unwrap();
        zip.add_directory(
            format!("{LEGACY_BUNDLE_NAME}/"),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        fs::write(&path, zip.finish().unwrap().into_inner()).unwrap();
        assert_eq!(
            extract(&path, directory.path(), &cancel).unwrap_err(),
            "Update ZIP contains an unexpected path"
        );
    }
    fn publish(server: &Server, bytes: &[u8], version: &str) {
        publish_with_prefix(server, bytes, version, LEGACY_PRODUCT);
    }
    fn publish_with_prefix(server: &Server, bytes: &[u8], version: &str, prefix: &str) {
        let name = format!("{prefix}-v{version}-{}.zip", target().unwrap());
        let manifest = format!("{prefix}-updates.json");
        server.reply("/assets/package", bytes.to_vec());
        server.json("/assets/manifest",json!({"version":version,"targets":{target().unwrap():{"asset":name,"sha256":format!("{:x}",Sha256::digest(bytes)),"size":bytes.len()}}}));
        server.json("/latest",json!({"tag_name":format!("v{version}"),"draft":false,"prerelease":false,"html_url":format!("{RELEASES_URL}/tag/v{version}"),"body":"Fixture Rust release","assets":[{"name":manifest,"url":format!("{}/assets/manifest",server.base)},{"name":name,"url":format!("{}/assets/package",server.base),"size":bytes.len()}]}));
    }
    #[test]
    fn checks_and_stages_a_release_with_only_level_editor_asset_names() {
        let server = Server::new();
        publish_with_prefix(&server, &archive(), "1.1.0", "level-editor");
        let cancel = AtomicBool::new(false);
        let release = check_release(&server.feed(), "0.1.0", None, &cancel)
            .unwrap()
            .unwrap();
        assert_eq!(
            release.asset,
            format!("level-editor-v1.1.0-{}.zip", target().unwrap())
        );
        let prepared = stage_release(&server.feed(), &release, None, &cancel, |_, _| {}).unwrap();
        assert_eq!(
            fs::read(executable(&prepared.root, PRODUCT)).unwrap(),
            b"fixture-new-executable"
        );
        fs::remove_dir_all(prepared.work).unwrap();
    }
    #[test]
    fn private_release_check_download_sha_and_executable_staging_use_native_http() {
        let server = Server::new();
        let bytes = archive_with_root(BUNDLE_NAMES[1], LEGACY_PRODUCT);
        publish(&server, &bytes, "1.1.0");
        let cancel = AtomicBool::new(false);
        let release = check_release(
            &server.feed(),
            "0.1.0",
            Some("fixture-session-token".into()),
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(release.version, "1.1.0");
        let mut progress = Vec::new();
        let prepared = stage_release(
            &server.feed(),
            &release,
            Some("fixture-session-token".into()),
            &cancel,
            |done, total| progress.push((done, total)),
        )
        .unwrap();
        assert_eq!(
            fs::read(executable(&prepared.root, LEGACY_PRODUCT)).unwrap(),
            b"fixture-new-executable"
        );
        assert_eq!(progress.first(), Some(&(0, bytes.len() as u64)));
        assert_eq!(
            progress.last(),
            Some(&(bytes.len() as u64, bytes.len() as u64))
        );
        assert!(
            server
                .requests
                .lock()
                .unwrap()
                .iter()
                .all(|(_, authenticated)| *authenticated)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(executable(&prepared.root, LEGACY_PRODUCT))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
        fs::remove_dir_all(prepared.work).unwrap();
    }
    #[test]
    fn current_semver_skips_asset_download_and_404_explains_private_access() {
        let server = Server::new();
        publish(&server, &archive(), "1.1.0");
        let cancel = AtomicBool::new(false);
        assert!(
            check_release(&server.feed(), "1.1.0", None, &cancel)
                .unwrap()
                .is_none()
        );
        assert_eq!(server.requests.lock().unwrap().len(), 1);
        server.routes.lock().unwrap().remove("/latest");
        let error = check_release(&server.feed(), "0.1.0", None, &cancel)
            .err()
            .unwrap();
        assert!(
            error.contains("HTTP 404")
                && error.contains("Contents: read")
                && error.contains("Draft releases")
        );
    }
    #[test]
    fn build_metadata_does_not_create_an_update_and_misflagged_prereleases_are_rejected() {
        let server = Server::new();
        let cancel = AtomicBool::new(false);
        publish(&server, &archive(), "0.1.0+build.7");
        assert!(
            check_release(&server.feed(), "0.1.0", None, &cancel)
                .unwrap()
                .is_none()
        );
        assert_eq!(server.requests.lock().unwrap().len(), 1);
        publish(&server, &archive(), "1.1.0-beta.1");
        assert!(
            check_release(&server.feed(), "0.1.0", None, &cancel)
                .err()
                .unwrap()
                .contains("Prerelease")
        );
    }
    #[test]
    fn cancelled_metadata_read_does_not_wait_for_a_slow_complete_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut one = [0];
            while socket.read(&mut one).unwrap_or(0) > 0 {
                request.push(one[0]);
                if request.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 40\r\nConnection: close\r\n\r\n")
                .unwrap();
            for _ in 0..40 {
                if done.load(Ordering::Relaxed) {
                    break;
                }
                if socket.write_all(b" ").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let timer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            signal.store(true, Ordering::Relaxed);
        });
        let feed = Feed {
            latest: base.clone(),
            assets: base.clone(),
            auth_origin: base.clone(),
            https: false,
        };
        let start = std::time::Instant::now();
        let result = Http::new(&feed, None).json(&base, false, &cancel);
        let elapsed = start.elapsed();
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        timer.join().unwrap();
        assert!(result.err().unwrap().contains("cancelled"));
        assert!(
            elapsed < Duration::from_millis(500),
            "cancel waited for the entire slow body: {elapsed:?}"
        );
    }
    #[test]
    fn checking_again_removes_an_abandoned_ready_stage_and_success_cleanup_is_scoped() {
        let server = Server::new();
        publish(&server, &archive(), env!("CARGO_PKG_VERSION"));
        let old = unique_work().unwrap();
        fs::write(old.join("unused-stage"), b"old").unwrap();
        let mut updater = Updater::default();
        updater.state = State::Ready(Prepared {
            version: "1.1.0".into(),
            root: old.clone(),
            work: old.clone(),
        });
        updater.check_feed(server.feed(), None, || {});
        for _ in 0..200 {
            updater.poll();
            if !updater.busy() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(updater.state, State::Current(_)));
        assert!(!old.exists());
        let success = unique_work().unwrap();
        fs::create_dir(success.join("old-package")).unwrap();
        fs::write(success.join("old-package/exe"), b"old").unwrap();
        fs::write(success.join("cleanup-pending"), b"installed").unwrap();
        let unrelated = tempfile::tempdir().unwrap();
        fs::write(
            unrelated.path().join("cleanup-pending"),
            b"not an updater-owned directory",
        )
        .unwrap();
        cleanup_pending_work();
        assert!(!success.exists());
        assert!(unrelated.path().join("cleanup-pending").exists());
    }
    #[test]
    fn closing_with_a_ready_update_cleans_only_the_uninstalled_stage() {
        let work = unique_work().unwrap();
        fs::write(work.join("downloaded-application"), b"fixture").unwrap();
        let mut updater = Updater::default();
        updater.state = State::Ready(Prepared {
            version: "1.1.0".into(),
            work: work.clone(),
            root: work.clone(),
        });
        drop(updater);
        assert!(!work.exists());
        let installing = unique_work().unwrap();
        let mut updater = Updater::default();
        updater.state = State::Installing;
        drop(updater);
        assert!(
            installing.exists(),
            "an install in progress must retain its helper workspace"
        );
        cleanup_work(&installing);
    }
    /// Explicit release acceptance only: requires network access, GH_TOKEN or
    /// GITHUB_TOKEN and a published stable release with this host's Rust asset.
    #[test]
    #[ignore = "requires authenticated access to the published private GitHub update channel"]
    fn live_private_release_downloads_verifies_and_stages_without_installing() {
        let token = environment_token()
            .expect("GH_TOKEN or GITHUB_TOKEN is required for private release acceptance");
        let cancel = AtomicBool::new(false);
        let feed = Feed::github();
        let release = check_release(&feed, "0.0.0", Some(token.clone()), &cancel)
            .expect("live release check failed")
            .expect("published release must be newer than acceptance version 0.0.0");
        let prepared = stage_release(&feed, &release, Some(token), &cancel, |_, _| {})
            .expect("live Rust asset verification/staging failed");
        validate_root(&prepared.root).unwrap();
        cleanup_work(&prepared.work);
        assert!(!prepared.work.exists());
    }
    #[test]
    fn cross_origin_asset_redirect_does_not_forward_the_session_token() {
        let first = Server::new();
        let second = Server::new();
        second.reply("/signed-download", b"download".to_vec());
        first.routes.lock().unwrap().insert(
            "/assets/redirect".into(),
            Reply {
                status: 302,
                body: Vec::new(),
                location: Some(format!("{}/signed-download", second.base)),
            },
        );
        let http = Http::new(&first.feed(), Some("fixture-session-token".into()));
        let mut response = http
            .get(
                &format!("{}/assets/redirect", first.base),
                true,
                &AtomicBool::new(false),
            )
            .unwrap();
        let mut body = String::new();
        response
            .body_mut()
            .as_reader()
            .read_to_string(&mut body)
            .unwrap();
        assert_eq!(body, "download");
        assert!(first.requests.lock().unwrap()[0].1);
        assert!(!second.requests.lock().unwrap()[0].1);
    }
    #[test]
    fn corrupt_download_and_cancel_never_produce_an_installable_stage() {
        let server = Server::new();
        publish(&server, &archive(), "1.1.0");
        let cancel = AtomicBool::new(false);
        let mut release = check_release(&server.feed(), "0.1.0", None, &cancel)
            .unwrap()
            .unwrap();
        release.sha256 = "0".repeat(64);
        assert!(
            stage_release(&server.feed(), &release, None, &cancel, |_, _| {})
                .err()
                .unwrap()
                .contains("SHA-256")
        );
        release.sha256 = format!("{:x}", Sha256::digest(archive()));
        assert!(
            stage_release(&server.feed(), &release, None, &cancel, |done, _| {
                if done > 0 {
                    cancel.store(true, Ordering::Relaxed)
                }
            })
            .is_err()
        );
    }
    #[test]
    fn unsafe_archive_paths_are_rejected_before_writing_outside_the_stage() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("bad.zip");
        let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
        zip.start_file("../outside", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
        assert!(extract(&archive, dir.path(), &AtomicBool::new(false)).is_err());
        assert!(!dir.path().parent().unwrap().join("outside").exists());
    }
}
