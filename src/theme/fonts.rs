use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, OnceLock, Weak},
};

const BUNDLED_CJK: &[u8] = include_bytes!("../../assets/fonts/LevelEditorCJK-Regular.otf");
const LATIN: [&str; 3] = ["system-ui-regular", "system-ui-bold", "system-ui-light"];
const CHINESE: [&str; 3] = ["system-cjk-regular", "system-cjk-bold", "system-cjk-light"];
const BUNDLED_LATIN: [&str; 3] = ["ldtk-regular", "ldtk-bold", "ldtk-light"];
const REGULAR: usize = 0;
const BOLD: usize = 1;
const LIGHT: usize = 2;

static FONT_DEFINITIONS: [LazyLock<FontDefinitions>; 2] = [
    LazyLock::new(|| font_definitions(false)),
    LazyLock::new(|| font_definitions(true)),
];

fn font_definitions(chinese: bool) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            BUNDLED_LATIN[REGULAR],
            &include_bytes!("../../assets/fonts/RobotoCondensed-Regular.ttf")[..],
        ),
        (
            BUNDLED_LATIN[BOLD],
            &include_bytes!("../../assets/fonts/RobotoCondensed-Bold.ttf")[..],
        ),
        (
            BUNDLED_LATIN[LIGHT],
            &include_bytes!("../../assets/fonts/RobotoCondensed-Light.ttf")[..],
        ),
        ("bundled-cjk", BUNDLED_CJK),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    for (cjk, names) in [(false, LATIN), (true, CHINESE)] {
        for (weight, name) in names.into_iter().enumerate() {
            if (!cjk || chinese || weight == REGULAR)
                && let Some(font) = cached_system_font(cjk, weight)
            {
                fonts.font_data.insert(name.into(), Arc::new(font));
            }
        }
    }
    static MONOSPACE: LazyLock<Option<SystemFace>> = LazyLock::new(system_monospace);
    if let Some(face) = MONOSPACE.as_ref() {
        fonts
            .font_data
            .insert("system-monospace".into(), Arc::new(face.font_data()));
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "system-monospace".into());
    }
    static SYMBOLS: LazyLock<Option<SystemFace>> = LazyLock::new(load_system_symbols);
    if let Some(face) = SYMBOLS.as_ref() {
        fonts
            .font_data
            .insert("system-symbols".into(), Arc::new(face.font_data()));
        for family in fonts.families.values_mut() {
            family.push("system-symbols".into());
        }
    }
    with_priorities(fonts, chinese)
}

struct SystemFace {
    bytes: Arc<Vec<u8>>,
    index: u32,
}

impl SystemFace {
    fn load(path: &Path, select: impl FnOnce(&[u8]) -> Option<u32>) -> Option<Self> {
        // A collection can supply several roles. Keep a single buffer per file while
        // any accepted face uses it, and release rejected candidates immediately.
        static FILES: LazyLock<Mutex<HashMap<PathBuf, Weak<Vec<u8>>>>> =
            LazyLock::new(Mutex::default);
        let bytes = {
            let mut files = FILES.lock().unwrap();
            if let Some(bytes) = files.get(path).and_then(Weak::upgrade) {
                bytes
            } else {
                let bytes = Arc::new(std::fs::read(path).ok()?);
                files.insert(path.to_owned(), Arc::downgrade(&bytes));
                bytes
            }
        };
        let index = select(&bytes)?;
        Some(Self { bytes, index })
    }

    fn font_data(&'static self) -> FontData {
        FontData {
            index: self.index,
            ..FontData::from_static(&self.bytes)
        }
    }
}

fn load_system_symbols() -> Option<SystemFace> {
    let symbols = if cfg!(target_os = "macos") {
        PathBuf::from("/System/Library/Fonts/Apple Symbols.ttf")
    } else if cfg!(target_os = "windows") {
        windows_font_directory().join("seguisym.ttf")
    } else {
        PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
    };
    SystemFace::load(&symbols, |bytes| {
        ttf_parser::Face::parse(bytes, 0).ok().map(|_| 0)
    })
}

fn cached_system_font(chinese: bool, weight: usize) -> Option<FontData> {
    // Egui copies owned font bytes when creating its atlas. Lend it the cached face instead.
    static FACES: [[OnceLock<Option<SystemFace>>; 3]; 2] =
        [const { [const { OnceLock::new() }; 3] }; 2];
    let face = FACES[usize::from(chinese)][weight]
        .get_or_init(|| system_font(chinese, weight))
        .as_ref()?;
    Some(face.font_data())
}

pub(crate) fn set_font_language(ctx: &egui::Context, chinese: bool) {
    ctx.set_fonts(FONT_DEFINITIONS[usize::from(chinese)].clone());
}

#[cfg(test)]
pub(crate) fn use_reference_layout_fonts(ctx: &egui::Context) {
    // These tests compare exact coordinates measured with the original UI typography.
    // Keep the reference font metrics local to those tests.
    static REFERENCE: LazyLock<FontDefinitions> = LazyLock::new(|| {
        let mut fonts = FONT_DEFINITIONS[0].clone();
        let arial = if cfg!(target_os = "macos") {
            PathBuf::from("/System/Library/Fonts/Supplemental")
        } else {
            windows_font_directory()
        };
        for (weight, file) in [(REGULAR, "Arial.ttf"), (BOLD, "Arial Bold.ttf")] {
            let file = if cfg!(target_os = "windows") && weight == BOLD {
                "arialbd.ttf"
            } else {
                file
            };
            if let Ok(bytes) = std::fs::read(arial.join(file)) {
                fonts
                    .font_data
                    .insert(LATIN[weight].into(), Arc::new(FontData::from_owned(bytes)));
            }
        }
        for (name, weight) in [
            ("heading", BOLD),
            ("panel-title", LIGHT),
            ("panel-header", REGULAR),
        ] {
            let sequence = fonts
                .families
                .get_mut(&FontFamily::Name(name.into()))
                .unwrap();
            sequence.retain(|entry| entry != BUNDLED_LATIN[weight]);
            sequence.insert(0, BUNDLED_LATIN[weight].into());
        }
        fonts
    });
    ctx.set_fonts(REFERENCE.clone());
}

fn with_priorities(mut fonts: FontDefinitions, chinese: bool) -> FontDefinitions {
    let defaults = fonts.families[&FontFamily::Proportional].clone();
    for (family, weight) in [
        (FontFamily::Proportional, REGULAR),
        (FontFamily::Name("heading".into()), BOLD),
        (FontFamily::Name("body-bold".into()), BOLD),
        (FontFamily::Name("panel-title".into()), LIGHT),
        (FontFamily::Name("panel-header".into()), REGULAR),
    ] {
        let preferred = if chinese {
            [
                CHINESE[weight],
                CHINESE[REGULAR],
                "bundled-cjk",
                LATIN[weight],
                LATIN[REGULAR],
            ]
        } else {
            [
                LATIN[weight],
                BUNDLED_LATIN[weight],
                LATIN[REGULAR],
                CHINESE[REGULAR],
                "bundled-cjk",
            ]
        };
        let mut names = Vec::new();
        for name in preferred
            .into_iter()
            .map(str::to_owned)
            .chain(defaults.iter().cloned())
        {
            if fonts.font_data.contains_key(&name) && !names.contains(&name) {
                names.push(name);
            }
        }
        fonts.families.insert(family, names);
    }
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    if fonts.font_data.contains_key(CHINESE[REGULAR]) {
        mono.push(CHINESE[REGULAR].into());
    }
    mono.push("bundled-cjk".into());
    fonts
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn load_font(
    path: &Path,
    indices: impl IntoIterator<Item = u32>,
    chinese: bool,
    weight: usize,
) -> Option<SystemFace> {
    SystemFace::load(path, |bytes| {
        indices.into_iter().find(|&index| {
            ttf_parser::Face::parse(bytes, index)
                .is_ok_and(|face| suitable_face(&face, chinese, weight))
        })
    })
}

fn suitable_face(face: &ttf_parser::Face<'_>, chinese: bool, weight: usize) -> bool {
    let actual = face.weight().to_number();
    let weight_matches = match weight {
        BOLD => actual >= 600,
        LIGHT => actual < 400,
        _ => (350..=500).contains(&actual),
    };
    weight_matches
        && !face.is_italic()
        && (' '..='~').all(|ch| face.glyph_index(ch).is_some())
        && (!chinese
            || "中文了解导出限制"
                .chars()
                .all(|ch| face.glyph_index(ch).is_some()))
}

#[cfg(target_os = "linux")]
fn system_font(chinese: bool, weight: usize) -> Option<SystemFace> {
    let lang = if chinese { "zh-cn" } else { "en" };
    let weight_name = ["regular", "bold", "light"][weight];
    if let Some((path, index)) =
        fontconfig_match(&format!("sans-serif:lang={lang}:weight={weight_name}"))
        && let Some(font) = load_font(&path, [index], chinese, weight)
    {
        return Some(font);
    }
    let (path, index) = if chinese {
        (
            [
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Light.ttc",
            ][weight],
            2,
        )
    } else {
        (
            [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-ExtraLight.ttf",
            ][weight],
            0,
        )
    };
    load_font(Path::new(path), [index], chinese, weight)
}

#[cfg(target_os = "linux")]
fn fontconfig_match(pattern: &str) -> Option<(PathBuf, u32)> {
    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{file}\t%{index}", pattern])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let (path, index) = std::str::from_utf8(&output.stdout)
        .ok()?
        .rsplit_once('\t')?;
    Some((path.into(), index.parse::<u32>().ok()? & 0xffff))
}

fn system_monospace() -> Option<SystemFace> {
    fn load(path: &Path, requested: Option<u32>) -> Option<SystemFace> {
        SystemFace::load(path, |bytes| {
            let count = ttf_parser::fonts_in_collection(bytes).unwrap_or(1);
            let mut indices = requested.map_or(0..count, |index| index..index + 1);
            indices.find(|&index| {
                ttf_parser::Face::parse(bytes, index)
                    .is_ok_and(|face| face.is_monospaced() && suitable_face(&face, false, REGULAR))
            })
        })
    }
    #[cfg(target_os = "linux")]
    if let Some((path, index)) = fontconfig_match("monospace:lang=en:weight=regular")
        && let Some(face) = load(&path, Some(index))
    {
        return Some(face);
    }
    if cfg!(target_os = "macos") {
        load(Path::new("/System/Library/Fonts/Menlo.ttc"), None)
    } else if cfg!(target_os = "windows") {
        load(&windows_font_directory().join("consola.ttf"), None)
    } else {
        load(
            Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
            None,
        )
    }
}

fn windows_font_directory() -> PathBuf {
    PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into())).join("Fonts")
}

#[cfg(target_os = "windows")]
fn system_font(chinese: bool, weight: usize) -> Option<SystemFace> {
    let files = if chinese {
        ["msyh.ttc", "msyhbd.ttc", "msyhl.ttc"]
    } else {
        ["segoeui.ttf", "segoeuib.ttf", "segoeuil.ttf"]
    };
    // Microsoft YaHei UI is the second face in the YaHei collection.
    load_font(
        &windows_font_directory().join(files[weight]),
        [u32::from(chinese), 0],
        chinese,
        weight,
    )
}

#[cfg(target_os = "macos")]
fn system_font(chinese: bool, weight: usize) -> Option<SystemFace> {
    use core_foundation::{
        base::{CFType, CFTypeRef, TCFType},
        string::{CFString, CFStringRef},
        url::CFURL,
    };

    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontCreateWithName(
            name: CFStringRef,
            size: f64,
            matrix: *const std::ffi::c_void,
        ) -> CFTypeRef;
        fn CTFontCreateUIFontForLanguage(kind: u32, size: f64, language: CFStringRef) -> CFTypeRef;
        fn CTFontCopyAttribute(font: CFTypeRef, attribute: CFStringRef) -> CFTypeRef;
        static kCTFontURLAttribute: CFStringRef;
    }

    let from_font = |font: CFTypeRef, name: Option<&str>| {
        if font.is_null() {
            return None;
        }
        let font = unsafe { CFType::wrap_under_create_rule(font) };
        let url = unsafe { CTFontCopyAttribute(font.as_CFTypeRef(), kCTFontURLAttribute) };
        if url.is_null() {
            return None;
        }
        let url = unsafe { CFURL::wrap_under_create_rule(url.cast()) };
        SystemFace::load(&url.to_path()?, |bytes| {
            (0..ttf_parser::fonts_in_collection(bytes).unwrap_or(1)).find(|&index| {
                ttf_parser::Face::parse(bytes, index).is_ok_and(|face| {
                    suitable_face(&face, chinese, weight)
                        && name.is_none_or(|name| {
                            face.names().into_iter().any(|entry| {
                                entry.name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                                    && entry.to_string().as_deref() == Some(name)
                            })
                        })
                })
            })
        })
    };

    if !chinese && weight != LIGHT {
        const SYSTEM: u32 = 2;
        const EMPHASIZED_SYSTEM: u32 = 3;
        let kind = if weight == BOLD {
            EMPHASIZED_SYSTEM
        } else {
            SYSTEM
        };
        let language = CFString::new("en");
        let font =
            unsafe { CTFontCreateUIFontForLanguage(kind, 0., language.as_concrete_TypeRef()) };
        if let Some(data) = from_font(font, None) {
            return Some(data);
        }
    }

    // Egui reads a font file's default variation. Use a named static face when
    // the native UI font's requested weight cannot be read from that file.
    let name = if chinese {
        [
            "PingFangSC-Regular",
            "PingFangSC-Semibold",
            "PingFangSC-Light",
        ][weight]
    } else {
        ["HelveticaNeue", "HelveticaNeue-Bold", "HelveticaNeue-Light"][weight]
    };
    let font_name = CFString::new(name);
    // Core Text resolves relocated system fonts as well as fonts in /System/Library/Fonts.
    let font =
        unsafe { CTFontCreateWithName(font_name.as_concrete_TypeRef(), 0., std::ptr::null()) };
    from_font(font, Some(name))
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn system_font(_: bool, _: usize) -> Option<SystemFace> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::FontId;

    #[test]
    fn chinese_roles_use_one_face_for_han_and_latin_and_keep_monospace() {
        let [english, chinese] = &FONT_DEFINITIONS;
        for weight in [BOLD, LIGHT] {
            assert!(!english.font_data.contains_key(CHINESE[weight]));
        }
        for fonts in [english, chinese] {
            for (name, face) in &fonts.font_data {
                if name.starts_with("system-") {
                    assert!(matches!(face.font, std::borrow::Cow::Borrowed(_)), "{name}");
                }
            }
        }
        for (family, weight) in [
            (FontFamily::Proportional, REGULAR),
            (FontFamily::Name("heading".into()), BOLD),
            (FontFamily::Name("body-bold".into()), BOLD),
            (FontFamily::Name("panel-title".into()), LIGHT),
            (FontFamily::Name("panel-header".into()), REGULAR),
        ] {
            let expected = [CHINESE[weight], CHINESE[REGULAR], "bundled-cjk"]
                .into_iter()
                .find(|name| chinese.font_data.contains_key(*name))
                .unwrap();
            assert_eq!(chinese.families[&family][0], expected);
            let data = &chinese.font_data[expected];
            let face = ttf_parser::Face::parse(&data.font, data.index).unwrap();
            assert!(
                "了解 Tiled TMX 导出限制"
                    .chars()
                    .all(|ch| face.glyph_index(ch).is_some()),
                "mixed script must fit in {expected} for {family:?}"
            );
            let expected = if english.font_data.contains_key(LATIN[weight]) {
                LATIN[weight]
            } else {
                BUNDLED_LATIN[weight]
            };
            assert_eq!(english.families[&family][0], expected);
        }
        assert_eq!(
            english.families[&FontFamily::Monospace][0],
            chinese.families[&FontFamily::Monospace][0],
        );
        if let Some(mono) = english.font_data.get("system-monospace") {
            assert_eq!(
                english.families[&FontFamily::Monospace][0],
                "system-monospace"
            );
            let face = ttf_parser::Face::parse(&mono.font, mono.index).unwrap();
            assert!(face.is_monospaced() && suitable_face(&face, false, REGULAR));
        }
    }

    #[test]
    fn system_faces_share_file_bytes_and_release_them_with_the_last_face() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts/RobotoCondensed-Regular.ttf");
        let load = || {
            SystemFace::load(&path, |bytes| {
                ttf_parser::Face::parse(bytes, 0).ok().map(|_| 0)
            })
            .unwrap()
        };
        let first = load();
        let second = load();
        assert!(Arc::ptr_eq(&first.bytes, &second.bytes));
        let bytes = Arc::downgrade(&first.bytes);
        drop((first, second));
        assert!(bytes.upgrade().is_none());
    }

    #[test]
    fn bundled_cjk_covers_the_simplified_chinese_interface_without_system_fonts() {
        let ctx = egui::Context::default();
        let mut fonts = FontDefinitions::empty();
        fonts.font_data.insert(
            "bundled-cjk".into(),
            Arc::new(FontData::from_static(BUNDLED_CJK)),
        );
        fonts
            .families
            .insert(FontFamily::Proportional, vec!["bundled-cjk".into()]);
        ctx.set_fonts(fonts);
        let messages: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../../assets/locales/zh-CN.json")).unwrap();
        let glyphs: std::collections::BTreeSet<_> = messages
            .values()
            .flat_map(|message| message.chars())
            .filter(|ch| matches!(*ch as u32, 0x3000..=0x30ff | 0x3400..=0x9fff | 0xff00..=0xffef))
            .collect();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.fonts_mut(|fonts| {
                for ch in &glyphs {
                    assert!(
                        fonts.has_glyph(&FontId::proportional(14.), *ch),
                        "missing {ch}"
                    );
                }
            });
        });
    }
}
