// gust-mcp: exposes the Gust compiler as an MCP (Model Context Protocol) server.
//
// Protocol: JSON-RPC 2.0 over stdin/stdout with Content-Length header framing.
// Each message is preceded by "Content-Length: N\r\n\r\n" followed by N bytes of JSON.
// Tools: gust_check, gust_build, gust_diagram, gust_format, gust_parse

use gust_mcp::{handle_request, read_message, write_message, JsonRpcRequest, JsonRpcResponse};
use serde_json::Value;
use std::io;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut out = stdout.lock();

    while let Some(body) = read_message(&mut reader) {
        let request: JsonRpcRequest = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                // Parse error: respond with id=null per JSON-RPC spec
                let response =
                    JsonRpcResponse::err(Value::Null, -32700, format!("Parse error: {e}"));
                let json = serde_json::to_string(&response).unwrap();
                write_message(&mut out, &json).unwrap();
                continue;
            }
        };

        if let Some(response) = handle_request(request) {
            let json = serde_json::to_string(&response).unwrap();
            write_message(&mut out, &json).unwrap();
        }
    }
}
