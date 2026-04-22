#![warn(missing_docs)]
//! MCP (Model Context Protocol) server library for Gust.
//!
//! Exposes JSON-RPC tools (`gust_parse`, `gust_check`, `gust_build`,
//! `gust_format`, `gust_diagram`) over stdin/stdout for AI-assisted
//! development workflows. The binary in `src/main.rs` dispatches
//! framed Content-Length messages into the handlers defined here.

use gust_lang::{
    format_program_preserving, parse_program_with_errors, validate_program, CffiCodegen, GoCodegen,
    NoStdCodegen, RustCodegen, WasmCodegen,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Write};

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request envelope received over the MCP transport.
#[derive(Deserialize)]
pub struct JsonRpcRequest {
    /// Protocol version string (must be `"2.0"`).
    #[allow(dead_code)]
    pub jsonrpc: String,
    /// Correlation id. `None` on notifications.
    pub id: Option<Value>,
    /// Method name (e.g. `"tools/list"`, `"tools/call"`).
    pub method: String,
    /// Opaque method parameters (shape varies by method).
    #[serde(default)]
    pub params: Value,
}

/// A JSON-RPC 2.0 response envelope sent back over the MCP transport.
#[derive(Serialize)]
pub struct JsonRpcResponse {
    /// Protocol version string (always `"2.0"`).
    pub jsonrpc: String,
    /// Correlation id echoing the originating request.
    pub id: Value,
    /// Method result payload — mutually exclusive with `error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error payload — mutually exclusive with `result`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error body (per the spec: numeric `code` + human `message`).
#[derive(Serialize)]
pub struct JsonRpcError {
    /// Numeric error code (e.g. `-32601` for method-not-found).
    pub code: i32,
    /// Human-readable error message.
    pub message: String,
}

impl JsonRpcResponse {
    /// Build a successful response for `id` carrying `result`.
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Build an error response for `id` with the given `code` and `message`.
    pub fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Content-Length framing helpers
// ---------------------------------------------------------------------------

/// Read a single message using Content-Length header framing.
/// Expects: `Content-Length: N\r\n\r\n` followed by exactly N bytes of JSON.
pub fn read_message(reader: &mut impl io::BufRead) -> Option<String> {
    let mut content_length: Option<usize> = None;
    let mut header = String::new();

    loop {
        header.clear();
        if reader.read_line(&mut header).ok()? == 0 {
            return None; // EOF
        }
        let trimmed = header.trim();
        if trimmed.is_empty() {
            break; // blank line signals end of headers
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str.parse().ok();
        }
        // Ignore unknown headers for forward compatibility.
    }

    let length = content_length?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    String::from_utf8(body).ok()
}

/// Write a single message with Content-Length header framing.
pub fn write_message(writer: &mut impl Write, json: &str) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
    writer.flush()
}

// ---------------------------------------------------------------------------
// Request dispatch
// ---------------------------------------------------------------------------

/// Dispatch a JSON-RPC request to the corresponding handler.
///
/// Returns `None` for notifications (requests without an `id`); the server
/// uses that to signal "no response needed."
pub fn handle_request(req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let id = req.id.clone().unwrap_or(Value::Null);

    match req.method.as_str() {
        "initialize" => Some(handle_initialize(id)),
        // Notifications have no id and require no response.
        "notifications/initialized" => None,
        "tools/list" => Some(handle_tools_list(id)),
        "tools/call" => Some(handle_tools_call(id, req.params)),
        _ => Some(JsonRpcResponse::err(
            id,
            -32601,
            format!("Method not found: {}", req.method),
        )),
    }
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

/// Handle the MCP `initialize` method — advertises server capabilities.
pub fn handle_initialize(id: Value) -> JsonRpcResponse {
    JsonRpcResponse::ok(
        id,
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "gust-mcp",
                "version": "0.1.0"
            }
        }),
    )
}

// ---------------------------------------------------------------------------
// tools/list
// ---------------------------------------------------------------------------

/// Handle the MCP `tools/list` method — returns the five exposed tools
/// (`gust_check`, `gust_build`, `gust_diagram`, `gust_format`,
/// `gust_parse`) along with their input JSON Schemas.
pub fn handle_tools_list(id: Value) -> JsonRpcResponse {
    JsonRpcResponse::ok(
        id,
        json!({
            "tools": [
                {
                    "name": "gust_check",
                    "description": "Validate a .gu file and return diagnostics (errors and warnings)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "file": {
                                "type": "string",
                                "description": "Absolute path to the .gu source file"
                            }
                        },
                        "required": ["file"]
                    }
                },
                {
                    "name": "gust_build",
                    "description": "Compile a .gu file to the specified target language",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "file": {
                                "type": "string",
                                "description": "Absolute path to the .gu source file"
                            },
                            "target": {
                                "type": "string",
                                "description": "Compilation target",
                                "enum": ["rust", "go", "wasm", "nostd", "ffi"],
                                "default": "rust"
                            },
                            "package": {
                                "type": "string",
                                "description": "Package name (required for the 'go' target)"
                            }
                        },
                        "required": ["file"]
                    }
                },
                {
                    "name": "gust_diagram",
                    "description": "Generate a Mermaid state diagram from a .gu file",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "file": {
                                "type": "string",
                                "description": "Absolute path to the .gu source file"
                            },
                            "machine": {
                                "type": "string",
                                "description": "Name of a specific machine to diagram (omit for all machines)"
                            }
                        },
                        "required": ["file"]
                    }
                },
                {
                    "name": "gust_format",
                    "description": "Format a .gu file and return the formatted source",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "file": {
                                "type": "string",
                                "description": "Absolute path to the .gu source file"
                            }
                        },
                        "required": ["file"]
                    }
                },
                {
                    "name": "gust_parse",
                    "description": "Parse a .gu file and return the AST as JSON",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "file": {
                                "type": "string",
                                "description": "Absolute path to the .gu source file"
                            }
                        },
                        "required": ["file"]
                    }
                }
            ]
        }),
    )
}

// ---------------------------------------------------------------------------
// tools/call
// ---------------------------------------------------------------------------

/// Handle the MCP `tools/call` method — dispatches to the appropriate
/// `tool_*` function based on the `name` field in `params`.
pub fn handle_tools_call(id: Value, params: Value) -> JsonRpcResponse {
    let name = match params.get("name").and_then(Value::as_str) {
        Some(n) => n.to_string(),
        None => {
            return JsonRpcResponse::err(id, -32602, "Missing required parameter: name");
        }
    };

    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    let result = match name.as_str() {
        "gust_check" => tool_check(&args),
        "gust_build" => tool_build(&args),
        "gust_diagram" => tool_diagram(&args),
        "gust_format" => tool_format(&args),
        "gust_parse" => tool_parse(&args),
        other => Err(format!("Unknown tool: {other}")),
    };

    match result {
        Ok(text) => JsonRpcResponse::ok(
            id,
            json!({
                "content": [{ "type": "text", "text": text }]
            }),
        ),
        Err(msg) => JsonRpcResponse::ok(
            id,
            json!({
                "content": [{ "type": "text", "text": msg }],
                "isError": true
            }),
        ),
    }
}

// ---------------------------------------------------------------------------
// Tool: gust_check
// ---------------------------------------------------------------------------

/// Implementation of the `gust_check` tool: parse and validate a `.gu`
/// file, returning a formatted diagnostic report.
pub fn tool_check(args: &Value) -> Result<String, String> {
    let file = require_string_arg(args, "file")?;
    let source = read_file(&file)?;

    let program = match parse_program_with_errors(&source, &file) {
        Ok(p) => p,
        Err(e) => {
            let result = json!({
                "errors": [{
                    "line": e.line,
                    "col": e.col,
                    "message": e.message,
                    "note": e.note,
                    "help": e.help
                }],
                "warnings": []
            });
            return serde_json::to_string_pretty(&result)
                .map_err(|e| format!("serialization failure: {e}"));
        }
    };

    let report = validate_program(&program, &file, &source);

    let errors: Vec<Value> = report
        .errors
        .iter()
        .map(|e| {
            json!({
                "line": e.line,
                "col": e.col,
                "message": e.message,
                "note": e.note,
                "help": e.help
            })
        })
        .collect();

    let warnings: Vec<Value> = report
        .warnings
        .iter()
        .map(|w| {
            json!({
                "line": w.line,
                "col": w.col,
                "message": w.message,
                "note": w.note,
                "help": w.help
            })
        })
        .collect();

    let result = json!({ "errors": errors, "warnings": warnings });
    serde_json::to_string_pretty(&result).map_err(|e| format!("serialization failure: {e}"))
}

// ---------------------------------------------------------------------------
// Tool: gust_build
// ---------------------------------------------------------------------------

/// Implementation of the `gust_build` tool: compile a `.gu` file to the
/// requested target (`rust`, `go`, `wasm`, `nostd`, or `ffi`).
pub fn tool_build(args: &Value) -> Result<String, String> {
    let file = require_string_arg(args, "file")?;
    let target = args.get("target").and_then(Value::as_str).unwrap_or("rust");
    let package = args
        .get("package")
        .and_then(Value::as_str)
        .unwrap_or("main");

    let source = read_file(&file)?;

    let program = parse_or_err(&source, &file)?;

    let output = match target {
        "rust" => RustCodegen::new().generate(&program),
        "go" => GoCodegen::new().generate(&program, package),
        "wasm" => WasmCodegen::new().generate(&program),
        "nostd" => NoStdCodegen::new().generate(&program),
        "ffi" => {
            let (rust_src, header) = CffiCodegen::new().generate(&program);
            format!("// === Rust source ===\n{rust_src}\n\n// === C header ===\n{header}")
        }
        other => {
            return Err(format!(
                "Unknown target '{other}'. Valid targets: rust, go, wasm, nostd, ffi"
            ))
        }
    };

    Ok(output)
}

// ---------------------------------------------------------------------------
// Tool: gust_diagram
// ---------------------------------------------------------------------------

/// Implementation of the `gust_diagram` tool: generate a Mermaid state
/// diagram from a `.gu` file, optionally filtered to a specific machine.
pub fn tool_diagram(args: &Value) -> Result<String, String> {
    let file = require_string_arg(args, "file")?;
    let machine_filter = args.get("machine").and_then(Value::as_str);

    let source = read_file(&file)?;

    let program = parse_or_err(&source, &file)?;

    if program.machines.is_empty() {
        return Err("No machine declarations found in file".to_string());
    }

    match machine_filter {
        Some(name) => {
            let machine = program
                .machines
                .iter()
                .find(|m| m.name == name)
                .ok_or_else(|| {
                    let available: Vec<&str> =
                        program.machines.iter().map(|m| m.name.as_str()).collect();
                    format!(
                        "Machine '{}' not found. Available: {}",
                        name,
                        available.join(", ")
                    )
                })?;
            Ok(render_machine_diagram(machine))
        }
        None => {
            let parts: Vec<String> = program
                .machines
                .iter()
                .map(|m| format!("%% Machine: {}\n{}", m.name, render_machine_diagram(m)))
                .collect();
            Ok(parts.join("\n"))
        }
    }
}

fn render_machine_diagram(machine: &gust_lang::ast::MachineDecl) -> String {
    let mut out = String::from("stateDiagram-v2\n");
    if let Some(first) = machine.states.first() {
        out.push_str(&format!("    [*] --> {}\n", first.name));
    }
    for t in &machine.transitions {
        for target in &t.targets {
            out.push_str(&format!("    {} --> {} : {}\n", t.from, target, t.name));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tool: gust_format
// ---------------------------------------------------------------------------

/// Implementation of the `gust_format` tool: reformat a `.gu` file and
/// return the comment-preserving canonical source.
pub fn tool_format(args: &Value) -> Result<String, String> {
    let file = require_string_arg(args, "file")?;
    let source = read_file(&file)?;

    let program = parse_or_err(&source, &file)?;

    Ok(format_program_preserving(&program, &source))
}

// ---------------------------------------------------------------------------
// Tool: gust_parse
// ---------------------------------------------------------------------------

/// Implementation of the `gust_parse` tool: parse a `.gu` file and
/// return a JSON-serialized AST (machines, states, transitions,
/// effects with their `kind` field).
pub fn tool_parse(args: &Value) -> Result<String, String> {
    let file = require_string_arg(args, "file")?;
    let source = read_file(&file)?;

    let program = parse_or_err(&source, &file)?;

    let ast = serialize_program(&program);
    serde_json::to_string_pretty(&ast).map_err(|e| format!("serialization failure: {e}"))
}

// ---------------------------------------------------------------------------
// AST serialization — manual because AST types don't derive Serialize
// ---------------------------------------------------------------------------

/// Serialize a parsed `Program` into the JSON shape exposed by
/// `gust_parse`. Effect entries include a `kind` field
/// (`"effect"` | `"action"`).
pub fn serialize_program(program: &gust_lang::ast::Program) -> Value {
    use gust_lang::ast::*;

    fn serialize_type_expr(ty: &TypeExpr) -> Value {
        match ty {
            TypeExpr::Unit => json!("()"),
            TypeExpr::Simple(name) => json!(name),
            TypeExpr::Generic(name, args) => json!({
                "generic": name,
                "args": args.iter().map(serialize_type_expr).collect::<Vec<_>>()
            }),
            TypeExpr::Tuple(elems) => json!({
                "tuple": elems.iter().map(serialize_type_expr).collect::<Vec<_>>()
            }),
        }
    }

    fn serialize_field(f: &Field) -> Value {
        json!({ "name": f.name, "type": serialize_type_expr(&f.ty) })
    }

    fn serialize_type_decl(td: &TypeDecl) -> Value {
        match td {
            TypeDecl::Struct { name, fields, .. } => json!({
                "kind": "struct",
                "name": name,
                "fields": fields.iter().map(serialize_field).collect::<Vec<_>>()
            }),
            TypeDecl::Enum { name, variants, .. } => json!({
                "kind": "enum",
                "name": name,
                "variants": variants.iter().map(|v| json!({
                    "name": v.name,
                    "payload": v.payload.iter().map(serialize_type_expr).collect::<Vec<_>>()
                })).collect::<Vec<_>>()
            }),
        }
    }

    fn serialize_channel_mode(mode: &gust_lang::ast::ChannelMode) -> &'static str {
        match mode {
            gust_lang::ast::ChannelMode::Broadcast => "broadcast",
            gust_lang::ast::ChannelMode::Mpsc => "mpsc",
        }
    }

    fn serialize_channel(ch: &ChannelDecl) -> Value {
        json!({
            "name": ch.name,
            "message_type": serialize_type_expr(&ch.message_type),
            "capacity": ch.capacity,
            "mode": serialize_channel_mode(&ch.mode)
        })
    }

    fn serialize_supervision_strategy(s: &SupervisionStrategy) -> &'static str {
        match s {
            SupervisionStrategy::OneForOne => "one_for_one",
            SupervisionStrategy::OneForAll => "one_for_all",
            SupervisionStrategy::RestForOne => "rest_for_one",
        }
    }

    fn serialize_time_unit(u: &TimeUnit) -> &'static str {
        match u {
            TimeUnit::Millis => "ms",
            TimeUnit::Seconds => "s",
            TimeUnit::Minutes => "m",
            TimeUnit::Hours => "h",
        }
    }

    fn serialize_state(s: &StateDecl) -> Value {
        json!({
            "name": s.name,
            "fields": s.fields.iter().map(serialize_field).collect::<Vec<_>>()
        })
    }

    fn serialize_transition(t: &TransitionDecl) -> Value {
        let timeout = t.timeout.map(|d| {
            json!({
                "value": d.value,
                "unit": serialize_time_unit(&d.unit)
            })
        });
        json!({
            "name": t.name,
            "from": t.from,
            "targets": t.targets,
            "timeout": timeout
        })
    }

    fn serialize_effect(e: &EffectDecl) -> Value {
        // `kind` distinguishes replay-safe `effect` from non-idempotent
        // `action`. Downstream workflow runtimes (e.g. Corsac) use this
        // field to decide retry/checkpoint semantics. See #40.
        let kind = match e.kind {
            EffectKind::Effect => "effect",
            EffectKind::Action => "action",
        };
        json!({
            "name": e.name,
            "params": e.params.iter().map(serialize_field).collect::<Vec<_>>(),
            "return_type": serialize_type_expr(&e.return_type),
            "is_async": e.is_async,
            "kind": kind
        })
    }

    fn serialize_binop(op: &BinOp) -> &'static str {
        match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Neq => "!=",
            BinOp::Lt => "<",
            BinOp::Lte => "<=",
            BinOp::Gt => ">",
            BinOp::Gte => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }

    fn serialize_expr(e: &Expr) -> Value {
        match e {
            Expr::IntLit(n) => json!({ "kind": "int", "value": n }),
            Expr::FloatLit(f) => json!({ "kind": "float", "value": f }),
            Expr::StringLit(s) => json!({ "kind": "string", "value": s }),
            Expr::BoolLit(b) => json!({ "kind": "bool", "value": b }),
            Expr::Ident(name) => json!({ "kind": "ident", "name": name }),
            Expr::FieldAccess(base, field) => json!({
                "kind": "field_access",
                "base": serialize_expr(base),
                "field": field
            }),
            Expr::FnCall(name, args) => json!({
                "kind": "call",
                "name": name,
                "args": args.iter().map(serialize_expr).collect::<Vec<_>>()
            }),
            Expr::BinOp(l, op, r, _) => json!({
                "kind": "binop",
                "op": serialize_binop(op),
                "left": serialize_expr(l),
                "right": serialize_expr(r)
            }),
            Expr::UnaryOp(op, e) => json!({
                "kind": "unary",
                "op": match op { UnaryOp::Not => "!", UnaryOp::Neg => "-" },
                "expr": serialize_expr(e)
            }),
            Expr::Perform(name, args, _) => json!({
                "kind": "perform",
                "effect": name,
                "args": args.iter().map(serialize_expr).collect::<Vec<_>>()
            }),
            Expr::Path(enum_name, variant) => json!({
                "kind": "path",
                "enum": enum_name,
                "variant": variant
            }),
        }
    }

    fn serialize_pattern(p: &Pattern) -> Value {
        match p {
            Pattern::Wildcard => json!("_"),
            Pattern::Ident(name) => json!({ "ident": name }),
            Pattern::Variant {
                enum_name,
                variant,
                bindings,
            } => json!({
                "enum": enum_name,
                "variant": variant,
                "bindings": bindings
            }),
        }
    }

    fn serialize_stmt(s: &Statement) -> Value {
        match s {
            Statement::Let { name, ty, value } => json!({
                "kind": "let",
                "name": name,
                "type": ty.as_ref().map(serialize_type_expr),
                "value": serialize_expr(value)
            }),
            Statement::Return(e) => json!({ "kind": "return", "value": serialize_expr(e) }),
            Statement::If {
                condition,
                then_block,
                else_block,
                span: _,
            } => json!({
                "kind": "if",
                "condition": serialize_expr(condition),
                "then": serialize_block(then_block),
                "else": else_block.as_ref().map(serialize_block)
            }),
            Statement::Goto { state, args, .. } => json!({
                "kind": "goto",
                "state": state,
                "args": args.iter().map(serialize_expr).collect::<Vec<_>>()
            }),
            Statement::Perform { effect, args, .. } => json!({
                "kind": "perform",
                "effect": effect,
                "args": args.iter().map(serialize_expr).collect::<Vec<_>>()
            }),
            Statement::Send {
                channel, message, ..
            } => json!({
                "kind": "send",
                "channel": channel,
                "message": serialize_expr(message)
            }),
            Statement::Spawn { machine, args, .. } => json!({
                "kind": "spawn",
                "machine": machine,
                "args": args.iter().map(serialize_expr).collect::<Vec<_>>()
            }),
            Statement::Match { scrutinee, arms } => json!({
                "kind": "match",
                "scrutinee": serialize_expr(scrutinee),
                "arms": arms.iter().map(|arm| json!({
                    "pattern": serialize_pattern(&arm.pattern),
                    "body": serialize_block(&arm.body)
                })).collect::<Vec<_>>()
            }),
            Statement::Expr(e) => json!({ "kind": "expr", "value": serialize_expr(e) }),
        }
    }

    fn serialize_block(b: &gust_lang::ast::Block) -> Value {
        json!({ "statements": b.statements.iter().map(serialize_stmt).collect::<Vec<_>>() })
    }

    fn serialize_handler(h: &OnHandler) -> Value {
        json!({
            "transition": h.transition_name,
            "params": h.params.iter().map(|p| json!({
                "name": p.name,
                "type": serialize_type_expr(&p.ty)
            })).collect::<Vec<_>>(),
            "return_type": h.return_type.as_ref().map(serialize_type_expr),
            "is_async": h.is_async,
            "body": serialize_block(&h.body)
        })
    }

    fn serialize_machine(m: &MachineDecl) -> Value {
        json!({
            "name": m.name,
            "generic_params": m.generic_params.iter().map(|gp| json!({
                "name": gp.name,
                "bounds": gp.bounds
            })).collect::<Vec<_>>(),
            "sends": m.sends,
            "receives": m.receives,
            "supervises": m.supervises.iter().map(|s| json!({
                "child": s.child_machine,
                "strategy": serialize_supervision_strategy(&s.strategy)
            })).collect::<Vec<_>>(),
            "states": m.states.iter().map(serialize_state).collect::<Vec<_>>(),
            "transitions": m.transitions.iter().map(serialize_transition).collect::<Vec<_>>(),
            "effects": m.effects.iter().map(serialize_effect).collect::<Vec<_>>(),
            "handlers": m.handlers.iter().map(serialize_handler).collect::<Vec<_>>()
        })
    }

    json!({
        "uses": program.uses.iter().map(|u| u.segments.join("::")).collect::<Vec<_>>(),
        "types": program.types.iter().map(serialize_type_decl).collect::<Vec<_>>(),
        "channels": program.channels.iter().map(serialize_channel).collect::<Vec<_>>(),
        "machines": program.machines.iter().map(serialize_machine).collect::<Vec<_>>()
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a `.gu` source string, mapping a parse failure to an error string
/// in the form `"Parse error at {line}:{col}: {message}"`. Used by tool
/// handlers that need a compiled program or an early-exit error response.
fn parse_or_err(source: &str, file: &str) -> Result<gust_lang::ast::Program, String> {
    parse_program_with_errors(source, file)
        .map_err(|e| format!("Parse error at {}:{}: {}", e.line, e.col, e.message))
}

/// Extract a required string argument from a tool-call `args` object,
/// returning an error string if the key is missing or not a string.
pub fn require_string_arg(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Missing required argument: '{key}'"))
}

fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("Cannot read '{path}': {e}"))
}
