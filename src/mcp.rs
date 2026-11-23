mod project_tools;
mod tools;

use project_tools::Session;
use serde_json::{Map, Value, json};
use std::io::{self, BufRead, BufWriter, Write};
use std::path::PathBuf;

const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2024-11-05"];

pub fn run(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.canonicalize()?;
    if !path.is_file() {
        return Err(format!("Project is not a regular file: {}", path.display()).into());
    }
    let mut session = Session::open(path)?;
    let mut version = None;
    let mut initialized = false;
    let mut stdout = BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().split(b'\n') {
        let response = match serde_json::from_slice::<Value>(&line?) {
            Ok(message) => {
                let id = message.get("id");
                let valid_id = id.filter(|id| id.is_string() || id.is_i64() || id.is_u64());
                match message["method"].as_str() {
                    Some(method) if message["jsonrpc"] == "2.0" && id == valid_id => {
                        if let Some(id) = valid_id {
                            request(
                                id,
                                method,
                                message.get("params"),
                                &mut version,
                                initialized,
                                &mut session,
                            )
                        } else {
                            if version.is_some()
                                && method == "notifications/initialized"
                                && message.get("params").is_none_or(Value::is_object)
                            {
                                initialized = true;
                            }
                            continue;
                        }
                    }
                    _ => error(valid_id.unwrap_or(&Value::Null), -32600, "Invalid Request"),
                }
            }
            Err(_) => error(&Value::Null, -32700, "Parse error"),
        };
        serde_json::to_writer(&mut stdout, &response)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}

fn request(
    id: &Value,
    method: &str,
    params: Option<&Value>,
    version: &mut Option<&'static str>,
    initialized: bool,
    session: &mut Session,
) -> Value {
    let empty = Map::new();
    let params = match params {
        Some(value) => match value.as_object() {
            Some(params) => params,
            None => return error(id, -32602, "Expected an object for params"),
        },
        None => &empty,
    };
    let result = match method {
        "initialize" => {
            if version.is_some() {
                return error(id, -32600, "MCP session already negotiated");
            }
            let Some(requested) = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                return error(id, -32602, "Expected a protocolVersion");
            };
            if !params.get("capabilities").is_some_and(Value::is_object)
                || !params.get("clientInfo").is_some_and(|client| {
                    client["name"].is_string() && client["version"].is_string()
                })
            {
                return error(id, -32602, "Expected capabilities and clientInfo");
            }
            let negotiated = VERSIONS
                .iter()
                .copied()
                .find(|supported| *supported == requested)
                .unwrap_or(VERSIONS[0]);
            *version = Some(negotiated);
            json!({
                "protocolVersion":negotiated,
                "capabilities":{"tools":{}},
                "serverInfo":{"name":"LevelEditor","version":env!("CARGO_PKG_VERSION")},
                "instructions":"This session edits one local LDtk project. Discover stable IIDs before editing. Changes stay in memory until save_project."
            })
        }
        "ping" => json!({}),
        "tools/list" | "tools/call" if !initialized => {
            return error(id, -32002, "MCP session is not initialized");
        }
        "tools/list" => {
            if params.contains_key("cursor") {
                return error(id, -32602, "Invalid tool-list cursor");
            }
            json!({"tools":tools::list()})
        }
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return error(id, -32602, "Expected a tool name");
            };
            let args = match params.get("arguments") {
                Some(value) => match value.as_object() {
                    Some(args) => args,
                    None => return error(id, -32602, "Expected an object for tool arguments"),
                },
                None => &empty,
            };
            let Some(tool) = tools::Tool::named(name) else {
                return error(id, -32602, &format!("Unknown tool: {name}"));
            };
            match tool.validate(args).and_then(|()| session.execute(name, args)) {
                Err(message) => json!({
                    "isError":true,"content":[{"type":"text","text":message}]
                }),
                Ok(data) => {
                    let mut result = json!({
                        "isError":false,"content":[{"type":"text","text":data.to_string()}]
                    });
                    if matches!(*version, Some("2025-11-25" | "2025-06-18")) {
                        result["structuredContent"] = data;
                    }
                    result
                }
            }
        }
        _ => return error(id, -32601, "Method not found"),
    };
    let mut response = json!({"jsonrpc":"2.0","id":id});
    response["result"] = result;
    response
}

fn error(id: &Value, code: i32, message: &str) -> Value {
    let mut response = json!({"jsonrpc":"2.0","error":{"code":code,"message":message}});
    if !id.is_null() {
        response["id"] = id.clone();
    }
    response
}
