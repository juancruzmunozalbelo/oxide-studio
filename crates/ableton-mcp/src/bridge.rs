//! Dual-Target TCP Client Bridge connecting to Ableton Live 12 LOM Remote Script.
//!
//! Connects to `127.0.0.1:9877` over newline-delimited JSON-RPC 2.0 with a 500ms timeout.
//! When Live 12 is online, forwards requests and returns live responses.
//! When Live 12 is offline (connection refused or timed out), returns structured fallback responses.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const DEFAULT_LOM_HOST: &str = "127.0.0.1";
pub const DEFAULT_LOM_PORT: u16 = 9877;
pub const DEFAULT_LOM_TIMEOUT_MS: u64 = 500;
pub const CONNECT_TIMEOUT_MS: u64 = 150;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpLomClient {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

impl Default for TcpLomClient {
    fn default() -> Self {
        Self {
            host: DEFAULT_LOM_HOST.to_string(),
            port: DEFAULT_LOM_PORT,
            timeout: Duration::from_millis(DEFAULT_LOM_TIMEOUT_MS),
        }
    }
}

impl TcpLomClient {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            timeout: Duration::from_millis(DEFAULT_LOM_TIMEOUT_MS),
        }
    }

    /// Check whether Ableton Live 12 LOM Remote Script is actively listening.
    pub fn is_online(&self) -> bool {
        match self.send_request("ping", json!({})) {
            Ok(res) => res.get("status").and_then(|s| s.as_str()) == Some("ok"),
            Err(_) => false,
        }
    }

    /// Send a JSON-RPC 2.0 command over TCP to the AbletonMCP remote script.
    pub fn send_request(&self, method: &str, params: Value) -> Result<Value, String> {
        let addr_str = format!("{}:{}", self.host, self.port);
        let addr: SocketAddr = addr_str
            .parse()
            .map_err(|e| format!("Invalid socket address {}: {}", addr_str, e))?;

        let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(CONNECT_TIMEOUT_MS))
            .map_err(|e| format!("Failed to connect to Live 12 on {}: {}", addr_str, e))?;

        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|e| format!("Failed to set read timeout: {}", e))?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|e| format!("Failed to set write timeout: {}", e))?;

        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let mut writer = stream;
        let mut req_str = req.to_string();
        req_str.push('\n');

        writer
            .write_all(req_str.as_bytes())
            .map_err(|e| format!("Failed to write request: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("Failed to flush request: {}", e))?;

        let mut reader = BufReader::new(&writer);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(|e| format!("Failed to read response line: {}", e))?;

        if response_line.trim().is_empty() {
            return Err("Empty response received from AbletonMCP remote script".into());
        }

        let resp: Value = serde_json::from_str(&response_line)
            .map_err(|e| format!("Failed to parse JSON-RPC response: {} (line: {})", e, response_line))?;

        if let Some(err) = resp.get("error") {
            let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown LOM error");
            return Err(format!("LOM Error: {}", msg));
        }

        Ok(resp.get("result").cloned().unwrap_or(json!({})))
    }

    /// Execute command with dual-target semantics:
    /// Returns connected live result if online, or fallback envelope if offline.
    pub fn execute_with_fallback<F>(
        &self,
        method: &str,
        params: Value,
        fallback_fn: F,
    ) -> Value
    where
        F: FnOnce() -> Value,
    {
        match self.send_request(method, params) {
            Ok(live_result) => json!({
                "connected": true,
                "target": "real_ableton_live_12",
                "result": live_result,
            }),
            Err(_) => {
                let fallback = fallback_fn();
                json!({
                    "connected": false,
                    "target": "real_ableton_live_12",
                    "message": "Real Ableton Live 12 Suite is not running or AbletonMCP remote script is not active on 127.0.0.1:9877. Falling back to ableton-rust engine state.",
                    "fallback_executed": true,
                    "result": fallback,
                })
            }
        }
    }
}
