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
        Self::start_with_binary(path, Path::new(env!("CARGO_BIN_EXE_level-editor")))
    }

    fn start_with_binary(path: &Path, binary: &Path) -> Self {
        let mut child = Command::new(binary)
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
fn mcp_level_lookup_tracks_reindexed_levels_after_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reload.ldtk");
    let mut project = Project::new();
    let old_iid = project.json["levels"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    fs::write(&path, serde_json::to_vec(&project.json).unwrap()).unwrap();
    let mut client = Client::start(&path);
    client.initialize(PROTOCOL);
    client.initialized();
    assert_eq!(
        client.tool("get_level", json!({"level_iid":old_iid}))["iid"],
        old_iid
    );

    let added = project.add_level(None, "Added", 64, 96).unwrap();
    let added_iid = project.level(added).unwrap()["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    project.json["levels"].as_array_mut().unwrap().reverse();
    fs::write(&path, serde_json::to_vec(&project.json).unwrap()).unwrap();
    assert!(client.tool("reload_project", json!({})).is_object());
    assert_eq!(
        client.tool("get_level", json!({"level_iid":old_iid}))["iid"],
        old_iid
    );
    assert_eq!(
        client.tool("get_level", json!({"level_iid":added_iid}))["px_height"],
        96
    );
    client.close();
}

#[test]
fn layer_response_preserves_escaped_json_for_every_protocol() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("escaping.ldtk");
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.add_layer("Terrain", "IntGrid", 16).unwrap();
    let level_iid = project.level(key).unwrap()["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let layer = project.layer_mut(key, 0).unwrap();
    layer["extension"] = json!({
        "text":"关卡 \"quote\"\nline\\path\t\0",
        "list":[-129,0,255,256,4294967296u64,u64::MAX,1.25,null]
    });
    let args = json!({"level_iid":level_iid,"layer_iid":layer["iid"]});
    let expected = json!({"layer":layer});
    fs::write(&path, serde_json::to_vec(&project.json).unwrap()).unwrap();

    for version in ["2025-11-25", "2025-06-18", "2024-11-05"] {
        let mut client = Client::start(&path);
        assert_eq!(client.initialize(version)["protocolVersion"], version);
        client.initialized();
        let id = "图层 \"quote\"\nline\\path";
        client.send(json!({
            "jsonrpc":"2.0","id":id,"method":"tools/call",
            "params":{"name":"get_layer","arguments":args}
        }));
        let output = client.receive();
        assert_eq!(output["id"], id);
        assert_eq!(output["jsonrpc"], "2.0");
        let result = &output["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], expected.to_string());
        if version == "2024-11-05" {
            assert!(result.get("structuredContent").is_none());
        } else {
            assert_eq!(result["structuredContent"], expected);
        }
        client.close();
    }
}

#[test]
#[ignore = "manual large MCP response performance comparison"]
fn benchmark_large_mcp_layer_response() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large-layer.ldtk");
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 512 * 16, 256 * 16, 0, 0).unwrap();
    project.add_layer("Terrain", "IntGrid", 16).unwrap();
    let level_iid = project.level(key).unwrap()["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let layer = project.layer_mut(key, 0).unwrap();
    layer["opaque_extra"] = json!("关卡 \"quoted\"\ntext\\path");
    for (i, value) in layer["intGridCsv"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        *value = (i % 256).into();
    }
    let args = json!({"level_iid":level_iid,"layer_iid":layer["iid"]});
    let layer = project.layer(key, 0).unwrap();
    fs::write(&path, serde_json::to_vec(&project.json).unwrap()).unwrap();
    let binary = std::env::var_os("LEVEL_EDITOR_MCP_BENCH_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_level-editor")));
    let mut client = Client::start_with_binary(&path, &binary);
    client.initialize(PROTOCOL);
    client.initialized();
    assert_eq!(&client.tool("get_layer", args.clone())["layer"], layer);
    let mut times = [Duration::ZERO; 5];
    for time in &mut times {
        let message = json!({
            "jsonrpc":"2.0","id":"large-layer","method":"tools/call",
            "params":{"name":"get_layer","arguments":args}
        });
        let start = Instant::now();
        let mut output = None;
        for _ in 0..5 {
            client.send(message.clone());
            output = Some(client.output.recv_timeout(TIMEOUT).unwrap());
        }
        *time = start.elapsed();
        let output: Value = serde_json::from_str(&output.unwrap()).unwrap();
        assert_eq!(output["id"], "large-layer");
        let result = &output["result"];
        assert_eq!(&result["structuredContent"]["layer"], layer);
        let text: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, result["structuredContent"]);
    }
    client.close();
    times.sort_unstable();
    eprintln!(
        "Large MCP layer response: median {:?}; samples {times:?}",
        times[2]
    );
}

#[test]
#[ignore = "manual large project MCP save performance comparison"]
fn benchmark_large_mcp_save() {
    benchmark_mcp_save(false, 0);
    benchmark_mcp_save(true, 0);
    benchmark_mcp_save(true, 7);
}

fn benchmark_mcp_save(external: bool, other_levels: usize) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large-save.ldtk");
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project
        .resize_level(key, 1024 * 16, 256 * 16, 0, 0)
        .unwrap();
    for i in 0..other_levels {
        project
            .add_level(None, &format!("Unedited_{i}"), 1024 * 16, 256 * 16)
            .unwrap();
    }
    project.add_layer("Terrain", "IntGrid", 16).unwrap();
    project.json["jsonStyle"] = json!("Minified");
    let level_iid = project.level(key).unwrap()["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let layer_iid = project.layer(key, 0).unwrap()["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    if external {
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
    } else {
        fs::write(&path, serde_json::to_vec(&project.json).unwrap()).unwrap();
    }

    let binary = std::env::var_os("LEVEL_EDITOR_MCP_BENCH_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_level-editor")));
    let mut client = Client::start_with_binary(&path, &binary);
    client.initialize(PROTOCOL);
    client.initialized();
    let mut times = [Duration::ZERO; 5];
    for (run, time) in times.iter_mut().enumerate() {
        let value = if run % 2 == 0 { 1 } else { 0 };
        assert_eq!(
            client.tool(
                "paint_int_grid",
                json!({
                    "level_iid":level_iid, "layer_iid":layer_iid, "x":0, "y":0, "value":value
                })
            )["changed"],
            true
        );
        let start = Instant::now();
        let result = client.tool("save_project", json!({}));
        *time = start.elapsed();
        assert_eq!(result, json!({"saved":true,"dirty":false}));
        let saved = Project::load(&path).unwrap();
        let layer = saved.layer(key, 0).unwrap();
        assert_eq!(layer["intGridCsv"][0], value);
        assert_eq!(layer["intGridCsv"].as_array().unwrap().len(), 1024 * 256);
        assert!(layer.get("intGrid").is_none());
    }
    let inspected = client.tool(
        "get_layer",
        json!({"level_iid":level_iid,"layer_iid":layer_iid}),
    );
    assert_eq!(
        &inspected["layer"],
        Project::load(&path).unwrap().layer(key, 0).unwrap()
    );
    client.close();
    times.sort_unstable();
    let storage = match (external, other_levels) {
        (false, _) => "inline",
        (true, 0) => "external",
        (true, _) => "multi-external",
    };
    eprintln!(
        "Large MCP {storage} save: median {:?}; samples {times:?}",
        times[2]
    );
}

#[test]
#[ignore = "manual MCP level lookup performance comparison"]
fn benchmark_large_mcp_level_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.ldtk");
    let mut project = Project::new();
    let template = project.json["levels"][0].clone();
    let count = 8_192;
    project.json["levels"] = (0..count)
        .map(|i| {
            let mut level = template.clone();
            level["uid"] = json!(i + 1);
            level["iid"] = json!(format!("level-{i}"));
            level["identifier"] = json!(format!("Level_{i}"));
            level["worldX"] = json!(i * 256);
            level
        })
        .collect::<Vec<_>>()
        .into();
    fs::write(&path, serde_json::to_vec(&project.json).unwrap()).unwrap();
    let mut client = Client::start(&path);
    client.initialize(PROTOCOL);
    client.initialized();
    assert_eq!(
        client.tool("get_level", json!({"level_iid":"level-0"}))["identifier"],
        "Level_0"
    );
    let mut times = [Duration::ZERO; 5];
    for time in &mut times {
        let start = Instant::now();
        for i in 0..128 {
            let index = i * 997 % count;
            let iid = format!("level-{index}");
            let level = client.tool("get_level", json!({"level_iid":iid}));
            assert_eq!(level["iid"], iid);
            assert_eq!(level["world_x"], index * 256);
        }
        *time = start.elapsed();
    }
    client.close();
    times.sort_unstable();
    eprintln!(
        "Large MCP level lookup: median {:?}; samples {times:?}",
        times[2]
    );
}

#[test]
#[ignore = "manual MCP level listing performance comparison"]
fn benchmark_large_mcp_level_listing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large-list.ldtk");
    let mut project = Project::new();
    let template = project.json["levels"][0].clone();
    let count = 8192;
    project.json["levels"] = (0..count)
        .map(|i| {
            let mut level = template.clone();
            level["uid"] = json!(i + 1);
            level["iid"] = json!(format!("level-{i}"));
            level["identifier"] = json!(format!("Level_{i}"));
            level["worldX"] = json!(i * 256);
            level
        })
        .collect::<Vec<_>>()
        .into();
    fs::write(&path, serde_json::to_vec(&project.json).unwrap()).unwrap();
    let binary = std::env::var_os("LEVEL_EDITOR_MCP_BENCH_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_level-editor")));
    let mut client = Client::start_with_binary(&path, &binary);
    client.initialize(PROTOCOL);
    client.initialized();
    let request = json!({
        "jsonrpc":"2.0", "id":"list", "method":"tools/call",
        "params":{"name":"list_levels","arguments":{}}
    });
    client.send(request.clone());
    client.output.recv_timeout(TIMEOUT).unwrap();
    let mut times = [Duration::ZERO; 5];
    for time in &mut times {
        let start = Instant::now();
        client.send(request.clone());
        let output = client.output.recv_timeout(TIMEOUT).unwrap();
        *time = start.elapsed();
        let output: Value = serde_json::from_str(&output).unwrap();
        let result = &output["result"];
        let text: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, result["structuredContent"]);
        let levels = text["levels"].as_array().unwrap();
        assert_eq!(levels.len(), count);
        for (index, level) in levels.iter().enumerate() {
            assert_eq!(level["iid"], format!("level-{index}"));
            assert_eq!(level["world_x"], index * 256);
        }
    }
    client.close();
    times.sort_unstable();
    eprintln!(
        "Large MCP level listing: median {:?}; samples {times:?}",
        times[2]
    );
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
    assert_eq!(initialize["serverInfo"]["name"], "level-editor");
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
fn paints_int_grid_cells_atomically_and_matches_sequential_auto_layers() {
    let temp = tempfile::tempdir().unwrap();
    let source = sample_path("AutoLayers_3_Mosaic.ldtk");
    let path = temp.path().join("mosaic.ldtk");
    fs::copy(&source, &path).unwrap();

    let mut client = Client::start(&path);
    client.initialize(PROTOCOL);
    client.initialized();
    let tools = client.request("tools/list", json!({}));
    let tool = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "paint_int_grid_cells")
        .expect("discoverable IntGrid batch tool");
    let cells_schema = &tool["inputSchema"]["properties"]["cells"];
    assert_eq!(cells_schema["type"], "array");
    assert_eq!(
        cells_schema["items"]["required"],
        json!(["x", "y", "value"])
    );
    assert_eq!(cells_schema["items"]["additionalProperties"], false);

    let levels = client.tool("list_levels", json!({}));
    let level_iid = &levels["levels"][0]["iid"];
    let level = client.tool("get_level", json!({"level_iid":level_iid}));
    let layer_iid = |name: &str| {
        &level["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["identifier"] == name)
            .unwrap()["iid"]
    };
    let walls = json!({"level_iid":level_iid,"layer_iid":layer_iid("Walls")});
    let background = json!({"level_iid":level_iid,"layer_iid":layer_iid("Background")});
    let batch = |cells| {
        let mut args = walls.clone();
        args["cells"] = cells;
        args
    };
    let before = client.tool("get_layer", walls.clone());
    let background_before = client.tool("get_layer", background.clone());
    let first = json!({"x":4,"y":4,"value":0});
    let second = json!({"x":5,"y":4,"value":0});
    for invalid in [
        json!({"x":5,"y":4,"value":0,"typo":true}),
        json!({"x":5,"y":4,"value":0.25}),
        json!({"x":23,"y":4,"value":0}),
        json!({"x":5,"y":4,"value":999}),
    ] {
        assert_eq!(
            client.call("paint_int_grid_cells", batch(json!([first, invalid])))["isError"],
            true
        );
    }
    assert_eq!(client.tool("project_info", json!({}))["dirty"], false);
    assert_eq!(client.tool("get_layer", walls.clone()), before);
    assert_eq!(
        client.tool("get_layer", background.clone()),
        background_before
    );

    let edits = batch(json!([first, {"x":4,"y":4,"value":1}, second, first]));
    assert_eq!(
        client.tool("paint_int_grid_cells", edits),
        json!({"changed":true,"changed_cells":2,"dirty":true})
    );
    let edited = client.tool("get_layer", walls.clone());
    let edited_background = client.tool("get_layer", background.clone());
    assert_ne!(edited["layer"]["intGridCsv"], before["layer"]["intGridCsv"]);
    assert_ne!(
        edited["layer"]["autoLayerTiles"],
        before["layer"]["autoLayerTiles"]
    );
    assert_ne!(
        edited_background["layer"]["autoLayerTiles"],
        background_before["layer"]["autoLayerTiles"]
    );
    assert_eq!(
        client.tool("paint_int_grid_cells", batch(json!([first, second]))),
        json!({"changed":false,"changed_cells":0,"dirty":true})
    );
    assert_eq!(fs::read(&path).unwrap(), fs::read(&source).unwrap());
    assert_eq!(client.tool("save_project", json!({}))["saved"], true);
    client.close();

    let mut reopened = Client::start(&path);
    reopened.initialize(PROTOCOL);
    reopened.initialized();
    assert_eq!(reopened.tool("get_layer", walls.clone()), edited);
    assert_eq!(
        reopened.tool("get_layer", background.clone()),
        edited_background
    );
    reopened.close();

    let mut sequential = Client::start(&source);
    sequential.initialize(PROTOCOL);
    sequential.initialized();
    for cell in [first, second] {
        let mut args = walls.clone();
        args.as_object_mut()
            .unwrap()
            .extend(cell.as_object().unwrap().clone());
        assert_eq!(sequential.tool("paint_int_grid", args)["changed"], true);
    }
    assert_eq!(sequential.tool("get_layer", walls), edited);
    assert_eq!(sequential.tool("get_layer", background), edited_background);
    sequential.close();
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
        root["flags"]
            .as_array_mut()
            .unwrap()
            .push(json!("PrependIndexToLevelFileNames"));
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
