//! PEG parser for the Gust language.
//!
//! Uses [pest](https://pest.rs) with the grammar defined in `grammar.pest`.
//! The public entry points are [`parse_program`] (returns a plain `String`
//! error) and [`parse_program_with_errors`] (returns a structured
//! [`GustError`]).
//!
//! Internally, each PEG rule has a corresponding `parse_*` function that
//! converts pest `Pair` nodes into strongly-typed [`crate::ast`] nodes.

use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;
use std::cell::RefCell;
use strsim::levenshtein;

use crate::ast::*;
use crate::error::GustError;

/// The pest-generated PEG parser, driven by `grammar.pest`.
#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct GustParser;

thread_local! {
    static PARSE_RECOVERY_ERRORS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Parse a `.gu` source string into a [`Program`] AST.
///
/// Returns the parsed AST on success, or a human-readable error message on
/// failure. For structured error diagnostics with source locations, use
/// [`parse_program_with_errors`] instead.
///
/// # Examples
///
/// ```rust
/// use gust_lang::parse_program;
///
/// let ast = parse_program("machine Foo { state A }").unwrap();
/// assert_eq!(ast.machines[0].name, "Foo");
/// ```
///
/// # Errors
///
/// Returns `Err(String)` when the source does not conform to the Gust
/// grammar, or when numeric literals are out of range.
pub fn parse_program(source: &str) -> Result<Program, String> {
    parse_program_inner(source).map_err(|e| format!("Parse error: {e}"))
}

/// Parse a `.gu` source string, returning a structured [`GustError`] on failure.
///
/// This variant includes the file path in the error and attempts to suggest
/// corrections for misspelled keywords using Levenshtein distance.
///
/// # Errors
///
/// Returns `Err(GustError)` with file, line, column, and a helpful message
/// when parsing fails.
pub fn parse_program_with_errors(source: &str, file: &str) -> Result<Program, GustError> {
    parse_program_inner(source)
        .map_err(|e| to_gust_error(source, file, &format!("Parse error: {e}")))
}

fn parse_program_inner(source: &str) -> Result<Program, String> {
    clear_parse_recovery_errors();

    let pairs = GustParser::parse(Rule::program, source).map_err(|e| e.to_string())?;

    let mut program = Program {
        uses: vec![],
        types: vec![],
        channels: vec![],
        machines: vec![],
    };

    for pair in pairs {
        if pair.as_rule() == Rule::program {
            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::use_decl => program.uses.push(parse_use_decl(inner)),
                    Rule::type_decl => program.types.push(parse_type_decl(inner)),
                    Rule::channel_decl => program.channels.push(parse_channel_decl(inner)),
                    Rule::machine_decl => program.machines.push(parse_machine_decl(inner)),
                    Rule::EOI => {}
                    _ => {}
                }
            }
        }
    }
    if let Some(error) = take_first_parse_recovery_error() {
        Err(error)
    } else {
        Ok(program)
    }
}

fn parse_channel_decl(pair: Pair<Rule>) -> ChannelDecl {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let message_type = parse_type_expr(inner.next().unwrap());
    let mut capacity = None;
    let mut mode = ChannelMode::Broadcast;

    if let Some(config) = inner.next() {
        for item in config.into_inner() {
            let actual = if item.as_rule() == Rule::channel_config_item {
                item.into_inner().next().unwrap()
            } else {
                item
            };

            match actual.as_rule() {
                Rule::capacity_item => {
                    let value_pair = actual.into_inner().next().unwrap();
                    let value = parse_i64_or_record(&value_pair, "channel capacity");
                    capacity = Some(value);
                }
                Rule::mode_item => {
                    let val = actual.into_inner().next().unwrap().as_str();
                    mode = match val {
                        "mpsc" => ChannelMode::Mpsc,
                        _ => ChannelMode::Broadcast,
                    };
                }
                _ => {}
            }
        }
    }

    ChannelDecl {
        name,
        message_type,
        capacity,
        mode,
    }
}

fn to_gust_error(source: &str, file: &str, text: &str) -> GustError {
    let (line, col) = extract_line_col(text);
    let is_grammar_error = text.contains("expected");
    let ident = if is_grammar_error {
        extract_ident_at(source, line, col)
    } else {
        None
    };
    let help = if is_grammar_error {
        ident
            .as_deref()
            .and_then(suggest_keyword)
            .map(|s| format!("did you mean '{}'?", s))
    } else {
        None
    };
    GustError {
        file: file.to_string(),
        line,
        col,
        message: ident
            .map(|i| format!("unexpected identifier '{}'", i))
            .unwrap_or_else(|| text.to_string()),
        note: None,
        help,
    }
}

fn extract_line_col(text: &str) -> (usize, usize) {
    if let Some(marker) = text.find("-->") {
        let tail = &text[marker + 3..];
        let mut digits = String::new();
        for ch in tail.chars() {
            if ch.is_ascii_digit() || ch == ':' {
                digits.push(ch);
            } else if !digits.is_empty() {
                break;
            }
        }
        let mut parts = digits.split(':');
        let line = parts
            .next()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(1);
        let col = parts
            .next()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(1);
        (line, col)
    } else {
        (1, 1)
    }
}

fn extract_ident_at(source: &str, line: usize, col: usize) -> Option<String> {
    let line_text = source.lines().nth(line.saturating_sub(1))?;
    let start = col.saturating_sub(1).min(line_text.len());
    let chars: Vec<char> = line_text.chars().collect();
    let mut i = start;
    while i < chars.len() && !chars[i].is_ascii_alphabetic() && chars[i] != '_' {
        i += 1;
    }
    let mut j = i;
    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
        j += 1;
    }
    if i < j {
        Some(chars[i..j].iter().collect())
    } else {
        None
    }
}

fn suggest_keyword(word: &str) -> Option<&'static str> {
    const KEYWORDS: &[&str] = &[
        "use",
        "type",
        "enum",
        "machine",
        "state",
        "transition",
        "effect",
        "on",
        "if",
        "else",
        "match",
        "return",
        "let",
        "goto",
        "perform",
        "async",
        "channel",
        "send",
        "spawn",
        "timeout",
        "sends",
        "receives",
        "supervises",
    ];
    KEYWORDS
        .iter()
        .filter_map(|k| {
            let d = levenshtein(word, k);
            if d <= 2 {
                Some((d, *k))
            } else {
                None
            }
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

fn parse_use_decl(pair: Pair<Rule>) -> UsePath {
    let path_pair = pair.into_inner().next().unwrap();
    let segments: Vec<String> = path_pair
        .into_inner()
        .map(|p| p.as_str().to_string())
        .collect();
    UsePath { segments }
}

fn parse_type_decl(pair: Pair<Rule>) -> TypeDecl {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::struct_decl => parse_struct_decl(inner),
        Rule::enum_decl => parse_enum_decl(inner),
        _ => unreachable!("unexpected type_decl rule: {:?}", inner.as_rule()),
    }
}

fn parse_struct_decl(pair: Pair<Rule>) -> TypeDecl {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let fields = parse_field_list(inner.next().unwrap());
    TypeDecl::Struct { name, fields }
}

fn parse_enum_decl(pair: Pair<Rule>) -> TypeDecl {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let variants_pair = inner.next().unwrap();
    let variants = variants_pair.into_inner().map(parse_variant).collect();
    TypeDecl::Enum { name, variants }
}

fn parse_variant(pair: Pair<Rule>) -> EnumVariant {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let payload = inner.map(parse_type_expr).collect();
    EnumVariant { name, payload }
}

fn parse_field_list(pair: Pair<Rule>) -> Vec<Field> {
    pair.into_inner().map(|p| parse_field(p)).collect()
}

fn parse_field(pair: Pair<Rule>) -> Field {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let ty = parse_type_expr(inner.next().unwrap());
    Field { name, ty }
}

fn parse_type_expr(pair: Pair<Rule>) -> TypeExpr {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::unit_type => TypeExpr::Unit,
        Rule::simple_type => {
            let name = inner.into_inner().next().unwrap().as_str().to_string();
            TypeExpr::Simple(name)
        }
        Rule::generic_type => {
            let mut parts = inner.into_inner();
            let name = parts.next().unwrap().as_str().to_string();
            let type_args: Vec<TypeExpr> = parts.map(|p| parse_type_expr(p)).collect();
            TypeExpr::Generic(name, type_args)
        }
        Rule::tuple_type => {
            let members = inner.into_inner().map(parse_type_expr).collect();
            TypeExpr::Tuple(members)
        }
        _ => unreachable!("unexpected type_expr rule: {:?}", inner.as_rule()),
    }
}

fn parse_machine_decl(pair: Pair<Rule>) -> MachineDecl {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let mut generic_params = Vec::new();
    let mut annotations_pair = None;
    let mut body = None;

    for item in inner {
        match item.as_rule() {
            Rule::generic_params => generic_params = parse_generic_params(item),
            Rule::machine_annotations => annotations_pair = Some(item),
            Rule::machine_body => body = Some(item),
            _ => {}
        }
    }
    let body = body.expect("machine body expected");

    let mut machine = MachineDecl {
        name,
        generic_params,
        sends: vec![],
        receives: vec![],
        supervises: vec![],
        states: vec![],
        transitions: vec![],
        handlers: vec![],
        effects: vec![],
    };

    if let Some(annotations) = annotations_pair {
        for ann in annotations.into_inner() {
            let ann = if ann.as_rule() == Rule::machine_annotation {
                ann.into_inner().next().unwrap()
            } else {
                ann
            };
            match ann.as_rule() {
                Rule::sends_annotation => {
                    let ch = ann.into_inner().next().unwrap().as_str().to_string();
                    machine.sends.push(ch);
                }
                Rule::receives_annotation => {
                    let ch = ann.into_inner().next().unwrap().as_str().to_string();
                    machine.receives.push(ch);
                }
                Rule::supervises_annotation => {
                    let mut parts = ann.into_inner();
                    let child = parts.next().unwrap().as_str().to_string();
                    let strategy = parts
                        .next()
                        .map(|p| match p.as_str() {
                            "one_for_all" => SupervisionStrategy::OneForAll,
                            "rest_for_one" => SupervisionStrategy::RestForOne,
                            _ => SupervisionStrategy::OneForOne,
                        })
                        .unwrap_or(SupervisionStrategy::OneForOne);
                    machine.supervises.push(SupervisionSpec {
                        child_machine: child,
                        strategy,
                    });
                }
                _ => {}
            }
        }
    }

    for item in body.into_inner() {
        // machine_item is a wrapper, get the actual item inside
        let actual_item = if item.as_rule() == Rule::machine_item {
            item.into_inner().next().unwrap()
        } else {
            item
        };

        match actual_item.as_rule() {
            Rule::state_decl => machine.states.push(parse_state_decl(actual_item)),
            Rule::transition_decl => machine.transitions.push(parse_transition_decl(actual_item)),
            Rule::on_handler => machine.handlers.push(parse_on_handler(actual_item)),
            Rule::effect_decl => machine.effects.push(parse_effect_decl(actual_item)),
            _ => {}
        }
    }

    machine
}

fn parse_generic_params(pair: Pair<Rule>) -> Vec<GenericParam> {
    pair.into_inner()
        .map(|param| {
            let mut inner = param.into_inner();
            let name = inner.next().unwrap().as_str().to_string();
            let bounds = inner
                .next()
                .map(|bounds_pair| {
                    bounds_pair
                        .into_inner()
                        .map(|b| b.as_str().to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            GenericParam { name, bounds }
        })
        .collect()
}

fn parse_state_decl(pair: Pair<Rule>) -> StateDecl {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let fields = inner
        .next()
        .map(|p| parse_field_list(p))
        .unwrap_or_default();
    StateDecl { name, fields }
}

fn parse_transition_decl(pair: Pair<Rule>) -> TransitionDecl {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let from = inner.next().unwrap().as_str().to_string();
    let targets_pair = inner.next().unwrap();
    let targets: Vec<String> = targets_pair
        .into_inner()
        .map(|p| p.as_str().to_string())
        .collect();
    let timeout = inner.next().map(parse_timeout_spec);
    TransitionDecl {
        name,
        from,
        targets,
        timeout,
    }
}

fn parse_timeout_spec(pair: Pair<Rule>) -> DurationSpec {
    let duration = pair.into_inner().next().unwrap();
    let mut parts = duration.into_inner();
    let value_pair = parts.next().unwrap();
    let value = parse_i64_or_record(&value_pair, "timeout duration");
    let unit = match parts.next().unwrap().as_str() {
        "ms" => TimeUnit::Millis,
        "m" => TimeUnit::Minutes,
        "h" => TimeUnit::Hours,
        _ => TimeUnit::Seconds,
    };
    DurationSpec { value, unit }
}

fn parse_effect_decl(pair: Pair<Rule>) -> EffectDecl {
    let mut inner = pair.into_inner();
    let mut is_async = false;
    if let Some(next) = inner.peek() {
        if next.as_rule() == Rule::async_modifier {
            is_async = true;
            inner.next();
        }
    }
    let name = inner.next().unwrap().as_str().to_string();
    let params = parse_field_list(inner.next().unwrap());
    let return_type = parse_type_expr(inner.next().unwrap());
    EffectDecl {
        name,
        params,
        return_type,
        is_async,
    }
}

fn parse_on_handler(pair: Pair<Rule>) -> OnHandler {
    let mut inner = pair.into_inner();
    let mut is_async = false;
    if let Some(next) = inner.peek() {
        if next.as_rule() == Rule::async_modifier {
            is_async = true;
            inner.next();
        }
    }
    let transition_name = inner.next().unwrap().as_str().to_string();
    let params = parse_param_list(inner.next().unwrap());

    // Check if next is a return type or a block
    let next = inner.next().unwrap();
    let (return_type, body) = match next.as_rule() {
        Rule::type_expr => {
            let rt = Some(parse_type_expr(next));
            let b = parse_block(inner.next().unwrap());
            (rt, b)
        }
        Rule::block => (None, parse_block(next)),
        _ => unreachable!(),
    };

    OnHandler {
        transition_name,
        params,
        return_type,
        body,
        is_async,
    }
}

fn parse_param_list(pair: Pair<Rule>) -> Vec<Param> {
    pair.into_inner().map(|p| parse_param(p)).collect()
}

fn parse_param(pair: Pair<Rule>) -> Param {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let ty = parse_type_expr(inner.next().unwrap());
    Param { name, ty }
}

fn parse_block(pair: Pair<Rule>) -> Block {
    let statements = pair.into_inner().map(|p| parse_statement(p)).collect();
    Block { statements }
}

fn parse_statement(pair: Pair<Rule>) -> Statement {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::let_stmt => {
            let mut parts = inner.into_inner();
            let name = parts.next().unwrap().as_str().to_string();
            // Could be type_expr or expr next
            let next = parts.next().unwrap();
            let (ty, value) = match next.as_rule() {
                Rule::type_expr => {
                    let t = Some(parse_type_expr(next));
                    let v = parse_expr(parts.next().unwrap());
                    (t, v)
                }
                Rule::expr => (None, parse_expr(next)),
                _ => unreachable!(),
            };
            Statement::Let { name, ty, value }
        }
        Rule::return_stmt => {
            let expr = parse_expr(inner.into_inner().next().unwrap());
            Statement::Return(expr)
        }
        Rule::if_stmt => parse_if_stmt(inner),
        Rule::match_stmt => parse_match_stmt(inner),
        Rule::transition_stmt => {
            let mut parts = inner.into_inner();
            let state = parts.next().unwrap().as_str().to_string();
            let args = parts.next().map(|p| parse_expr_list(p)).unwrap_or_default();
            Statement::Goto { state, args }
        }
        Rule::effect_stmt => {
            let mut parts = inner.into_inner();
            let effect = parts.next().unwrap().as_str().to_string();
            let args = parse_expr_list(parts.next().unwrap());
            Statement::Perform { effect, args }
        }
        Rule::send_stmt => {
            let mut parts = inner.into_inner();
            let channel = parts.next().unwrap().as_str().to_string();
            let message = parse_expr(parts.next().unwrap());
            Statement::Send { channel, message }
        }
        Rule::spawn_stmt => {
            let mut parts = inner.into_inner();
            let machine = parts.next().unwrap().as_str().to_string();
            let args = parse_expr_list(parts.next().unwrap());
            Statement::Spawn { machine, args }
        }
        Rule::expr_stmt => {
            let expr = parse_expr(inner.into_inner().next().unwrap());
            Statement::Expr(expr)
        }
        _ => unreachable!("unexpected statement rule: {:?}", inner.as_rule()),
    }
}

fn parse_match_stmt(pair: Pair<Rule>) -> Statement {
    let mut inner = pair.into_inner();
    let scrutinee = parse_expr(inner.next().unwrap());
    let arms = inner
        .map(|arm| {
            let mut parts = arm.into_inner();
            let pattern = parse_pattern(parts.next().unwrap());
            let body = parse_block(parts.next().unwrap());
            MatchArm { pattern, body }
        })
        .collect();
    Statement::Match { scrutinee, arms }
}

fn parse_pattern(pair: Pair<Rule>) -> Pattern {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::wildcard_pattern => Pattern::Wildcard,
        Rule::variant_pattern => {
            let mut enum_name = None;
            let mut items = inner.into_inner();
            let first = items.next().unwrap().as_str().to_string();
            let second = items.next();

            let (variant, mut bindings) = match second {
                Some(p) if p.as_rule() == Rule::ident => {
                    enum_name = Some(first);
                    (p.as_str().to_string(), Vec::new())
                }
                Some(p) if p.as_rule() == Rule::ident_list => (
                    first,
                    p.into_inner().map(|id| id.as_str().to_string()).collect(),
                ),
                Some(_) => unreachable!(),
                None => (first, Vec::new()),
            };

            if let Some(next) = items.next() {
                if next.as_rule() == Rule::ident_list {
                    bindings = next
                        .into_inner()
                        .map(|id| id.as_str().to_string())
                        .collect();
                }
            }

            if enum_name.is_none() && bindings.is_empty() {
                Pattern::Ident(variant)
            } else {
                Pattern::Variant {
                    enum_name,
                    variant,
                    bindings,
                }
            }
        }
        _ => unreachable!("unexpected pattern rule: {:?}", inner.as_rule()),
    }
}

fn parse_if_stmt(pair: Pair<Rule>) -> Statement {
    let mut inner = pair.into_inner();
    let condition = parse_expr(inner.next().unwrap());
    let then_block = parse_block(inner.next().unwrap());
    let else_block = inner.next().map(|p| match p.as_rule() {
        Rule::block => parse_block(p),
        Rule::if_stmt => Block {
            statements: vec![parse_if_stmt(p)],
        },
        _ => unreachable!(),
    });
    Statement::If {
        condition,
        then_block,
        else_block,
    }
}

fn parse_expr(pair: Pair<Rule>) -> Expr {
    parse_or_expr(pair.into_inner().next().unwrap())
}

fn parse_or_expr(pair: Pair<Rule>) -> Expr {
    let mut inner = pair.into_inner();
    let mut left = parse_and_expr(inner.next().unwrap());
    for right_pair in inner {
        let right = parse_and_expr(right_pair);
        left = Expr::BinOp(Box::new(left), BinOp::Or, Box::new(right));
    }
    left
}

fn parse_and_expr(pair: Pair<Rule>) -> Expr {
    let mut inner = pair.into_inner();
    let mut left = parse_cmp_expr(inner.next().unwrap());
    for right_pair in inner {
        let right = parse_cmp_expr(right_pair);
        left = Expr::BinOp(Box::new(left), BinOp::And, Box::new(right));
    }
    left
}

fn parse_cmp_expr(pair: Pair<Rule>) -> Expr {
    let mut inner = pair.into_inner();
    let left = parse_add_expr(inner.next().unwrap());
    if let Some(op_pair) = inner.next() {
        let op = match op_pair.as_str() {
            "==" => BinOp::Eq,
            "!=" => BinOp::Neq,
            "<" => BinOp::Lt,
            "<=" => BinOp::Lte,
            ">" => BinOp::Gt,
            ">=" => BinOp::Gte,
            _ => unreachable!(),
        };
        let right = parse_add_expr(inner.next().unwrap());
        Expr::BinOp(Box::new(left), op, Box::new(right))
    } else {
        left
    }
}

fn parse_add_expr(pair: Pair<Rule>) -> Expr {
    let mut inner = pair.into_inner();
    let mut left = parse_mul_expr(inner.next().unwrap());
    while let Some(op_pair) = inner.next() {
        let op = match op_pair.as_str() {
            "+" => BinOp::Add,
            "-" => BinOp::Sub,
            _ => unreachable!(),
        };
        let right = parse_mul_expr(inner.next().unwrap());
        left = Expr::BinOp(Box::new(left), op, Box::new(right));
    }
    left
}

fn parse_mul_expr(pair: Pair<Rule>) -> Expr {
    let mut inner = pair.into_inner();
    let mut left = parse_unary_expr(inner.next().unwrap());
    while let Some(op_pair) = inner.next() {
        let op = match op_pair.as_str() {
            "*" => BinOp::Mul,
            "/" => BinOp::Div,
            "%" => BinOp::Mod,
            _ => unreachable!(),
        };
        let right = parse_unary_expr(inner.next().unwrap());
        left = Expr::BinOp(Box::new(left), op, Box::new(right));
    }
    left
}

fn parse_unary_expr(pair: Pair<Rule>) -> Expr {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    match first.as_rule() {
        Rule::unary_op => {
            let op = match first.as_str() {
                "!" => UnaryOp::Not,
                "-" => UnaryOp::Neg,
                _ => unreachable!(),
            };
            let expr = parse_primary(inner.next().unwrap());
            Expr::UnaryOp(op, Box::new(expr))
        }
        Rule::primary => parse_primary(first),
        _ => unreachable!(),
    }
}

fn parse_primary(pair: Pair<Rule>) -> Expr {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::literal => parse_literal(inner),
        Rule::perform_expr => {
            let mut parts = inner.into_inner();
            let name = parts.next().unwrap().as_str().to_string();
            let args = parts.next().map(|p| parse_expr_list(p)).unwrap_or_default();
            Expr::Perform(name, args)
        }
        Rule::qualified_path => {
            let mut parts = inner.into_inner();
            let enum_name = parts.next().unwrap().as_str().to_string();
            let variant = parts.next().unwrap().as_str().to_string();
            Expr::Path(enum_name, variant)
        }
        Rule::fn_call => {
            let mut parts = inner.into_inner();
            let name = parts.next().unwrap().as_str().to_string();
            let args = parts.next().map(|p| parse_expr_list(p)).unwrap_or_default();
            Expr::FnCall(name, args)
        }
        Rule::field_access => {
            let mut parts = inner.into_inner();
            let base = parts.next().unwrap().as_str().to_string();
            let mut expr = Expr::Ident(base);
            for field_part in parts {
                let field_name = field_part.as_str().to_string();
                expr = Expr::FieldAccess(Box::new(expr), field_name);
            }
            expr
        }
        Rule::ident_expr => {
            let name = inner.into_inner().next().unwrap().as_str().to_string();
            Expr::Ident(name)
        }
        Rule::expr => parse_expr(inner),
        _ => unreachable!("unexpected primary rule: {:?}", inner.as_rule()),
    }
}

fn parse_literal(pair: Pair<Rule>) -> Expr {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::int_lit => Expr::IntLit(parse_i64_or_record(&inner, "integer")),
        Rule::float_lit => Expr::FloatLit(parse_f64_or_record(&inner, "float")),
        Rule::string_lit => {
            let s = inner.as_str();
            // Strip surrounding quotes
            Expr::StringLit(s[1..s.len() - 1].to_string())
        }
        Rule::bool_lit => Expr::BoolLit(inner.as_str() == "true"),
        _ => unreachable!(),
    }
}

fn clear_parse_recovery_errors() {
    PARSE_RECOVERY_ERRORS.with(|errors| errors.borrow_mut().clear());
}

fn take_first_parse_recovery_error() -> Option<String> {
    PARSE_RECOVERY_ERRORS.with(|errors| errors.borrow_mut().drain(..).next())
}

fn record_parse_recovery_error(pair: &Pair<Rule>, message: String) {
    let (line, col) = pair.as_span().start_pos().line_col();
    let diagnostic = format!("{message}\n  --> {line}:{col}");
    PARSE_RECOVERY_ERRORS.with(|errors| errors.borrow_mut().push(diagnostic));
}

fn parse_i64_or_record(pair: &Pair<Rule>, context: &str) -> i64 {
    let raw = pair.as_str();
    match raw.parse::<i64>() {
        Ok(value) => value,
        Err(_) => {
            record_parse_recovery_error(
                pair,
                format!("{context} literal '{raw}' is out of range for i64"),
            );
            0
        }
    }
}

fn parse_f64_or_record(pair: &Pair<Rule>, context: &str) -> f64 {
    let raw = pair.as_str();
    match raw.parse::<f64>() {
        Ok(value) => value,
        Err(_) => {
            record_parse_recovery_error(pair, format!("invalid {context} literal '{raw}'"));
            0.0
        }
    }
}

fn parse_expr_list(pair: Pair<Rule>) -> Vec<Expr> {
    pair.into_inner().map(|p| parse_expr(p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_machine() {
        let source = r#"
            machine Counter {
                state Idle(count: i64)
                state Running(count: i64)

                transition start: Idle -> Running
                transition stop: Running -> Idle

                on start(ctx: Context) {
                    goto Running(0);
                }

                on stop(ctx: Context) {
                    goto Idle(ctx.count);
                }
            }
        "#;

        let program = parse_program(source).expect("should parse");
        assert_eq!(program.machines.len(), 1);

        let machine = &program.machines[0];
        assert_eq!(machine.name, "Counter");
        assert_eq!(machine.states.len(), 2);
        assert_eq!(machine.transitions.len(), 2);
        assert_eq!(machine.handlers.len(), 2);
    }
}
