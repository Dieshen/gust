use crate::ast::{
    Block, EffectDecl, EffectKind, Expr, Field, MachineDecl, Param, Pattern, Program, Span,
    StateDecl, Statement, TransitionDecl, TypeDecl, TypeExpr,
};
use crate::error::{GustError, GustWarning};
use std::collections::{HashMap, HashSet};
use strsim::levenshtein;

/// The aggregate output of [`validate_program`]: lists of hard errors
/// and advisory warnings with source locations.
#[derive(Debug, Default, Clone)]
pub struct ValidationReport {
    /// Hard errors — prevent codegen.
    pub errors: Vec<GustError>,
    /// Advisory warnings — surface issues without blocking codegen.
    pub warnings: Vec<GustWarning>,
}

impl ValidationReport {
    /// Returns true if validation produced no errors (warnings still allowed).
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Run semantic validation over a parsed [`Program`] and return a
/// [`ValidationReport`]. `file` is embedded in diagnostic locations;
/// `_source` is currently unused (spans on AST nodes provide positions).
pub fn validate_program(program: &Program, file: &str, _source: &str) -> ValidationReport {
    let mut report = ValidationReport::default();
    let declared_channels: HashSet<String> =
        program.channels.iter().map(|c| c.name.clone()).collect();
    let declared_channel_names: Vec<String> =
        program.channels.iter().map(|c| c.name.clone()).collect();
    let declared_machine_names: Vec<String> =
        program.machines.iter().map(|m| m.name.clone()).collect();
    let declared_machine_set: HashSet<String> = declared_machine_names.iter().cloned().collect();

    // Constructor arity per machine, for checking `spawn`.
    //
    // A machine's generated `new()` takes the fields of its **first** state, so
    // that count is what a `spawn` argument list has to match. Nothing checked
    // this, and a mismatch is not caught until the host compiler rejects the
    // generated call — `E0061: this function takes 0 arguments but 1 was
    // supplied` — in a file the author is told never to edit.
    let machine_ctor_arity: HashMap<&str, usize> = program
        .machines
        .iter()
        .map(|m| {
            (
                m.name.as_str(),
                m.states.first().map(|s| s.fields.len()).unwrap_or(0),
            )
        })
        .collect();

    // Program-wide, because a type expression is not machine-scoped: the same
    // unknown constructor is equally wrong in a `type` declaration, a channel,
    // a state field, and an effect signature.
    validate_generic_constructors(program, file, &mut report);

    // Build a map of enum name -> variant names for match exhaustiveness checking.
    let enum_variants: HashMap<String, Vec<String>> = program
        .types
        .iter()
        .filter_map(|t| match t {
            TypeDecl::Enum { name, variants, .. } => {
                let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                Some((name.clone(), variant_names))
            }
            _ => None,
        })
        .collect();

    for machine in &program.machines {
        let state_names: Vec<String> = machine.states.iter().map(|s| s.name.clone()).collect();
        let state_set: HashSet<String> = state_names.iter().cloned().collect();
        let declared_effects: HashSet<String> =
            machine.effects.iter().map(|e| e.name.clone()).collect();
        let declared_effect_names: Vec<String> =
            machine.effects.iter().map(|e| e.name.clone()).collect();
        // Names of declarations whose kind is `action`. Used by handler-safety
        // diagnostics to tell apart replay-safe `effect` from non-idempotent
        // `action` invocations. See #40.
        let declared_actions: HashSet<String> = machine
            .effects
            .iter()
            .filter(|e| e.kind == EffectKind::Action)
            .map(|e| e.name.clone())
            .collect();
        let state_fields: HashMap<&str, &StateDecl> = machine
            .states
            .iter()
            .map(|s| (s.name.as_str(), s))
            .collect();
        let effect_params: HashMap<&str, &[Field]> = machine
            .effects
            .iter()
            .map(|e| (e.name.as_str(), e.params.as_slice()))
            .collect();

        // The `sends` / `receives` machine-header annotations name program-scope
        // channels, so they are checked once per machine rather than per handler.
        validate_channel_annotations(
            machine,
            &declared_channels,
            &declared_channel_names,
            file,
            &mut report,
        );

        let mut seen_states = HashSet::new();
        for state in &machine.states {
            if !seen_states.insert(state.name.clone()) {
                report.errors.push(GustError {
                    file: file.to_string(),
                    line: state.span.start_line,
                    col: state.span.start_col,
                    message: format!("duplicate state name '{}'", state.name),
                    note: Some("state names must be unique within a machine".to_string()),
                    help: None,
                });
            }
        }

        let mut seen_transitions = HashSet::new();
        for transition in &machine.transitions {
            if !seen_transitions.insert(transition.name.clone()) {
                report.errors.push(GustError {
                    file: file.to_string(),
                    line: transition.span.start_line,
                    col: transition.span.start_col,
                    message: format!("duplicate transition name '{}'", transition.name),
                    note: Some("transition names must be unique within a machine".to_string()),
                    help: None,
                });
            }

            if !state_set.contains(&transition.from) {
                report.errors.push(GustError {
                    file: file.to_string(),
                    line: transition.span.start_line,
                    col: transition.span.start_col,
                    message: format!("undefined state '{}' in transition source", transition.from),
                    note: Some(format!("declared states: {}", state_names.join(", "))),
                    help: suggest_name(&transition.from, &state_names),
                });
            }

            for target in &transition.targets {
                if !state_set.contains(target) {
                    report.errors.push(GustError {
                        file: file.to_string(),
                        line: transition.span.start_line,
                        col: transition.span.start_col,
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
        // Build name -> span map for unreachable state warnings
        let state_span_map: HashMap<&str, Span> = machine
            .states
            .iter()
            .map(|s| (s.name.as_str(), s.span))
            .collect();
        for (state, count) in incoming {
            if count == 0 {
                let span = state_span_map
                    .get(state.as_str())
                    .copied()
                    .unwrap_or_default();
                report.warnings.push(GustWarning {
                    file: file.to_string(),
                    line: span.start_line,
                    col: span.start_col,
                    message: format!("unreachable state '{}'", state),
                    note: Some("no transitions lead to this state".to_string()),
                    help: None,
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
                report.warnings.push(GustWarning {
                    file: file.to_string(),
                    line: transition.span.start_line,
                    col: transition.span.start_col,
                    message: format!("transition '{}' has no handler", transition.name),
                    note: Some(format!(
                        "add an 'on {}(...)' handler for this transition",
                        transition.name
                    )),
                    help: None,
                });
            }
        }

        // Build a map from transition name to its declared target states
        let transition_targets: HashMap<&str, &[String]> = machine
            .transitions
            .iter()
            .map(|t| (t.name.as_str(), t.targets.as_slice()))
            .collect();

        // Build name -> span map for effect declarations
        let effect_span_map: HashMap<&str, Span> = machine
            .effects
            .iter()
            .map(|e| (e.name.as_str(), e.span))
            .collect();

        // Maps for goto field type inference.
        let effect_map: HashMap<&str, &EffectDecl> = machine
            .effects
            .iter()
            .map(|e| (e.name.as_str(), e))
            .collect();
        let type_map: HashMap<&str, &TypeDecl> =
            program.types.iter().map(|t| (t.name(), t)).collect();
        let generic_param_set: HashSet<String> = machine
            .generic_params
            .iter()
            .map(|g| g.name.clone())
            .collect();

        validate_generic_param_usage(machine, file, &mut report);

        let mut used_declared_effects = HashSet::new();
        let mut unknown_effects = Vec::new();
        for handler in &machine.handlers {
            // Reject handler return types (not yet supported in codegen)
            if handler.return_type.is_some() {
                report.errors.push(GustError {
                    file: file.to_string(),
                    line: handler.span.start_line,
                    col: handler.span.start_col,
                    message: "handler return types are not yet supported".to_string(),
                    note: Some(format!(
                        "remove the return type from handler '{}'",
                        handler.transition_name
                    )),
                    help: None,
                });
            }

            // Reject bare `return` statements in handlers (codegen always uses Result<(), ...>)
            reject_return_in_block(&handler.body, handler.span, file, &mut report);

            collect_effects_from_block(
                &handler.body,
                &declared_effects,
                &mut used_declared_effects,
                &mut unknown_effects,
            );
            validate_goto_arity(&handler.body, &state_fields, file, &mut report);
            validate_perform_arity(&handler.body, &effect_params, file, &mut report);
            validate_no_free_calls(
                &handler.body,
                &declared_effect_names,
                handler.span,
                file,
                &mut report,
            );

            // The ctx parameter is the from-state accessor: `ctx.foo` resolves
            // to the state field `foo`, so ident collection has to know its name
            // or every `ctx.foo` read would look like a read of `ctx` alone.
            let ctx_param_name = handler
                .params
                .iter()
                .find(|p| p.name == "ctx")
                .map(|p| p.name.clone());
            validate_unused_let_bindings(
                &handler.body,
                ctx_param_name.as_deref(),
                file,
                &mut report,
            );
            validate_result_error_erasure(&handler.body, &machine.effects, file, &mut report);

            if let Some(from_fields) = machine
                .transitions
                .iter()
                .find(|t| t.name == handler.transition_name)
                .and_then(|t| state_fields.get(t.from.as_str()).copied())
            {
                validate_shadowed_handler_params(
                    &handler.params,
                    &from_fields.fields,
                    handler.span,
                    &handler.transition_name,
                    file,
                    &mut report,
                );
            }

            // Validate goto argument types match target state field types.
            {
                let mut variables: HashMap<String, TypeExpr> = HashMap::new();
                for param in &handler.params {
                    // Skip the special `ctx` parameter — its fields resolve via from-state.
                    if param.name != "ctx" {
                        variables.insert(param.name.clone(), param.ty.clone());
                    }
                }

                let from_state = machine
                    .transitions
                    .iter()
                    .find(|t| t.name == handler.transition_name)
                    .and_then(|t| state_fields.get(t.from.as_str()).copied());

                let mut type_ctx = TypeContext {
                    variables,
                    effects: &effect_map,
                    types: &type_map,
                    from_state,
                    generic_params: &generic_param_set,
                };

                validate_goto_types(
                    &handler.body,
                    &state_fields,
                    &mut type_ctx,
                    file,
                    &mut report,
                );
                // Extra expression-level type checks (perform-let annotations,
                // binary op operand compatibility). Uses its own variables scope so
                // goto type validation (above) is unaffected by ordering.
                let mut expr_ctx = TypeContext {
                    variables: type_ctx.variables.clone(),
                    effects: type_ctx.effects,
                    types: type_ctx.types,
                    from_state: type_ctx.from_state,
                    generic_params: type_ctx.generic_params,
                };
                validate_expression_types(&handler.body, &mut expr_ctx, file, &mut report);
            }

            // Warn when if/else branches have inconsistent termination.
            check_if_branch_consistency(
                &handler.body,
                &handler.transition_name,
                &enum_variants,
                file,
                &mut report,
            );

            // Validate that goto targets are declared targets of the transition
            if let Some(targets) = transition_targets.get(handler.transition_name.as_str()) {
                validate_goto_targets(
                    &handler.body,
                    &handler.transition_name,
                    targets,
                    file,
                    &mut report,
                );
            }

            // Task 2: warn when a handler has code paths that don't end in a goto.
            if !block_always_terminates(&handler.body, &enum_variants) {
                report.warnings.push(GustWarning {
                    file: file.to_string(),
                    line: handler.span.start_line,
                    col: handler.span.start_col,
                    message: format!(
                        "handler '{}' has code paths that don't end with a goto",
                        handler.transition_name
                    ),
                    note: Some("all handler paths should transition to a new state".to_string()),
                    help: None,
                });
            }

            // Handler-safety diagnostics for actions (#40 item 4).
            check_handler_action_safety(
                &handler.body,
                &handler.transition_name,
                handler.span,
                &declared_actions,
                file,
                &mut report,
            );
            validate_send_targets(
                &handler.body,
                &declared_channels,
                &declared_channel_names,
                file,
                &mut report,
            );
            validate_spawn_targets(
                &handler.body,
                &declared_machine_set,
                &declared_machine_names,
                &machine_ctor_arity,
                file,
                &mut report,
            );
            // Check match exhaustiveness for enum types
            check_match_exhaustiveness(
                &handler.body,
                &enum_variants,
                handler.span,
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
                    handler.span,
                    file,
                    &mut report,
                );
            }
        }

        for effect in declared_effects {
            if !used_declared_effects.contains(&effect) {
                let span = effect_span_map
                    .get(effect.as_str())
                    .copied()
                    .unwrap_or_default();
                report.warnings.push(GustWarning {
                    file: file.to_string(),
                    line: span.start_line,
                    col: span.start_col,
                    message: format!("unused effect '{}'", effect),
                    note: Some("effect is declared but never performed".to_string()),
                    help: None,
                });
            }
        }

        for effect in &unknown_effects {
            // Walk all handler statements to find where this effect is performed,
            // so we can point the error at the perform site rather than the machine header.
            let stmts: Vec<_> = machine
                .handlers
                .iter()
                .flat_map(|h| h.body.statements.iter())
                .collect();
            let span = stmts
                .iter()
                .find_map(|stmt| {
                    if let Statement::Perform {
                        effect: e, span, ..
                    } = stmt
                    {
                        if e == effect {
                            return Some(*span);
                        }
                    }
                    None
                })
                .unwrap_or_default();
            report.errors.push(GustError {
                file: file.to_string(),
                line: span.start_line,
                col: span.start_col,
                message: format!("undeclared effect '{}'", effect),
                note: Some("effect is used but never declared in this machine".to_string()),
                help: suggest_name(effect, &declared_effect_names),
            });
        }
    }

    report
}

/// Returns true when every code path through `block` ends with a `Goto` or `Return`.
/// Used to detect handlers that might fall through without transitioning to a new state.
fn block_always_terminates(block: &Block, enum_variants: &HashMap<String, Vec<String>>) -> bool {
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
        }) => {
            block_always_terminates(then_block, enum_variants)
                && block_always_terminates(else_block, enum_variants)
        }
        Some(Statement::Match { arms, .. }) => {
            let has_wildcard = arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard));

            // An enum match with every variant covered is also exhaustive, even without `_`.
            let is_enum_exhaustive = if !has_wildcard {
                match_covers_all_enum_variants(arms, enum_variants)
            } else {
                false
            };

            (has_wildcard || is_enum_exhaustive)
                && arms
                    .iter()
                    .all(|a| block_always_terminates(&a.body, enum_variants))
        }
        Some(_) => false,
    }
}

/// Determines the enum name being matched based on the variant patterns in the arms.
/// Returns `Some(enum_name)` if all variant arms reference the same known enum.
fn infer_matched_enum<'a>(
    arms: &[crate::ast::MatchArm],
    enum_variants: &'a HashMap<String, Vec<String>>,
) -> Option<&'a str> {
    // First, try to find an explicit enum_name from Pattern::Variant arms.
    for arm in arms {
        if let Pattern::Variant {
            enum_name: Some(en),
            ..
        } = &arm.pattern
        {
            if enum_variants.contains_key(en) {
                return Some(enum_variants.get_key_value(en).unwrap().0.as_str());
            }
        }
    }

    // No explicit enum name: pick an enum whose variants cover every bare variant name.
    let variant_names: Vec<&str> = arms
        .iter()
        .filter_map(|arm| match &arm.pattern {
            Pattern::Variant { variant, .. } => Some(variant.as_str()),
            _ => None,
        })
        .collect();

    if variant_names.is_empty() {
        return None;
    }

    for (enum_name, variants) in enum_variants {
        if variant_names
            .iter()
            .all(|v| variants.iter().any(|ev| ev == v))
        {
            return Some(enum_name.as_str());
        }
    }

    None
}

/// Returns true if the match arms cover every variant of a known enum.
fn match_covers_all_enum_variants(
    arms: &[crate::ast::MatchArm],
    enum_variants: &HashMap<String, Vec<String>>,
) -> bool {
    let enum_name = match infer_matched_enum(arms, enum_variants) {
        Some(name) => name,
        None => return false,
    };

    let all_variants = match enum_variants.get(enum_name) {
        Some(v) => v,
        None => return false,
    };

    let covered: HashSet<&str> = arms
        .iter()
        .filter_map(|arm| match &arm.pattern {
            Pattern::Variant { variant, .. } => Some(variant.as_str()),
            _ => None,
        })
        .collect();

    all_variants.iter().all(|v| covered.contains(v.as_str()))
}

/// Walks a block recursively and emits warnings for non-exhaustive match statements
/// on known enum types.
fn check_match_exhaustiveness(
    block: &Block,
    enum_variants: &HashMap<String, Vec<String>>,
    handler_span: Span,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Match { arms, .. } => {
                for arm in arms {
                    check_match_exhaustiveness(
                        &arm.body,
                        enum_variants,
                        handler_span,
                        file,
                        report,
                    );
                }

                let has_wildcard = arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard));
                if has_wildcard {
                    continue;
                }

                if let Some(enum_name) = infer_matched_enum(arms, enum_variants) {
                    let all_variants = &enum_variants[enum_name];
                    let covered: HashSet<&str> = arms
                        .iter()
                        .filter_map(|arm| match &arm.pattern {
                            Pattern::Variant { variant, .. } => Some(variant.as_str()),
                            _ => None,
                        })
                        .collect();

                    let missing: Vec<&str> = all_variants
                        .iter()
                        .filter(|v| !covered.contains(v.as_str()))
                        .map(|v| v.as_str())
                        .collect();

                    if !missing.is_empty() {
                        report.warnings.push(GustWarning {
                            file: file.to_string(),
                            line: handler_span.start_line,
                            col: handler_span.start_col,
                            message: format!(
                                "non-exhaustive match on enum '{}': missing variant(s) {}",
                                enum_name,
                                missing.join(", ")
                            ),
                            note: Some(
                                "add the missing variants or a wildcard '_' arm to ensure all cases are handled"
                                    .to_string(),
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
                check_match_exhaustiveness(then_block, enum_variants, handler_span, file, report);
                if let Some(else_block) = else_block {
                    check_match_exhaustiveness(
                        else_block,
                        enum_variants,
                        handler_span,
                        file,
                        report,
                    );
                }
            }
            _ => {}
        }
    }
}

fn validate_goto_arity(
    block: &Block,
    states: &HashMap<&str, &StateDecl>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Goto { state, args, span } => {
                if let Some(target) = states.get(state.as_str()) {
                    if target.fields.len() != args.len() {
                        report.errors.push(GustError {
                            file: file.to_string(),
                            line: span.start_line,
                            col: span.start_col,
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
                validate_goto_arity(then_block, states, file, report);
                if let Some(else_block) = else_block {
                    validate_goto_arity(else_block, states, file, report);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    validate_goto_arity(&arm.body, states, file, report);
                }
            }
            _ => {}
        }
    }
}

fn validate_perform_arity(
    block: &Block,
    effects: &HashMap<&str, &[Field]>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Perform { effect, args, span } => {
                check_perform_args(effect, args, *span, effects, file, report);
            }
            Statement::Let { value, .. } | Statement::Expr(value) | Statement::Return(value) => {
                check_expr_perform_arity(value, effects, file, report);
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                validate_perform_arity(then_block, effects, file, report);
                if let Some(else_block) = else_block {
                    validate_perform_arity(else_block, effects, file, report);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    validate_perform_arity(&arm.body, effects, file, report);
                }
            }
            _ => {}
        }
    }
}

fn check_perform_args(
    effect: &str,
    args: &[Expr],
    span: Span,
    effects: &HashMap<&str, &[Field]>,
    file: &str,
    report: &mut ValidationReport,
) {
    if let Some(params) = effects.get(effect) {
        if params.len() != args.len() {
            report.errors.push(GustError {
                file: file.to_string(),
                line: span.start_line,
                col: span.start_col,
                message: format!(
                    "effect '{}' expects {} argument(s) but got {}",
                    effect,
                    params.len(),
                    args.len()
                ),
                note: Some("perform argument count must match effect parameter count".to_string()),
                help: None,
            });
        }
    }
    // Unknown effects are already reported by collect_effects_from_block - skip here.
}

fn check_expr_perform_arity(
    expr: &Expr,
    effects: &HashMap<&str, &[Field]>,
    file: &str,
    report: &mut ValidationReport,
) {
    match expr {
        Expr::Perform(effect, args, span) => {
            check_perform_args(effect, args, *span, effects, file, report);
        }
        Expr::BinOp(left, _, right, _) => {
            check_expr_perform_arity(left, effects, file, report);
            check_expr_perform_arity(right, effects, file, report);
        }
        Expr::UnaryOp(_, inner) => {
            check_expr_perform_arity(inner, effects, file, report);
        }
        Expr::FieldAccess(base, _) => {
            check_expr_perform_arity(base, effects, file, report);
        }
        Expr::FnCall(_, args) => {
            for arg in args {
                check_expr_perform_arity(arg, effects, file, report);
            }
        }
        _ => {}
    }
}

fn validate_goto_targets(
    block: &Block,
    transition_name: &str,
    valid_targets: &[String],
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Goto { state, span, .. } if !valid_targets.iter().any(|t| t == state) => {
                let targets_list = valid_targets.join(", ");
                report.errors.push(GustError {
                    file: file.to_string(),
                    line: span.start_line,
                    col: span.start_col,
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
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                validate_goto_targets(then_block, transition_name, valid_targets, file, report);
                if let Some(else_block) = else_block {
                    validate_goto_targets(else_block, transition_name, valid_targets, file, report);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    validate_goto_targets(&arm.body, transition_name, valid_targets, file, report);
                }
            }
            _ => {}
        }
    }
}

fn reject_return_in_block(
    block: &Block,
    handler_span: Span,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Return(_) => {
                report.errors.push(GustError {
                    file: file.to_string(),
                    line: handler_span.start_line,
                    col: handler_span.start_col,
                    message:
                        "return statements are not supported in handlers; use goto to transition"
                            .to_string(),
                    note: Some("codegen requires goto for state transitions".to_string()),
                    help: None,
                });
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                reject_return_in_block(then_block, handler_span, file, report);
                if let Some(else_block) = else_block {
                    reject_return_in_block(else_block, handler_span, file, report);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    reject_return_in_block(&arm.body, handler_span, file, report);
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
                span: _,
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

/// Collects every `let` binding in a block, recursing into nested `if` and
/// `match` bodies.
fn collect_let_bindings<'a>(block: &'a Block, out: &mut Vec<(&'a str, Span)>) {
    for stmt in &block.statements {
        match stmt {
            Statement::Let { name, span, .. } => out.push((name.as_str(), *span)),
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                collect_let_bindings(then_block, out);
                if let Some(else_block) = else_block {
                    collect_let_bindings(else_block, out);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    collect_let_bindings(&arm.body, out);
                }
            }
            _ => {}
        }
    }
}

/// Reports a machine type parameter that nothing in the machine references.
///
/// An unused parameter is not merely untidy — it does not compile. The Rust
/// backend emits `pub enum MState<T>` whose variants never mention `T`, which
/// is `E0392: type parameter 'T' is never used`. `machine CircuitBreaker<T>`
/// and `machine RateLimiter<K>` in `gust-stdlib` both had one, and neither
/// produced compilable Rust as a result.
///
/// An error rather than a warning, on the rule this validator already uses:
/// warn when backends disagree, error when the code is wrong for all of them.
/// Nothing can give meaning to a parameter that is never referenced.
///
/// "Used" deliberately spans the whole machine, not just state fields — a
/// parameter that appears only in an effect signature is legitimately generic:
///
/// ```text
/// machine Cache<T> {
///     state Empty
///     effect fetch(key: String) -> T    // T is used; this is fine
/// }
/// ```
fn validate_generic_param_usage(machine: &MachineDecl, file: &str, report: &mut ValidationReport) {
    for param in &machine.generic_params {
        let used = machine
            .states
            .iter()
            .flat_map(|s| s.fields.iter())
            .any(|f| type_expr_mentions(&f.ty, &param.name))
            || machine.effects.iter().any(|e| {
                e.params
                    .iter()
                    .any(|p| type_expr_mentions(&p.ty, &param.name))
                    || type_expr_mentions(&e.return_type, &param.name)
            })
            || machine
                .handlers
                .iter()
                .flat_map(|h| h.params.iter())
                .any(|p| type_expr_mentions(&p.ty, &param.name));

        if used {
            continue;
        }

        report.errors.push(GustError {
            file: file.to_string(),
            line: machine.span.start_line,
            col: machine.span.start_col,
            message: format!(
                "type parameter '{}' is declared by machine '{}' but never used",
                param.name, machine.name
            ),
            note: Some(
                "a machine's type parameter must appear in a state field, an effect signature, \
                 or a handler parameter; generated Rust rejects an unused one with E0392"
                    .to_string(),
            ),
            help: Some(format!(
                "remove '{}' from the machine header, or use it — e.g. a state field 'key: {}'",
                param.name, param.name
            )),
        });
    }
}

/// Whether `name` appears anywhere inside a type expression, including nested
/// positions like the `T` in `Vec<T>` or `Result<T, String>`.
fn type_expr_mentions(ty: &TypeExpr, name: &str) -> bool {
    match ty {
        TypeExpr::Unit => false,
        TypeExpr::Simple(n) => n == name,
        TypeExpr::Generic(head, args) => {
            head == name || args.iter().any(|a| type_expr_mentions(a, name))
        }
        TypeExpr::Tuple(items) => items.iter().any(|t| type_expr_mentions(t, name)),
    }
}

/// Rejects a call to anything that is not a declared effect.
///
/// Gust has no function declarations, so a bare `foo(x)` in a handler names
/// nothing the compiler knows — and both backends emitted it verbatim. That is
/// the whole sandbox boundary, and it was open: `let x = exit(ctx.n);` reported
/// "Check passed" and emitted `let _ = exit(n);`, resolving against whatever the
/// generated file's module or package happened to have in scope. `use os;`
/// becoming a real Go `import "os"` widened it further.
///
/// The effect declarations are the contract with the host. A handler reaching
/// past them defeats the point of declaring them, and no `.gu` in this
/// repository does it. Rejecting outright is also forward-compatible: when
/// top-level `fn` declarations land (ROADMAP phase 6), declared functions become
/// legal call targets and this check grows a second allowed set rather than
/// being torn out.
fn validate_no_free_calls(
    block: &Block,
    declared_effects: &[String],
    span: Span,
    file: &str,
    report: &mut ValidationReport,
) {
    let mut calls = Vec::new();
    collect_free_calls_in_block(block, &mut calls);

    for name in calls {
        // Naming a declared effect without `perform` is the overwhelmingly
        // likely mistake, so it gets the exact fix rather than the generic one.
        let help = if declared_effects.iter().any(|e| e == &name) {
            format!("'{name}' is a declared effect — call it as `perform {name}(...)`")
        } else if let Some(suggestion) = suggest_name(&name, declared_effects) {
            format!("did you mean `perform {suggestion}(...)`?")
        } else {
            format!(
                "declare '{name}' as an effect on this machine and call it with `perform`, \
                 so the host implements it through the generated trait"
            )
        };

        report.errors.push(GustError {
            file: file.to_string(),
            line: span.start_line,
            col: span.start_col,
            message: format!("call to undeclared function '{name}'"),
            note: Some(
                "handlers may only call declared effects; Gust has no function declarations, \
                 so this would be emitted verbatim into generated code"
                    .to_string(),
            ),
            help: Some(help),
        });
    }
}

fn collect_free_calls_in_block(block: &Block, out: &mut Vec<String>) {
    for stmt in &block.statements {
        collect_free_calls_in_stmt(stmt, out);
    }
}

fn collect_free_calls_in_stmt(stmt: &Statement, out: &mut Vec<String>) {
    match stmt {
        Statement::Let { value, .. } | Statement::Return(value) | Statement::Expr(value) => {
            collect_free_calls_in_expr(value, out)
        }
        Statement::Goto { args, .. }
        | Statement::Perform { args, .. }
        | Statement::Spawn { args, .. } => {
            for arg in args {
                collect_free_calls_in_expr(arg, out);
            }
        }
        Statement::Send { message, .. } => collect_free_calls_in_expr(message, out),
        Statement::If {
            condition,
            then_block,
            else_block,
            ..
        } => {
            collect_free_calls_in_expr(condition, out);
            collect_free_calls_in_block(then_block, out);
            if let Some(else_block) = else_block {
                collect_free_calls_in_block(else_block, out);
            }
        }
        Statement::Match { scrutinee, arms } => {
            collect_free_calls_in_expr(scrutinee, out);
            for arm in arms {
                collect_free_calls_in_block(&arm.body, out);
            }
        }
    }
}

fn collect_free_calls_in_expr(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::FnCall(name, args) => {
            out.push(name.clone());
            for arg in args {
                collect_free_calls_in_expr(arg, out);
            }
        }
        Expr::Perform(_, args, _) => {
            for arg in args {
                collect_free_calls_in_expr(arg, out);
            }
        }
        Expr::BinOp(left, _, right, _) => {
            collect_free_calls_in_expr(left, out);
            collect_free_calls_in_expr(right, out);
        }
        Expr::UnaryOp(_, inner) => collect_free_calls_in_expr(inner, out),
        Expr::FieldAccess(base, _) => collect_free_calls_in_expr(base, out),
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::StringLit(_)
        | Expr::BoolLit(_)
        | Expr::Ident(_)
        | Expr::Path(_, _) => {}
    }
}

/// The generic type constructors every backend can actually lower.
///
/// Exhaustive by construction: `type_expr_to_go` and the Rust emitter special-case
/// exactly these, and `SchemaCodegen::generic_type_schema` maps exactly these.
/// Anything else reaches a backend as a bare name.
const KNOWN_GENERIC_CONSTRUCTORS: &[&str] = &["Vec", "Option", "Result"];

/// Rejects a generic type whose constructor no backend knows how to lower.
///
/// An unrecognised head used to pass straight through to codegen. `HashMap<K, V>`
/// reported "Check passed" and then emitted `HashMap[K, V]` into Go — which has
/// no such type and does not compile — and a bare `HashMap<K, V>` into Rust with
/// no accompanying `use std::collections::HashMap`, so it resolved only if the
/// including module happened to import it. The JSON Schema emitter, meanwhile,
/// produced `{"description": "Unresolved generic type: HashMap"}`.
///
/// Three backends disagreeing three different ways is the signal that this
/// belongs at the source, not in each emitter. A map type may well be worth
/// having — but adding one is a language feature with a schema representation
/// and a lowering per backend, not something to infer from a name that looks
/// plausible. Until then, saying so at `gust check` beats three different
/// failures downstream. See #133.
fn validate_generic_constructors(program: &Program, file: &str, report: &mut ValidationReport) {
    fn walk(
        ty: &TypeExpr,
        allowed: &HashSet<String>,
        where_: &str,
        span: &Span,
        file: &str,
        report: &mut ValidationReport,
    ) {
        match ty {
            TypeExpr::Unit | TypeExpr::Simple(_) => {}
            TypeExpr::Tuple(items) => {
                for item in items {
                    walk(item, allowed, where_, span, file, report);
                }
            }
            TypeExpr::Generic(head, args) => {
                if !allowed.contains(head.as_str()) {
                    report.errors.push(GustError {
                        file: file.to_string(),
                        line: span.start_line,
                        col: span.start_col,
                        message: format!(
                            "unknown generic type '{head}' in {where_}; no backend can lower it"
                        ),
                        note: Some(format!(
                            "the generic types Gust lowers are {}",
                            KNOWN_GENERIC_CONSTRUCTORS.join(", ")
                        )),
                        help: Some(generic_constructor_help(head)),
                    });
                }
                for arg in args {
                    walk(arg, allowed, where_, span, file, report);
                }
            }
        }
    }

    let mut base: HashSet<String> = KNOWN_GENERIC_CONSTRUCTORS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    // A machine type parameter is not a constructor Gust knows, but rejecting
    // `T<...>` here would report against a shape the grammar admits and no
    // fixture exercises. Left permissive deliberately: this check exists to
    // catch a plausible-looking name reaching codegen, not to police generics.
    for machine in &program.machines {
        base.extend(machine.generic_params.iter().map(|p| p.name.clone()));
    }

    for decl in &program.types {
        match decl {
            TypeDecl::Struct { name, fields, span } => {
                for field in fields {
                    let where_ = format!("field '{}' of type '{name}'", field.name);
                    walk(&field.ty, &base, &where_, span, file, report);
                }
            }
            TypeDecl::Enum {
                name,
                variants,
                span,
            } => {
                for variant in variants {
                    let where_ = format!("variant '{}' of enum '{name}'", variant.name);
                    for payload in &variant.payload {
                        walk(payload, &base, &where_, span, file, report);
                    }
                }
            }
        }
    }

    for channel in &program.channels {
        let where_ = format!("channel '{}'", channel.name);
        walk(
            &channel.message_type,
            &base,
            &where_,
            &channel.span,
            file,
            report,
        );
    }

    for machine in &program.machines {
        for state in &machine.states {
            for field in &state.fields {
                let where_ = format!("field '{}' of state '{}'", field.name, state.name);
                walk(&field.ty, &base, &where_, &state.span, file, report);
            }
        }
        for effect in &machine.effects {
            for param in &effect.params {
                let where_ = format!("parameter '{}' of '{}'", param.name, effect.name);
                walk(&param.ty, &base, &where_, &effect.span, file, report);
            }
            let where_ = format!("return type of '{}'", effect.name);
            walk(
                &effect.return_type,
                &base,
                &where_,
                &effect.span,
                file,
                report,
            );
        }
        for handler in &machine.handlers {
            for param in &handler.params {
                let where_ = format!(
                    "parameter '{}' of handler '{}'",
                    param.name, handler.transition_name
                );
                walk(&param.ty, &base, &where_, &handler.span, file, report);
            }
        }
    }
}

/// The `help` line for an unknown generic constructor.
///
/// Map- and set-shaped names get a specific answer because they are what people
/// actually reach for, and "use Vec instead" is useless advice for someone who
/// wanted a lookup table.
fn generic_constructor_help(head: &str) -> String {
    match head {
        "HashMap" | "BTreeMap" | "Map" | "Dict" | "Dictionary" => {
            "Gust has no map type. Model the pairs as a `Vec` of a two-field `type`, \
             or keep the map in the host and expose lookups as an effect."
                .to_string()
        }
        "HashSet" | "BTreeSet" | "Set" => {
            "Gust has no set type. Use `Vec<T>`, or keep the set in the host and expose \
             membership as an effect."
                .to_string()
        }
        "List" | "Array" | "Slice" => format!("did you mean 'Vec'? '{head}' is not a Gust type"),
        "Maybe" | "Optional" | "Nullable" => {
            format!("did you mean 'Option'? '{head}' is not a Gust type")
        }
        "Either" => "did you mean 'Result'?".to_string(),
        _ => format!(
            "declare '{head}' as a `type` if it is your own, or replace it with Vec, Option, or Result"
        ),
    }
}

/// Warns about `let` bindings the handler never reads.
///
/// This is not merely untidy. Rust warns, but Go rejects an unused local
/// outright (`declared and not used`), so the same `.gu` source produces a
/// package that will not build. Reporting it against the `.gu` means the
/// author hears about it once, at the source, rather than as a backend-specific
/// surprise in generated output. See #100.
fn validate_unused_let_bindings(
    block: &Block,
    ctx_param: Option<&str>,
    file: &str,
    report: &mut ValidationReport,
) {
    let referenced = crate::codegen_common::collect_referenced_idents(block, ctx_param);

    let mut bindings = Vec::new();
    collect_let_bindings(block, &mut bindings);

    for (name, span) in bindings {
        // No underscore-prefix exemption. Gust has never documented one — the
        // two `_`-prefixed bindings in the tree predate any decision, and bare
        // `perform f();` has been valid since the first commit, so there is one
        // clear way to discard a result. Exempting `_name` would also be
        // misleading: Go accepts only a bare `_`, never `_name`, so an exempted
        // binding would silently reach the backend this diagnostic exists to
        // protect. See #100.
        //
        // A binding whose name is read anywhere in the handler counts as used.
        // Deliberately coarse: shadowed rebindings of the same name are treated
        // as used rather than risking a false positive on legitimate code.
        if referenced.contains(name) {
            continue;
        }
        report.warnings.push(GustWarning {
            file: file.to_string(),
            line: span.start_line,
            col: span.start_col,
            message: format!("unused binding '{name}'"),
            note: Some(
                "the value is never read; Go codegen rejects unused locals outright".to_string(),
            ),
            help: Some(format!(
                "remove the binding, or call the effect without binding it: `perform ...;` instead of `let {name} = perform ...;`"
            )),
        });
    }
}

/// Warns when an `Err` binding names a payload the Go backend cannot produce.
///
/// Go signals failure with a single `error`, so an effect declared
/// `-> Result<T, E>` lowers to `(T, error)` and `E` is erased. When `E` is
/// `String` the erasure is lossless — the `Err` binding becomes `err.Error()`.
/// For any other `E` the binding is a Go `error`, and using it where `E` is
/// expected does not compile.
///
/// A warning rather than an error, following the unused-`let` precedent: the
/// same source is perfectly valid Rust, so blocking it would penalise Rust-only
/// users. Reporting it against the `.gu` means a Go-targeting author hears about
/// it at the source instead of as a backend-specific surprise.
fn validate_result_error_erasure(
    block: &Block,
    effects: &[EffectDecl],
    file: &str,
    report: &mut ValidationReport,
) {
    // Effects whose declared error type Go cannot carry, mapped to that type as
    // written. `Result<T, String>` is deliberately absent: that one round-trips
    // through `error.Error()`.
    let lossy: HashMap<&str, String> = effects
        .iter()
        .filter_map(|effect| {
            let TypeExpr::Generic(name, args) = &effect.return_type else {
                return None;
            };
            if name != "Result" {
                return None;
            }
            let error_type = args.get(1)?;
            if matches!(error_type, TypeExpr::Simple(n) if n == "String") {
                return None;
            }
            Some((effect.name.as_str(), type_expr_to_display(error_type)))
        })
        .collect();
    if lossy.is_empty() {
        return;
    }

    let mut bindings: Vec<(&String, Span, &str)> = Vec::new();
    collect_lossy_result_bindings(block, &lossy, &mut bindings);

    for (binding, span, effect_name) in bindings {
        if !destructures_used_error(block, binding) {
            continue;
        }
        let error_type = &lossy[effect_name];
        report.warnings.push(GustWarning {
            file: file.to_string(),
            line: span.start_line,
            col: span.start_col,
            message: format!("Go cannot represent the error type of effect '{effect_name}'"),
            note: Some(format!(
                "Go signals failure with a single `error`, so `Result<_, {error_type}>` lowers to `(_, error)` and the `Err` payload type is lost; the Rust backend is unaffected"
            )),
            help: Some(
                "declare the effect as `Result<_, String>` if this machine must also target Go — the `Err` binding then receives the error message"
                    .to_string(),
            ),
        });
    }
}

/// Collect `let` bindings whose value comes from an effect in `lossy`.
fn collect_lossy_result_bindings<'a>(
    block: &'a Block,
    lossy: &HashMap<&str, String>,
    out: &mut Vec<(&'a String, Span, &'a str)>,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Let {
                name,
                value: Expr::Perform(effect, _, _),
                span,
                ..
            } => {
                if lossy.contains_key(effect.as_str()) {
                    out.push((name, *span, effect.as_str()));
                }
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                collect_lossy_result_bindings(then_block, lossy, out);
                if let Some(else_block) = else_block {
                    collect_lossy_result_bindings(else_block, lossy, out);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    collect_lossy_result_bindings(&arm.body, lossy, out);
                }
            }
            _ => {}
        }
    }
}

/// Whether `binding` is matched with an `Err` arm that binds a name that arm's
/// own body reads. Only then does the erased error type reach real Go code, and
/// the scope test has to match the one the Go backend applies when it decides
/// whether to emit the binding at all.
fn destructures_used_error(block: &Block, binding: &str) -> bool {
    block.statements.iter().any(|stmt| match stmt {
        Statement::Match { scrutinee, arms } => {
            let matches_binding = matches!(scrutinee, Expr::Ident(name) if name == binding);
            let uses_error = matches_binding
                && arms.iter().any(|arm| match &arm.pattern {
                    Pattern::Variant {
                        variant, bindings, ..
                    } if variant == "Err" => bindings.first().is_some_and(|b| {
                        b != "_"
                            && crate::codegen_common::collect_bare_idents(&arm.body).contains(b)
                    }),
                    _ => false,
                });
            uses_error
                || arms
                    .iter()
                    .any(|arm| destructures_used_error(&arm.body, binding))
        }
        Statement::If {
            then_block,
            else_block,
            ..
        } => {
            destructures_used_error(then_block, binding)
                || else_block
                    .as_ref()
                    .is_some_and(|b| destructures_used_error(b, binding))
        }
        _ => false,
    })
}

/// Render a type expression the way it was written, for diagnostic text.
fn type_expr_to_display(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Unit => "()".to_string(),
        TypeExpr::Simple(name) => name.clone(),
        TypeExpr::Generic(name, args) => {
            let inner = args
                .iter()
                .map(type_expr_to_display)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name}<{inner}>")
        }
        TypeExpr::Tuple(types) => {
            let inner = types
                .iter()
                .map(type_expr_to_display)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({inner})")
        }
    }
}

/// Warns when a handler parameter shares a name with a field of the state the
/// transition leaves from.
///
/// Codegen destructures the from-state inside the transition method, so that
/// binding shadows the parameter and the parameter becomes unreachable. The
/// generated method still takes it, producing a dead argument the author never
/// asked for.
fn validate_shadowed_handler_params(
    params: &[Param],
    from_state_fields: &[Field],
    handler_span: Span,
    transition_name: &str,
    file: &str,
    report: &mut ValidationReport,
) {
    for param in params {
        // `ctx` is the designated from-state accessor, not a value parameter.
        if param.name == "ctx" {
            continue;
        }
        if from_state_fields.iter().any(|f| f.name == param.name) {
            report.warnings.push(GustWarning {
                file: file.to_string(),
                line: handler_span.start_line,
                col: handler_span.start_col,
                message: format!(
                    "handler parameter '{}' is shadowed by the from-state field of the same name",
                    param.name
                ),
                note: Some(format!(
                    "handler '{transition_name}' destructures the from-state, so references to '{}' resolve to the state field and the parameter is unreachable",
                    param.name
                )),
                help: Some(format!(
                    "rename the parameter, or drop it and read '{}' from the state",
                    param.name
                )),
            });
        }
    }
}

/// Check the `sends` / `receives` annotations in a machine header against the
/// channels declared at program scope.
///
/// This is an error, not a warning, for two reasons. First, it is definitely
/// wrong for every backend: the annotation is a reference into the program-scope
/// channel namespace, and no target can give meaning to a name that is not in it.
/// Second, an undeclared channel named by `send` is already a hard error
/// (`validate_send_targets`), and the same name resolving in one position but not
/// the other would be incoherent — every name-resolution check in this validator
/// is an error.
///
/// The failure it prevents is silent. `machine.sends` is what the Rust and Go
/// backends iterate to emit the `send_*` / `Send*` helpers, and each looks the
/// name up with `channels.iter().find(...)`. A miss yields `None`, so a typo
/// makes the helper vanish from the generated API with no diagnostic anywhere.
fn validate_channel_annotations(
    machine: &MachineDecl,
    channels: &HashSet<String>,
    channel_names: &[String],
    file: &str,
    report: &mut ValidationReport,
) {
    let declared = if channel_names.is_empty() {
        "no channels are declared in this program".to_string()
    } else {
        format!("declared channels: {}", channel_names.join(", "))
    };

    let annotations = [("sends", &machine.sends), ("receives", &machine.receives)];
    for (keyword, annotated) in annotations {
        for channel in annotated {
            if channels.contains(channel) {
                continue;
            }
            report.errors.push(GustError {
                file: file.to_string(),
                line: machine.span.start_line,
                col: machine.span.start_col,
                message: format!(
                    "undeclared channel '{}' in '{}' annotation on machine '{}'",
                    channel, keyword, machine.name
                ),
                note: Some(format!(
                    "a '{keyword}' annotation must name a channel declared at program scope; {declared}"
                )),
                help: Some(suggest_name(channel, channel_names).unwrap_or_else(|| {
                    format!(
                        "declare 'channel {channel}: <Type>' at program scope, or remove '{keyword} {channel}' from the machine header"
                    )
                })),
            });
        }
    }
}

fn validate_send_targets(
    block: &Block,
    channels: &HashSet<String>,
    channel_names: &[String],
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Send { channel, span, .. } if !channels.contains(channel) => {
                report.errors.push(GustError {
                    file: file.to_string(),
                    line: span.start_line,
                    col: span.start_col,
                    message: format!("undeclared channel '{}'", channel),
                    note: Some("channel is used but never declared in this program".to_string()),
                    help: suggest_name(channel, channel_names),
                });
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                validate_send_targets(then_block, channels, channel_names, file, report);
                if let Some(else_block) = else_block {
                    validate_send_targets(else_block, channels, channel_names, file, report);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    validate_send_targets(&arm.body, channels, channel_names, file, report);
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
    ctor_arity: &HashMap<&str, usize>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Spawn { machine, span, .. } if !machines.contains(machine) => {
                report.errors.push(GustError {
                    file: file.to_string(),
                    line: span.start_line,
                    col: span.start_col,
                    message: format!("undeclared machine '{}'", machine),
                    note: Some("spawn target must be a declared machine".to_string()),
                    help: suggest_name(machine, machine_names),
                });
            }
            Statement::Spawn {
                machine,
                args,
                span,
            } => {
                // Arity against the child's generated constructor, which takes
                // the fields of its first state. `goto` is already checked the
                // same way against its target state's fields.
                if let Some(&expected) = ctor_arity.get(machine.as_str()) {
                    if expected != args.len() {
                        report.errors.push(GustError {
                            file: file.to_string(),
                            line: span.start_line,
                            col: span.start_col,
                            message: format!(
                                "spawn of '{}' passes {} argument{}, but its constructor takes {}",
                                machine,
                                args.len(),
                                if args.len() == 1 { "" } else { "s" },
                                expected
                            ),
                            note: Some(format!(
                                "a machine is constructed from the fields of its first state; \
                                 '{machine}' declares {expected} there"
                            )),
                            help: Some(format!(
                                "pass exactly {expected} argument{} to `spawn {machine}(...)`, \
                                 or change the fields of its first state",
                                if expected == 1 { "" } else { "s" }
                            )),
                        });
                    }
                }
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                validate_spawn_targets(
                    then_block,
                    machines,
                    machine_names,
                    ctor_arity,
                    file,
                    report,
                );
                if let Some(else_block) = else_block {
                    validate_spawn_targets(
                        else_block,
                        machines,
                        machine_names,
                        ctor_arity,
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
                        ctor_arity,
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
    handler_span: Span,
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
            // Use handler span as fallback — ctx field access spans require expression-level tracking
            report.errors.push(GustError {
                file: file.to_string(),
                line: handler_span.start_line,
                col: handler_span.start_col,
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
            span: _,
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
        Expr::BinOp(l, _, r, _) => {
            collect_ctx_fields_from_expr(l, out);
            collect_ctx_fields_from_expr(r, out);
        }
        Expr::UnaryOp(_, e) => collect_ctx_fields_from_expr(e, out),
        Expr::FnCall(_, args) | Expr::Perform(_, args, _) => {
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
        Expr::Perform(effect, args, _) => {
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
        Expr::BinOp(left, _, right, _) => {
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

// === Goto field type validation ===

/// Context for type inference within a handler body.
struct TypeContext<'a> {
    /// Variables in scope: handler params + let bindings.
    variables: HashMap<String, TypeExpr>,
    /// Effect declarations for resolving `perform` return types.
    effects: &'a HashMap<&'a str, &'a EffectDecl>,
    /// Type declarations (structs/enums) for resolving field access.
    types: &'a HashMap<&'a str, &'a TypeDecl>,
    /// The from-state for resolving `ctx.field` references.
    from_state: Option<&'a StateDecl>,
    /// Generic type parameters from the machine declaration.
    generic_params: &'a HashSet<String>,
}

/// Format a `TypeExpr` as a human-readable string for error messages.
fn format_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Unit => "()".to_string(),
        TypeExpr::Simple(name) => name.clone(),
        TypeExpr::Generic(name, params) => {
            let inner: Vec<String> = params.iter().map(format_type_expr).collect();
            format!("{}<{}>", name, inner.join(", "))
        }
        TypeExpr::Tuple(types) => {
            let inner: Vec<String> = types.iter().map(format_type_expr).collect();
            format!("({})", inner.join(", "))
        }
    }
}

/// Check if two types are compatible. Returns `true` when they match OR when
/// either side is a generic type parameter (conservative — avoids false positives).
fn types_compatible(
    expected: &TypeExpr,
    actual: &TypeExpr,
    generic_params: &HashSet<String>,
) -> bool {
    if is_generic_param(expected, generic_params) || is_generic_param(actual, generic_params) {
        return true;
    }

    match (expected, actual) {
        (TypeExpr::Unit, TypeExpr::Unit) => true,
        (TypeExpr::Simple(a), TypeExpr::Simple(b)) => a == b,
        (TypeExpr::Generic(name_a, params_a), TypeExpr::Generic(name_b, params_b)) => {
            name_a == name_b
                && params_a.len() == params_b.len()
                && params_a
                    .iter()
                    .zip(params_b.iter())
                    .all(|(a, b)| types_compatible(a, b, generic_params))
        }
        (TypeExpr::Tuple(types_a), TypeExpr::Tuple(types_b)) => {
            types_a.len() == types_b.len()
                && types_a
                    .iter()
                    .zip(types_b.iter())
                    .all(|(a, b)| types_compatible(a, b, generic_params))
        }
        _ => false,
    }
}

/// Returns true if the type expression is (or contains) a generic type parameter.
fn is_generic_param(ty: &TypeExpr, generic_params: &HashSet<String>) -> bool {
    match ty {
        TypeExpr::Simple(name) => generic_params.contains(name),
        TypeExpr::Generic(name, params) => {
            generic_params.contains(name)
                || params.iter().any(|p| is_generic_param(p, generic_params))
        }
        TypeExpr::Tuple(types) => types.iter().any(|t| is_generic_param(t, generic_params)),
        TypeExpr::Unit => false,
    }
}

/// Try to infer the type of an expression. Returns `None` when the type cannot
/// be determined — callers should skip validation in that case.
fn infer_expr_type(expr: &Expr, ctx: &TypeContext<'_>) -> Option<TypeExpr> {
    match expr {
        Expr::IntLit(_) => Some(TypeExpr::Simple("i64".to_string())),
        Expr::FloatLit(_) => Some(TypeExpr::Simple("f64".to_string())),
        Expr::StringLit(_) => Some(TypeExpr::Simple("String".to_string())),
        Expr::BoolLit(_) => Some(TypeExpr::Simple("bool".to_string())),
        Expr::Ident(name) => ctx.variables.get(name).cloned(),
        Expr::Path(enum_name, _variant) => Some(TypeExpr::Simple(enum_name.clone())),
        Expr::Perform(effect_name, _, _) => ctx
            .effects
            .get(effect_name.as_str())
            .map(|e| e.return_type.clone()),
        Expr::FieldAccess(base, field) => infer_field_access_type(base, field, ctx),
        Expr::BinOp(left, op, _right, _) => {
            use crate::ast::BinOp::*;
            match op {
                Eq | Neq | Lt | Lte | Gt | Gte | And | Or => {
                    Some(TypeExpr::Simple("bool".to_string()))
                }
                Add | Sub | Mul | Div | Mod => infer_expr_type(left, ctx),
            }
        }
        Expr::UnaryOp(op, inner) => {
            use crate::ast::UnaryOp::*;
            match op {
                Not => Some(TypeExpr::Simple("bool".to_string())),
                Neg => infer_expr_type(inner, ctx),
            }
        }
        // Function calls — we don't know the return type.
        Expr::FnCall(_, _) => None,
    }
}

/// Infer the type of a field access expression (e.g., `ctx.order`, `total.cents`,
/// `ctx.order.id`).
fn infer_field_access_type(base: &Expr, field: &str, ctx: &TypeContext<'_>) -> Option<TypeExpr> {
    // ctx.field resolves against the handler's from-state fields.
    if let Expr::Ident(name) = base {
        if name == "ctx" {
            return ctx
                .from_state
                .and_then(|s| find_field_type(&s.fields, field));
        }
    }

    // General case: infer the base type, then look up the field on its type decl.
    let base_type = infer_expr_type(base, ctx)?;
    resolve_field_type(&base_type, field, ctx.types)
}

/// Find a field's type in a list of fields.
fn find_field_type(fields: &[Field], field_name: &str) -> Option<TypeExpr> {
    fields
        .iter()
        .find(|f| f.name == field_name)
        .map(|f| f.ty.clone())
}

/// Given a resolved type and a field name, look up the field's type in type declarations.
fn resolve_field_type(
    base_type: &TypeExpr,
    field_name: &str,
    types: &HashMap<&str, &TypeDecl>,
) -> Option<TypeExpr> {
    if let TypeExpr::Simple(type_name) = base_type {
        if let Some(type_decl) = types.get(type_name.as_str()) {
            return find_field_type(type_decl.fields(), field_name);
        }
    }
    None
}

/// Validate that goto argument types match target state field types within a block.
/// Tracks let bindings as it walks statements sequentially; scopes are reset at
/// if/else and match arm boundaries.
fn validate_goto_types(
    block: &Block,
    states: &HashMap<&str, &StateDecl>,
    ctx: &mut TypeContext<'_>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Let {
                name, ty, value, ..
            } => {
                let binding_type = if let Some(annotated) = ty {
                    Some(annotated.clone())
                } else {
                    infer_expr_type(value, ctx)
                };
                if let Some(resolved_type) = binding_type {
                    ctx.variables.insert(name.clone(), resolved_type);
                }
            }
            Statement::Goto { state, args, span } => {
                if let Some(target) = states.get(state.as_str()) {
                    // Only check types when arity already matches — arity is checked separately.
                    if target.fields.len() == args.len() {
                        for (i, (field, arg)) in target.fields.iter().zip(args.iter()).enumerate() {
                            if let Some(arg_type) = infer_expr_type(arg, ctx) {
                                if !types_compatible(&field.ty, &arg_type, ctx.generic_params) {
                                    report.errors.push(GustError {
                                        file: file.to_string(),
                                        line: span.start_line,
                                        col: span.start_col,
                                        message: format!(
                                            "goto '{}' argument {} has type {}, but field '{}' expects {}",
                                            state,
                                            i + 1,
                                            format_type_expr(&arg_type),
                                            field.name,
                                            format_type_expr(&field.ty),
                                        ),
                                        note: Some(
                                            "argument types must match target state field types"
                                                .to_string(),
                                        ),
                                        help: None,
                                    });
                                }
                            }
                            // arg_type == None: can't determine, skip (conservative).
                        }
                    }
                }
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                // Let-bindings inside branches don't leak to siblings or parents.
                let saved = ctx.variables.clone();
                validate_goto_types(then_block, states, ctx, file, report);
                ctx.variables = saved.clone();
                if let Some(else_block) = else_block {
                    validate_goto_types(else_block, states, ctx, file, report);
                }
                ctx.variables = saved;
            }
            Statement::Match { arms, .. } => {
                let saved = ctx.variables.clone();
                for arm in arms {
                    ctx.variables = saved.clone();
                    validate_goto_types(&arm.body, states, ctx, file, report);
                }
                ctx.variables = saved;
            }
            _ => {}
        }
    }
}

// === Handler expression type checking (issue #30 items 2 and 4) ===

/// Walks handler statements and checks:
/// - **Item 2**: `let x: T = perform e(...)` — annotated `T` must match the effect's return type.
/// - **Item 4**: binary operator operand types — both sides must be type-compatible.
///
/// Tracks let bindings with the same scoping rules as `validate_goto_types`
/// (branches get an isolated scope copy).
fn validate_expression_types(
    block: &Block,
    ctx: &mut TypeContext<'_>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Let {
                name, ty, value, ..
            } => {
                // Item 2: explicitly annotated let with a perform RHS must agree with the effect's return type.
                if let (Some(annotated), Expr::Perform(effect_name, _, _)) = (ty, value) {
                    if let Some(effect) = ctx.effects.get(effect_name.as_str()) {
                        if !types_compatible(annotated, &effect.return_type, ctx.generic_params) {
                            report.errors.push(GustError {
                                file: file.to_string(),
                                line: effect.span.start_line,
                                col: effect.span.start_col,
                                message: format!(
                                    "let '{}' annotated as {}, but effect '{}' returns {}",
                                    name,
                                    format_type_expr(annotated),
                                    effect_name,
                                    format_type_expr(&effect.return_type),
                                ),
                                note: Some(
                                    "let-binding annotation must match the effect's declared return type"
                                        .to_string(),
                                ),
                                help: None,
                            });
                        }
                    }
                }

                // Item 4: walk the RHS for binop operand mismatches.
                check_binop_types_in_expr(value, ctx, file, report);

                // Update scope with the binding's inferred or annotated type.
                let binding_type = if let Some(annotated) = ty {
                    Some(annotated.clone())
                } else {
                    infer_expr_type(value, ctx)
                };
                if let Some(resolved_type) = binding_type {
                    ctx.variables.insert(name.clone(), resolved_type);
                }
            }
            Statement::Goto { args, .. } => {
                for arg in args {
                    check_binop_types_in_expr(arg, ctx, file, report);
                }
            }
            Statement::Perform { args, .. } => {
                for arg in args {
                    check_binop_types_in_expr(arg, ctx, file, report);
                }
            }
            Statement::Return(value) | Statement::Expr(value) => {
                check_binop_types_in_expr(value, ctx, file, report);
            }
            Statement::Send { message, .. } => {
                check_binop_types_in_expr(message, ctx, file, report);
            }
            Statement::Spawn { args, .. } => {
                for arg in args {
                    check_binop_types_in_expr(arg, ctx, file, report);
                }
            }
            Statement::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                check_binop_types_in_expr(condition, ctx, file, report);
                let saved = ctx.variables.clone();
                validate_expression_types(then_block, ctx, file, report);
                ctx.variables = saved.clone();
                if let Some(else_block) = else_block {
                    validate_expression_types(else_block, ctx, file, report);
                }
                ctx.variables = saved;
            }
            Statement::Match { scrutinee, arms } => {
                check_binop_types_in_expr(scrutinee, ctx, file, report);
                let saved = ctx.variables.clone();
                for arm in arms {
                    ctx.variables = saved.clone();
                    validate_expression_types(&arm.body, ctx, file, report);
                }
                ctx.variables = saved;
            }
        }
    }
}

/// Recursively check binary operator operand type compatibility in an expression.
fn check_binop_types_in_expr(
    expr: &Expr,
    ctx: &TypeContext<'_>,
    file: &str,
    report: &mut ValidationReport,
) {
    match expr {
        Expr::BinOp(left, op, right, span) => {
            check_binop_types_in_expr(left, ctx, file, report);
            check_binop_types_in_expr(right, ctx, file, report);

            // Only report when we can infer both sides AND neither is generic.
            if let (Some(left_ty), Some(right_ty)) =
                (infer_expr_type(left, ctx), infer_expr_type(right, ctx))
            {
                if !types_compatible(&left_ty, &right_ty, ctx.generic_params) {
                    use crate::ast::BinOp::*;
                    let op_str = match op {
                        Add => "+",
                        Sub => "-",
                        Mul => "*",
                        Div => "/",
                        Mod => "%",
                        Eq => "==",
                        Neq => "!=",
                        Lt => "<",
                        Lte => "<=",
                        Gt => ">",
                        Gte => ">=",
                        And => "&&",
                        Or => "||",
                    };
                    report.errors.push(GustError {
                        file: file.to_string(),
                        line: span.start_line,
                        col: span.start_col,
                        message: format!(
                            "binary operator '{}' has incompatible operand types: {} vs {}",
                            op_str,
                            format_type_expr(&left_ty),
                            format_type_expr(&right_ty),
                        ),
                        note: Some(
                            "both operands of a binary operator must have the same type"
                                .to_string(),
                        ),
                        help: None,
                    });
                }
            }
        }
        Expr::UnaryOp(_, inner) => check_binop_types_in_expr(inner, ctx, file, report),
        Expr::FieldAccess(base, _) => check_binop_types_in_expr(base, ctx, file, report),
        Expr::FnCall(_, args) | Expr::Perform(_, args, _) => {
            for a in args {
                check_binop_types_in_expr(a, ctx, file, report);
            }
        }
        // Leaves: nothing to recurse into.
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::StringLit(_)
        | Expr::BoolLit(_)
        | Expr::Ident(_)
        | Expr::Path(_, _) => {}
    }
}

// === If/else branch termination consistency (issue #30 item 3) ===

/// Warn when an `if/else` has one branch that terminates (goto/return/exhaustive match)
/// and another branch that falls through. This catches "forgot to goto in one branch"
/// bugs that would otherwise only surface as the generic whole-handler fall-through warning.
fn check_if_branch_consistency(
    block: &Block,
    handler_name: &str,
    enum_variants: &HashMap<String, Vec<String>>,
    file: &str,
    report: &mut ValidationReport,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::If {
                then_block,
                else_block,
                span,
                ..
            } => {
                // Recurse first so nested if/else are checked independently.
                check_if_branch_consistency(then_block, handler_name, enum_variants, file, report);
                if let Some(else_block) = else_block {
                    check_if_branch_consistency(
                        else_block,
                        handler_name,
                        enum_variants,
                        file,
                        report,
                    );

                    let then_terminates = block_always_terminates(then_block, enum_variants);
                    let else_terminates = block_always_terminates(else_block, enum_variants);
                    if then_terminates != else_terminates {
                        let (fall_through_branch, terminating_branch) = if then_terminates {
                            ("else", "then")
                        } else {
                            ("then", "else")
                        };
                        report.warnings.push(GustWarning {
                            file: file.to_string(),
                            line: span.start_line,
                            col: span.start_col,
                            message: format!(
                                "handler '{}' has inconsistent if/else: the {} branch transitions but the {} branch may fall through",
                                handler_name, terminating_branch, fall_through_branch,
                            ),
                            note: Some(
                                "either add a goto/return to the fall-through branch, or remove the goto from the other branch"
                                    .to_string(),
                            ),
                            help: None,
                        });
                    }
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    check_if_branch_consistency(
                        &arm.body,
                        handler_name,
                        enum_variants,
                        file,
                        report,
                    );
                }
            }
            _ => {}
        }
    }
}

// === Handler-safety diagnostics for actions (#40 item 4) ===

/// Classification of a single perform or external-call side effect, used to
/// reason about action placement within a handler body.
///
/// Each `(SideEffectKind, Span)` pair in the analysis list corresponds to one
/// observable side effect — pure bindings and control-flow produce no entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SideEffectKind {
    /// A `perform` of a declared `effect` — assumed replay-safe.
    Effect,
    /// A `perform` of a declared `action` — not replay-safe.
    Action,
    /// `send` or `spawn` — externally visible but not a declared action.
    OtherExternal,
}

/// Warn when a handler performs more than one `action`, or when an action
/// is not the last side-effectful step before the handler transitions.
/// Branches (if/else, match arms) are analyzed as independent sequences
/// so an action in one branch and another in a sibling branch don't
/// falsely trigger the "more than one" rule.
fn check_handler_action_safety(
    block: &Block,
    handler_name: &str,
    handler_span: Span,
    actions: &HashSet<String>,
    file: &str,
    report: &mut ValidationReport,
) {
    if actions.is_empty() {
        return;
    }
    walk_sequence_for_action_safety(block, handler_name, handler_span, actions, file, report);
}

fn walk_sequence_for_action_safety(
    block: &Block,
    handler_name: &str,
    handler_span: Span,
    actions: &HashSet<String>,
    file: &str,
    report: &mut ValidationReport,
) {
    // Classify each top-level statement into zero or more (kind, span) pairs.
    // A single expression statement can contain multiple nested `perform` calls
    // (e.g. `let x = perform a() + perform b()`), so we flatten everything into
    // one ordered list before applying the rules.
    let kinds: Vec<(SideEffectKind, Span)> = block
        .statements
        .iter()
        .flat_map(|s| classify_statement_side_effects(s, actions))
        .collect();

    // Rule 1: more than one action in this sequence.
    let action_count = kinds
        .iter()
        .filter(|(k, _)| matches!(k, SideEffectKind::Action))
        .count();
    if action_count > 1 {
        // Point at the first offending action perform rather than the handler header.
        let first_action_span = kinds
            .iter()
            .find(|(k, _)| matches!(k, SideEffectKind::Action))
            .map(|(_, s)| *s)
            .unwrap_or(handler_span);
        report.warnings.push(GustWarning {
            file: file.to_string(),
            line: first_action_span.start_line,
            col: first_action_span.start_col,
            message: format!(
                "handler '{handler_name}' performs {action_count} actions in a single sequence"
            ),
            note: Some(
                "actions are not replay-safe; prefer at most one per handler \
                 path so workflow runtimes can checkpoint cleanly (#40)"
                    .to_string(),
            ),
            help: None,
        });
    }

    // Rule 2: action is followed by another side-effectful step.
    if let Some(last_action_idx) = kinds
        .iter()
        .rposition(|(k, _)| matches!(k, SideEffectKind::Action))
    {
        // Every entry in `kinds` is a real side effect (pure bindings produce
        // no entries), so any entry after the last action is a later side effect.
        let has_later_side_effect = kinds.len() > last_action_idx + 1;
        if has_later_side_effect {
            let action_span = kinds[last_action_idx].1;
            report.warnings.push(GustWarning {
                file: file.to_string(),
                line: action_span.start_line,
                col: action_span.start_col,
                message: format!(
                    "handler '{handler_name}' has side-effectful steps after an action"
                ),
                note: Some(
                    "an `action` should be the last externally visible step before the \
                     transition so workflows can resume at a clean checkpoint (#40)"
                        .to_string(),
                ),
                help: None,
            });
        }
    }

    // Recurse into branches — each arm is an independent sequence.
    for stmt in &block.statements {
        match stmt {
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                walk_sequence_for_action_safety(
                    then_block,
                    handler_name,
                    handler_span,
                    actions,
                    file,
                    report,
                );
                if let Some(else_block) = else_block {
                    walk_sequence_for_action_safety(
                        else_block,
                        handler_name,
                        handler_span,
                        actions,
                        file,
                        report,
                    );
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    walk_sequence_for_action_safety(
                        &arm.body,
                        handler_name,
                        handler_span,
                        actions,
                        file,
                        report,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Classify a statement into zero or more `(SideEffectKind, Span)` pairs,
/// one per observable side effect found in the statement.
///
/// Unlike the old single-return version, this correctly handles expression
/// statements that embed multiple `perform` calls (e.g. binops).  Each
/// `Expr::Perform` is emitted as its own entry so the action count and span
/// are both accurate.
fn classify_statement_side_effects(
    stmt: &Statement,
    actions: &HashSet<String>,
) -> Vec<(SideEffectKind, Span)> {
    match stmt {
        Statement::Perform { effect, span, .. } => {
            let kind = if actions.contains(effect) {
                SideEffectKind::Action
            } else {
                SideEffectKind::Effect
            };
            vec![(kind, *span)]
        }
        Statement::Send { span, .. } => vec![(SideEffectKind::OtherExternal, *span)],
        Statement::Spawn { span, .. } => vec![(SideEffectKind::OtherExternal, *span)],
        Statement::Let { value, .. } | Statement::Expr(value) | Statement::Return(value) => {
            collect_performs_from_expr(value, actions)
        }
        // Control-flow statements are inspected recursively by the caller; at
        // this level they carry no direct side effect.
        Statement::If { .. } | Statement::Match { .. } | Statement::Goto { .. } => vec![],
    }
}

/// Walk an expression tree and collect every `perform` as a `(SideEffectKind, Span)`.
///
/// This correctly handles nested performs inside binary operators, unary operators,
/// function call arguments, and field access chains — so
/// `let x = perform action_a() + perform action_b()` yields two Action entries.
fn collect_performs_from_expr(
    expr: &Expr,
    actions: &HashSet<String>,
) -> Vec<(SideEffectKind, Span)> {
    match expr {
        Expr::Perform(name, args, span) => {
            let kind = if actions.contains(name.as_str()) {
                SideEffectKind::Action
            } else {
                SideEffectKind::Effect
            };
            // Also walk arguments for any nested performs.
            let mut result = vec![(kind, *span)];
            for arg in args {
                result.extend(collect_performs_from_expr(arg, actions));
            }
            result
        }
        Expr::BinOp(left, _, right, _) => {
            let mut result = collect_performs_from_expr(left, actions);
            result.extend(collect_performs_from_expr(right, actions));
            result
        }
        Expr::UnaryOp(_, inner) => collect_performs_from_expr(inner, actions),
        Expr::FieldAccess(base, _) => collect_performs_from_expr(base, actions),
        Expr::FnCall(_, args) => {
            let mut result = Vec::new();
            for arg in args {
                result.extend(collect_performs_from_expr(arg, actions));
            }
            result
        }
        // Leaves and non-perform expressions produce no side-effect entries.
        _ => vec![],
    }
}

fn suggest_name(name: &str, names: &[String]) -> Option<String> {
    names
        .iter()
        .filter_map(|candidate| {
            let d = levenshtein(name, candidate);
            if d <= 2 { Some((d, candidate)) } else { None }
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| format!("did you mean '{}'?", c))
}
