use crate::ast::{Block, Expr, Pattern, Program, StateDecl, Statement, TransitionDecl};
use crate::error::{GustError, GustWarning};
use std::collections::{HashMap, HashSet};
use strsim::levenshtein;

#[derive(Debug, Default, Clone)]
pub struct ValidationReport {
    pub errors: Vec<GustError>,
    pub warnings: Vec<GustWarning>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn validate_program(program: &Program, file: &str, source: &str) -> ValidationReport {
    let mut report = ValidationReport::default();
    let locator = SourceLocator::new(source);
    let declared_channels: HashSet<String> =
        program.channels.iter().map(|c| c.name.clone()).collect();
    let declared_channel_names: Vec<String> =
        program.channels.iter().map(|c| c.name.clone()).collect();
    let declared_machine_names: Vec<String> =
        program.machines.iter().map(|m| m.name.clone()).collect();
    let declared_machine_set: HashSet<String> = declared_machine_names.iter().cloned().collect();

    for machine in &program.machines {
        let state_names: Vec<String> = machine.states.iter().map(|s| s.name.clone()).collect();
        let state_set: HashSet<String> = state_names.iter().cloned().collect();
        let declared_effects: HashSet<String> =
            machine.effects.iter().map(|e| e.name.clone()).collect();
        let declared_effect_names: Vec<String> =
            machine.effects.iter().map(|e| e.name.clone()).collect();
        let state_fields: HashMap<&str, &StateDecl> = machine
            .states
            .iter()
            .map(|s| (s.name.as_str(), s))
            .collect();

        let mut seen_states = HashSet::new();
        for state in &machine.states {
            if !seen_states.insert(state.name.clone()) {
                let (line, col) = locator.find_state(&state.name);
                report.errors.push(GustError {
                    file: file.to_string(),
                    line,
                    col,
                    message: format!("duplicate state name '{}'", state.name),
                    note: Some("state names must be unique within a machine".to_string()),
                    help: None,
                });
            }
        }

        let mut seen_transitions = HashSet::new();
        for transition in &machine.transitions {
            if !seen_transitions.insert(transition.name.clone()) {
                let (line, col) = locator.find_transition(&transition.name);
                report.errors.push(GustError {
                    file: file.to_string(),
                    line,
                    col,
                    message: format!("duplicate transition name '{}'", transition.name),
                    note: Some("transition names must be unique within a machine".to_string()),
                    help: None,
                });
            }

            if !state_set.contains(&transition.from) {
                let (line, col) = locator.find_transition(&transition.name);
                report.errors.push(GustError {
                    file: file.to_string(),
                    line,
                    col,
                    message: format!("undefined state '{}' in transition source", transition.from),
                    note: Some(format!("declared states: {}", state_names.join(", "))),
                    help: suggest_name(&transition.from, &state_names),
                });
            }

            for target in &transition.targets {
                if !state_set.contains(target) {
                    let (line, col) = locator.find_transition_target(&transition.name, target);
                    report.errors.push(GustError {
                        file: file.to_string(),
                        line,
                        col,
                        message: format!("undefined state '{}' in transition target", target),
                        note: Some(format!("declared states: {}", state_names.join(", "))),
                        help: suggest_name(target, &state_names),
                    });
                }
            }
        }

        let mut incoming = HashMap::<String, usize>::new();
        for state in &machine.states {
            incoming.insert(state.name.clone(), 0);
        }
        for t in &machine.transitions {
            for target in &t.targets {
                if let Some(v) = incoming.get_mut(target) {
                    *v += 1;
                }
            }
        }
        if let Some(first) = machine.states.first() {
            incoming.remove(&first.name);
        }
        for (state, count) in incoming {
            if count == 0 {
                let (line, col) = locator.find_state(&state);
                report.warnings.push(GustWarning {
                    file: file.to_string(),
                    line,
                    col,
                    message: format!("unreachable state '{}'", state),
                    note: Some("no transitions lead to this state".to_string()),
                });
            }
        }

        // Task 1: warn on transitions that have no corresponding handler.
        let handled_transitions: HashSet<&str> = machine
            .handlers
            .iter()
            .map(|h| h.transition_name.as_str())
            .collect();
        for transition in &machine.transitions {
            if !handled_transitions.contains(transition.name.as_str()) {
                let (line, col) = locator.find_transition(&transition.name);
                report.warnings.push(GustWarning {
                    file: file.to_string(),
                    line,
                    col,
                    message: format!("transition '{}' has no handler", transition.name),
                    note: Some(format!(
                        "add an 'on {}(...)' handler for this transition",
                        transition.name
                    )),
                });
            }
        }

        // Build a map from transition name to its declared target states
        let transition_targets: HashMap<&str, &[String]> = machine
            .transitions
            .iter()
            .map(|t| (t.name.as_str(), t.targets.as_slice()))
            .collect();

        let mut used_declared_effects = HashSet::new();
        let mut unknown_effects = Vec::new();
        for handler in &machine.handlers {
            // Reject handler return types (not yet supported in codegen)
            if handler.return_type.is_some() {
                let (line, col) = locator.find_handler(&handler.transition_name);
                report.errors.push(GustError {
                    file: file.to_string(),
                    line,
                    col,
                    message: "handler return types are not yet supported".to_string(),
                    note: Some(format!(
                        "remove the return type from handler '{}'",
                        handler.transition_name
                    )),
                    help: None,
                });
            }

            // Reject bare `return` statements in handlers (codegen always uses Result<(), ...>)
            reject_return_in_block(
                &handler.body,
                &handler.transition_name,
                &locator,
                file,
                &mut report,
            );

            collect_effects_from_block(
                &handler.body,
                &declared_effects,
                &mut used_declared_effects,
                &mut unknown_effects,
            );
            validate_goto_arity(&handler.body, &state_fields, &locator, file, &mut report);

            // Validate that goto targets are declared targets of the transition
            if let Some(targets) = transition_targets.get(handler.transition_name.as_str()) {
                validate_goto_targets(
                    &handler.body,
                    &handler.transition_name,
                    targets,
                    &locator,
                    file,
                    &mut report,
                );
            }

            // Task 2: warn when a handler has code paths that don't end in a goto.
            if !block_always_terminates(&handler.body) {
                let (line, col) = locator.find_handler(&handler.transition_name);
                report.warnings.push(GustWarning {
                    file: file.to_string(),
                    line,
                    col,
                    message: format!(
                        "handler '{}' has code paths that don't end with a goto",
                        handler.transition_name
                    ),
                    note: Some("all handler paths should transition to a new state".to_string()),
                });
            }
            validate_send_targets(
                &handler.body,
                &declared_channels,
                &declared_channel_names,
                &locator,
                file,
                &mut report,
            );
            validate_spawn_targets(
                &handler.body,
                &declared_machine_set,
                &declared_machine_names,
                &locator,
                file,
                &mut report,
            );
            // Check that ctx.field references only access fields available in the from-state
            if let Some(transition) = machine
                .transitions
                .iter()
                .find(|t| t.name == handler.transition_name)
            {
                validate_ctx_field_access(
                    &handler.body,
                    transition,
                    &state_fields,
                    &locator,
                    file,
                    &mut report,
                );
            }
        }

        for effect in declared_effects {
            if !used_declared_effects.contains(&effect) {
                let (line, col) = locator.find_effect(&effect);
                report.warnings.push(GustWarning {
                    file: file.to_string(),
                    line,
                    col,
                    message: format!("unused effect '{}'", effect),
                    note: Some("effect is declared but never performed".to_string()),
                });
            }
        }

        for effect in unknown_effects {
            let (line, col) = locator.find_perform(&effect);
            report.errors.push(GustError {
                file: file.to_string(),
                line,
                col,
                message: format!("undeclared effect '{}'", effect),
                note: Some("effect is used but never declared in this machine".to_string()),
                help: suggest_name(&effect, &declared_effect_names),
            });
        }
    }

    report
}

/// Returns true when every code path through `block` ends with a `Goto` or `Return`.
/// Used to detect handlers that might fall through without transitioning to a new state.
fn block_always_terminates(block: &Block) -> bool {
    match block.statements.last() {
        None => false,
        Some(Statement::Goto { .. }) => true,
        Some(Statement::Return(_)) => true,
        Some(Statement::If {
            else_block: None, ..
        }) => false,
        Some(Statement::If {
            then_block,
            else_block: Some(else_block),
            ..
        }) => block_always_terminates(then_block) && block_always_terminates(else_block),
        Some(Statement::Match { arms, .. }) => {
            // Exhaustive only when at least one wildcard arm exists and every arm terminates.
            let has_wildcard = arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard));
            has_wildcard && arms.iter().all(|a| block_always_terminates(&a.body))
        }
        Some(_) => false,
    }
}

fn validate_goto_arity(
    block: &Block,
    states: &HashMap<&str, &StateDecl>,
    locator: &SourceLocator<'_>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Goto { state, args } => {
                if let Some(target) = states.get(state.as_str()) {
                    if target.fields.len() != args.len() {
                        let (line, col) = locator.find_goto(state);
                        report.errors.push(GustError {
                            file: file.to_string(),
                            line,
                            col,
                            message: format!(
                                "goto '{}' expects {} argument(s) but got {}",
                                state,
                                target.fields.len(),
                                args.len()
                            ),
                            note: Some(
                                "goto argument count must match target state fields".to_string(),
                            ),
                            help: None,
                        });
                    }
                }
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                validate_goto_arity(then_block, states, locator, file, report);
                if let Some(else_block) = else_block {
                    validate_goto_arity(else_block, states, locator, file, report);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    validate_goto_arity(&arm.body, states, locator, file, report);
                }
            }
            _ => {}
        }
    }
}

fn validate_goto_targets(
    block: &Block,
    transition_name: &str,
    valid_targets: &[String],
    locator: &SourceLocator<'_>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Goto { state, .. } => {
                if !valid_targets.iter().any(|t| t == state) {
                    let (line, col) = locator.find_goto(state);
                    let targets_list = valid_targets.join(", ");
                    report.errors.push(GustError {
                        file: file.to_string(),
                        line,
                        col,
                        message: format!(
                            "goto target '{}' is not a declared target of transition '{}'; valid targets are: {}",
                            state, transition_name, targets_list
                        ),
                        note: Some(format!(
                            "transition '{}' can only go to: {}",
                            transition_name, targets_list
                        )),
                        help: suggest_name(state, valid_targets),
                    });
                }
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                validate_goto_targets(
                    then_block,
                    transition_name,
                    valid_targets,
                    locator,
                    file,
                    report,
                );
                if let Some(else_block) = else_block {
                    validate_goto_targets(
                        else_block,
                        transition_name,
                        valid_targets,
                        locator,
                        file,
                        report,
                    );
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    validate_goto_targets(
                        &arm.body,
                        transition_name,
                        valid_targets,
                        locator,
                        file,
                        report,
                    );
                }
            }
            _ => {}
        }
    }
}

fn reject_return_in_block(
    block: &Block,
    handler_name: &str,
    locator: &SourceLocator<'_>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Return(_) => {
                let (line, col) = locator.find_handler(handler_name);
                report.errors.push(GustError {
                    file: file.to_string(),
                    line,
                    col,
                    message: "return statements are not supported in handlers; use goto to transition".to_string(),
                    note: Some(format!(
                        "handler '{}' contains a return statement, but codegen requires goto for state transitions",
                        handler_name
                    )),
                    help: None,
                });
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                reject_return_in_block(then_block, handler_name, locator, file, report);
                if let Some(else_block) = else_block {
                    reject_return_in_block(else_block, handler_name, locator, file, report);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    reject_return_in_block(&arm.body, handler_name, locator, file, report);
                }
            }
            _ => {}
        }
    }
}

fn collect_effects_from_block(
    block: &Block,
    declared: &HashSet<String>,
    used_declared: &mut HashSet<String>,
    unknown: &mut Vec<String>,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Perform { effect, .. } => {
                register_effect(effect, declared, used_declared, unknown)
            }
            Statement::Let { value, .. } | Statement::Return(value) | Statement::Expr(value) => {
                collect_effects_from_expr(value, declared, used_declared, unknown)
            }
            Statement::If {
                condition,
                then_block,
                else_block,
            } => {
                collect_effects_from_expr(condition, declared, used_declared, unknown);
                collect_effects_from_block(then_block, declared, used_declared, unknown);
                if let Some(else_block) = else_block {
                    collect_effects_from_block(else_block, declared, used_declared, unknown);
                }
            }
            Statement::Goto { args, .. } => {
                for arg in args {
                    collect_effects_from_expr(arg, declared, used_declared, unknown);
                }
            }
            Statement::Send { message, .. } => {
                collect_effects_from_expr(message, declared, used_declared, unknown);
            }
            Statement::Spawn { args, .. } => {
                for arg in args {
                    collect_effects_from_expr(arg, declared, used_declared, unknown);
                }
            }
            Statement::Match { scrutinee, arms } => {
                collect_effects_from_expr(scrutinee, declared, used_declared, unknown);
                for arm in arms {
                    collect_effects_from_block(&arm.body, declared, used_declared, unknown);
                }
            }
        }
    }
}

fn validate_send_targets(
    block: &Block,
    channels: &HashSet<String>,
    channel_names: &[String],
    locator: &SourceLocator<'_>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Send { channel, .. } => {
                if !channels.contains(channel) {
                    let (line, col) = locator.find_send(channel);
                    report.errors.push(GustError {
                        file: file.to_string(),
                        line,
                        col,
                        message: format!("undeclared channel '{}'", channel),
                        note: Some(
                            "channel is used but never declared in this program".to_string(),
                        ),
                        help: suggest_name(channel, channel_names),
                    });
                }
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                validate_send_targets(then_block, channels, channel_names, locator, file, report);
                if let Some(else_block) = else_block {
                    validate_send_targets(
                        else_block,
                        channels,
                        channel_names,
                        locator,
                        file,
                        report,
                    );
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    validate_send_targets(
                        &arm.body,
                        channels,
                        channel_names,
                        locator,
                        file,
                        report,
                    );
                }
            }
            _ => {}
        }
    }
}

fn validate_spawn_targets(
    block: &Block,
    machines: &HashSet<String>,
    machine_names: &[String],
    locator: &SourceLocator<'_>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Spawn { machine, .. } => {
                if !machines.contains(machine) {
                    let (line, col) = locator.find_spawn(machine);
                    report.errors.push(GustError {
                        file: file.to_string(),
                        line,
                        col,
                        message: format!("undeclared machine '{}'", machine),
                        note: Some("spawn target must be a declared machine".to_string()),
                        help: suggest_name(machine, machine_names),
                    });
                }
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                validate_spawn_targets(then_block, machines, machine_names, locator, file, report);
                if let Some(else_block) = else_block {
                    validate_spawn_targets(
                        else_block,
                        machines,
                        machine_names,
                        locator,
                        file,
                        report,
                    );
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    validate_spawn_targets(
                        &arm.body,
                        machines,
                        machine_names,
                        locator,
                        file,
                        report,
                    );
                }
            }
            _ => {}
        }
    }
}

fn validate_ctx_field_access(
    block: &Block,
    transition: &TransitionDecl,
    states: &HashMap<&str, &StateDecl>,
    locator: &SourceLocator<'_>,
    file: &str,
    report: &mut ValidationReport,
) {
    let from_state = match states.get(transition.from.as_str()) {
        Some(s) => s,
        None => return, // from-state not found — already reported by transition validation
    };
    let field_names: HashSet<&str> = from_state.fields.iter().map(|f| f.name.as_str()).collect();
    let field_name_list: Vec<String> = from_state.fields.iter().map(|f| f.name.clone()).collect();

    let mut ctx_fields = Vec::new();
    collect_ctx_fields_from_block(block, &mut ctx_fields);

    for field in ctx_fields {
        if !field_names.contains(field.as_str()) {
            let (line, col) = locator.find_ctx_field(&field);
            report.errors.push(GustError {
                file: file.to_string(),
                line,
                col,
                message: format!(
                    "field '{}' not available in state '{}'",
                    field, transition.from
                ),
                note: if field_name_list.is_empty() {
                    Some(format!("state '{}' has no fields", transition.from))
                } else {
                    Some(format!("available fields: {}", field_name_list.join(", ")))
                },
                help: suggest_name(&field, &field_name_list),
            });
        }
    }
}

/// Collect the immediate field names from `ctx.field` expressions in a block
fn collect_ctx_fields_from_block(block: &Block, out: &mut Vec<String>) {
    for stmt in &block.statements {
        collect_ctx_fields_from_stmt(stmt, out);
    }
}

fn collect_ctx_fields_from_stmt(stmt: &Statement, out: &mut Vec<String>) {
    match stmt {
        Statement::Let { value, .. } => collect_ctx_fields_from_expr(value, out),
        Statement::Return(expr) | Statement::Expr(expr) => collect_ctx_fields_from_expr(expr, out),
        Statement::Perform { args, .. }
        | Statement::Goto { args, .. }
        | Statement::Spawn { args, .. } => {
            for arg in args {
                collect_ctx_fields_from_expr(arg, out);
            }
        }
        Statement::Send { message, .. } => collect_ctx_fields_from_expr(message, out),
        Statement::If {
            condition,
            then_block,
            else_block,
        } => {
            collect_ctx_fields_from_expr(condition, out);
            collect_ctx_fields_from_block(then_block, out);
            if let Some(else_block) = else_block {
                collect_ctx_fields_from_block(else_block, out);
            }
        }
        Statement::Match { scrutinee, arms } => {
            collect_ctx_fields_from_expr(scrutinee, out);
            for arm in arms {
                collect_ctx_fields_from_block(&arm.body, out);
            }
        }
    }
}

fn collect_ctx_fields_from_expr(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::FieldAccess(base, field) => {
            if let Expr::Ident(name) = base.as_ref() {
                if name == "ctx" {
                    if !out.contains(field) {
                        out.push(field.clone());
                    }
                    return;
                }
            }
            // For nested access like ctx.config.name, recurse to find the ctx.config part
            collect_ctx_fields_from_expr(base, out);
        }
        Expr::BinOp(l, _, r) => {
            collect_ctx_fields_from_expr(l, out);
            collect_ctx_fields_from_expr(r, out);
        }
        Expr::UnaryOp(_, e) => collect_ctx_fields_from_expr(e, out),
        Expr::FnCall(_, args) | Expr::Perform(_, args) => {
            for arg in args {
                collect_ctx_fields_from_expr(arg, out);
            }
        }
        _ => {}
    }
}

fn collect_effects_from_expr(
    expr: &Expr,
    declared: &HashSet<String>,
    used_declared: &mut HashSet<String>,
    unknown: &mut Vec<String>,
) {
    match expr {
        Expr::Perform(effect, args) => {
            register_effect(effect, declared, used_declared, unknown);
            for arg in args {
                collect_effects_from_expr(arg, declared, used_declared, unknown);
            }
        }
        Expr::FieldAccess(base, _) | Expr::UnaryOp(_, base) => {
            collect_effects_from_expr(base, declared, used_declared, unknown)
        }
        Expr::FnCall(_, args) => {
            for arg in args {
                collect_effects_from_expr(arg, declared, used_declared, unknown);
            }
        }
        Expr::BinOp(left, _, right) => {
            collect_effects_from_expr(left, declared, used_declared, unknown);
            collect_effects_from_expr(right, declared, used_declared, unknown);
        }
        _ => {}
    }
}

fn register_effect(
    effect: &str,
    declared: &HashSet<String>,
    used_declared: &mut HashSet<String>,
    unknown: &mut Vec<String>,
) {
    if declared.contains(effect) {
        used_declared.insert(effect.to_string());
    } else if !unknown.iter().any(|e| e == effect) {
        unknown.push(effect.to_string());
    }
}

fn suggest_name(name: &str, names: &[String]) -> Option<String> {
    names
        .iter()
        .filter_map(|candidate| {
            let d = levenshtein(name, candidate);
            if d <= 2 {
                Some((d, candidate))
            } else {
                None
            }
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| format!("did you mean '{}'?", c))
}

struct SourceLocator<'a> {
    lines: Vec<&'a str>,
}

impl<'a> SourceLocator<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            lines: source.lines().collect(),
        }
    }

    fn find_state(&self, state: &str) -> (usize, usize) {
        self.find(&format!("state {state}"))
    }

    fn find_transition(&self, transition: &str) -> (usize, usize) {
        self.find(&format!("transition {transition}:"))
    }

    fn find_transition_target(&self, transition: &str, target: &str) -> (usize, usize) {
        let marker = format!("transition {transition}:");
        for (i, line) in self.lines.iter().enumerate() {
            if line.contains(&marker) {
                let col = line.find(target).map(|c| c + 1).unwrap_or(1);
                return (i + 1, col);
            }
        }
        (1, 1)
    }

    fn find_effect(&self, effect: &str) -> (usize, usize) {
        for pattern in [
            format!("effect {effect}("),
            format!("async effect {effect}("),
        ] {
            let found = self.find(&pattern);
            if found != (1, 1) {
                return found;
            }
        }
        (1, 1)
    }

    fn find_perform(&self, effect: &str) -> (usize, usize) {
        self.find(&format!("perform {effect}("))
    }

    fn find_goto(&self, state: &str) -> (usize, usize) {
        self.find(&format!("goto {state}"))
    }

    fn find_send(&self, channel: &str) -> (usize, usize) {
        self.find(&format!("send {channel}("))
    }

    fn find_spawn(&self, machine: &str) -> (usize, usize) {
        self.find(&format!("spawn {machine}("))
    }

    fn find_handler(&self, transition_name: &str) -> (usize, usize) {
        self.find(&format!("on {transition_name}"))
    }

    fn find_ctx_field(&self, field: &str) -> (usize, usize) {
        self.find(&format!("ctx.{field}"))
    }

    fn find(&self, needle: &str) -> (usize, usize) {
        for (i, line) in self.lines.iter().enumerate() {
            if let Some(col) = line.find(needle) {
                return (i + 1, col + 1);
            }
        }
        (1, 1)
    }
}
