use crate::ast::*;

pub fn format_program(program: &Program) -> String {
    let mut out = String::new();

    for use_path in &program.uses {
        out.push_str(&format!("use {};\n", use_path.segments.join("::")));
    }
    if !program.uses.is_empty() {
        out.push('\n');
    }

    for type_decl in &program.types {
        format_type_decl(type_decl, &mut out);
        out.push('\n');
    }

    for channel in &program.channels {
        format_channel_decl(channel, &mut out);
        out.push('\n');
    }

    for machine in &program.machines {
        format_machine(machine, &mut out);
        out.push('\n');
    }

    out.trim_end().to_string() + "\n"
}

fn format_type_decl(decl: &TypeDecl, out: &mut String) {
    match decl {
        TypeDecl::Struct { name, fields } => {
            out.push_str(&format!("type {name} {{\n"));
            for field in fields {
                out.push_str(&format!("    {}: {},\n", field.name, format_type_expr(&field.ty)));
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

fn format_machine(machine: &MachineDecl, out: &mut String) {
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
        let async_kw = if handler.is_async { "async " } else { "" };
        let params = handler
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, format_type_expr(&p.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "    {async_kw}on {}({params}) {{\n",
            handler.transition_name
        ));
        out.push_str("        // formatter preserves structure only\n");
        out.push_str("    }\n");
    }

    out.push_str("}\n");
}

fn format_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Simple(s) => s.clone(),
        TypeExpr::Generic(name, args) => {
            let args = args.iter().map(format_type_expr).collect::<Vec<_>>().join(", ");
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
