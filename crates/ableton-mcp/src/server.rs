//! MCP Server JSON-RPC 2.0 stdio dispatch loop and message processor.

use std::io::{self, BufRead, Write};
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
        Self {
            state: DawState::new(),
            is_initialized: false,
        }
    }

    /// Process a parsed JSON-RPC request and produce an optional response.
    /// Notifications (requests without `id`) return `None`.
    pub fn handle_request(&mut self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        let req_id = match req.id {
            Some(ref id) if !id.is_null() => id.clone(),
            _ => {
                // Notification (e.g. "notifications/initialized")
                if req.method == "notifications/initialized" {
                    self.is_initialized = true;
                }
                return None;
            }
        };

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
                let params = req.params.unwrap_or(json!({}));
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
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let result = call_tool(&mut self.state, name, &args);
                JsonRpcResponse::success(req_id, serde_json::to_value(&result).unwrap_or(json!({})))
            }
            "resources/list" => {
                let resources = get_resource_definitions();
                JsonRpcResponse::success(req_id, json!({ "resources": resources }))
            }
            "resources/read" => {
                let params = req.params.unwrap_or(json!({}));
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
                    Err(e) => JsonRpcResponse::error(req_id, INTERNAL_ERROR, e, None),
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

    /// Process a single incoming raw JSON string line.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
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

        self.handle_request(req)
            .and_then(|resp| serde_json::to_string(&resp).ok())
    }

    /// Run line-delimited stdio JSON-RPC server loop.
    pub fn run_stdio(&mut self) {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        for line_res in stdin.lock().lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => break,
            };

            if let Some(resp_str) = self.handle_line(&line) {
                if writeln!(stdout, "{}", resp_str).is_err() {
                    break;
                }
                let _ = stdout.flush();
            }
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}
