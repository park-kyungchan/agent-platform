use crate::catalog;
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

// Bounds framing as well as catalog strings; discard oversized frames entirely.
const MAX_FRAME: usize = 64 * 1024;

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

pub fn dispatch(request: Value) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("jsonrpc") != Some(&json!("2.0"))
        || !request.get("method").is_some_and(Value::is_string)
        || !(id.is_null() || id.is_string() || id.is_number())
    {
        return Some(error(Value::Null, -32600, "invalid_request"));
    }
    // Notifications never dispatch a tool or produce a response.
    request.get("id")?;
    let params = request.get("params").cloned().unwrap_or(json!({}));
    if !params.is_object() {
        return Some(error(id, -32602, "invalid_params"));
    }
    let result = match request["method"].as_str()? {
        "initialize" => catalog::initialize(),
        "ping" => json!({}),
        "tools/list" => catalog::tools(),
        "tools/call" => {
            if params
                .as_object()?
                .keys()
                .any(|k| !matches!(k.as_str(), "name" | "arguments" | "_meta"))
            {
                return Some(error(id, -32602, "invalid_params"));
            }
            if params.get("_meta").is_some_and(|m| !m.is_object()) {
                return Some(error(id, -32602, "invalid_params"));
            }
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "invalid_params"));
            };
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            if let Err(message) = catalog::parse_call(name, args) {
                return Some(error(id, -32602, message));
            }
            // P03 has no configured backend. Never fabricate a successful read.
            json!({"isError":true,"content":[{"type":"text","text":"current_memory_unavailable"}]})
        }
        _ => return Some(error(id, -32601, "method_not_found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

pub fn serve(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut frame = Vec::new();
    let mut oversized = false;
    loop {
        let bytes = input.fill_buf()?;
        if bytes.is_empty() {
            // A final frame without LF is still handled once before EOF.
            if frame.is_empty() && !oversized {
                return Ok(());
            }
        } else {
            let count = bytes
                .iter()
                .position(|b| *b == b'\n')
                .map_or(bytes.len(), |i| i + 1);
            let end = bytes[count - 1] == b'\n';
            if !oversized && frame.len() + count <= MAX_FRAME {
                frame.extend_from_slice(&bytes[..count]);
            } else {
                oversized = true;
                frame.clear();
            }
            input.consume(count);
            if !end {
                continue;
            }
        }
        let reply = if oversized {
            Some(error(Value::Null, -32600, "request_too_large"))
        } else {
            match serde_json::from_slice(&frame) {
                Ok(request) => dispatch(request),
                Err(_) => Some(error(Value::Null, -32700, "parse_error")),
            }
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
        frame.clear();
        oversized = false;
    }
}
