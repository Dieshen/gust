//! Shared AST analysis helpers used by multiple code-generation backends.
//!
//! These functions walk AST blocks/expressions to detect usage patterns
//! (perform, spawn, channels, ctx references) without emitting any
//! target-language code.

use crate::ast::*;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Block-level feature detection
// ---------------------------------------------------------------------------

/// Whether a handler body uses any `perform` expressions or statements.
pub fn handler_uses_perform(block: &Block) -> bool {
    block.statements.iter().any(|stmt| match stmt {
        Statement::Perform { .. } => true,
        Statement::Let { value, .. } => expr_has_perform(value),
        Statement::Return(expr) | Statement::Expr(expr) => expr_has_perform(expr),
        Statement::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_has_perform(condition)
                || handler_uses_perform(then_block)
                || else_block.as_ref().is_some_and(handler_uses_perform)
        }
        Statement::Match { scrutinee, arms } => {
            expr_has_perform(scrutinee) || arms.iter().any(|arm| handler_uses_perform(&arm.body))
        }
        Statement::Send { message, .. } => expr_has_perform(message),
        Statement::Spawn { args, .. } | Statement::Goto { args, .. } => {
            args.iter().any(expr_has_perform)
        }
    })
}

/// Whether a handler body uses any `spawn` statements.
pub fn handler_uses_spawn(block: &Block) -> bool {
    block.statements.iter().any(|stmt| match stmt {
        Statement::Spawn { .. } => true,
        Statement::If {
            then_block,
            else_block,
            ..
        } => handler_uses_spawn(then_block) || else_block.as_ref().is_some_and(handler_uses_spawn),
        Statement::Match { arms, .. } => arms.iter().any(|arm| handler_uses_spawn(&arm.body)),
        _ => false,
    })
}

/// Collect distinct channel names referenced by `send` statements in a block.
pub fn handler_used_channels(block: &Block) -> Vec<String> {
    let mut set = HashSet::new();
    collect_channels(block, &mut set);
    let mut out: Vec<String> = set.into_iter().collect();
    out.sort();
    out
}

/// Whether a handler body references `ctx` (used to detect implicit ctx access
/// when no explicit ctx parameter is declared).
pub fn handler_body_references_ctx(block: &Block) -> bool {
    block.statements.iter().any(stmt_references_ctx)
}

/// Whether any machine in the program has a timeout on a transition.
pub fn has_timeout_transition(program: &Program) -> bool {
    program
        .machines
        .iter()
        .flat_map(|m| &m.transitions)
        .any(|t| t.timeout.is_some())
}

// ---------------------------------------------------------------------------
// Known-type population (shared by both Rust and Go backends)
// ---------------------------------------------------------------------------

/// Builtin type names that both backends recognise as "not a ctx parameter".
const BUILTIN_TYPES: &[&str] = &[
    "String", "i64", "i32", "u64", "u32", "f64", "f32", "bool", "Vec", "Option", "Result",
];

/// Build the set of known type names from a program's type declarations plus
/// the language builtins.
pub fn collect_known_types(program: &Program) -> HashSet<String> {
    let mut set: HashSet<String> = program.types.iter().map(|t| t.name().to_string()).collect();
    for builtin in BUILTIN_TYPES {
        set.insert((*builtin).to_string());
    }
    set
}

// ---------------------------------------------------------------------------
// Ctx-param detection (shared by both backends)
// ---------------------------------------------------------------------------

/// Detect the ctx parameter name for a handler.
///
/// A ctx param is either:
/// 1. An explicit handler param whose type is not in `known_types`, or
/// 2. The implicit `"ctx"` keyword when the handler body references `ctx`.
pub fn detect_ctx_param(handler: &OnHandler, known_types: &HashSet<String>) -> Option<String> {
    let explicit = handler
        .params
        .iter()
        .find(|p| {
            let type_name = match &p.ty {
                TypeExpr::Simple(name) => name.as_str(),
                _ => return false,
            };
            !known_types.contains(type_name)
        })
        .map(|p| p.name.clone());
    if explicit.is_some() {
        return explicit;
    }
    if handler_body_references_ctx(&handler.body) {
        Some("ctx".to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// String case conversion
// ---------------------------------------------------------------------------

/// Convert a PascalCase or camelCase string to snake_case.
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

/// Convert a snake_case string to PascalCase.
pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Escape a source-language string for use inside Rust/Go style quoted literals.
pub fn escape_string_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            c if c.is_control() => escaped.push_str(&format!("\\x{:02X}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}

// ---------------------------------------------------------------------------
// Referenced-identifier collection (for unused-field detection)
// ---------------------------------------------------------------------------

/// Collect identifiers referenced in a handler body, accounting for ctx rewriting.
///
/// When `ctx_param` is `Some("ctx")`, expressions like `ctx.field` are rewritten
/// to just `field` during codegen, so this function extracts the immediate field
/// name from ctx field accesses instead of the `ctx` identifier itself.
pub fn collect_referenced_idents(block: &Block, ctx_param: Option<&str>) -> HashSet<String> {
    let mut set = HashSet::new();
    for stmt in &block.statements {
        collect_idents_stmt(stmt, ctx_param, &mut set);
    }
    set
}

fn collect_idents_stmt(stmt: &Statement, ctx_param: Option<&str>, set: &mut HashSet<String>) {
    match stmt {
        Statement::Let { value, .. } => collect_idents_expr(value, ctx_param, set),
        Statement::Return(expr) | Statement::Expr(expr) => {
            collect_idents_expr(expr, ctx_param, set);
        }
        Statement::Perform { args, .. }
        | Statement::Goto { args, .. }
        | Statement::Spawn { args, .. } => {
            for a in args {
                collect_idents_expr(a, ctx_param, set);
            }
        }
        Statement::Send { message, .. } => collect_idents_expr(message, ctx_param, set),
        Statement::If {
            condition,
            then_block,
            else_block,
        } => {
            collect_idents_expr(condition, ctx_param, set);
            for s in &then_block.statements {
                collect_idents_stmt(s, ctx_param, set);
            }
            if let Some(eb) = else_block {
                for s in &eb.statements {
                    collect_idents_stmt(s, ctx_param, set);
                }
            }
        }
        Statement::Match { scrutinee, arms } => {
            collect_idents_expr(scrutinee, ctx_param, set);
            for arm in arms {
                for s in &arm.body.statements {
                    collect_idents_stmt(s, ctx_param, set);
                }
            }
        }
    }
}

fn collect_idents_expr(expr: &Expr, ctx_param: Option<&str>, set: &mut HashSet<String>) {
    match expr {
        Expr::Ident(name) => {
            if ctx_param.is_none_or(|ctx| name != ctx) {
                set.insert(name.clone());
            }
        }
        Expr::FieldAccess(base, _field) => {
            if let Some(ctx) = ctx_param {
                if let Some(root_field) = extract_ctx_root_field(expr, ctx) {
                    set.insert(root_field);
                    return;
                }
            }
            collect_idents_expr(base, ctx_param, set);
        }
        Expr::FnCall(_, args) | Expr::Perform(_, args) => {
            for a in args {
                collect_idents_expr(a, ctx_param, set);
            }
        }
        Expr::BinOp(l, _, r) => {
            collect_idents_expr(l, ctx_param, set);
            collect_idents_expr(r, ctx_param, set);
        }
        Expr::UnaryOp(_, e) => collect_idents_expr(e, ctx_param, set),
        _ => {}
    }
}

/// For an expression like `ctx.config.name`, extract the root field after ctx (`"config"`).
fn extract_ctx_root_field(expr: &Expr, ctx_param: &str) -> Option<String> {
    match expr {
        Expr::FieldAccess(base, field) => {
            if let Expr::Ident(name) = base.as_ref() {
                if name == ctx_param {
                    return Some(field.clone());
                }
            }
            extract_ctx_root_field(base, ctx_param)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn collect_channels(block: &Block, set: &mut HashSet<String>) {
    for stmt in &block.statements {
        match stmt {
            Statement::Send { channel, .. } => {
                set.insert(channel.clone());
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                collect_channels(then_block, set);
                if let Some(else_block) = else_block {
                    collect_channels(else_block, set);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    collect_channels(&arm.body, set);
                }
            }
            _ => {}
        }
    }
}

fn stmt_references_ctx(stmt: &Statement) -> bool {
    match stmt {
        Statement::Let { value, .. } => expr_references_ctx(value),
        Statement::Return(expr) | Statement::Expr(expr) => expr_references_ctx(expr),
        Statement::Perform { args, .. }
        | Statement::Goto { args, .. }
        | Statement::Spawn { args, .. } => args.iter().any(expr_references_ctx),
        Statement::Send { message, .. } => expr_references_ctx(message),
        Statement::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_references_ctx(condition)
                || handler_body_references_ctx(then_block)
                || else_block.as_ref().is_some_and(handler_body_references_ctx)
        }
        Statement::Match { scrutinee, arms } => {
            expr_references_ctx(scrutinee)
                || arms
                    .iter()
                    .any(|arm| handler_body_references_ctx(&arm.body))
        }
    }
}

pub fn expr_references_ctx(expr: &Expr) -> bool {
    match expr {
        Expr::Ident(name) => name == "ctx",
        Expr::FieldAccess(base, _) => expr_references_ctx(base),
        Expr::FnCall(_, args) | Expr::Perform(_, args) => args.iter().any(expr_references_ctx),
        Expr::BinOp(l, _, r) => expr_references_ctx(l) || expr_references_ctx(r),
        Expr::UnaryOp(_, e) => expr_references_ctx(e),
        _ => false,
    }
}

fn expr_has_perform(expr: &Expr) -> bool {
    match expr {
        Expr::Perform(_, _) => true,
        Expr::BinOp(l, _, r) => expr_has_perform(l) || expr_has_perform(r),
        Expr::UnaryOp(_, e) => expr_has_perform(e),
        Expr::FnCall(_, args) => args.iter().any(expr_has_perform),
        Expr::FieldAccess(base, _) => expr_has_perform(base),
        _ => false,
    }
}
