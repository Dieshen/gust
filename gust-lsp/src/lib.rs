#![warn(missing_docs)]
//! Gust LSP helper functions.
//!
//! These pure functions are extracted from the LSP server so they can be
//! unit-tested independently of tower-lsp plumbing.

use gust_lang::ast::{MachineDecl, TypeExpr};
use gust_lang::{parse_program_with_errors, validate_program};
use std::collections::HashSet;

// Re-export gust_lang types for convenience in tests
pub use gust_lang;

// ── Token / position helpers ──────────────────────────────────────────

/// Extract the identifier token surrounding the given byte-column in `line`.
pub fn token_at_col(line: &str, col: usize) -> Option<String> {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut start = col.min(bytes.len().saturating_sub(1));
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = col.min(bytes.len());
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }
    if start < end {
        Some(line[start..end].to_string())
    } else {
        None
    }
}

/// Return the first contiguous identifier at the beginning of `s`.
pub fn first_ident(s: &str) -> Option<&str> {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some(&s[..end])
    }
}

/// Returns true if `b` is a valid identifier character (alphanumeric or underscore).
pub fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ── Display helpers ───────────────────────────────────────────────────

/// Format a [`TypeExpr`] as the human-readable string shown in hover tooltips.
pub fn type_expr_label(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Unit => "()".to_string(),
        TypeExpr::Simple(name) => name.clone(),
        TypeExpr::Generic(name, args) => {
            let args = args
                .iter()
                .map(type_expr_label)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name}<{args}>")
        }
        TypeExpr::Tuple(items) => {
            let items = items
                .iter()
                .map(type_expr_label)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({items})")
        }
    }
}

// ── Hover helpers ─────────────────────────────────────────────────────

/// Build a markdown hover string from a signature line and optional doc comment.
pub fn make_hover_content(signature: &str, doc: &str) -> String {
    if doc.is_empty() {
        format!("```gust\n{signature}\n```")
    } else {
        format!("{doc}\n\n---\n\n```gust\n{signature}\n```")
    }
}

// ── Doc comment helpers ───────────────────────────────────────────────

/// Given the source text and a declaration name, find the declaration line and
/// walk backwards collecting contiguous `//` comment lines above it.
/// Returns the doc comment as a single string (lines joined by `\n`), or empty.
pub fn collect_doc_comments(text: &str, decl_name: &str) -> String {
    let patterns = [
        format!("state {decl_name}"),
        format!("effect {decl_name}"),
        format!("async effect {decl_name}"),
        format!("transition {decl_name}"),
        format!("type {decl_name}"),
        format!("struct {decl_name}"),
        format!("enum {decl_name}"),
    ];

    let lines: Vec<&str> = text.lines().collect();
    let decl_line = lines.iter().enumerate().find(|(_, l)| {
        let trimmed = l.trim_start();
        patterns.iter().any(|p| trimmed.starts_with(p.as_str()))
    });

    let Some((idx, _)) = decl_line else {
        return String::new();
    };

    let mut comment_lines = Vec::new();
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let trimmed = lines[i].trim();
        if let Some(content) = trimmed.strip_prefix("//") {
            comment_lines.push(content.trim().to_string());
        } else if trimmed.is_empty() {
            // Allow one blank line between comments and declaration
            continue;
        } else {
            break;
        }
    }

    comment_lines.reverse();
    if comment_lines.is_empty() {
        String::new()
    } else {
        comment_lines.join("  \n")
    }
}

// ── Source search helpers ─────────────────────────────────────────────

/// Returns a (start_line, start_col, end_line, end_col) tuple for the first
/// line that contains the given identifier as a declaration keyword prefix.
/// Falls back to (0,0,0,0) when not found.
pub fn find_decl_line(text: &str, name: &str) -> (u32, u32, u32, u32) {
    let patterns = [
        format!("machine {name}"),
        format!("state {name}"),
        format!("effect {name}"),
        format!("async effect {name}"),
        format!("transition {name}"),
        format!("struct {name}"),
        format!("enum {name}"),
        format!("type {name}"),
    ];
    for (idx, line) in text.lines().enumerate() {
        if patterns
            .iter()
            .any(|p| line.trim_start().starts_with(p.as_str()))
        {
            return (idx as u32, 0, idx as u32, line.len() as u32);
        }
    }
    (0, 0, 0, 0)
}

/// Returns the 0-based line index of the first line matching a prefix, or None.
pub fn find_line_index(text: &str, prefix: &str) -> Option<usize> {
    text.lines()
        .enumerate()
        .find(|(_, l)| l.trim_start().starts_with(prefix))
        .map(|(i, _)| i)
}

/// Returns all (line_index, col_start) pairs where `word` appears as a whole
/// word (surrounded by non-identifier characters or string boundaries).
pub fn find_all_word_occurrences(text: &str, word: &str) -> Vec<(usize, usize)> {
    let mut results = Vec::new();
    for (line_idx, line) in text.lines().enumerate() {
        let bytes = line.as_bytes();
        let word_bytes = word.as_bytes();
        let wlen = word_bytes.len();
        if wlen == 0 || wlen > bytes.len() {
            continue;
        }
        let mut col = 0usize;
        while col + wlen <= bytes.len() {
            if &bytes[col..col + wlen] == word_bytes {
                let before_ok = col == 0 || !is_ident_char(bytes[col - 1]);
                let after_ok = col + wlen == bytes.len() || !is_ident_char(bytes[col + wlen]);
                if before_ok && after_ok {
                    results.push((line_idx, col));
                }
            }
            col += 1;
        }
    }
    results
}

/// Finds the best line after which to insert a new `on` handler in a machine block.
/// Prefers after the last existing handler; falls back to after the last effect;
/// falls back to the machine declaration line itself.
pub fn find_handler_insert_line(text: &str, machine: &MachineDecl) -> usize {
    // Try last handler
    if let Some(last_handler) = machine.handlers.last() {
        if let Some(start) = find_line_index(text, &format!("on {}", last_handler.transition_name))
        {
            if let Some(end) = find_closing_brace_line(text, start) {
                return end + 1;
            }
        }
    }

    // Try after the last effect declaration
    if let Some(last_effect) = machine.effects.last() {
        let pattern = if last_effect.is_async {
            format!("async effect {}", last_effect.name)
        } else {
            format!("effect {}", last_effect.name)
        };
        if let Some(line) = find_line_index(text, &pattern) {
            return line + 1;
        }
    }

    // Fall back: machine declaration line + 1
    find_line_index(text, &format!("machine {}", machine.name))
        .map(|l| l + 1)
        .unwrap_or(0)
}

/// Scans forward from `start_line` to find the matching closing `}` at depth 0.
pub fn find_closing_brace_line(text: &str, start_line: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut found_open = false;
    for (idx, line) in text.lines().enumerate().skip(start_line) {
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    found_open = true;
                }
                '}' => {
                    depth -= 1;
                    if found_open && depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Finds the line in a handler body that contains `let <name> =`.
pub fn find_let_line(text: &str, var_name: &str) -> Option<usize> {
    let pattern = format!("let {var_name}");
    text.lines().enumerate().find_map(|(i, l)| {
        if l.trim_start().starts_with(pattern.as_str()) {
            Some(i)
        } else {
            None
        }
    })
}

/// Returns the column immediately after `name` in `line_text`, used to place
/// an inlay hint after the variable name in a `let` statement.
pub fn find_name_end_col(line_text: &str, name: &str) -> usize {
    let pattern = format!("let {name}");
    if let Some(pos) = line_text.find(pattern.as_str()) {
        return pos + "let ".len() + name.len();
    }
    0
}

/// Searches `prefix` (text on the current line up to the cursor) for the most
/// recent `perform <name>(` invocation that has not yet been closed.
/// Returns the effect name if found.
pub fn find_perform_effect_name(prefix: &str) -> Option<String> {
    let mut search = prefix;
    while let Some(pos) = search.rfind("perform ") {
        let after = &search[pos + "perform ".len()..];
        if let Some(name) = first_ident(after) {
            let after_name = &after[name.len()..];
            if after_name.trim_start().starts_with('(') {
                let paren_start = pos + "perform ".len() + name.len();
                let rest = &prefix[paren_start..];
                let mut depth: i32 = 0;
                for ch in rest.chars() {
                    match ch {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                }
                if depth > 0 {
                    return Some(name.to_string());
                }
            }
        }
        search = &search[..pos];
    }
    None
}

// ── Diagnostic helpers ────────────────────────────────────────────────

/// Diagnostic severity matching the LSP protocol values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagSeverity {
    /// Hard error — prevents compilation.
    Error,
    /// Advisory warning — does not prevent compilation.
    Warning,
}

/// A simplified diagnostic for testing without tower-lsp dependency.
#[derive(Debug, Clone)]
pub struct SimpleDiag {
    /// 0-based line number.
    pub line: u32,
    /// 0-based column number.
    pub col: u32,
    /// Severity classification.
    pub severity: DiagSeverity,
    /// Human-readable diagnostic text.
    pub message: String,
}

/// Generate diagnostics from Gust source text, mirroring the LSP's
/// `publish_diagnostics` logic.
pub fn diagnostics_from_source(text: &str, file_path: &str) -> Vec<SimpleDiag> {
    let mut diags = Vec::new();
    match parse_program_with_errors(text, file_path) {
        Err(err) => {
            let line = err.line.saturating_sub(1) as u32;
            let col = err.col.saturating_sub(1) as u32;
            diags.push(SimpleDiag {
                line,
                col,
                severity: DiagSeverity::Error,
                message: err.message,
            });
        }
        Ok(program) => {
            let report = validate_program(&program, file_path, text);
            for warning in report.warnings {
                let line = warning.line.saturating_sub(1) as u32;
                let col = warning.col.saturating_sub(1) as u32;
                diags.push(SimpleDiag {
                    line,
                    col,
                    severity: DiagSeverity::Warning,
                    message: warning.message,
                });
            }
            for error in report.errors {
                let line = error.line.saturating_sub(1) as u32;
                let col = error.col.saturating_sub(1) as u32;
                diags.push(SimpleDiag {
                    line,
                    col,
                    severity: DiagSeverity::Error,
                    message: error.message,
                });
            }
        }
    }
    diags
}

// ── Hover info extraction ─────────────────────────────────────────────

/// Extract hover information for a token at a given position in Gust source.
/// Returns `Some((signature, doc_comment))` or `None`.
pub fn hover_info(text: &str, line_idx: usize, col: usize) -> Option<(String, String)> {
    let line = text.lines().nth(line_idx).unwrap_or("");
    let token = token_at_col(line, col)?;

    let program = parse_program_with_errors(text, "test.gu").ok()?;

    for machine in &program.machines {
        // Check states
        if let Some(state) = machine.states.iter().find(|s| s.name == token) {
            let fields = if state.fields.is_empty() {
                "no fields".to_string()
            } else {
                state
                    .fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, type_expr_label(&f.ty)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let doc = collect_doc_comments(text, &state.name);
            let sig = format!("state {}({fields})", state.name);
            return Some((sig, doc));
        }

        // Check effects
        if let Some(effect) = machine.effects.iter().find(|e| e.name == token) {
            let params_str = effect
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, type_expr_label(&p.ty)))
                .collect::<Vec<_>>()
                .join(", ");
            let doc = collect_doc_comments(text, &effect.name);
            let sig = format!(
                "{}effect {}({}) -> {}",
                if effect.is_async { "async " } else { "" },
                effect.name,
                params_str,
                type_expr_label(&effect.return_type)
            );
            return Some((sig, doc));
        }

        // Check transitions
        if let Some(tr) = machine.transitions.iter().find(|t| t.name == token) {
            let targets = tr.targets.join(" | ");
            let timeout_str = match &tr.timeout {
                Some(d) => {
                    let unit = match d.unit {
                        gust_lang::ast::TimeUnit::Millis => "ms",
                        gust_lang::ast::TimeUnit::Seconds => "s",
                        gust_lang::ast::TimeUnit::Minutes => "m",
                        gust_lang::ast::TimeUnit::Hours => "h",
                    };
                    format!(" [timeout: {}{}]", d.value, unit)
                }
                None => String::new(),
            };
            let doc = collect_doc_comments(text, &tr.name);
            let sig = format!(
                "transition {}: {} -> {}{}",
                tr.name, tr.from, targets, timeout_str
            );
            return Some((sig, doc));
        }
    }

    // Check top-level type declarations
    for ty in &program.types {
        match ty {
            gust_lang::ast::TypeDecl::Struct { name, fields, .. } if name == &token => {
                let field_str = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, type_expr_label(&f.ty)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let doc = collect_doc_comments(text, name);
                let sig = format!("type {name} {{ {field_str} }}");
                return Some((sig, doc));
            }
            gust_lang::ast::TypeDecl::Enum { name, variants, .. } if name == &token => {
                let variant_str = variants
                    .iter()
                    .map(|v| {
                        if v.payload.is_empty() {
                            v.name.clone()
                        } else {
                            let payload = v
                                .payload
                                .iter()
                                .map(type_expr_label)
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("{}({})", v.name, payload)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let doc = collect_doc_comments(text, name);
                let sig = format!("enum {name} {{ {variant_str} }}");
                return Some((sig, doc));
            }
            _ => {}
        }
    }

    None
}

// ── Document symbol extraction ────────────────────────────────────────

/// Simplified symbol kind for testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleSymbolKind {
    /// A Gust `type` (struct) declaration.
    Struct,
    /// A Gust `enum` declaration.
    Enum,
    /// A Gust `machine` declaration.
    Class,
    /// A variant of an enum.
    EnumMember,
    /// A transition declaration.
    Event,
    /// A top-level function-like item (effect, action).
    Function,
    /// A state handler (`on` block).
    Method,
}

/// A simplified document symbol for testing.
#[derive(Debug, Clone)]
pub struct SimpleSymbol {
    /// Display name of the symbol (e.g. machine name, state name).
    pub name: String,
    /// Symbol classification for LSP SymbolKind mapping.
    pub kind: SimpleSymbolKind,
    /// Optional supplementary text (e.g. type hint, arity).
    pub detail: Option<String>,
    /// Nested symbols (states within a machine, variants within an enum).
    pub children: Vec<SimpleSymbol>,
}

/// Extract document symbols from Gust source text, mirroring the LSP's
/// `document_symbol` handler logic.
pub fn document_symbols(text: &str) -> Option<Vec<SimpleSymbol>> {
    let program = parse_program_with_errors(text, "test.gu").ok()?;
    let mut symbols = Vec::new();

    for ty in &program.types {
        let (name, kind) = match ty {
            gust_lang::ast::TypeDecl::Struct { name, .. } => {
                (name.as_str(), SimpleSymbolKind::Struct)
            }
            gust_lang::ast::TypeDecl::Enum { name, .. } => (name.as_str(), SimpleSymbolKind::Enum),
        };
        symbols.push(SimpleSymbol {
            name: name.to_string(),
            kind,
            detail: None,
            children: Vec::new(),
        });
    }

    for machine in &program.machines {
        let mut children = Vec::new();

        for state in &machine.states {
            children.push(SimpleSymbol {
                name: state.name.clone(),
                kind: SimpleSymbolKind::EnumMember,
                detail: Some(format!("{} field(s)", state.fields.len())),
                children: Vec::new(),
            });
        }

        for tr in &machine.transitions {
            let detail = format!("{} -> {}", tr.from, tr.targets.join(" | "));
            children.push(SimpleSymbol {
                name: tr.name.clone(),
                kind: SimpleSymbolKind::Event,
                detail: Some(detail),
                children: Vec::new(),
            });
        }

        for effect in &machine.effects {
            let params_str = effect
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, type_expr_label(&p.ty)))
                .collect::<Vec<_>>()
                .join(", ");
            let detail = format!(
                "{}({}) -> {}",
                if effect.is_async { "async " } else { "" },
                params_str,
                type_expr_label(&effect.return_type)
            );
            children.push(SimpleSymbol {
                name: effect.name.clone(),
                kind: SimpleSymbolKind::Function,
                detail: Some(detail),
                children: Vec::new(),
            });
        }

        for handler in &machine.handlers {
            children.push(SimpleSymbol {
                name: format!("on {}", handler.transition_name),
                kind: SimpleSymbolKind::Method,
                detail: None,
                children: Vec::new(),
            });
        }

        symbols.push(SimpleSymbol {
            name: machine.name.clone(),
            kind: SimpleSymbolKind::Class,
            detail: None,
            children,
        });
    }

    Some(symbols)
}

// ── Code action helpers ───────────────────────────────────────────────

/// Represents a code action stub for a missing handler.
#[derive(Debug, Clone)]
pub struct MissingHandlerAction {
    /// Human-readable code-action title shown in the editor UI.
    pub title: String,
    /// Name of the transition whose handler is missing.
    pub transition_name: String,
    /// Line to insert the stub at (0-based).
    pub insert_line: usize,
    /// Source text of the handler stub to insert.
    pub stub_text: String,
}

/// Find code actions (missing handler stubs) for a given cursor line.
pub fn code_actions_at(text: &str, cursor_line: u32) -> Vec<MissingHandlerAction> {
    let Ok(program) = parse_program_with_errors(text, "test.gu") else {
        return Vec::new();
    };

    let mut actions = Vec::new();

    for machine in &program.machines {
        let handled: HashSet<&str> = machine
            .handlers
            .iter()
            .map(|h| h.transition_name.as_str())
            .collect();

        for tr in &machine.transitions {
            if handled.contains(tr.name.as_str()) {
                continue;
            }

            let tr_line = find_line_index(text, &format!("transition {}", tr.name));
            let is_near_cursor = tr_line
                .map(|l| l as u32 == cursor_line || l as u32 + 1 == cursor_line)
                .unwrap_or(false);

            if !is_near_cursor {
                continue;
            }

            let insert_line = find_handler_insert_line(text, machine);

            let ctx_type = format!("{}Ctx", tr.from);
            let stub = format!(
                "\n    on {}(ctx: {}) {{\n        // TODO: handle {} transition\n        goto {};\n    }}\n",
                tr.name,
                ctx_type,
                tr.name,
                tr.targets.first().cloned().unwrap_or_else(|| tr.from.clone()),
            );

            actions.push(MissingHandlerAction {
                title: format!("Add handler for transition '{}'", tr.name),
                transition_name: tr.name.clone(),
                insert_line,
                stub_text: stub,
            });
        }
    }

    actions
}

// ── Inlay hint helpers ────────────────────────────────────────────────

/// Simplified inlay hint for testing.
#[derive(Debug, Clone)]
pub struct SimpleInlayHint {
    /// 0-based line position.
    pub line: u32,
    /// 0-based column position.
    pub col: u32,
    /// Text rendered inline in the editor.
    pub label: String,
}

/// Extract inlay hints from Gust source text, mirroring the LSP's
/// `inlay_hint` handler.
pub fn inlay_hints(text: &str) -> Vec<SimpleInlayHint> {
    let Ok(program) = parse_program_with_errors(text, "test.gu") else {
        return Vec::new();
    };

    let mut hints = Vec::new();

    for machine in &program.machines {
        for handler in &machine.handlers {
            for stmt in &handler.body.statements {
                if let gust_lang::ast::Statement::Let {
                    name,
                    ty: None,
                    value,
                } = stmt
                {
                    let effect_name = match value {
                        gust_lang::ast::Expr::Perform(name, _) => Some(name.as_str()),
                        _ => None,
                    };

                    let Some(effect_name) = effect_name else {
                        continue;
                    };

                    let return_type = machine
                        .effects
                        .iter()
                        .find(|e| e.name == effect_name)
                        .map(|e| type_expr_label(&e.return_type));

                    let Some(return_type) = return_type else {
                        continue;
                    };

                    let Some(line_idx) = find_let_line(text, name) else {
                        continue;
                    };

                    let line_text = text.lines().nth(line_idx).unwrap_or("");
                    let col = find_name_end_col(line_text, name);

                    hints.push(SimpleInlayHint {
                        line: line_idx as u32,
                        col: col as u32,
                        label: format!(": {return_type}"),
                    });
                }
            }
        }
    }

    hints
}

// ── Go-to-definition helper ───────────────────────────────────────────

/// Find the definition location of the token at the given position.
/// Returns `Some((line, col_start, col_end))` for the definition line.
pub fn goto_definition(text: &str, line_idx: usize, col: usize) -> Option<(u32, u32, u32)> {
    let line = text.lines().nth(line_idx).unwrap_or("");
    let token = token_at_col(line, col)?;

    for (idx, l) in text.lines().enumerate() {
        let starts = [
            format!("state {token}"),
            format!("effect {token}"),
            format!("async effect {token}"),
            format!("transition {token}"),
        ];
        if starts.iter().any(|s| l.trim_start().starts_with(s)) {
            return Some((idx as u32, 0, l.len() as u32));
        }
    }
    None
}

// ── Signature help helper ─────────────────────────────────────────────

/// Signature help result for testing.
#[derive(Debug, Clone)]
pub struct SimpleSignatureHelp {
    /// Complete signature string shown in the popup.
    pub label: String,
    /// Individual parameter labels (`name: Type`).
    pub parameters: Vec<String>,
    /// Index of the parameter currently highlighted by the cursor.
    pub active_parameter: Option<u32>,
}

/// Extract signature help for a perform call at the given position.
pub fn signature_help(text: &str, line_idx: usize, col: usize) -> Option<SimpleSignatureHelp> {
    let line = text.lines().nth(line_idx).unwrap_or("");
    let prefix = &line[..col.min(line.len())];

    let effect_name = find_perform_effect_name(prefix)?;

    let program = parse_program_with_errors(text, "test.gu").ok()?;

    let effect = program
        .machines
        .iter()
        .flat_map(|m| m.effects.iter())
        .find(|e| e.name == effect_name)?;

    let params_str = effect
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, type_expr_label(&p.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let label = format!(
        "{}{}({}) -> {}",
        if effect.is_async { "async " } else { "" },
        effect.name,
        params_str,
        type_expr_label(&effect.return_type)
    );

    let open_paren_pos = prefix.rfind(&format!("{}(", effect_name));
    let active_parameter = open_paren_pos.map(|p| {
        let after_paren = &prefix[p + effect_name.len() + 1..];
        let mut depth: i32 = 0;
        let mut commas: u32 = 0;
        for ch in after_paren.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => commas += 1,
                _ => {}
            }
        }
        commas
    });

    Some(SimpleSignatureHelp {
        label,
        parameters: effect
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, type_expr_label(&p.ty)))
            .collect(),
        active_parameter,
    })
}
