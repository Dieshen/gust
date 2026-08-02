// Go Code Generator: emits Go source code from a Gust AST.
//
// Each `machine` becomes:
//   - State constants via iota
//   - A state data struct per state variant (Go lacks sum types)
//   - An effects interface
//   - A machine struct with current state + state data
//   - Transition methods with runtime state validation
//
// Key differences from Rust codegen:
//   - No sum types -> use state enum (iota) + interface for state data
//   - No exhaustiveness checking -> runtime panics on invalid transitions
//   - Effects trait -> Go interface (more idiomatic)
//   - Serde -> json struct tags (free with encoding/json)
//   - Error handling -> Go error returns, no Result<T,E>: an effect declared
//     `-> Result<T, E>` becomes `(T, error)` and an Ok/Err match becomes a nil
//     check, so `E` itself is erased
//   - No destructuring in the state check -> source-state fields the handler
//     reads by bare name are lifted into locals

use crate::ast::*;
use crate::codegen_common::{
    collect_bare_idents, collect_known_types, collect_let_bindings, collect_referenced_idents,
    detect_ctx_param, escape_string_literal, expr_references_ctx, handler_used_channels,
    handler_uses_perform, handler_uses_spawn, has_timeout_transition, machine_known_types,
    to_pascal_case, to_snake_case,
};
use std::collections::{HashMap, HashSet};

/// How a Gust effect's declared return type maps onto Go's return values.
///
/// Go has no `Result`, so an effect declared `-> Result<T, E>` lowers to the
/// `(T, error)` idiom — the same shape an `async` effect already used. `E` is
/// erased to Go's `error` interface, because that is the only error type the
/// idiom admits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GoEffectReturn {
    /// Nothing at all: a synchronous, infallible effect returning `()`.
    Nothing,
    /// A single value and no error: a synchronous, infallible effect.
    Value,
    /// Only `error`: a fallible effect whose success type is `()`.
    ErrorOnly,
    /// `(T, error)`: a fallible effect with a success value.
    ValueAndError,
}

/// A `let` binding holding the value half of a `(T, error)` pair, consumed by a
/// following `match` with `Ok`/`Err` arms.
#[derive(Clone)]
struct ResultMatch {
    /// The `E` of the producing effect's `Result<T, E>`, when declared.
    error_type: Option<TypeExpr>,
    /// Whether anything reads the success value. When nothing does, it has to be
    /// discarded — Go rejects a local that is declared and never used.
    binds_value: bool,
}

/// Go code generator. Consumes a validated [`Program`] and emits
/// idiomatic `.g.go` source.
pub struct GoCodegen {
    output: String,
    indent: usize,
    ctx_param: Option<String>,
    from_state_name: Option<String>,
    machine_name: Option<String>,
    /// The type-argument list (`[T]`) of the machine being emitted, empty for a
    /// non-generic machine. Every reference to a generated generic type needs
    /// it: Go rejects `&BoxFullData{...}` for `type BoxFullData[T any]` with
    /// "cannot use generic type without instantiation".
    machine_generic_use: String,
    /// Program-wide type names — declared types plus the language builtins.
    program_types: HashSet<String>,
    /// `program_types` plus the generic parameters of the machine currently
    /// being emitted. This is what ctx detection consults.
    known_types: HashSet<String>,
    async_effects: HashSet<String>,
    /// How each of the current machine's effects maps onto Go return values.
    effect_returns: HashMap<String, GoEffectReturn>,
    /// For each effect declared `-> Result<T, E>`, that `E`.
    result_effects: HashMap<String, Option<TypeExpr>>,
    /// `Result` bindings in the handler currently being emitted that a `match`
    /// destructures, keyed by binding name.
    result_matches: HashMap<String, ResultMatch>,
    /// Identifiers the handler currently being emitted actually reads.
    ///
    /// Go rejects an unused local outright, so a `let` the handler never reads
    /// has to be lowered to a discard rather than a binding. Rust only warns,
    /// which is why this backend needs the check and the Rust one does not.
    referenced_idents: HashSet<String>,
}

impl GoCodegen {
    /// Construct a new Go code generator.
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            ctx_param: None,
            from_state_name: None,
            machine_name: None,
            machine_generic_use: String::new(),
            program_types: HashSet::new(),
            known_types: HashSet::new(),
            async_effects: HashSet::new(),
            effect_returns: HashMap::new(),
            result_effects: HashMap::new(),
            result_matches: HashMap::new(),
            referenced_idents: HashSet::new(),
        }
    }

    /// Generate the full `.g.go` source for `program` under the given
    /// Go `package_name`.
    pub fn generate(mut self, program: &Program, package_name: &str) -> String {
        self.emit_prelude(program, package_name);

        self.program_types = collect_known_types(program);

        for channel in &program.channels {
            self.emit_channel_decl(channel);
            self.newline();
        }

        if program.machines.iter().any(|m| !m.supervises.is_empty()) {
            self.emit_supervision_types();
            self.newline();
        }

        // Emit type declarations as Go structs
        for type_decl in &program.types {
            self.emit_type_decl(type_decl);
            self.newline();
        }

        // Emit each machine
        for machine in &program.machines {
            self.emit_machine(machine, &program.channels);
            self.newline();
        }

        self.finish()
    }

    fn finish(mut self) -> String {
        while self.output.ends_with("\n\n") {
            self.output.pop();
        }
        self.output
    }

    fn emit_prelude(&mut self, program: &Program, package_name: &str) {
        self.line("// Code generated by Gust compiler — DO NOT EDIT.");
        self.newline();
        self.line(&format!("package {package_name}"));
        self.newline();

        let mut imports = vec!["encoding/json".to_string(), "fmt".to_string()];
        for use_path in &program.uses {
            if use_path.segments.is_empty() {
                continue;
            }
            // `std::*` is a Gust-virtual namespace for stdlib machines/types.
            // The consumer's build pipeline is responsible for compiling
            // stdlib sources into the same Go package, so no import is
            // emitted. See issue #66.
            if use_path.segments.first().map(String::as_str) == Some("std") {
                continue;
            }
            imports.push(map_use_path_to_go_import(&use_path.segments));
        }
        if program
            .machines
            .iter()
            .any(|m| m.handlers.iter().any(|h| h.is_async))
            || has_timeout_transition(program)
        {
            imports.push("context".to_string());
        }
        if has_timeout_transition(program) {
            imports.push("time".to_string());
        }
        if program
            .channels
            .iter()
            .any(|c| matches!(c.mode, ChannelMode::Broadcast))
        {
            imports.push("sync".to_string());
        }
        imports.sort();
        imports.dedup();

        self.line("import (");
        self.indent += 1;
        for import in imports {
            self.line(&format!("\"{import}\""));
        }
        self.indent -= 1;
        self.line(")");
        self.newline();
        // Suppress unused import warnings
        self.line("var _ = json.Marshal");
        self.line("var _ = fmt.Errorf");
        if has_timeout_transition(program) {
            self.line("var _ = time.Second");
        }
        if program
            .channels
            .iter()
            .any(|c| matches!(c.mode, ChannelMode::Broadcast))
        {
            self.line("var _ sync.RWMutex");
        }
        self.newline();
    }

    fn emit_channel_decl(&mut self, channel: &ChannelDecl) {
        let struct_name = format!("{}Channel", channel.name);
        let msg_ty = self.type_expr_to_go(&channel.message_type);
        let capacity = channel.capacity.unwrap_or(1024).max(1);
        match channel.mode {
            ChannelMode::Broadcast => {
                self.line(&format!("type {struct_name} struct {{"));
                self.indent += 1;
                self.line(&format!("subs []chan {msg_ty}"));
                self.line("capacity int");
                self.line("mu sync.RWMutex");
                self.indent -= 1;
                self.line("}");
                self.newline();
                self.line(&format!("func New{struct_name}() *{struct_name} {{"));
                self.indent += 1;
                self.line(&format!("return &{struct_name}{{capacity: {capacity}}}"));
                self.indent -= 1;
                self.line("}");
                self.newline();
                self.line(&format!(
                    "func (c *{struct_name}) Subscribe() <-chan {msg_ty} {{"
                ));
                self.indent += 1;
                self.line(&format!("ch := make(chan {msg_ty}, c.capacity)"));
                self.line("c.mu.Lock()");
                self.line("c.subs = append(c.subs, ch)");
                self.line("c.mu.Unlock()");
                self.line("return ch");
                self.indent -= 1;
                self.line("}");
                self.newline();
                self.line(&format!("func (c *{struct_name}) Publish(msg {msg_ty}) {{"));
                self.indent += 1;
                self.line("c.mu.RLock()");
                self.line("defer c.mu.RUnlock()");
                self.line("for _, sub := range c.subs {");
                self.indent += 1;
                self.line("select {");
                self.indent += 1;
                self.line("case sub <- msg:");
                self.line("default:");
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            ChannelMode::Mpsc => {
                self.line(&format!("type {struct_name} struct {{"));
                self.indent += 1;
                self.line(&format!("ch chan {msg_ty}"));
                self.indent -= 1;
                self.line("}");
                self.newline();
                self.line(&format!("func New{struct_name}() *{struct_name} {{"));
                self.indent += 1;
                self.line(&format!(
                    "return &{struct_name}{{ch: make(chan {msg_ty}, {capacity})}}"
                ));
                self.indent -= 1;
                self.line("}");
                self.newline();
                self.line(&format!(
                    "func (c *{struct_name}) Send(msg {msg_ty}) bool {{"
                ));
                self.indent += 1;
                self.line("select {");
                self.indent += 1;
                self.line("case c.ch <- msg:");
                self.indent += 1;
                self.line("return true");
                self.indent -= 1;
                self.line("default:");
                self.indent += 1;
                self.line("return false");
                self.indent -= 1;
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
                self.newline();
                self.line(&format!(
                    "func (c *{struct_name}) Receive() <-chan {msg_ty} {{"
                ));
                self.indent += 1;
                self.line("return c.ch");
                self.indent -= 1;
                self.line("}");
            }
        }
    }

    fn emit_supervision_types(&mut self) {
        self.line("type SupervisionStrategy string");
        self.newline();
        self.line("const (");
        self.indent += 1;
        self.line("OneForOne SupervisionStrategy = \"one_for_one\"");
        self.line("OneForAll SupervisionStrategy = \"one_for_all\"");
        self.line("RestForOne SupervisionStrategy = \"rest_for_one\"");
        self.indent -= 1;
        self.line(")");
        self.newline();
        self.line("type SupervisionSpec struct {");
        self.indent += 1;
        self.line("Child string");
        self.line("Strategy SupervisionStrategy");
        self.indent -= 1;
        self.line("}");
        self.newline();
        self.line("type SupervisorRuntime interface {");
        self.indent += 1;
        self.line("SpawnNamed(name string, fn func() error) error");
        self.indent -= 1;
        self.line("}");
    }

    fn emit_supervision_metadata(&mut self, machine: &MachineDecl) {
        // Host-supplied runners, mirroring the Rust backend's
        // `{Machine}Supervision` trait. Gust constructs the child; a machine is
        // passive, so the host owns the loop that drives it — the same split
        // the effects interface already uses.
        self.line(&format!(
            "// {}Children supplies a runner for each child {} supervises.",
            machine.name, machine.name
        ));
        self.line(&format!("type {}Children interface {{", machine.name));
        self.indent += 1;
        for spec in &machine.supervises {
            let child = &spec.child_machine;
            self.line(&format!(
                "// Run{child} drives a supervised {child} to completion. A non-nil"
            ));
            self.line("// error marks the child failed, which is what the restart");
            self.line("// strategy acts on.");
            self.line(&format!("Run{child}(child *{child}) error"));
        }
        self.indent -= 1;
        self.line("}");
        self.newline();

        self.line(&format!(
            "var {}Supervision = []SupervisionSpec{{",
            machine.name
        ));
        self.indent += 1;
        for spec in &machine.supervises {
            let strategy = match spec.strategy {
                SupervisionStrategy::OneForOne => "OneForOne",
                SupervisionStrategy::OneForAll => "OneForAll",
                SupervisionStrategy::RestForOne => "RestForOne",
            };
            self.line(&format!(
                "{{Child: \"{}\", Strategy: {strategy}}},",
                spec.child_machine
            ));
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_channel_helpers(&mut self, machine: &MachineDecl, channels: &[ChannelDecl]) {
        let generic_use = go_generic_use(&machine.generic_params);
        for channel_name in &machine.sends {
            if let Some(channel) = channels.iter().find(|c| c.name == *channel_name) {
                let msg_ty = self.type_expr_to_go(&channel.message_type);
                let method = format!("Send{}", to_pascal_case(channel_name));
                let channel_ty = format!("*{}Channel", channel.name);
                self.line(&format!(
                    "func (m *{}) {method}(msg {msg_ty}, ch {channel_ty}) {{",
                    machine.name.to_string() + &generic_use
                ));
                self.indent += 1;
                match channel.mode {
                    ChannelMode::Broadcast => self.line("ch.Publish(msg)"),
                    ChannelMode::Mpsc => self.line("ch.Send(msg)"),
                }
                self.indent -= 1;
                self.line("}");
            }
        }
    }

    // === Type Declarations ===

    fn emit_type_decl(&mut self, decl: &TypeDecl) {
        match decl {
            TypeDecl::Struct { name, fields, .. } => {
                self.line(&format!("type {name} struct {{"));
                self.indent += 1;
                for field in fields {
                    let field_name = to_pascal_case(&field.name);
                    let go_type = self.type_expr_to_go(&field.ty);
                    let json_tag = &field.name;
                    self.line(&format!("{field_name} {go_type} `json:\"{json_tag}\"`"));
                }
                self.indent -= 1;
                self.line("}");
            }
            TypeDecl::Enum { name, variants, .. } => {
                self.line(&format!("type {name} string"));
                self.newline();
                self.line("const (");
                self.indent += 1;
                for variant in variants {
                    self.line(&format!(
                        "{name}{} {name} = \"{}\"",
                        variant.name, variant.name
                    ));
                }
                self.indent -= 1;
                self.line(")");
            }
        }
    }

    // === Machine ===

    fn emit_machine(&mut self, machine: &MachineDecl, channels: &[ChannelDecl]) {
        let name = &machine.name;
        let generic_decl = go_generic_decl(&machine.generic_params);
        let generic_use = go_generic_use(&machine.generic_params);

        self.machine_generic_use = generic_use.clone();
        self.known_types = machine_known_types(&self.program_types, machine);
        // Populated before the effects interface is emitted, because the
        // interface's method shapes and the handler bodies that call them have
        // to agree on how many values each effect returns.
        self.async_effects = machine
            .effects
            .iter()
            .filter(|e| e.is_async)
            .map(|e| e.name.clone())
            .collect();
        self.effect_returns = machine
            .effects
            .iter()
            .map(|e| (e.name.clone(), go_effect_return(e)))
            .collect();
        self.result_effects = machine
            .effects
            .iter()
            .filter_map(|e| result_error_type(&e.return_type).map(|err| (e.name.clone(), err)))
            .collect();

        // --- State enum via iota ---
        self.emit_state_constants(name, &machine.states, &generic_decl);
        self.newline();

        // --- State name helper ---
        self.emit_state_name_func(name, &machine.states, &generic_use);
        self.newline();

        // --- State data structs ---
        for state in &machine.states {
            if !state.fields.is_empty() {
                self.emit_state_data_struct(name, state, &generic_decl);
                self.newline();
            }
        }

        // --- Effects interface ---
        if !machine.effects.is_empty() {
            self.emit_effects_interface(name, &machine.effects, &generic_decl);
            self.newline();
        }

        // --- Machine struct ---
        self.emit_machine_struct(name, &machine.states, &generic_decl, &generic_use);
        self.newline();

        // --- clearStateData helper ---
        self.emit_clear_state_data(name, &machine.states, &generic_use);
        if machine.states.iter().any(|s| !s.fields.is_empty()) {
            self.newline();
        }

        // --- Constructor ---
        self.emit_constructor(name, &machine.states, &generic_decl, &generic_use);
        self.newline();

        self.emit_channel_helpers(machine, channels);
        if !machine.sends.is_empty() {
            self.newline();
        }

        if !machine.supervises.is_empty() {
            self.emit_supervision_metadata(machine);
            self.newline();
        }

        // --- Transition error ---
        self.emit_transition_error(name);
        self.newline();

        // --- Transition methods ---
        for transition in &machine.transitions {
            self.emit_transition_method(
                name,
                transition,
                &machine.handlers,
                &machine.states,
                &machine.effects,
                channels,
                &generic_use,
            );
            self.newline();
        }
        self.async_effects.clear();
        self.effect_returns.clear();
        self.result_effects.clear();

        // --- JSON marshaling ---
        self.emit_json_helpers(name, &generic_decl, &generic_use);
    }

    fn emit_state_constants(
        &mut self,
        machine_name: &str,
        states: &[StateDecl],
        generic_decl: &str,
    ) {
        let type_name = format!("{machine_name}State");
        self.line(&format!("type {type_name}{generic_decl} int"));
        self.newline();
        self.line("const (");
        self.indent += 1;
        for (i, state) in states.iter().enumerate() {
            if i == 0 {
                self.line(&format!("{machine_name}State{} = iota", state.name));
            } else {
                self.line(&format!("{machine_name}State{}", state.name));
            }
        }
        self.indent -= 1;
        self.line(")");
    }

    fn emit_state_name_func(
        &mut self,
        machine_name: &str,
        states: &[StateDecl],
        generic_use: &str,
    ) {
        let type_name = format!("{machine_name}State");
        self.line(&format!(
            "func (s {type_name}{generic_use}) String() string {{"
        ));
        self.indent += 1;
        self.line("switch s {");
        for state in states {
            self.line(&format!("case {machine_name}State{}:", state.name));
            self.indent += 1;
            self.line(&format!("return \"{}\"", state.name));
            self.indent -= 1;
        }
        self.line("default:");
        self.indent += 1;
        self.line("return \"Unknown\"");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
    }

    fn emit_state_data_struct(
        &mut self,
        machine_name: &str,
        state: &StateDecl,
        generic_decl: &str,
    ) {
        let struct_name = format!("{machine_name}{}Data", state.name);
        self.line(&format!("type {struct_name}{generic_decl} struct {{"));
        self.indent += 1;
        for field in &state.fields {
            let field_name = to_pascal_case(&field.name);
            let go_type = self.type_expr_to_go(&field.ty);
            let json_tag = &field.name;
            self.line(&format!("{field_name} {go_type} `json:\"{json_tag}\"`"));
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_effects_interface(
        &mut self,
        machine_name: &str,
        effects: &[EffectDecl],
        generic_decl: &str,
    ) {
        self.line(&format!(
            "type {machine_name}Effects{generic_decl} interface {{"
        ));
        self.indent += 1;
        for effect in effects {
            let method_name = to_pascal_case(&effect.name);
            let mut params: Vec<String> = Vec::new();
            if effect.is_async {
                params.push("ctx context.Context".to_string());
            }
            params.extend(
                effect
                    .params
                    .iter()
                    .map(|p| format!("{} {}", p.name, self.type_expr_to_go(&p.ty))),
            );
            // `type_expr_to_go` already unwraps `Result<T, E>` to `T`, so this
            // is the success type in both the plain and the fallible case.
            let return_type = self.type_expr_to_go(&effect.return_type);
            // Keep the annotation directly above the method so downstream
            // tooling can parse generated Go without reading .gu source.
            self.line(&format!(
                "// gust:{} -- {}",
                effect.kind.keyword(),
                effect.kind.annotation_description()
            ));
            let params = params.join(", ");
            match go_effect_return(effect) {
                GoEffectReturn::Nothing => self.line(&format!("{method_name}({params})")),
                GoEffectReturn::Value => {
                    self.line(&format!("{method_name}({params}) {return_type}"))
                }
                GoEffectReturn::ErrorOnly => self.line(&format!("{method_name}({params}) error")),
                GoEffectReturn::ValueAndError => {
                    self.line(&format!("{method_name}({params}) ({return_type}, error)"))
                }
            }
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_machine_struct(
        &mut self,
        machine_name: &str,
        states: &[StateDecl],
        generic_decl: &str,
        generic_use: &str,
    ) {
        let state_type = format!("{machine_name}State");
        self.line(&format!("type {machine_name}{generic_decl} struct {{"));
        self.indent += 1;
        self.line(&format!("State {state_type}{generic_use} `json:\"state\"`"));
        // One optional data field per state that has data
        for state in states {
            if !state.fields.is_empty() {
                let data_type = format!("{machine_name}{}Data", state.name);
                let json_tag = format!("{}_data,omitempty", to_snake_case(&state.name));
                self.line(&format!(
                    "{}Data *{data_type}{generic_use} `json:\"{json_tag}\"`",
                    state.name
                ));
            }
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_constructor(
        &mut self,
        machine_name: &str,
        states: &[StateDecl],
        generic_decl: &str,
        generic_use: &str,
    ) {
        let first = match states.first() {
            Some(s) => s,
            None => return,
        };

        if first.fields.is_empty() {
            self.line(&format!(
                "func New{machine_name}{generic_decl}() *{machine_name}{generic_use} {{"
            ));
            self.indent += 1;
            self.line(&format!("return &{machine_name}{generic_use}{{"));
            self.indent += 1;
            self.line(&format!("State: {machine_name}State{},", first.name));
            self.indent -= 1;
            self.line("}");
            self.indent -= 1;
            self.line("}");
        } else {
            let params: Vec<String> = first
                .fields
                .iter()
                .map(|f| format!("{} {}", f.name, self.type_expr_to_go(&f.ty)))
                .collect();
            self.line(&format!(
                "func New{machine_name}{generic_decl}({}) *{machine_name}{generic_use} {{",
                params.join(", ")
            ));
            self.indent += 1;
            self.line(&format!("return &{machine_name}{generic_use}{{"));
            self.indent += 1;
            self.line(&format!("State: {machine_name}State{},", first.name));
            let data_type = format!("{machine_name}{}Data", first.name);
            self.line(&format!("{}Data: &{data_type}{generic_use}{{", first.name));
            self.indent += 1;
            for field in &first.fields {
                self.line(&format!("{}: {},", to_pascal_case(&field.name), field.name));
            }
            self.indent -= 1;
            self.line("},");
            self.indent -= 1;
            self.line("}");
            self.indent -= 1;
            self.line("}");
        }
    }

    fn emit_transition_error(&mut self, machine_name: &str) {
        self.line(&format!("type {machine_name}Error struct {{"));
        self.indent += 1;
        self.line("Transition string");
        self.line("From      string");
        self.line("Message   string");
        self.indent -= 1;
        self.line("}");
        self.newline();
        self.line(&format!("func (e *{machine_name}Error) Error() string {{"));
        self.indent += 1;
        self.line("if e.Message != \"\" {");
        self.indent += 1;
        self.line("return fmt.Sprintf(\"transition '%s' failed: %s\", e.Transition, e.Message)");
        self.indent -= 1;
        self.line("}");
        self.line(
            "return fmt.Sprintf(\"invalid transition '%s' from state '%s'\", e.Transition, e.From)",
        );
        self.indent -= 1;
        self.line("}");
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_transition_method(
        &mut self,
        machine_name: &str,
        transition: &TransitionDecl,
        handlers: &[OnHandler],
        states: &[StateDecl],
        effects: &[EffectDecl],
        channels: &[ChannelDecl],
        generic_use: &str,
    ) {
        let method_name = to_pascal_case(&transition.name);
        let handler = handlers
            .iter()
            .find(|h| h.transition_name == transition.name);

        let ctx_param_name = handler.and_then(|h| detect_ctx_param(h, &self.known_types));

        // Build parameter list
        let mut params = Vec::new();
        if let Some(h) = handler {
            if h.is_async || transition.timeout.is_some() {
                params.push("ctx context.Context".to_string());
            }
        } else if transition.timeout.is_some() {
            params.push("ctx context.Context".to_string());
        }

        // Add handler params, filtering out the ctx param
        if let Some(h) = handler {
            for p in &h.params {
                if ctx_param_name.as_ref() != Some(&p.name) {
                    params.push(format!("{} {}", p.name, self.type_expr_to_go(&p.ty)));
                }
            }
        }

        // Add effects param if this handler uses perform
        let uses_effects = handler
            .map(|h| handler_uses_perform(&h.body))
            .unwrap_or(false);
        if uses_effects && !effects.is_empty() {
            params.push(format!("effects {machine_name}Effects{generic_use}"));
        }
        let uses_spawn = handler
            .map(|h| handler_uses_spawn(&h.body))
            .unwrap_or(false);
        if uses_spawn {
            params.push("supervisor SupervisorRuntime".to_string());
            // Mirrors the Rust backend's `children: &impl {Machine}Supervision`.
            // Gust builds the child; the host drives it.
            params.push(format!("children {}Children", machine_name));
        }
        let used_channels = handler
            .map(|h| handler_used_channels(&h.body))
            .unwrap_or_default();
        for channel_name in used_channels {
            if let Some(channel) = channels.iter().find(|c| c.name == channel_name) {
                params.push(format!(
                    "{}Ch *{}Channel",
                    to_snake_case(&channel.name),
                    channel.name
                ));
            }
        }

        self.line(&format!(
            "func (m *{machine_name}{generic_use}) {method_name}({}) error {{",
            params.join(", ")
        ));
        self.indent += 1;

        // State check
        self.line(&format!(
            "if m.State != {machine_name}State{} {{",
            transition.from
        ));
        self.indent += 1;
        self.line(&format!(
            "return &{machine_name}Error{{Transition: \"{}\", From: m.State.String()}}",
            transition.name
        ));
        self.indent -= 1;
        self.line("}");
        self.newline();

        // Set ctx rewriting state for handler body emission
        if let Some(ref ctx_name) = ctx_param_name {
            self.ctx_param = Some(ctx_name.clone());
            self.from_state_name = Some(transition.from.clone());
            self.machine_name = Some(machine_name.to_string());
        }

        // Which identifiers this handler actually reads, so unused `let`
        // bindings can be lowered to discards. Go rejects unused locals.
        self.referenced_idents = handler
            .map(|h| collect_referenced_idents(&h.body, ctx_param_name.as_deref()))
            .unwrap_or_default();

        self.result_matches = handler
            .map(|h| collect_result_matches(&h.body, &self.result_effects))
            .unwrap_or_default();

        if let Some(h) = handler {
            self.emit_from_state_locals(h, transition, states, ctx_param_name.as_deref());
        }

        // Handler body or default transition
        if let Some(h) = handler {
            if let Some(timeout) = transition.timeout {
                let duration = self.duration_to_go(timeout);
                self.line(&format!(
                    "timeoutCtx, cancel := context.WithTimeout(ctx, {duration})"
                ));
                self.line("defer cancel()");
                self.emit_block_go(&h.body, machine_name, states, effects, channels);
                self.line("select {");
                self.indent += 1;
                self.line("case <-timeoutCtx.Done():");
                self.indent += 1;
                self.line("if timeoutCtx.Err() == context.DeadlineExceeded {");
                self.indent += 1;
                self.line(&format!(
                    "return &{machine_name}Error{{Transition: \"{}\", From: m.State.String(), Message: fmt.Sprintf(\"transition '{}' timed out after %s\", {})}}",
                    transition.name,
                    transition.name,
                    duration
                ));
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("default:");
                self.indent -= 1;
                self.line("}");
            } else {
                self.emit_block_go(&h.body, machine_name, states, effects, channels);
            }
        } else if let Some(first_target) = transition.targets.first() {
            self.emit_goto_go(machine_name, first_target, &[], states);
        }

        // Clear ctx rewriting state
        self.ctx_param = None;
        self.from_state_name = None;
        self.machine_name = None;
        self.result_matches.clear();

        self.newline();
        self.line("return nil");
        self.indent -= 1;
        self.line("}");
    }

    /// Lift the source state's fields into locals so the handler can read them
    /// by bare name.
    ///
    /// The Rust backend gets this for free: it matches on `&self.state` and
    /// destructures the from-state variant, which puts every field in scope.
    /// The Go backend has no such arm — it checks the state tag and falls
    /// through — so a handler that reads `tokens` rather than `ctx.tokens`
    /// emitted Go with `undefined: tokens`. Every `gust-stdlib` machine is
    /// written in that style.
    fn emit_from_state_locals(
        &mut self,
        handler: &OnHandler,
        transition: &TransitionDecl,
        states: &[StateDecl],
        ctx_param: Option<&str>,
    ) {
        let Some(from_state) = states.iter().find(|s| s.name == transition.from) else {
            return;
        };
        if from_state.fields.is_empty() {
            return;
        }

        // Bare reads only. `collect_referenced_idents` folds `ctx.config` into
        // `config`, which would make a ctx-style handler look like it reads the
        // field by bare name and produce a local nothing uses — and Go rejects a
        // local that is declared and never used.
        let bare = collect_bare_idents(&handler.body);

        // Names Go would refuse to redeclare in the function's own scope, or
        // that a `let` later in the body rebinds. The validator already warns
        // about a handler parameter shadowing a source-state field (#105); this
        // keeps the emitted Go compiling in the meantime by letting the
        // inner binding win.
        let mut shadowed: HashSet<String> = handler.params.iter().map(|p| p.name.clone()).collect();
        shadowed.extend(collect_let_bindings(&handler.body));
        if let Some(ctx) = ctx_param {
            shadowed.insert(ctx.to_string());
        }

        let data = format!("m.{}Data", transition.from);
        let mut emitted = false;
        for field in &from_state.fields {
            if !bare.contains(&field.name) || shadowed.contains(&field.name) {
                continue;
            }
            self.line(&format!(
                "{} := {data}.{}",
                field.name,
                to_pascal_case(&field.name)
            ));
            emitted = true;
        }
        if emitted {
            self.newline();
        }
    }

    fn emit_block_go(
        &mut self,
        block: &Block,
        machine_name: &str,
        states: &[StateDecl],
        effects: &[EffectDecl],
        channels: &[ChannelDecl],
    ) {
        for stmt in &block.statements {
            self.emit_statement_go(stmt, machine_name, states, effects, channels);
        }
    }

    fn emit_statement_go(
        &mut self,
        stmt: &Statement,
        machine_name: &str,
        states: &[StateDecl],
        effects: &[EffectDecl],
        channels: &[ChannelDecl],
    ) {
        match stmt {
            Statement::Let {
                name, ty, value, ..
            } => {
                // How many values the RHS yields, when it is a `perform`.
                let effect_return = match value {
                    Expr::Perform(eff, _, _) => self.effect_returns.get(eff.as_str()).copied(),
                    _ => None,
                };
                let expr = self.expr_to_go(value);
                // Go rejects an unused local outright ("declared and not used"),
                // so a binding the handler never reads must become a discard.
                // `_` is the only name Go accepts for this — an underscore
                // *prefix* like `_slept` is still an ordinary identifier and
                // still an error. See #100.
                let binding = if self.referenced_idents.contains(name) {
                    name.as_str()
                } else {
                    "_"
                };
                if let Some(info) = self.result_matches.get(name).cloned() {
                    // A following `match` branches on the error, so the usual
                    // early return would pre-empt the `Err` arm.
                    let value_var = if info.binds_value { name.as_str() } else { "_" };
                    self.line(&format!(
                        "{value_var}, {} := {expr}",
                        result_err_var(name.as_str())
                    ));
                } else if effect_return == Some(GoEffectReturn::Nothing) {
                    // Nothing to bind — emit the call and drop the binding.
                    self.line(&expr);
                } else if effect_return == Some(GoEffectReturn::ErrorOnly) {
                    self.line(&format!("if err := {expr}; err != nil {{"));
                    self.indent += 1;
                    self.line("return err");
                    self.indent -= 1;
                    self.line("}");
                } else if effect_return == Some(GoEffectReturn::ValueAndError) {
                    self.line(&format!("{binding}, err := {expr}"));
                    self.line("if err != nil {");
                    self.indent += 1;
                    self.line("return err");
                    self.indent -= 1;
                    self.line("}");
                } else if binding == "_" {
                    // `var _ T = expr` is legal but pointless; the plain
                    // discard keeps the annotation's type-check off the table
                    // for a value nothing reads.
                    self.line(&format!("_ = {expr}"));
                } else if let Some(t) = ty {
                    let go_type = self.type_expr_to_go(t);
                    self.line(&format!("var {name} {go_type} = {expr}"));
                } else {
                    self.line(&format!("{name} := {expr}"));
                }
            }
            Statement::Return(expr) => {
                let val = self.expr_to_go(expr);
                self.line(&format!("return {val}"));
            }
            Statement::If {
                condition,
                then_block,
                else_block,
                span: _,
            } => {
                let cond = self.expr_to_go(condition);
                // Strip outer parens for Go if-conditions
                let cond = if cond.starts_with('(') && cond.ends_with(')') {
                    &cond[1..cond.len() - 1]
                } else {
                    &cond
                };
                self.line(&format!("if {cond} {{"));
                self.indent += 1;
                self.emit_block_go(then_block, machine_name, states, effects, channels);
                self.indent -= 1;
                if let Some(else_b) = else_block {
                    self.line("} else {");
                    self.indent += 1;
                    self.emit_block_go(else_b, machine_name, states, effects, channels);
                    self.indent -= 1;
                }
                self.line("}");
            }
            Statement::Goto { state, args, .. } => {
                self.emit_goto_go(machine_name, state, args, states);
            }
            Statement::Perform { effect, args, .. } => {
                let method = to_pascal_case(effect);
                let is_async = effects.iter().any(|e| e.name == *effect && e.is_async);
                let arg_strs: Vec<String> = args.iter().map(|a| self.expr_to_go(a)).collect();
                let all_args = if is_async {
                    let mut a = vec!["ctx".to_string()];
                    a.extend(arg_strs);
                    a
                } else {
                    arg_strs
                };
                let call = format!("effects.{}({})", method, all_args.join(", "));
                // The receiver list has to match the interface method exactly:
                // an async `-> ()` effect returns only `error`, so binding two
                // values was an assignment-count mismatch Go rejects.
                let receivers = match self
                    .effect_returns
                    .get(effect.as_str())
                    .copied()
                    .unwrap_or(GoEffectReturn::Nothing)
                {
                    GoEffectReturn::ErrorOnly => Some("err"),
                    GoEffectReturn::ValueAndError => Some("_, err"),
                    // Nothing to check — a bare call statement discards any
                    // single return value, which Go allows.
                    GoEffectReturn::Nothing | GoEffectReturn::Value => None,
                };
                match receivers {
                    Some(receivers) => {
                        self.line(&format!("if {receivers} := {call}; err != nil {{"));
                        self.indent += 1;
                        self.line("return err");
                        self.indent -= 1;
                        self.line("}");
                    }
                    None => self.line(&call),
                }
            }
            Statement::Send {
                channel, message, ..
            } => {
                let msg = self.expr_to_go(message);
                let channel_var = format!("{}Ch", to_snake_case(channel));
                let mode = channels
                    .iter()
                    .find(|c| c.name == *channel)
                    .map(|c| c.mode)
                    .unwrap_or(ChannelMode::Broadcast);
                match mode {
                    ChannelMode::Broadcast => self.line(&format!("{channel_var}.Publish({msg})")),
                    ChannelMode::Mpsc => self.line(&format!("{channel_var}.Send({msg})")),
                }
            }
            Statement::Spawn { machine, args, .. } => {
                // Construct the child and hand it to the host's runner. This
                // previously emitted `_ = []interface{}{args}; return nil` —
                // the arguments were discarded into a throwaway slice and no
                // child was ever built, so `spawn` compiled and did nothing.
                let arg_strs = args
                    .iter()
                    .map(|a| self.expr_to_go(a))
                    .collect::<Vec<_>>()
                    .join(", ");
                let runner = format!("Run{machine}");
                self.line(&format!(
                    "if err := supervisor.SpawnNamed(\"{machine}\", func() error {{"
                ));
                self.indent += 1;
                self.line(&format!(
                    "return children.{runner}(New{machine}({arg_strs}))"
                ));
                self.indent -= 1;
                self.line("}); err != nil {");
                self.indent += 1;
                self.line("return err");
                self.indent -= 1;
                self.line("}");
            }
            Statement::Match { scrutinee, arms } => {
                if let Expr::Ident(name) = scrutinee {
                    if let Some(info) = self.result_matches.get(name).cloned() {
                        self.emit_result_match_go(
                            name,
                            &info,
                            arms,
                            machine_name,
                            states,
                            effects,
                            channels,
                        );
                        return;
                    }
                }
                self.line(&format!("switch {} {{", self.expr_to_go(scrutinee)));
                self.indent += 1;
                for arm in arms {
                    if matches!(arm.pattern, Pattern::Wildcard) {
                        self.line("default:");
                    } else {
                        self.line(&format!("case {}:", self.pattern_to_go(&arm.pattern)));
                    }
                    self.indent += 1;
                    self.emit_block_go(&arm.body, machine_name, states, effects, channels);
                    self.indent -= 1;
                }
                self.indent -= 1;
                self.line("}");
            }
            Statement::Expr(expr) => {
                let val = self.expr_to_go(expr);
                self.line(&val);
            }
        }
    }

    /// Lower `match r { Ok(v) => .., Err(e) => .. }` where `r` holds the value
    /// half of a `(T, error)` pair.
    ///
    /// Go has no `Result` to switch on, so the arms become a nil check on the
    /// error. Emitting a `switch` here produced `undefined: Ok` /
    /// `undefined: v`, which is why every `Result`-matching `gust-stdlib`
    /// machine was Rust-only.
    #[allow(clippy::too_many_arguments)]
    fn emit_result_match_go(
        &mut self,
        binding: &str,
        info: &ResultMatch,
        arms: &[MatchArm],
        machine_name: &str,
        states: &[StateDecl],
        effects: &[EffectDecl],
        channels: &[ChannelDecl],
    ) {
        let err_var = result_err_var(binding);
        let ok_arm = arms.iter().find(|a| arm_variant(a) == Some("Ok"));
        let err_arm = arms.iter().find(|a| arm_variant(a) == Some("Err"));
        let default_arm = arms.iter().find(|a| matches!(a.pattern, Pattern::Wildcard));

        self.line(&format!("if {err_var} == nil {{"));
        self.indent += 1;
        if let Some(arm) = ok_arm.or(default_arm) {
            // The success value is already in `binding`; the pattern's name is
            // an alias for it.
            if let Some(name) = self.arm_binding(arm) {
                if name != binding {
                    self.line(&format!("{name} := {binding}"));
                }
            }
            self.emit_block_go(&arm.body, machine_name, states, effects, channels);
        }
        self.indent -= 1;
        if let Some(arm) = err_arm.or(default_arm) {
            self.line("} else {");
            self.indent += 1;
            if let Some(name) = self.arm_binding(arm) {
                // `E` is erased to Go's `error` — the only error type the
                // `(T, error)` idiom admits. When `E` is `String` the message
                // is the faithful value; otherwise the raw `error` is the
                // closest Go has, and `type_expr_to_go` has already erased the
                // declared type everywhere else too.
                if matches!(&info.error_type, Some(TypeExpr::Simple(n)) if n == "String") {
                    self.line(&format!("{name} := {err_var}.Error()"));
                } else {
                    self.line(&format!("{name} := {err_var}"));
                }
            }
            self.emit_block_go(&arm.body, machine_name, states, effects, channels);
            self.indent -= 1;
        }
        self.line("}");
    }

    /// The name an arm's pattern binds, when that arm's own body reads it.
    ///
    /// Scoped to the arm, not the handler: a pattern binding is only in scope
    /// inside its arm, and Go rejects a local that is declared and never used —
    /// so `Ok(x) => { … }` with no read of `x` must not emit a binding even if
    /// some other arm happens to use the same name.
    fn arm_binding<'a>(&self, arm: &'a MatchArm) -> Option<&'a str> {
        let Pattern::Variant { bindings, .. } = &arm.pattern else {
            return None;
        };
        let name = bindings.first()?.as_str();
        if name == "_" || !arm_body_reads(arm, name) {
            return None;
        }
        Some(name)
    }

    fn emit_clear_state_data(
        &mut self,
        machine_name: &str,
        states: &[StateDecl],
        generic_use: &str,
    ) {
        let has_data = states.iter().any(|s| !s.fields.is_empty());
        if !has_data {
            return;
        }

        self.line(&format!(
            "func (m *{machine_name}{generic_use}) clearStateData() {{"
        ));
        self.indent += 1;
        for state in states {
            if !state.fields.is_empty() {
                self.line(&format!("m.{}Data = nil", state.name));
            }
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_goto_go(
        &mut self,
        machine_name: &str,
        target_state: &str,
        args: &[Expr],
        states: &[StateDecl],
    ) {
        let target = states.iter().find(|s| s.name == target_state);
        let has_any_fields = states.iter().any(|s| !s.fields.is_empty());

        // Evaluate args that reference state data into temp vars BEFORE clearStateData
        // to avoid nil pointer dereference (clearStateData nils all data pointers).
        // Args that don't reference state data (literals, simple idents) are safe to inline.
        let mut arg_values: Vec<(String, bool)> = Vec::new(); // (value_expr, is_temp_var)
        if let Some(t) = target {
            if has_any_fields && !t.fields.is_empty() {
                for (i, field) in t.fields.iter().enumerate() {
                    if i < args.len() {
                        let needs_temp = expr_references_ctx(&args[i]);
                        let value = self.expr_to_go(&args[i]);
                        if needs_temp {
                            let var_name =
                                format!("__goto_{}_{}", target_state.to_lowercase(), field.name);
                            let go_type = self.type_expr_to_go(&field.ty);
                            self.line(&format!("var {var_name} {go_type} = {value}"));
                            arg_values.push((var_name, true));
                        } else {
                            arg_values.push((value, false));
                        }
                    }
                }
            }
        }

        self.line(&format!("m.State = {machine_name}State{target_state}"));

        // Use clearStateData helper instead of individual nil assignments
        if has_any_fields {
            self.line("m.clearStateData()");
        }

        // Set the target state's data if it has fields
        if let Some(t) = target {
            if !t.fields.is_empty() {
                let data_type = format!("{machine_name}{}Data", target_state);
                let generic_use = self.machine_generic_use.clone();
                self.line(&format!(
                    "m.{}Data = &{data_type}{generic_use}{{",
                    target_state
                ));
                self.indent += 1;
                for (i, field) in t.fields.iter().enumerate() {
                    let value = if i < arg_values.len() {
                        arg_values[i].0.clone()
                    } else if i < args.len() {
                        self.expr_to_go(&args[i])
                    } else {
                        self.zero_value(&field.ty)
                    };
                    self.line(&format!("{}: {},", to_pascal_case(&field.name), value));
                }
                self.indent -= 1;
                self.line("}");
            }
        }
    }

    fn expr_to_go(&self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(v) => format!("{v}"),
            Expr::FloatLit(v) => format!("{v}"),
            Expr::StringLit(s) => format!("\"{}\"", escape_string_literal(s)),
            Expr::BoolLit(b) => format!("{b}"),
            Expr::Ident(name) => name.clone(),
            Expr::FieldAccess(base, field) => {
                if let Expr::Ident(name) = base.as_ref() {
                    if self.ctx_param.as_deref() == Some(name.as_str()) {
                        if let (Some(_machine), Some(from)) =
                            (&self.machine_name, &self.from_state_name)
                        {
                            return format!("m.{}Data.{}", from, to_pascal_case(field));
                        }
                    }
                }
                let base_str = self.expr_to_go(base);
                format!("{base_str}.{}", to_pascal_case(field))
            }
            Expr::FnCall(name, args) => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.expr_to_go(a)).collect();
                format!("{}({})", name, arg_strs.join(", "))
            }
            Expr::BinOp(left, op, right, _) => {
                let l = self.expr_to_go(left);
                let r = self.expr_to_go(right);
                let op_str = match op {
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
                };
                format!("({l} {op_str} {r})")
            }
            Expr::UnaryOp(op, inner) => {
                let val = self.expr_to_go(inner);
                match op {
                    UnaryOp::Not => format!("(!{val})"),
                    UnaryOp::Neg => format!("(-{val})"),
                }
            }
            Expr::Perform(name, args, _) => {
                let method = to_pascal_case(name);
                let is_async = self.async_effects.contains(name.as_str());
                let arg_strs: Vec<String> = args.iter().map(|a| self.expr_to_go(a)).collect();
                let all_args = if is_async {
                    let mut a = vec!["ctx".to_string()];
                    a.extend(arg_strs);
                    a
                } else {
                    arg_strs
                };
                format!("effects.{}({})", method, all_args.join(", "))
            }
            Expr::Path(enum_name, variant) => format!("{enum_name}{variant}"),
        }
    }

    // === Helpers ===

    fn pattern_to_go(&self, pattern: &Pattern) -> String {
        match pattern {
            Pattern::Ident(name) => name.clone(),
            Pattern::Variant {
                enum_name, variant, ..
            } => {
                if let Some(name) = enum_name {
                    format!("{name}{variant}")
                } else {
                    variant.clone()
                }
            }
            // Go blank identifier — handles any wildcard that reaches here directly
            // rather than panicking if a future language feature routes a wildcard
            // through pattern_to_go.
            Pattern::Wildcard => "_".to_string(),
        }
    }

    fn type_expr_to_go(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Unit => "struct{}".to_string(),
            TypeExpr::Simple(name) => map_go_type(name),
            TypeExpr::Generic(name, args) => match name.as_str() {
                "Vec" => {
                    let inner = args
                        .first()
                        .map(|a| self.type_expr_to_go(a))
                        .unwrap_or("interface{}".to_string());
                    format!("[]{inner}")
                }
                "Option" => {
                    let inner = args
                        .first()
                        .map(|a| self.type_expr_to_go(a))
                        .unwrap_or("interface{}".to_string());
                    format!("*{inner}")
                }
                "Result" => {
                    // Go doesn't have Result — just use the success type + error return
                    args.first()
                        .map(|a| self.type_expr_to_go(a))
                        .unwrap_or("interface{}".to_string())
                }
                other => {
                    let arg_strs: Vec<String> =
                        args.iter().map(|a| self.type_expr_to_go(a)).collect();
                    format!("{other}[{}]", arg_strs.join(", "))
                }
            },
            TypeExpr::Tuple(types) => {
                let members = types
                    .iter()
                    .enumerate()
                    .map(|(i, t)| format!("F{i} {}", self.type_expr_to_go(t)))
                    .collect::<Vec<_>>()
                    .join("; ");
                format!("struct {{ {members} }}")
            }
        }
    }

    fn zero_value(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Unit => "struct{}{}".to_string(),
            TypeExpr::Simple(name) => match name.as_str() {
                "String" => "\"\"".to_string(),
                "i64" | "i32" | "u64" | "u32" => "0".to_string(),
                "f64" | "f32" => "0.0".to_string(),
                "bool" => "false".to_string(),
                other => format!("{other}{{}}"),
            },
            TypeExpr::Generic(name, _) => match name.as_str() {
                "Vec" => "nil".to_string(),
                "Option" => "nil".to_string(),
                _ => "nil".to_string(),
            },
            TypeExpr::Tuple(_) => format!("{}{{}}", self.type_expr_to_go(ty)),
        }
    }

    fn duration_to_go(&self, duration: DurationSpec) -> String {
        match duration.unit {
            TimeUnit::Millis => format!("time.Duration({}) * time.Millisecond", duration.value),
            TimeUnit::Seconds => format!("time.Duration({}) * time.Second", duration.value),
            TimeUnit::Minutes => format!("time.Duration({}) * time.Minute", duration.value),
            TimeUnit::Hours => format!("time.Duration({}) * time.Hour", duration.value),
        }
    }

    fn emit_json_helpers(&mut self, machine_name: &str, generic_decl: &str, generic_use: &str) {
        // ToJSON
        self.line(&format!(
            "func (m *{machine_name}{generic_use}) ToJSON() ([]byte, error) {{"
        ));
        self.indent += 1;
        self.line("return json.MarshalIndent(m, \"\", \"  \")");
        self.indent -= 1;
        self.line("}");
        self.newline();

        // FromJSON
        self.line(&format!("func {machine_name}FromJSON{generic_decl}(data []byte) (*{machine_name}{generic_use}, error) {{"));
        self.indent += 1;
        self.line(&format!("var m {machine_name}{generic_use}"));
        self.line("if err := json.Unmarshal(data, &m); err != nil {");
        self.indent += 1;
        self.line("return nil, err");
        self.indent -= 1;
        self.line("}");
        self.line("return &m, nil");
        self.indent -= 1;
        self.line("}");
    }

    fn line(&mut self, text: &str) {
        let indent = "\t".repeat(self.indent);
        self.output.push_str(&indent);
        self.output.push_str(text);
        self.output.push('\n');
    }

    fn newline(&mut self) {
        self.output.push('\n');
    }
}

// === Utility Functions ===

/// The `E` of an effect declared `-> Result<T, E>`, or `None` if the return type
/// is not a `Result` at all. The outer `Option` distinguishes "not a Result"
/// from "a Result whose error type was omitted".
fn result_error_type(return_type: &TypeExpr) -> Option<Option<TypeExpr>> {
    match return_type {
        TypeExpr::Generic(name, args) if name == "Result" => Some(args.get(1).cloned()),
        _ => None,
    }
}

/// Classify how an effect's declared return type maps onto Go return values.
fn go_effect_return(effect: &EffectDecl) -> GoEffectReturn {
    // A `Result` is fallible whether or not the effect is `async`: Go signals
    // failure with a trailing `error`, and dropping it would discard the
    // failure silently.
    let (value_type, fallible) = match &effect.return_type {
        TypeExpr::Generic(name, args) if name == "Result" => (args.first(), true),
        other => (Some(other), effect.is_async),
    };
    let has_value = !matches!(value_type, None | Some(TypeExpr::Unit));
    match (has_value, fallible) {
        (true, true) => GoEffectReturn::ValueAndError,
        (true, false) => GoEffectReturn::Value,
        (false, true) => GoEffectReturn::ErrorOnly,
        (false, false) => GoEffectReturn::Nothing,
    }
}

/// Go variable holding the error half of a `Result`-returning effect call.
/// Double-underscore prefixed like the `__goto_*` temporaries so it cannot
/// collide with a user-declared binding.
fn result_err_var(binding: &str) -> String {
    format!("__{binding}_err")
}

/// The enum variant an arm matches, when its pattern is a variant pattern.
fn arm_variant(arm: &MatchArm) -> Option<&str> {
    match &arm.pattern {
        Pattern::Variant { variant, .. } => Some(variant.as_str()),
        _ => None,
    }
}

/// Whether an arm's body reads `name` by bare name.
fn arm_body_reads(arm: &MatchArm, name: &str) -> bool {
    collect_bare_idents(&arm.body).contains(name)
}

/// Find `let` bindings whose value comes from a `Result`-returning effect and
/// that a `match` in the same handler destructures with `Ok`/`Err`.
///
/// Both halves are required. A `Result` binding with no matching `match` keeps
/// the plain early-return lowering, and a `match` on anything else keeps the
/// `switch` lowering.
fn collect_result_matches(
    body: &Block,
    result_effects: &HashMap<String, Option<TypeExpr>>,
) -> HashMap<String, ResultMatch> {
    let mut candidates: HashMap<String, Option<TypeExpr>> = HashMap::new();
    collect_result_bindings(body, result_effects, &mut candidates);
    let mut out = HashMap::new();
    collect_matched_results(body, &candidates, &mut out);
    out
}

fn collect_result_bindings(
    body: &Block,
    result_effects: &HashMap<String, Option<TypeExpr>>,
    out: &mut HashMap<String, Option<TypeExpr>>,
) {
    for stmt in &body.statements {
        match stmt {
            Statement::Let {
                name,
                value: Expr::Perform(effect, _, _),
                ..
            } => {
                if let Some(error_type) = result_effects.get(effect.as_str()) {
                    out.insert(name.clone(), error_type.clone());
                }
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                collect_result_bindings(then_block, result_effects, out);
                if let Some(else_block) = else_block {
                    collect_result_bindings(else_block, result_effects, out);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    collect_result_bindings(&arm.body, result_effects, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_matched_results(
    body: &Block,
    candidates: &HashMap<String, Option<TypeExpr>>,
    out: &mut HashMap<String, ResultMatch>,
) {
    for stmt in &body.statements {
        match stmt {
            Statement::Match { scrutinee, arms } => {
                if let Expr::Ident(name) = scrutinee {
                    if let Some(error_type) = candidates.get(name) {
                        let ok_arm = arms.iter().find(|a| arm_variant(a) == Some("Ok"));
                        let err_arm = arms.iter().find(|a| arm_variant(a) == Some("Err"));
                        let has_wildcard =
                            arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard));
                        // Both outcomes have to have somewhere to go, and at
                        // least one arm has to actually name `Ok` or `Err` —
                        // otherwise this is a plain value match and the `switch`
                        // lowering is the right one.
                        let names_result = ok_arm.is_some() || err_arm.is_some();
                        let covers_both = has_wildcard || (ok_arm.is_some() && err_arm.is_some());
                        if names_result && covers_both {
                            // The success value only needs a name if the `Ok`
                            // arm reads its binding, or an arm reads the `let`
                            // binding directly. Otherwise it is discarded — Go
                            // rejects an unused local.
                            let binds_value =
                                ok_arm.is_some_and(|arm| match &arm.pattern {
                                    Pattern::Variant { bindings, .. } => bindings
                                        .first()
                                        .is_some_and(|b| b != "_" && arm_body_reads(arm, b)),
                                    _ => false,
                                }) || arms.iter().any(|arm| arm_body_reads(arm, name));
                            out.insert(
                                name.clone(),
                                ResultMatch {
                                    error_type: error_type.clone(),
                                    binds_value,
                                },
                            );
                        }
                    }
                }
                for arm in arms {
                    collect_matched_results(&arm.body, candidates, out);
                }
            }
            Statement::If {
                then_block,
                else_block,
                ..
            } => {
                collect_matched_results(then_block, candidates, out);
                if let Some(else_block) = else_block {
                    collect_matched_results(else_block, candidates, out);
                }
            }
            _ => {}
        }
    }
}

fn map_go_type(name: &str) -> String {
    match name {
        "String" => "string".to_string(),
        "i64" => "int64".to_string(),
        "i32" => "int32".to_string(),
        "u64" => "uint64".to_string(),
        "u32" => "uint32".to_string(),
        "f64" => "float64".to_string(),
        "f32" => "float32".to_string(),
        "bool" => "bool".to_string(),
        other => other.to_string(), // User-defined types pass through
    }
}

impl Default for GoCodegen {
    fn default() -> Self {
        Self::new()
    }
}

fn map_use_path_to_go_import(segments: &[String]) -> String {
    if segments.len() >= 2 {
        let tld = segments[1].as_str();
        if matches!(tld, "com" | "org" | "net" | "io" | "dev" | "edu" | "gov") {
            let mut path = format!("{}.{}", segments[0], segments[1]);
            if segments.len() > 2 {
                path.push('/');
                path.push_str(&segments[2..].join("/"));
            }
            return path;
        }
    }

    segments.join("/")
}

fn go_generic_decl(params: &[GenericParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let joined = params
        .iter()
        .map(|p| format!("{} any", p.name))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{joined}]")
}

fn go_generic_use(params: &[GenericParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let joined = params
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{joined}]")
}
