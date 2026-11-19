//! MCP Server JSON-RPC 2.0 stdio dispatch loop and message processor.

use std::io;
use serde_json::{json, Value};

use crate::protocol::*;
use crate::resources::{get_resource_definitions, read_resource};
use crate::state::DawState;
use crate::tools::{call_tool, get_tool_definitions};

pub struct McpServer {
    pub state: DawState,
    pub is_initialized: bool,
}

impl McpServer {
    pub fn new() -> Self {
        let mut state = DawState::new();
        if let Ok(port_str) = std::env::var("OXIDE_LOM_PORT") {
            if let Ok(port) = port_str.trim().parse::<u16>() {
                state.lom_client.port = port;
            }
        }
        Self {
            state,
            is_initialized: false,
        }
    }

    /// Construct a server bound to a specific Live GUI RPC TCP port (useful for isolated tests).
    pub fn with_lom_port(port: u16) -> Self {
        let mut server = Self::new();
        server.state.lom_client.port = port;
        server
    }

    /// Process a parsed JSON-RPC request and produce an optional response.
    /// Notifications (omitted `id`) return `None`; explicit `"id": null` is treated as a request.
    pub fn handle_request(&mut self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        if req.jsonrpc != "2.0" {
            let err_id = match req.id {
                Some(ref id) if id.is_string() || id.is_number() || id.is_null() => id.clone(),
                _ => Value::Null,
            };
            return Some(JsonRpcResponse::error(
                err_id,
                INVALID_REQUEST,
                "Invalid Request: 'jsonrpc' must be \"2.0\"",
                None,
            ));
        }

        let req_id = match req.id {
            None => {
                // Notification (no "id" key present)
                if req.method == "notifications/initialized" {
                    self.is_initialized = true;
                }
                return None;
            }
            Some(ref id) if id.is_string() || id.is_number() || id.is_null() => id.clone(),
            Some(_) => {
                return Some(JsonRpcResponse::error(
                    Value::Null,
                    INVALID_REQUEST,
                    "Invalid Request: 'id' must be a string, number, or null",
                    None,
                ));
            }
        };

        if let Some(ref p) = req.params {
            if !p.is_object() {
                return Some(JsonRpcResponse::error(
                    req_id,
                    INVALID_PARAMS,
                    "Invalid params: 'params' must be a JSON object",
                    None,
                ));
            }
        }

        let response = match req.method.as_str() {
            "initialize" => {
                self.is_initialized = true;
                let res = InitializeResult {
                    protocol_version: MCP_PROTOCOL_VERSION.to_string(),
                    capabilities: ServerCapabilities {
                        tools: Some(json!({})),
                        resources: Some(json!({})),
                    },
                    server_info: ServerInfo {
                        name: SERVER_NAME.to_string(),
                        version: SERVER_VERSION.to_string(),
                    },
                };
                JsonRpcResponse::success(req_id, serde_json::to_value(&res).unwrap_or(json!({})))
            }
            "ping" => JsonRpcResponse::success(req_id, json!({})),
            "tools/list" => {
                let tools = get_tool_definitions();
                JsonRpcResponse::success(req_id, json!({ "tools": tools }))
            }
            "tools/call" => {
                let params = match req.params {
                    Some(Value::Object(map)) => map,
                    _ => {
                        return Some(JsonRpcResponse::error(
                            req_id,
                            INVALID_PARAMS,
                            "Invalid params: tools/call requires a JSON object",
                            None,
                        ))
                    }
                };
                let name = match params.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n,
                    None => {
                        return Some(JsonRpcResponse::error(
                            req_id,
                            INVALID_PARAMS,
                            "Missing 'name' in tools/call parameters",
                            None,
                        ))
                    }
                };
                let args = match params.get("arguments") {
                    None => Value::Object(serde_json::Map::new()),
                    Some(v) if v.is_object() => v.clone(),
                    Some(_) => {
                        return Some(JsonRpcResponse::error(
                            req_id,
                            INVALID_PARAMS,
                            "Invalid params: 'arguments' must be a JSON object",
                            None,
                        ))
                    }
                };
                let result = call_tool(&mut self.state, name, &args);
                JsonRpcResponse::success(req_id, serde_json::to_value(&result).unwrap_or(json!({})))
            }
            "resources/list" => {
                let resources = get_resource_definitions();
                JsonRpcResponse::success(req_id, json!({ "resources": resources }))
            }
            "resources/read" => {
                let params = match req.params {
                    Some(Value::Object(map)) => map,
                    _ => {
                        return Some(JsonRpcResponse::error(
                            req_id,
                            INVALID_PARAMS,
                            "Invalid params: resources/read requires a JSON object",
                            None,
                        ))
                    }
                };
                let uri = match params.get("uri").and_then(|v| v.as_str()) {
                    Some(u) => u,
                    None => {
                        return Some(JsonRpcResponse::error(
                            req_id,
                            INVALID_PARAMS,
                            "Missing 'uri' in resources/read parameters",
                            None,
                        ))
                    }
                };
                match read_resource(&mut self.state, uri) {
                    Ok(res) => {
                        JsonRpcResponse::success(req_id, serde_json::to_value(&res).unwrap_or(json!({})))
                    }
                    Err(e) => JsonRpcResponse::error(req_id, RESOURCE_NOT_FOUND, e, None),
                }
            }
            unknown => JsonRpcResponse::error(
                req_id,
                METHOD_NOT_FOUND,
                format!("Method '{}' not found", unknown),
                None,
            ),
        };

        Some(response)
    }

    fn handle_value(&mut self, raw: Value) -> Option<JsonRpcResponse> {
        let obj = match raw.as_object() {
            Some(o) => o,
            None => {
                return Some(JsonRpcResponse::error(
                    Value::Null,
                    INVALID_REQUEST,
                    "Invalid Request: expected a JSON-RPC 2.0 request object",
                    None,
                ));
            }
        };

        let raw_id = obj
            .get("id")
            .filter(|id| id.is_string() || id.is_number() || id.is_null())
            .cloned()
            .unwrap_or(Value::Null);

        if obj.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
            return Some(JsonRpcResponse::error(
                raw_id,
                INVALID_REQUEST,
                "Invalid Request: 'jsonrpc' must be \"2.0\"",
                None,
            ));
        }

        let req: JsonRpcRequest = match serde_json::from_value(raw) {
            Ok(r) => r,
            Err(e) => {
                return Some(JsonRpcResponse::error(
                    raw_id,
                    INVALID_REQUEST,
                    format!("Invalid Request: {}", e),
                    None,
                ));
            }
        };

        self.handle_request(req)
    }

    /// Process a single incoming raw JSON string (single object or JSON-RPC 2.0 batch array).
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let raw: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = JsonRpcResponse::error(
                    Value::Null,
                    PARSE_ERROR,
                    format!("JSON parse error: {}", e),
                    None,
                );
                return Some(serde_json::to_string(&err_resp).unwrap_or_default());
            }
        };

        if let Value::Array(items) = raw {
            if items.is_empty() {
                let err_resp = JsonRpcResponse::error(
                    Value::Null,
                    INVALID_REQUEST,
                    "Invalid Request: empty JSON-RPC batch",
                    None,
                );
                return Some(serde_json::to_string(&err_resp).unwrap_or_default());
            }
            let responses: Vec<JsonRpcResponse> = items
                .into_iter()
                .filter_map(|item| self.handle_value(item))
                .collect();
            if responses.is_empty() {
                return None;
            }
            return serde_json::to_string(&responses).ok();
        }

        self.handle_value(raw)
            .and_then(|resp| serde_json::to_string(&resp).ok())
    }

    /// Run stdio JSON-RPC server loop supporting both `\n`-delimited JSON and `Content-Length:` MCP framing.
    pub fn run_stdio(&mut self) {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut reader = io::BufReader::new(stdin.lock());
        let mut writer = stdout.lock();
        self.process_stdio(&mut reader, &mut writer);
    }

    pub fn process_stdio<R: io::BufRead, W: io::Write>(&mut self, reader: &mut R, writer: &mut W) {
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let is_header_line =
                !trimmed.starts_with('{') && !trimmed.starts_with('[') && trimmed.contains(':');
            if is_header_line {
                let mut content_length: Option<usize> = None;
                let mut malformed_content_length = false;

                let mut inspect_header = |hdr_line: &str| {
                    if let Some((name, val)) = hdr_line.split_once(':') {
                        if name.trim().eq_ignore_ascii_case("content-length") {
                            match val.trim().parse::<usize>() {
                                Ok(n) => content_length = Some(n),
                                Err(_) => malformed_content_length = true,
                            }
                        }
                    }
                };
                inspect_header(trimmed);

                loop {
                    let mut hdr = String::new();
                    match reader.read_line(&mut hdr) {
                        Ok(0) | Err(_) => return,
                        Ok(_) => {}
                    }
                    let hdr_trimmed = hdr.trim();
                    if hdr_trimmed.is_empty() {
                        break;
                    }
                    inspect_header(hdr_trimmed);
                }

                if malformed_content_length {
                    let err_resp = JsonRpcResponse::error(
                        Value::Null,
                        INVALID_REQUEST,
                        "Malformed Content-Length header",
                        None,
                    );
                    if let Ok(body) = serde_json::to_string(&err_resp) {
                        let _ = write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body);
                        let _ = writer.flush();
                    }
                    continue;
                }

                let len = match content_length {
                    Some(n) => n,
                    None => {
                        let err_resp = JsonRpcResponse::error(
                            Value::Null,
                            INVALID_REQUEST,
                            "Missing Content-Length header",
                            None,
                        );
                        if let Ok(body) = serde_json::to_string(&err_resp) {
                            let _ = write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body);
                            let _ = writer.flush();
                        }
                        continue;
                    }
                };
                if len > 16 * 1024 * 1024 {
                    let err_resp = JsonRpcResponse::error(
                        Value::Null,
                        INVALID_REQUEST,
                        "Content-Length exceeds 16 MiB limit",
                        None,
                    );
                    if let Ok(body) = serde_json::to_string(&err_resp) {
                        let _ = write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body);
                        let _ = writer.flush();
                    }
                    break;
                }

                let mut buf = vec![0u8; len];
                if reader.read_exact(&mut buf).is_err() {
                    break;
                }
                let payload = String::from_utf8_lossy(&buf);
                if let Some(resp_str) = self.handle_line(&payload) {
                    if write!(writer, "Content-Length: {}\r\n\r\n{}", resp_str.len(), resp_str).is_err() {
                        break;
                    }
                    let _ = writer.flush();
                }
            } else if let Some(resp_str) = self.handle_line(trimmed) {
                if writeln!(writer, "{}", resp_str).is_err() {
                    break;
                }
                let _ = writer.flush();
            }
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_content_type_before_content_length_and_malformed_length_and_invalid_params() {
        let mut server = McpServer::new();
        let req_json = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let frame = format!(
            "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
            req_json.len(),
            req_json
        );
        let mut reader = io::Cursor::new(frame.into_bytes());
        let mut out = Vec::new();
        server.process_stdio(&mut reader, &mut out);
        let out_str = String::from_utf8(out).unwrap();
        assert!(out_str.contains("\"id\":1") && out_str.contains("\"result\":{}"));

        // Malformed Content-Length emits INVALID_REQUEST error cleanly
        let bad_frame = "Content-Type: application/json\r\nContent-Length: notanint\r\n\r\n";
        let mut bad_reader = io::Cursor::new(bad_frame.as_bytes());
        let mut bad_out = Vec::new();
        server.process_stdio(&mut bad_reader, &mut bad_out);
        let bad_str = String::from_utf8(bad_out).unwrap();
        assert!(bad_str.contains("-32600"), "Expected INVALID_REQUEST, got {}", bad_str);

        // Non-object params / arguments return -32602
        let r1 = server.handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"ping","params":[1,2]}"#).unwrap();
        assert!(r1.contains("-32602"));
        let r2 = server.handle_line(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_get_overview","arguments":[1]}}"#).unwrap();
        assert!(r2.contains("-32602"));
    }
}
