mod project_tools;
mod tools;

use project_tools::{Session, ToolPayload};
use serde::Serialize;
use serde_json::ser::{CompactFormatter, Formatter};
use serde_json::{Map, Value, json};
use std::io::{self, BufRead, BufWriter, Write};
use std::path::PathBuf;
use std::sync::LazyLock;

const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2024-11-05"];
const RESPONSE_BUFFER_LIMIT: usize = 1024 * 1024;
static SMALL_INTEGERS: LazyLock<[String; 256]> =
    LazyLock::new(|| std::array::from_fn(|value| value.to_string()));

enum Response<'a> {
    Json(Value),
    Tool {
        id: &'a Value,
        data: ToolPayload<'a>,
        structured: bool,
    },
}

struct SmallIntegerFormatter<'a>(&'a [String; 256]);

impl Formatter for SmallIntegerFormatter<'_> {
    fn write_u64<W: Write + ?Sized>(&mut self, writer: &mut W, value: u64) -> io::Result<()> {
        if value < self.0.len() as u64 {
            writer.write_all(self.0[value as usize].as_bytes())
        } else {
            CompactFormatter.write_u64(writer, value)
        }
    }
}

impl Response<'_> {
    fn write(self, mut writer: impl Write, buffer: &mut Vec<u8>) -> io::Result<()> {
        match self {
            Self::Json(value) => Ok(serde_json::to_writer(writer, &value)?),
            Self::Tool {
                id,
                data,
                structured,
            } => {
                buffer.clear();
                data.serialize(&mut serde_json::Serializer::with_formatter(
                    &mut *buffer,
                    SmallIntegerFormatter(&SMALL_INTEGERS),
                ))?;
                writer.write_all(br#"{"jsonrpc":"2.0","id":"#)?;
                serde_json::to_writer(&mut writer, id)?;
                writer.write_all(
                    br#","result":{"isError":false,"content":[{"type":"text","text":"#,
                )?;
                write_json_as_text(&mut writer, buffer)?;
                writer.write_all(b"}]")?;
                if structured {
                    writer.write_all(br#","structuredContent":"#)?;
                    writer.write_all(buffer)?;
                }
                writer.write_all(b"}}")?;
                if buffer.capacity() > RESPONSE_BUFFER_LIMIT {
                    *buffer = Vec::new();
                }
                Ok(())
            }
        }
    }
}

fn write_json_as_text(writer: &mut impl Write, json: &[u8]) -> io::Result<()> {
    // The JSON serializer already emits UTF-8 and escapes control bytes.
    // Only quotes and backslashes need another escape in the MCP text string.
    const LOW_BITS: u64 = u64::from_le_bytes([0x7f; 8]);
    const QUOTES: u64 = u64::from_le_bytes([b'"'; 8]);
    const BACKSLASHES: u64 = u64::from_le_bytes([b'\\'; 8]);
    // The addition sets the high bit for each nonzero low part without
    // carrying between bytes. The inverse flags exactly the zero bytes.
    let zero_bytes = |word: u64| !(((word & LOW_BITS) + LOW_BITS) | word | LOW_BITS);
    writer.write_all(b"\"")?;
    let mut start = 0;
    let (words, tail) = json.as_chunks::<8>();
    for (block, bytes) in words.iter().enumerate() {
        let word = u64::from_le_bytes(*bytes);
        let mut escapes = zero_bytes(word ^ QUOTES) | zero_bytes(word ^ BACKSLASHES);
        while escapes != 0 {
            let index = block * 8 + escapes.trailing_zeros() as usize / 8;
            writer.write_all(&json[start..index])?;
            writer.write_all(&[b'\\', json[index]])?;
            start = index + 1;
            escapes &= escapes - 1;
        }
    }
    for (offset, &byte) in tail.iter().enumerate() {
        if matches!(byte, b'"' | b'\\') {
            let index = words.len() * 8 + offset;
            writer.write_all(&json[start..index])?;
            writer.write_all(&[b'\\', byte])?;
            start = index + 1;
        }
    }
    writer.write_all(&json[start..])?;
    writer.write_all(b"\"")
}

pub fn run(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.canonicalize()?;
    if !path.is_file() {
        return Err(format!("Project is not a regular file: {}", path.display()).into());
    }
    let mut session = Session::open(path)?;
    let mut version = None;
    let mut initialized = false;
    let mut stdout = BufWriter::new(io::stdout().lock());
    let mut response_buffer = Vec::new();
    for line in io::stdin().lock().split(b'\n') {
        let message = serde_json::from_slice::<Value>(&line?);
        let response = match &message {
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
        response.write(&mut stdout, &mut response_buffer)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}

fn request<'a>(
    id: &'a Value,
    method: &str,
    params: Option<&Value>,
    version: &mut Option<&'static str>,
    initialized: bool,
    session: &'a mut Session,
) -> Response<'a> {
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
                "serverInfo":{"name":env!("CARGO_PKG_NAME"),"version":env!("CARGO_PKG_VERSION")},
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
            match tool
                .validate(args)
                .and_then(|()| session.execute(name, args))
            {
                Err(message) => json!({
                    "isError":true,"content":[{"type":"text","text":message}]
                }),
                Ok(data) => {
                    return Response::Tool {
                        id,
                        data,
                        structured: matches!(*version, Some("2025-11-25" | "2025-06-18")),
                    };
                }
            }
        }
        _ => return error(id, -32601, "Method not found"),
    };
    let mut response = json!({"jsonrpc":"2.0","id":id});
    response["result"] = result;
    Response::Json(response)
}

fn error<'a>(id: &Value, code: i32, message: &str) -> Response<'a> {
    let mut response = json!({"jsonrpc":"2.0","error":{"code":code,"message":message}});
    if !id.is_null() {
        response["id"] = id.clone();
    }
    Response::Json(response)
}
