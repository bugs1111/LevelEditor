use ldtk_core::Project;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(10);
const PROTOCOL: &str = "2025-11-25";

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<String>,
    id: u32,
}

impl Client {
    fn start(path: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ldtk-rs"))
            .args(["--mcp"])
            .arg(path)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            output,
            id: 0,
        }
    }

    fn send(&mut self, message: Value) {
        writeln!(self.input.as_mut().unwrap(), "{message}").unwrap();
    }

    fn receive(&self) -> Value {
        let response = self
            .output
            .recv_timeout(TIMEOUT)
            .expect("LevelEditor MCP should return a response over stdout");
        serde_json::from_str(&response).unwrap_or_else(|_| panic!("Invalid MCP line: {response}"))
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        self.send(json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params}));
        let response = self.receive();
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], self.id, "{response}");
        response
    }

    fn initialize(&mut self, version: &str) -> Value {
        self.request(
            "initialize",
            json!({
                "protocolVersion":version,
                "capabilities":{},
                "clientInfo":{"name":"stdio-test","version":"1"}
            }),
        )["result"]
            .clone()
    }

    fn initialized(&mut self) {
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    }

    fn call(&mut self, name: &str, args: Value) -> Value {
        let response = self.request("tools/call", json!({"name":name,"arguments":args}));
        let result = &response["result"];
        assert!(result.is_object(), "{response}");
        result.clone()
    }

    fn tool(&mut self, name: &str, args: Value) -> Value {
        let result = self.call(name, args);
        assert_eq!(result["isError"], false, "{name} failed: {result}");
        let text = result["content"][0]["text"].as_str().unwrap();
        let value: Value = serde_json::from_str(text).unwrap();
        if let Some(structured) = result.get("structuredContent") {
            assert_eq!(structured, &value);
        }
        value
    }

    fn close(mut self) {
        self.input.take();
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "{status}");
                assert!(self.output.recv_timeout(TIMEOUT).is_err());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "MCP process stayed alive after EOF"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/original")
        .join(name)
}

fn copy_external_project(directory: &Path) -> PathBuf {
    let sample = "SeparateLevelFiles";
    let path = directory.join(format!("{sample}.ldtk"));
    fs::copy(sample_path("SeparateLevelFiles.ldtk"), &path).unwrap();
    fs::create_dir(directory.join(sample)).unwrap();
    for source in fs::read_dir(sample_path(sample)).unwrap() {
        let source = source.unwrap();
        fs::copy(
            source.path(),
            directory.join(sample).join(source.file_name()),
        )
        .unwrap();
    }
    path
}

#[test]
fn negotiates_mcp_and_discover_tools_without_a_window() {
    let path = sample_path("Typical_TopDown_example.ldtk");
    let mut client = Client::start(&path);
    writeln!(client.input.as_mut().unwrap(), "{{").unwrap();
    let malformed = client.receive();
    assert_eq!(malformed["error"]["code"], -32700);
    assert!(malformed.get("id").is_none());
    client.send(json!([]));
    assert_eq!(client.receive()["error"]["code"], -32600);
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));
    assert_eq!(
        client.request("initialize", json!({}))["error"]["code"],
        -32602
    );
    let initialize = client.initialize("2099-01-01");
    assert_eq!(initialize["protocolVersion"], PROTOCOL);
    assert_eq!(initialize["serverInfo"]["name"], "LevelEditor");
    assert!(initialize["capabilities"]["tools"].is_object());
    client.send(json!({"jsonrpc":"2.0","id":"ping-1","method":"ping"}));
    assert_eq!(client.receive()["id"], "ping-1");
    client.send(json!({"jsonrpc":"2.0","id":1.5,"method":"ping"}));
    assert_eq!(client.receive()["error"]["code"], -32600);
    assert!(client.request("tools/list", json!({}))["error"].is_object());
    client.initialized();
    client
        .send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}));
    let list = client.request("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().unwrap();
    for name in [
        "project_info",
        "get_definitions",
        "paint_int_grid",
        "save_project",
    ] {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
    assert_eq!(
        client.request("tools/call", json!({"name":"missing"}))["error"]["code"],
        -32602
    );
    assert_eq!(
        client.request("tools/call", json!({"name":"project_info","arguments":[]}))["error"]["code"],
        -32602
    );
    assert_eq!(
        client.request("missing", json!({}))["error"]["code"],
        -32601
    );
    let result = client.tool("project_info", json!({}));
    assert_eq!(result["path"], json!(path.canonicalize().unwrap()));
    assert_eq!(result["dirty"], false);
    client.close();

    let mut client = Client::start(&path);
    assert_eq!(
        client.initialize("2024-11-05")["protocolVersion"],
        "2024-11-05"
    );
    client.initialized();
    let result = client.call("project_info", json!({}));
    assert_eq!(result["isError"], false);
    assert!(result.get("structuredContent").is_none());
    client.close();
}

#[test]
fn world_filter_distinguishes_empty_unknown_and_legacy_root_worlds() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("worlds.ldtk");
    let mut root = Project::new().json;
    let level = root["levels"][0].take();
    root["levels"] = json!([]);
    root["dummyWorldIid"] = json!("legacy");
    root["worlds"] = json!([{"iid":"empty","levels":[]}, {"iid":"populated","levels":[level]}]);
    fs::write(&path, serde_json::to_vec(&root).unwrap()).unwrap();
    let mut client = Client::start(&path);
    client.initialize(PROTOCOL);
    client.initialized();
    assert_eq!(
        client.tool("list_worlds", json!({}))["worlds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        client.tool("list_levels", json!({"world_iid":"empty"}))["levels"],
        json!([])
    );
    assert_eq!(
        client.call("list_levels", json!({"world_iid":"legacy"}))["isError"],
        true
    );
    let all = client.tool("list_levels", json!({}));
    assert_eq!(all["levels"].as_array().unwrap().len(), 1);
    assert_eq!(all["levels"][0]["world_iid"], "populated");
    client.close();
}

#[test]
fn inspects_worlds_edits_cells_and_entities_and_reopens_the_saved_project() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("中文 示例.ldtk");
    let mut root: Value =
        serde_json::from_slice(&fs::read(sample_path("Typical_TopDown_example.ldtk")).unwrap())
            .unwrap();
    let level = root["levels"].as_array_mut().unwrap().remove(0);
    root["worlds"] = json!([{
        "iid":"world-for-mcp",
        "identifier":"TestWorld",
        "worldLayout":"Free",
        "levels":[level]
    }]);
    let original = serde_json::to_vec(&root).unwrap();
    fs::write(&path, &original).unwrap();

    let mut client = Client::start(&path);
    assert_eq!(client.initialize(PROTOCOL)["protocolVersion"], PROTOCOL);
    client.initialized();
    let worlds = client.tool("list_worlds", json!({}));
    assert!(
        worlds["worlds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["iid"] == root["dummyWorldIid"])
    );
    assert!(
        worlds["worlds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["iid"] == "world-for-mcp")
    );
    let levels = client.tool("list_levels", json!({"world_iid":"world-for-mcp"}));
    assert_eq!(levels["levels"].as_array().unwrap().len(), 1);
    let level_iid = levels["levels"][0]["iid"].as_str().unwrap();
    let level = client.tool("get_level", json!({"level_iid":level_iid}));
    let layers = level["layers"].as_array().unwrap();
    let layer_iid = |name| {
        layers.iter().find(|l| l["identifier"] == name).unwrap()["iid"]
            .as_str()
            .unwrap()
    };
    let collision = layer_iid("Collisions");
    let tiles = layer_iid("Custom_floor");
    let entities = layer_iid("Entities");
    let collisions_args = json!({"level_iid":level_iid,"layer_iid":collision});
    let definitions = client.tool("get_definitions", json!({}));
    let item_uid = definitions["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["identifier"] == "Item")
        .unwrap()["uid"]
        .as_i64()
        .unwrap();
    assert_eq!(
        client.tool("get_layer", collisions_args.clone())["layer"]["intGridCsv"][0],
        0
    );
    assert_eq!(
        client.call(
            "paint_int_grid",
            json!({
                "level_iid":level_iid,"layer_iid":collision,"x":0,"y":0,"value":999
            })
        )["isError"],
        true
    );
    assert_eq!(client.tool("project_info", json!({}))["dirty"], false);
    assert_eq!(
        client.tool(
            "paint_int_grid",
            json!({
                "level_iid":level_iid,"layer_iid":collision,"x":0,"y":0,"value":1
            })
        )["changed"],
        true
    );
    assert_eq!(client.tool("paint_tile", json!({
        "level_iid":level_iid,"layer_iid":tiles,"x":0,"y":0,"tile_id":0,"src_x":0,"src_y":0,"flips":1
    }))["changed"], true);
    let item = client.tool(
        "add_entity",
        json!({
            "level_iid":level_iid,"layer_iid":entities,"def_uid":item_uid,"x":80,"y":80
        }),
    );
    let item_iid = item["entity_iid"].as_str().unwrap();
    assert_eq!(
        client.tool(
            "move_entity",
            json!({
                "level_iid":level_iid,"layer_iid":entities,"entity_iid":item_iid,"x":96,"y":80
            })
        )["changed"],
        true
    );
    assert_eq!(
        client.tool("get_layer", collisions_args.clone())["layer"]["intGridCsv"][0],
        1
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(client.tool("project_info", json!({}))["dirty"], true);
    assert_eq!(client.call("reload_project", json!({}))["isError"], true);
    assert_eq!(client.tool("save_project", json!({}))["saved"], true);
    assert_eq!(client.tool("save_project", json!({}))["saved"], false);
    client.close();

    let project = Project::load(&path).unwrap();
    let saved = project
        .level_keys()
        .into_iter()
        .find_map(|key| project.level(key).filter(|level| level["iid"] == level_iid))
        .unwrap();
    let saved_layers = saved["layerInstances"].as_array().unwrap();
    let get_layer = |iid| {
        saved_layers
            .iter()
            .find(|layer| layer["iid"] == iid)
            .unwrap()
    };
    assert_eq!(get_layer(collision)["intGridCsv"][0], 1);
    assert!(
        get_layer(tiles)["gridTiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tile| tile["px"] == json!([0, 0]) && tile["t"] == 0 && tile["f"] == 1)
    );
    let saved_entity = get_layer(entities)["entityInstances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["iid"] == item_iid)
        .unwrap();
    assert_eq!(saved_entity["defUid"], item_uid);
    assert_eq!(saved_entity["px"], json!([96, 80]));

    let mut client = Client::start(&path);
    client.initialize(PROTOCOL);
    client.initialized();
    assert_eq!(
        client.tool(
            "remove_entity",
            json!({
                "level_iid":level_iid,"layer_iid":entities,"entity_iid":item_iid
            })
        )["changed"],
        true
    );
    assert_eq!(
        client.tool(
            "erase_tile",
            json!({
                "level_iid":level_iid,"layer_iid":tiles,"x":0,"y":0
            })
        )["changed"],
        true
    );
    client.close();
    assert_eq!(Project::load(&path).unwrap().json, project.json);
}

#[test]
fn save_detects_changes_to_root_external_levels_and_watched_assets() {
    let temp = tempfile::tempdir().unwrap();
    let path = copy_external_project(temp.path());
    let external_path = path.with_extension("").join("World_Level_0.ldtkl");
    let asset = temp.path().join("watched.png");
    fs::write(&asset, vec![0; 128 * 1024 + 1]).unwrap();
    let mut root: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    root["levels"][0]["bgRelPath"] = json!("watched.png");
    fs::write(&path, serde_json::to_vec(&root).unwrap()).unwrap();
    let external: Value = serde_json::from_slice(&fs::read(&external_path).unwrap()).unwrap();
    let level_iid = root["levels"][0]["iid"].as_str().unwrap();
    let layer_iid = external["layerInstances"][0]["iid"].as_str().unwrap();
    let paint = json!({"level_iid":level_iid,"layer_iid":layer_iid,"x":0,"y":0,"value":1});
    let inspect = json!({"level_iid":level_iid,"layer_iid":layer_iid});

    let mut client = Client::start(&path);
    client.initialize(PROTOCOL);
    client.initialized();
    for changed in [&path, &external_path, &asset] {
        assert_eq!(
            client.tool("paint_int_grid", paint.clone())["changed"],
            true
        );
        let mut bytes = fs::read(changed).unwrap();
        if changed == &asset {
            *bytes.last_mut().unwrap() = 1;
        } else {
            bytes.push(b'\n');
        }
        fs::write(changed, &bytes).unwrap();
        let root_before = fs::read(&path).unwrap();
        let level_before = fs::read(&external_path).unwrap();
        let conflict = client.call("save_project", json!({}));
        assert_eq!(conflict["isError"], true, "{conflict}");
        assert!(
            conflict["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains(changed.file_name().unwrap().to_str().unwrap())
        );
        assert_eq!(fs::read(&path).unwrap(), root_before);
        assert_eq!(fs::read(&external_path).unwrap(), level_before);
        assert_eq!(
            client.tool("get_layer", inspect.clone())["layer"]["intGridCsv"][0],
            1
        );
        assert_eq!(client.call("reload_project", json!({}))["isError"], true);
        assert_eq!(
            client.tool("reload_project", json!({"discard_changes":true}))["dirty"],
            false
        );
        assert_eq!(
            client.tool("get_layer", inspect.clone())["layer"]["intGridCsv"][0],
            0
        );
    }
    assert_eq!(client.tool("paint_int_grid", paint)["changed"], true);
    assert_eq!(client.tool("save_project", json!({}))["saved"], true);
    client.close();
    let saved: Value = serde_json::from_slice(&fs::read(external_path).unwrap()).unwrap();
    assert_eq!(saved["layerInstances"][0]["intGridCsv"][0], 1);
}

#[test]
fn refuses_to_replace_an_unrelated_file_at_a_normalized_external_level_path() {
    for indexed in [false, true] {
        check_external_path_collision(indexed);
    }
}

fn check_external_path_collision(indexed: bool) {
    let temp = tempfile::tempdir().unwrap();
    let path = copy_external_project(temp.path());
    let normalized = path.with_extension("").join("World_Level_0.ldtkl");
    let source = normalized.with_file_name("legacy name.ldtkl");
    fs::rename(&normalized, &source).unwrap();
    let normalized = if indexed {
        normalized.with_file_name("0000-World_Level_0.ldtkl")
    } else {
        normalized
    };
    let unrelated = b"Unrelated file at the default external level path";
    fs::write(&normalized, unrelated).unwrap();
    let mut root: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    if indexed {
        root["flags"].as_array_mut().unwrap().push(json!("PrependIndexToLevelFileNames"));
    }
    root["levels"][0]["externalRelPath"] = json!("SeparateLevelFiles/legacy name.ldtkl");
    let root_bytes = serde_json::to_vec(&root).unwrap();
    fs::write(&path, &root_bytes).unwrap();
    let source_bytes = fs::read(&source).unwrap();
    let external: Value = serde_json::from_slice(&source_bytes).unwrap();
    let mut client = Client::start(&path);
    client.initialize(PROTOCOL);
    client.initialized();
    assert_eq!(
        client.tool("paint_int_grid", json!({
            "level_iid":root["levels"][0]["iid"], "layer_iid":external["layerInstances"][0]["iid"],
            "x":0, "y":0, "value":1
        }))["changed"],
        true
    );
    assert_eq!(client.call("save_project", json!({}))["isError"], true);
    assert_eq!(fs::read(&normalized).unwrap(), unrelated);
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_eq!(fs::read(&path).unwrap(), root_bytes);
    assert_eq!(client.tool("project_info", json!({}))["dirty"], true);
    client.close();
}
