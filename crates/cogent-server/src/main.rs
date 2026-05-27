#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{wrap_tool_response, ToolRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::{Command, Stdio};
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Parser)]
#[command(name = "cogent-server", about = "JSON-RPC daemon for Cogent tools")]
struct Cli {
    /// Transport mode: stdio or tcp
    #[arg(long, default_value = "stdio")]
    mode: String,

    /// TCP port (only used with --mode tcp)
    #[arg(long, default_value_t = 9876)]
    port: u16,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, PartialEq)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Serialize)]
struct ToolCatalogEntry {
    name: String,
    description: String,
    binary: String,
    args_schema: Value,
}

#[derive(Debug, Serialize)]
struct McpTool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

fn tool_catalog() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "debt-scan".to_string(),
            description: "Scan for TODO/FIXME/XXX markers".to_string(),
            binary: "debt".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to crate root" }
                },
                "required": ["path"]
            }),
        },
        ToolCatalogEntry {
            name: "doc-coverage".to_string(),
            description: "Check public API documentation coverage".to_string(),
            binary: "doccov".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        ToolCatalogEntry {
            name: "crap-metric".to_string(),
            description: "Calculate CRAP score for functions".to_string(),
            binary: "crap".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max": { "type": "number" }
                },
                "required": ["path"]
            }),
        },
        ToolCatalogEntry {
            name: "coupling".to_string(),
            description: "Analyze module coupling".to_string(),
            binary: "coupling".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        ToolCatalogEntry {
            name: "risk-map".to_string(),
            description: "Map risk by file churn and complexity".to_string(),
            binary: "riskmap".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        ToolCatalogEntry {
            name: "duplication".to_string(),
            description: "Detect duplicate code".to_string(),
            binary: "dupfind".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        ToolCatalogEntry {
            name: "mutation-test".to_string(),
            description: "Mutation testing".to_string(),
            binary: "mutate".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "strategy": { "type": "string", "default": "all" }
                },
                "required": ["path"]
            }),
        },
        ToolCatalogEntry {
            name: "fuzz-surface".to_string(),
            description: "Find fuzzable functions".to_string(),
            binary: "fuzz".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        ToolCatalogEntry {
            name: "prop-cov".to_string(),
            description: "Property-based test coverage".to_string(),
            binary: "propcov".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        ToolCatalogEntry {
            name: "taint-scan".to_string(),
            description: "Taint analysis for data flow".to_string(),
            binary: "taint".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
    ]
}

fn mcp_tools_list() -> Value {
    let mcp_tools: Vec<McpTool> = tool_catalog()
        .into_iter()
        .map(|e| McpTool {
            name: e.name,
            description: e.description,
            input_schema: e.args_schema,
        })
        .collect();
    serde_json::json!({ "tools": mcp_tools })
}

async fn handle_request(req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    match req.method.as_str() {
        // ── MCP lifecycle ──────────────────────────────────────────────────
        "initialize" => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req.id,
            result: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "cogent",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
            error: None,
        }),
        // MCP notification — client sends after initialize, no response needed
        "notifications/initialized" => None,

        // ── tools/list: supports both legacy array and MCP envelope ────────
        "tools/list" => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req.id,
            result: Some(mcp_tools_list()),
            error: None,
        }),

        // ── tools/call: MCP standard shape ────────────────────────────────
        "tools/call" => {
            let result = run_tool_mcp(req.params).await;
            Some(match result {
                Ok(value) => JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: req.id,
                    result: Some(value),
                    error: None,
                },
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: req.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: e,
                        data: None,
                    }),
                },
            })
        }

        // ── Legacy methods (backward compat) ──────────────────────────────
        "ping" => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req.id,
            result: Some(serde_json::json!({ "pong": true })),
            error: None,
        }),
        "tools/run" => {
            let result = run_tool(req.params).await;
            Some(match result {
                Ok(value) => JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: req.id,
                    result: Some(value),
                    error: None,
                },
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: req.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: e,
                        data: None,
                    }),
                },
            })
        }
        "tools/run_stream" => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message: "tools/run_stream requires stdio transport mode".to_string(),
                data: None,
            }),
        }),
        "shutdown" => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req.id,
            result: Some(serde_json::json!({ "shutdown": true })),
            error: None,
        }),
        _ => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
                data: None,
            }),
        }),
    }
}

async fn run_tool_mcp(params: Option<Value>) -> Result<Value, String> {
    let params = params.ok_or("Missing params")?;
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'name' field in tools/call params")?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    let wrapped_params = serde_json::json!({ "tool": name, "args": arguments });
    let inner = run_tool(Some(wrapped_params)).await?;

    let text = serde_json::to_string_pretty(&inner).unwrap_or_else(|_| inner.to_string());
    Ok(serde_json::json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

async fn run_tool(params: Option<Value>) -> Result<Value, String> {
    let params = params.ok_or("Missing params")?;
    let tool_req: ToolRequest = serde_json::from_value(params).map_err(|e| e.to_string())?;

    let catalog = tool_catalog();
    let entry = catalog
        .iter()
        .find(|e| e.name == tool_req.tool || e.binary == tool_req.tool)
        .ok_or_else(|| format!("Unknown tool: {}", tool_req.tool))?;

    let path = tool_req
        .args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let mut args = vec![path.to_string(), "--format".to_string(), "json".to_string()];

    if let Value::Object(map) = &tool_req.args {
        for (key, value) in map {
            if key == "path" {
                continue;
            }
            if let Some(v) = value.as_str() {
                args.push(format!("--{}", key));
                args.push(v.to_string());
            }
        }
    }

    let start = Instant::now();

    let output = Command::new(&entry.binary)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to execute {}: {}", entry.binary, e))?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let (data, error) = match serde_json::from_str::<Value>(&stdout) {
        Ok(json) => (json, None),
        Err(_) => {
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                (
                    Value::Null,
                    Some(format!("No output. stderr: {}", stderr.trim())),
                )
            } else {
                (serde_json::json!({ "raw": trimmed }), None)
            }
        }
    };

    let success = error.is_none() && output.status.success();
    let response = wrap_tool_response(
        &tool_req.tool,
        env!("CARGO_PKG_VERSION"),
        success,
        duration_ms,
        data,
        None,
        error,
    );

    Ok(serde_json::to_value(response).unwrap())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.mode.as_str() {
        "tcp" => run_tcp(cli.port).await,
        _ => run_stdio().await,
    }
}

async fn run_stdio() {
    let stdin: tokio::io::Stdin = tokio::io::stdin();
    let stdout: tokio::io::Stdout = tokio::io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();
    let mut stdout = stdout;

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: None,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                };
                let msg = serde_json::to_string(&resp).unwrap();
                let _ = stdout.write_all(msg.as_bytes()).await;
                let _ = stdout.write_all(b"\n").await;
                let _ = stdout.flush().await;
                continue;
            }
        };

        let is_shutdown = req.method == "shutdown";
        if let Some(resp) = handle_request(req).await {
            let msg = serde_json::to_string(&resp).unwrap();
            let _ = stdout.write_all(msg.as_bytes()).await;
            let _ = stdout.write_all(b"\n").await;
            let _ = stdout.flush().await;
        }
        if is_shutdown {
            break;
        }
    }
}

async fn run_tcp(port: u16) {
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .unwrap();
    println!("cogent-server listening on 127.0.0.1:{}", port);

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            let (reader, mut writer) = socket.into_split();
            let reader = BufReader::new(reader);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }

                let req: JsonRpcRequest = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(e) => {
                        let resp = JsonRpcResponse {
                            jsonrpc: "2.0",
                            id: None,
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32700,
                                message: format!("Parse error: {}", e),
                                data: None,
                            }),
                        };
                        let msg = serde_json::to_string(&resp).unwrap();
                        let _ = writer.write_all(msg.as_bytes()).await;
                        let _ = writer.write_all(b"\n").await;
                        continue;
                    }
                };

                let is_shutdown = req.method == "shutdown";
                if let Some(resp) = handle_request(req).await {
                    let msg = serde_json::to_string(&resp).unwrap();
                    let _ = writer.write_all(msg.as_bytes()).await;
                    let _ = writer.write_all(b"\n").await;
                }
                if is_shutdown {
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ping() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "ping".to_string(),
            params: None,
        };
        let resp = handle_request(req).await.unwrap();
        assert!(resp.result.is_some());
        assert_eq!(resp.error, None);
    }

    #[tokio::test]
    async fn test_tools_list() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "tools/list".to_string(),
            params: None,
        };
        let resp = handle_request(req).await.unwrap();
        assert!(resp.result.is_some());
        let binding = resp.result.unwrap();
        let tools = binding.get("tools").unwrap();
        assert!(tools.as_array().unwrap().is_empty() == false);
    }

    #[tokio::test]
    async fn test_shutdown() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "shutdown".to_string(),
            params: None,
        };
        let resp = handle_request(req).await.unwrap();
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_initialize() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {}
            })),
        };
        let resp = handle_request(req).await.unwrap();
        assert!(resp.result.is_some());
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["serverInfo"]["name"] == "cogent");
    }

    #[tokio::test]
    async fn test_notifications_initialized_returns_none() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "notifications/initialized".to_string(),
            params: None,
        };
        let resp = handle_request(req).await;
        assert!(resp.is_none());
    }
}
