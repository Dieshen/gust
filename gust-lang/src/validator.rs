use crate::ast::{Block, Expr, Program, StateDecl, Statement};
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

    for machine in &program.machines {
        let state_names: Vec<String> = machine.states.iter().map(|s| s.name.clone()).collect();
        let state_set: HashSet<String> = state_names.iter().cloned().collect();
        let declared_effects: HashSet<String> =
            machine.effects.iter().map(|e| e.name.clone()).collect();
        let declared_effect_names: Vec<String> = machine.effects.iter().map(|e| e.name.clone()).collect();
        let state_fields: HashMap<&str, &StateDecl> =
            machine.states.iter().map(|s| (s.name.as_str(), s)).collect();

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

        let mut used_declared_effects = HashSet::new();
        let mut unknown_effects = Vec::new();
        for handler in &machine.handlers {
            collect_effects_from_block(
                &handler.body,
                &declared_effects,
                &mut used_declared_effects,
                &mut unknown_effects,
            );
            validate_goto_arity(
                &handler.body,
                &state_fields,
                &locator,
                file,
                &mut report,
            );
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
                            note: Some("goto argument count must match target state fields".to_string()),
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

fn collect_effects_from_block(
    block: &Block,
    declared: &HashSet<String>,
    used_declared: &mut HashSet<String>,
    unknown: &mut Vec<String>,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Perform { effect, .. } => register_effect(effect, declared, used_declared, unknown),
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
            Statement::Match { scrutinee, arms } => {
                collect_effects_from_expr(scrutinee, declared, used_declared, unknown);
                for arm in arms {
                    collect_effects_from_block(&arm.body, declared, used_declared, unknown);
                }
            }
        }
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
        for pattern in [format!("effect {effect}("), format!("async effect {effect}(")] {
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

    fn find(&self, needle: &str) -> (usize, usize) {
        for (i, line) in self.lines.iter().enumerate() {
            if let Some(col) = line.find(needle) {
                return (i + 1, col + 1);
            }
        }
        (1, 1)
    }
}
