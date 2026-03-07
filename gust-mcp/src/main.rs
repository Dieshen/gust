// gust-mcp: exposes the Gust compiler as an MCP (Model Context Protocol) server.
//
// Protocol: JSON-RPC 2.0 over newline-delimited stdin/stdout.
// Tools: gust_check, gust_build, gust_diagram, gust_format, gust_parse

use gust_lang::{
    format_program, parse_program_with_errors, validate_program, CffiCodegen, GoCodegen,
    NoStdCodegen, RustCodegen, WasmCodegen,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
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
// Main loop
// ---------------------------------------------------------------------------

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(_) => break,
        };

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                // Parse error: respond with id=null per JSON-RPC spec
                let response = JsonRpcResponse::err(Value::Null, -32700, format!("Parse error: {e}"));
                let json = serde_json::to_string(&response).unwrap();
                writeln!(out, "{json}").unwrap();
                out.flush().unwrap();
                continue;
            }
        };

        if let Some(response) = handle_request(request) {
            let json = serde_json::to_string(&response).unwrap();
            writeln!(out, "{json}").unwrap();
            out.flush().unwrap();
        }
    }
}

// ---------------------------------------------------------------------------
// Request dispatch
// ---------------------------------------------------------------------------

fn handle_request(req: JsonRpcRequest) -> Option<JsonRpcResponse> {
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

fn handle_initialize(id: Value) -> JsonRpcResponse {
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

fn handle_tools_list(id: Value) -> JsonRpcResponse {
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

fn handle_tools_call(id: Value, params: Value) -> JsonRpcResponse {
    let name = match params.get("name").and_then(Value::as_str) {
        Some(n) => n.to_string(),
        None => {
            return JsonRpcResponse::err(id, -32602, "Missing required parameter: name");
        }
    };

    let args = params.get("arguments").cloned().unwrap_or(Value::Object(Default::default()));

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

fn tool_check(args: &Value) -> Result<String, String> {
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
            return Ok(serde_json::to_string_pretty(&result).unwrap());
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
                "note": w.note
            })
        })
        .collect();

    let result = json!({ "errors": errors, "warnings": warnings });
    Ok(serde_json::to_string_pretty(&result).unwrap())
}

// ---------------------------------------------------------------------------
// Tool: gust_build
// ---------------------------------------------------------------------------

fn tool_build(args: &Value) -> Result<String, String> {
    let file = require_string_arg(args, "file")?;
    let target = args
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or("rust");
    let package = args
        .get("package")
        .and_then(Value::as_str)
        .unwrap_or("main");

    let source = read_file(&file)?;

    let program = parse_program_with_errors(&source, &file)
        .map_err(|e| format!("Parse error at {}:{}: {}", e.line, e.col, e.message))?;

    let output = match target {
        "rust" => RustCodegen::new().generate(&program),
        "go" => GoCodegen::new().generate(&program, package),
        "wasm" => WasmCodegen::new().generate(&program),
        "nostd" => NoStdCodegen::new().generate(&program),
        "ffi" => {
            let (rust_src, header) = CffiCodegen::new().generate(&program);
            format!("// === Rust source ===\n{rust_src}\n\n// === C header ===\n{header}")
        }
        other => return Err(format!("Unknown target '{other}'. Valid targets: rust, go, wasm, nostd, ffi")),
    };

    Ok(output)
}

// ---------------------------------------------------------------------------
// Tool: gust_diagram
// ---------------------------------------------------------------------------

fn tool_diagram(args: &Value) -> Result<String, String> {
    let file = require_string_arg(args, "file")?;
    let machine_filter = args.get("machine").and_then(Value::as_str);

    let source = read_file(&file)?;

    let program = parse_program_with_errors(&source, &file)
        .map_err(|e| format!("Parse error at {}:{}: {}", e.line, e.col, e.message))?;

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

fn tool_format(args: &Value) -> Result<String, String> {
    let file = require_string_arg(args, "file")?;
    let source = read_file(&file)?;

    let program = parse_program_with_errors(&source, &file)
        .map_err(|e| format!("Parse error at {}:{}: {}", e.line, e.col, e.message))?;

    Ok(format_program(&program))
}

// ---------------------------------------------------------------------------
// Tool: gust_parse
// ---------------------------------------------------------------------------

fn tool_parse(args: &Value) -> Result<String, String> {
    let file = require_string_arg(args, "file")?;
    let source = read_file(&file)?;

    let program = parse_program_with_errors(&source, &file)
        .map_err(|e| format!("Parse error at {}:{}: {}", e.line, e.col, e.message))?;

    let ast = serialize_program(&program);
    Ok(serde_json::to_string_pretty(&ast).unwrap())
}

// ---------------------------------------------------------------------------
// AST serialization — manual because AST types don't derive Serialize
// ---------------------------------------------------------------------------

fn serialize_program(program: &gust_lang::ast::Program) -> Value {
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
            TypeDecl::Struct { name, fields } => json!({
                "kind": "struct",
                "name": name,
                "fields": fields.iter().map(serialize_field).collect::<Vec<_>>()
            }),
            TypeDecl::Enum { name, variants } => json!({
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
        let timeout = t.timeout.map(|d| json!({
            "value": d.value,
            "unit": serialize_time_unit(&d.unit)
        }));
        json!({
            "name": t.name,
            "from": t.from,
            "targets": t.targets,
            "timeout": timeout
        })
    }

    fn serialize_effect(e: &EffectDecl) -> Value {
        json!({
            "name": e.name,
            "params": e.params.iter().map(serialize_field).collect::<Vec<_>>(),
            "return_type": serialize_type_expr(&e.return_type),
            "is_async": e.is_async
        })
    }

    fn serialize_binop(op: &BinOp) -> &'static str {
        match op {
            BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
            BinOp::Mod => "%", BinOp::Eq => "==", BinOp::Neq => "!=",
            BinOp::Lt => "<", BinOp::Lte => "<=", BinOp::Gt => ">", BinOp::Gte => ">=",
            BinOp::And => "&&", BinOp::Or => "||",
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
            Expr::BinOp(l, op, r) => json!({
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
            Expr::Perform(name, args) => json!({
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
            Pattern::Variant { enum_name, variant, bindings } => json!({
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
            Statement::If { condition, then_block, else_block } => json!({
                "kind": "if",
                "condition": serialize_expr(condition),
                "then": serialize_block(then_block),
                "else": else_block.as_ref().map(serialize_block)
            }),
            Statement::Goto { state, args } => json!({
                "kind": "goto",
                "state": state,
                "args": args.iter().map(serialize_expr).collect::<Vec<_>>()
            }),
            Statement::Perform { effect, args } => json!({
                "kind": "perform",
                "effect": effect,
                "args": args.iter().map(serialize_expr).collect::<Vec<_>>()
            }),
            Statement::Send { channel, message } => json!({
                "kind": "send",
                "channel": channel,
                "message": serialize_expr(message)
            }),
            Statement::Spawn { machine, args } => json!({
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

fn require_string_arg<'a>(args: &'a Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Missing required argument: '{key}'"))
}

fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read '{path}': {e}"))
}
