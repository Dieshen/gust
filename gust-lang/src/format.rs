use crate::ast::*;
use crate::codegen_common::escape_string_literal;
use std::collections::HashMap;

pub fn format_program(program: &Program) -> String {
    format_program_with_source(program, None)
}

pub fn format_program_preserving(program: &Program, source: &str) -> String {
    format_program_with_source(program, Some(source))
}

fn format_program_with_source(program: &Program, source: Option<&str>) -> String {
    let comments = source.map(|s| extract_comment_map(s)).unwrap_or_default();
    let mut out = String::new();

    for use_path in &program.uses {
        out.push_str(&format!("use {};\n", use_path.segments.join("::")));
    }
    if !program.uses.is_empty() {
        out.push('\n');
    }

    // File-level comments (before any declaration)
    if let Some(src) = source {
        let file_comments = extract_leading_file_comments(src);
        if !file_comments.is_empty() {
            for line in &file_comments {
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
    }

    for type_decl in &program.types {
        let (kind, name) = match type_decl {
            TypeDecl::Struct { name, .. } => ("type", name.as_str()),
            TypeDecl::Enum { name, .. } => ("enum", name.as_str()),
        };
        emit_comments(&comments, &format!("{kind}:{name}"), "", &mut out);
        format_type_decl(type_decl, &mut out);
        out.push('\n');
    }

    for channel in &program.channels {
        emit_comments(&comments, &format!("channel:{}", channel.name), "", &mut out);
        format_channel_decl(channel, &mut out);
        out.push('\n');
    }

    for machine in &program.machines {
        format_machine_with_comments(machine, &comments, &mut out);
        out.push('\n');
    }

    out.trim_end().to_string() + "\n"
}

fn format_type_decl(decl: &TypeDecl, out: &mut String) {
    match decl {
        TypeDecl::Struct { name, fields } => {
            out.push_str(&format!("type {name} {{\n"));
            for field in fields {
                out.push_str(&format!(
                    "    {}: {},\n",
                    field.name,
                    format_type_expr(&field.ty)
                ));
            }
            out.push_str("}\n");
        }
        TypeDecl::Enum { name, variants } => {
            out.push_str(&format!("enum {name} {{\n"));
            for variant in variants {
                if variant.payload.is_empty() {
                    out.push_str(&format!("    {},\n", variant.name));
                } else {
                    let payload = variant
                        .payload
                        .iter()
                        .map(format_type_expr)
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("    {}({payload}),\n", variant.name));
                }
            }
            out.push_str("}\n");
        }
    }
}

fn format_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Unit => "()".to_string(),
        TypeExpr::Simple(s) => s.clone(),
        TypeExpr::Generic(name, args) => {
            let args = args
                .iter()
                .map(format_type_expr)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name}<{args}>")
        }
        TypeExpr::Tuple(items) => {
            let inner = items
                .iter()
                .map(format_type_expr)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({inner})")
        }
    }
}

fn format_channel_decl(channel: &ChannelDecl, out: &mut String) {
    let mut cfg = Vec::new();
    if let Some(capacity) = channel.capacity {
        cfg.push(format!("capacity: {capacity}"));
    }
    cfg.push(format!(
        "mode: {}",
        match channel.mode {
            ChannelMode::Broadcast => "broadcast",
            ChannelMode::Mpsc => "mpsc",
        }
    ));
    out.push_str(&format!(
        "channel {}: {} ({})\n",
        channel.name,
        format_type_expr(&channel.message_type),
        cfg.join(", ")
    ));
}

fn format_block(block: &Block, indent: usize) -> String {
    let mut out = String::new();
    for stmt in &block.statements {
        out.push_str(&format_statement(stmt, indent));
    }
    out
}

fn format_statement(stmt: &Statement, indent: usize) -> String {
    let pad = "    ".repeat(indent);
    match stmt {
        Statement::Let { name, ty, value } => {
            if let Some(t) = ty {
                format!(
                    "{pad}let {name}: {} = {};\n",
                    format_type_expr(t),
                    format_expr(value)
                )
            } else {
                format!("{pad}let {name} = {};\n", format_expr(value))
            }
        }
        Statement::Return(expr) => format!("{pad}return {};\n", format_expr(expr)),
        Statement::Goto { state, args } => {
            if args.is_empty() {
                format!("{pad}goto {state};\n")
            } else {
                let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
                format!("{pad}goto {state}({});\n", arg_strs.join(", "))
            }
        }
        Statement::Perform { effect, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("{pad}perform {effect}({});\n", arg_strs.join(", "))
        }
        Statement::Send { channel, message } => {
            format!("{pad}send {channel}({});\n", format_expr(message))
        }
        Statement::Spawn { machine, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("{pad}spawn {machine}({});\n", arg_strs.join(", "))
        }
        Statement::If {
            condition,
            then_block,
            else_block,
        } => {
            let mut out = format!("{pad}if {} {{\n", format_expr(condition));
            out.push_str(&format_block(then_block, indent + 1));
            if let Some(else_blk) = else_block {
                out.push_str(&format!("{pad}}} else {{\n"));
                out.push_str(&format_block(else_blk, indent + 1));
            }
            out.push_str(&format!("{pad}}}\n"));
            out
        }
        Statement::Match { scrutinee, arms } => {
            let mut out = format!("{pad}match {} {{\n", format_expr(scrutinee));
            for arm in arms {
                out.push_str(&format!(
                    "{pad}    {} => {{\n",
                    format_pattern(&arm.pattern)
                ));
                out.push_str(&format_block(&arm.body, indent + 2));
                out.push_str(&format!("{pad}    }}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
            out
        }
        Statement::Expr(expr) => format!("{pad}{};\n", format_expr(expr)),
    }
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::IntLit(v) => format!("{v}"),
        Expr::FloatLit(v) => format!("{v}"),
        Expr::StringLit(s) => format!("\"{}\"", escape_string_literal(s)),
        Expr::BoolLit(b) => format!("{b}"),
        Expr::Ident(name) => name.clone(),
        Expr::FieldAccess(base, field) => format!("{}.{field}", format_expr(base)),
        Expr::FnCall(name, args) => {
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("{name}({})", arg_strs.join(", "))
        }
        Expr::BinOp(left, op, right) => {
            format!(
                "{} {} {}",
                format_expr(left),
                format_binop(op),
                format_expr(right)
            )
        }
        Expr::UnaryOp(op, inner) => {
            let op_str = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{op_str}{}", format_expr(inner))
        }
        Expr::Perform(effect, args) => {
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("perform {effect}({})", arg_strs.join(", "))
        }
        Expr::Path(enum_name, variant) => format!("{enum_name}::{variant}"),
    }
}

fn format_binop(op: &BinOp) -> &'static str {
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

fn format_pattern(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Wildcard => "_".to_string(),
        Pattern::Ident(name) => name.clone(),
        Pattern::Variant {
            enum_name,
            variant,
            bindings,
        } => {
            let prefix = enum_name
                .as_ref()
                .map(|e| format!("{e}::"))
                .unwrap_or_default();
            if bindings.is_empty() {
                format!("{prefix}{variant}")
            } else {
                format!("{prefix}{variant}({})", bindings.join(", "))
            }
        }
    }
}

fn format_duration(duration: DurationSpec) -> String {
    format!(
        "{}{}",
        duration.value,
        match duration.unit {
            TimeUnit::Millis => "ms",
            TimeUnit::Seconds => "s",
            TimeUnit::Minutes => "m",
            TimeUnit::Hours => "h",
        }
    )
}

// --- Comment preservation ---

/// Declaration keyword prefixes used to identify declaration lines.
const DECL_PREFIXES: &[&str] = &[
    "machine ", "state ", "effect ", "async effect ",
    "transition ", "type ", "struct ", "enum ",
    "channel ", "on ", "async on ",
];

/// Extract the file-level header comment block: contiguous `//` lines at the
/// very start of the file, ending at the first blank line or declaration.
fn extract_leading_file_comments(source: &str) -> Vec<String> {
    let mut comments = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            comments.push(line.to_string());
        } else {
            // Stop at the first blank line or non-comment line
            break;
        }
    }
    comments
}

/// Build a map of "kind:name" -> Vec<comment_lines> by scanning the original
/// source. Uses composite keys so that e.g. `transition reap` and `on reap`
/// don't collide. Also extracts comment lines inside handler bodies keyed as
/// "body:handler_name".
fn extract_comment_map(source: &str) -> HashMap<String, Vec<String>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    let mut in_handler: Option<(String, usize, i32)> = None; // (name, start_line, brace_depth)
    let mut body_comments: HashMap<String, Vec<String>> = HashMap::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Track handler body scope for inline comments
        if let Some((ref name, _, ref mut depth)) = in_handler {
            for ch in trimmed.chars() {
                match ch {
                    '{' => *depth += 1,
                    '}' => *depth -= 1,
                    _ => {}
                }
            }
            if trimmed.starts_with("//") {
                body_comments
                    .entry(format!("body:{name}"))
                    .or_default()
                    .push(trimmed.to_string());
            }
            if in_handler.as_ref().map(|(_, _, d)| *d <= 0).unwrap_or(false) {
                in_handler = None;
            }
            continue;
        }

        // Check if this line is a declaration
        let decl_key = DECL_PREFIXES.iter().find_map(|prefix| {
            trimmed.strip_prefix(prefix).and_then(|rest| {
                let name_end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len());
                if name_end > 0 {
                    let name = rest[..name_end].to_string();
                    // Determine the kind from the prefix
                    let kind = prefix.trim();
                    Some((format!("{kind}:{name}"), name))
                } else {
                    None
                }
            })
        });

        let Some((key, name)) = decl_key else {
            continue;
        };

        // Track entry into handler body
        if key.starts_with("on:") || key.starts_with("async on:") {
            if trimmed.contains('{') {
                let mut depth: i32 = 0;
                for ch in trimmed.chars() {
                    match ch {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                if depth > 0 {
                    in_handler = Some((name, idx, depth));
                }
            }
        }

        // Walk backwards to collect the contiguous comment block directly
        // above this declaration. Stop at blank lines or non-comment lines.
        let mut comment_lines = Vec::new();
        let mut i = idx;
        while i > 0 {
            i -= 1;
            let prev = lines[i].trim();
            if prev.starts_with("//") {
                comment_lines.push(lines[i].trim_start().to_string());
            } else {
                break;
            }
        }
        comment_lines.reverse();

        if !comment_lines.is_empty() {
            map.insert(key, comment_lines);
        }
    }

    // Merge body comments into main map
    map.extend(body_comments);
    map
}

/// Emit comment lines for the given composite key at the given indentation.
fn emit_comments(
    comments: &HashMap<String, Vec<String>>,
    key: &str,
    indent: &str,
    out: &mut String,
) {
    if let Some(lines) = comments.get(key) {
        for line in lines {
            out.push_str(indent);
            out.push_str(line);
            out.push('\n');
        }
    }
}

/// Format a machine block, re-inserting comments from the original source.
fn format_machine_with_comments(
    machine: &MachineDecl,
    comments: &HashMap<String, Vec<String>>,
    out: &mut String,
) {
    let generic_params = if machine.generic_params.is_empty() {
        String::new()
    } else {
        let params = machine
            .generic_params
            .iter()
            .map(|p| {
                if p.bounds.is_empty() {
                    p.name.clone()
                } else {
                    format!("{}: {}", p.name, p.bounds.join(" + "))
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("<{params}>")
    };
    let mut annotations = Vec::new();
    annotations.extend(machine.sends.iter().map(|c| format!("sends {c}")));
    annotations.extend(machine.receives.iter().map(|c| format!("receives {c}")));
    annotations.extend(machine.supervises.iter().map(|s| {
        format!(
            "supervises {}({})",
            s.child_machine,
            match s.strategy {
                SupervisionStrategy::OneForOne => "one_for_one",
                SupervisionStrategy::OneForAll => "one_for_all",
                SupervisionStrategy::RestForOne => "rest_for_one",
            }
        )
    }));

    emit_comments(comments, &format!("machine:{}", machine.name), "", out);
    if annotations.is_empty() {
        out.push_str(&format!("machine {}{} {{\n", machine.name, generic_params));
    } else {
        out.push_str(&format!(
            "machine {}{}({}) {{\n",
            machine.name,
            generic_params,
            annotations.join(", ")
        ));
    }

    for state in &machine.states {
        emit_comments(comments, &format!("state:{}", state.name), "    ", out);
        if state.fields.is_empty() {
            out.push_str(&format!("    state {}\n", state.name));
        } else {
            let fields = state
                .fields
                .iter()
                .map(|f| format!("{}: {}", f.name, format_type_expr(&f.ty)))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("    state {}({fields})\n", state.name));
        }
    }
    if !machine.states.is_empty() {
        out.push('\n');
    }

    for transition in &machine.transitions {
        emit_comments(comments, &format!("transition:{}", transition.name), "    ", out);
        let timeout = transition
            .timeout
            .map(format_duration)
            .map(|d| format!(" timeout {d}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "    transition {}: {} -> {}{}\n",
            transition.name,
            transition.from,
            transition.targets.join(" | "),
            timeout
        ));
    }
    if !machine.transitions.is_empty() {
        out.push('\n');
    }

    for effect in &machine.effects {
        emit_comments(comments, &format!("effect:{}", effect.name), "    ", out);
        let async_kw = if effect.is_async { "async " } else { "" };
        let params = effect
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, format_type_expr(&p.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "    {async_kw}effect {}({params}) -> {}\n",
            effect.name,
            format_type_expr(&effect.return_type)
        ));
    }
    if !machine.effects.is_empty() {
        out.push('\n');
    }

    for handler in &machine.handlers {
        let on_key = if handler.is_async {
            format!("async on:{}", handler.transition_name)
        } else {
            format!("on:{}", handler.transition_name)
        };
        emit_comments(comments, &on_key, "    ", out);
        let async_kw = if handler.is_async { "async " } else { "" };
        let params = handler
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, format_type_expr(&p.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        let ret_ty = handler
            .return_type
            .as_ref()
            .map(|t| format!(" -> {}", format_type_expr(t)))
            .unwrap_or_default();
        out.push_str(&format!(
            "    {async_kw}on {}({params}){ret_ty} {{\n",
            handler.transition_name
        ));
        // Re-insert inline comments from the original handler body
        let body_key = format!("body:{}", handler.transition_name);
        if let Some(body_comments) = comments.get(&body_key) {
            // Place body comments before the formatted statements
            for c in body_comments {
                out.push_str(&format!("        {c}\n"));
            }
        }
        out.push_str(&format_block(&handler.body, 2));
        out.push_str("    }\n\n");
    }

    out.push_str("}\n");
}
