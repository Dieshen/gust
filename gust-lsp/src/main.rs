use gust_lang::{
    ast::TypeDecl, ast::TypeExpr, format_program_preserving, parse_program_with_errors,
    validate_program,
};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
    docs: RwLock<HashMap<Url, String>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "gust-lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![" ".to_string(), ":".to_string()]),
                    ..CompletionOptions::default()
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    ..Default::default()
                }),
                rename_provider: None,
                references_provider: None,
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
                    InlayHintOptions::default(),
                ))),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "gust-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.docs.write().await.insert(uri.clone(), text.clone());
        self.publish_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(content) = params.content_changes.first().map(|c| c.text.clone()) {
            let uri = params.text_document.uri;
            self.docs.write().await.insert(uri.clone(), content.clone());
            self.publish_diagnostics(uri, &content).await;
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };
        let line = text.lines().nth(pos.line as usize).unwrap_or("");
        let token = token_at_col(line, pos.character as usize);
        let Some(token) = token else {
            return Ok(None);
        };

        for (idx, l) in text.lines().enumerate() {
            let starts = [
                format!("state {token}"),
                format!("effect {token}"),
                format!("async effect {token}"),
                format!("transition {token}"),
            ];
            if starts.iter().any(|s| l.trim_start().starts_with(s)) {
                let range = Range {
                    start: Position::new(idx as u32, 0),
                    end: Position::new(idx as u32, l.len() as u32),
                };
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range,
                })));
            }
        }
        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };
        let line = text.lines().nth(pos.line as usize).unwrap_or("");
        let token = token_at_col(line, pos.character as usize);
        let Some(token) = token else {
            return Ok(None);
        };

        if let Ok(program) = parse_program_with_errors(text, uri.path()) {
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
                    return Ok(Some(make_hover(&sig, &doc)));
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
                    return Ok(Some(make_hover(&sig, &doc)));
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
                    return Ok(Some(make_hover(&sig, &doc)));
                }
            }

            // Check top-level type declarations
            for ty in &program.types {
                match ty {
                    TypeDecl::Struct { name, fields } if name == &token => {
                        let field_str = fields
                            .iter()
                            .map(|f| format!("{}: {}", f.name, type_expr_label(&f.ty)))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let doc = collect_doc_comments(text, name);
                        let sig = format!("type {name} {{ {field_str} }}");
                        return Ok(Some(make_hover(&sig, &doc)));
                    }
                    TypeDecl::Enum { name, variants } if name == &token => {
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
                        return Ok(Some(make_hover(&sig, &doc)));
                    }
                    _ => {}
                }
            }
        }
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };
        let line = text.lines().nth(pos.line as usize).unwrap_or("");
        let mut items = Vec::new();

        if line.contains("goto ") {
            for l in text.lines() {
                if let Some(name) = l.trim_start().strip_prefix("state ").and_then(first_ident) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::CLASS),
                        ..CompletionItem::default()
                    });
                }
            }
        } else if line.contains("perform ") {
            for l in text.lines() {
                let t = l.trim_start();
                if let Some(rest) = t
                    .strip_prefix("effect ")
                    .or_else(|| t.strip_prefix("async effect "))
                {
                    if let Some(name) = first_ident(rest) {
                        items.push(CompletionItem {
                            label: name.to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            ..CompletionItem::default()
                        });
                    }
                }
            }
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };

        let Ok(program) = parse_program_with_errors(text, uri.path()) else {
            // Don't format documents with parse errors
            return Ok(None);
        };

        let formatted = format_program_preserving(&program, text);
        let line_count = text.lines().count() as u32;
        let last_line = text.lines().last().unwrap_or("");

        let edit = TextEdit {
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(line_count, last_line.len() as u32),
            },
            new_text: formatted,
        };

        Ok(Some(vec![edit]))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };

        let Ok(program) = parse_program_with_errors(text, uri.path()) else {
            return Ok(None);
        };

        let mut symbols: Vec<DocumentSymbol> = Vec::new();

        // Top-level type declarations
        for ty in &program.types {
            let (name, kind) = match ty {
                TypeDecl::Struct { name, .. } => (name.as_str(), SymbolKind::STRUCT),
                TypeDecl::Enum { name, .. } => (name.as_str(), SymbolKind::ENUM),
            };
            let range = find_decl_range(text, name);
            #[allow(deprecated)]
            symbols.push(DocumentSymbol {
                name: name.to_string(),
                detail: None,
                kind,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: None,
            });
        }

        // Machine declarations
        for machine in &program.machines {
            let machine_range = find_decl_range(text, &machine.name);
            let mut children: Vec<DocumentSymbol> = Vec::new();

            for state in &machine.states {
                let r = find_decl_range(text, &state.name);
                #[allow(deprecated)]
                children.push(DocumentSymbol {
                    name: state.name.clone(),
                    detail: Some(format!("{} field(s)", state.fields.len())),
                    kind: SymbolKind::ENUM_MEMBER,
                    tags: None,
                    deprecated: None,
                    range: r,
                    selection_range: r,
                    children: None,
                });
            }

            for tr in &machine.transitions {
                let r = find_decl_range(text, &tr.name);
                let detail = format!("{} -> {}", tr.from, tr.targets.join(" | "));
                #[allow(deprecated)]
                children.push(DocumentSymbol {
                    name: tr.name.clone(),
                    detail: Some(detail),
                    kind: SymbolKind::EVENT,
                    tags: None,
                    deprecated: None,
                    range: r,
                    selection_range: r,
                    children: None,
                });
            }

            for effect in &machine.effects {
                let r = find_decl_range(text, &effect.name);
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
                #[allow(deprecated)]
                children.push(DocumentSymbol {
                    name: effect.name.clone(),
                    detail: Some(detail),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range: r,
                    selection_range: r,
                    children: None,
                });
            }

            for handler in &machine.handlers {
                let r = find_decl_range(text, &handler.transition_name);
                #[allow(deprecated)]
                children.push(DocumentSymbol {
                    name: format!("on {}", handler.transition_name),
                    detail: None,
                    kind: SymbolKind::METHOD,
                    tags: None,
                    deprecated: None,
                    range: r,
                    selection_range: r,
                    children: None,
                });
            }

            #[allow(deprecated)]
            symbols.push(DocumentSymbol {
                name: machine.name.clone(),
                detail: None,
                kind: SymbolKind::CLASS,
                tags: None,
                deprecated: None,
                range: machine_range,
                selection_range: machine_range,
                children: Some(children),
            });
        }

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };

        let line = text.lines().nth(pos.line as usize).unwrap_or("");
        // Only look at the text up to the cursor column
        let col = pos.character as usize;
        let prefix = &line[..col.min(line.len())];

        // Find the innermost `perform <name>(` before the cursor
        let Some(effect_name) = find_perform_effect_name(prefix) else {
            return Ok(None);
        };

        let Ok(program) = parse_program_with_errors(text, uri.path()) else {
            return Ok(None);
        };

        // Search all machines for the effect
        let effect = program
            .machines
            .iter()
            .flat_map(|m| m.effects.iter())
            .find(|e| e.name == effect_name);

        let Some(effect) = effect else {
            return Ok(None);
        };

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

        // Count commas between the opening paren and the cursor to find active param
        let open_paren_pos = prefix.rfind(&format!("{}(", effect_name));
        let active_parameter = open_paren_pos.map(|p| {
            let after_paren = &prefix[p + effect_name.len() + 1..];
            // Count commas that are not inside nested parens
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

        Ok(Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label,
                documentation: None,
                parameters: Some(
                    effect
                        .params
                        .iter()
                        .map(|p| ParameterInformation {
                            label: ParameterLabel::Simple(format!(
                                "{}: {}",
                                p.name,
                                type_expr_label(&p.ty)
                            )),
                            documentation: None,
                        })
                        .collect(),
                ),
                active_parameter,
            }],
            active_signature: Some(0),
            active_parameter,
        }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let new_name = params.new_name;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };

        let line = text.lines().nth(pos.line as usize).unwrap_or("");
        let Some(token) = token_at_col(line, pos.character as usize) else {
            return Ok(None);
        };

        let edits = find_all_word_occurrences(text, &token)
            .into_iter()
            .map(|(line_idx, col_start)| TextEdit {
                range: Range {
                    start: Position::new(line_idx as u32, col_start as u32),
                    end: Position::new(line_idx as u32, (col_start + token.len()) as u32),
                },
                new_text: new_name.clone(),
            })
            .collect::<Vec<_>>();

        if edits.is_empty() {
            return Ok(None);
        }

        let mut changes = HashMap::new();
        changes.insert(uri, edits);

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };

        let line = text.lines().nth(pos.line as usize).unwrap_or("");
        let Some(token) = token_at_col(line, pos.character as usize) else {
            return Ok(None);
        };

        let locations = find_all_word_occurrences(text, &token)
            .into_iter()
            .map(|(line_idx, col_start)| Location {
                uri: uri.clone(),
                range: Range {
                    start: Position::new(line_idx as u32, col_start as u32),
                    end: Position::new(line_idx as u32, (col_start + token.len()) as u32),
                },
            })
            .collect::<Vec<_>>();

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let cursor_line = params.range.start.line;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };

        let Ok(program) = parse_program_with_errors(text, uri.path()) else {
            return Ok(None);
        };

        let mut actions: CodeActionResponse = Vec::new();

        for machine in &program.machines {
            let handled: std::collections::HashSet<&str> = machine
                .handlers
                .iter()
                .map(|h| h.transition_name.as_str())
                .collect();

            for tr in &machine.transitions {
                if handled.contains(tr.name.as_str()) {
                    continue;
                }

                // Check if the cursor is near the transition declaration line
                let tr_line = find_line_index(text, &format!("transition {}", tr.name));
                let is_near_cursor = tr_line
                    .map(|l| l as u32 == cursor_line || l as u32 + 1 == cursor_line)
                    .unwrap_or(false);

                if !is_near_cursor {
                    continue;
                }

                // Find where to insert the handler — after last handler, or after effects block
                let insert_line = find_handler_insert_line(text, machine);

                // Build the handler stub text
                let ctx_type = format!("{}Ctx", tr.from);
                let stub = format!(
                    "\n    on {}(ctx: {}) {{\n        // TODO: handle {} transition\n        goto {};\n    }}\n",
                    tr.name,
                    ctx_type,
                    tr.name,
                    tr.targets.first().cloned().unwrap_or_else(|| tr.from.clone()),
                );

                let insert_pos = Position::new(insert_line as u32, 0);
                let edit = TextEdit {
                    range: Range {
                        start: insert_pos,
                        end: insert_pos,
                    },
                    new_text: stub,
                };

                let mut changes = HashMap::new();
                changes.insert(uri.clone(), vec![edit]);

                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Add handler for transition '{}'", tr.name),
                    kind: Some(CodeActionKind::QUICKFIX),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    }),
                    ..CodeAction::default()
                }));
            }
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };

        let Ok(program) = parse_program_with_errors(text, uri.path()) else {
            return Ok(None);
        };

        let mut hints: Vec<InlayHint> = Vec::new();

        // For each handler body, find `let x = perform EffectName(...)` without a type annotation
        // and add an inlay hint showing the effect's return type.
        for machine in &program.machines {
            for handler in &machine.handlers {
                for stmt in &handler.body.statements {
                    if let gust_lang::ast::Statement::Let {
                        name,
                        ty: None,
                        value,
                    } = stmt
                    {
                        // Check if the value is a Perform expression
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

                        // Find the line that has `let <name> =`
                        let hint_line = find_let_line(text, name);
                        let Some(line_idx) = hint_line else {
                            continue;
                        };

                        // Place the hint right after the variable name
                        let line_text = text.lines().nth(line_idx).unwrap_or("");
                        let col = find_name_end_col(line_text, name);

                        hints.push(InlayHint {
                            position: Position::new(line_idx as u32, col as u32),
                            label: InlayHintLabel::String(format!(": {return_type}")),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: None,
                            padding_left: Some(false),
                            padding_right: Some(false),
                            data: None,
                        });
                    }
                }
            }
        }

        if hints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hints))
        }
    }
}

impl Backend {
    async fn publish_diagnostics(&self, uri: Url, text: &str) {
        let mut diags = Vec::new();
        match parse_program_with_errors(text, uri.path()) {
            Err(err) => {
                let line = err.line.saturating_sub(1) as u32;
                let col = err.col.saturating_sub(1) as u32;
                diags.push(Diagnostic {
                    range: Range {
                        start: Position::new(line, col),
                        end: Position::new(line, col.saturating_add(1)),
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("gust-lsp".to_string()),
                    message: err.message,
                    ..Diagnostic::default()
                });
            }
            Ok(program) => {
                let report = validate_program(&program, uri.path(), text);
                for warning in report.warnings {
                    let line = warning.line.saturating_sub(1) as u32;
                    let col = warning.col.saturating_sub(1) as u32;
                    diags.push(Diagnostic {
                        range: Range {
                            start: Position::new(line, col),
                            end: Position::new(line, col.saturating_add(1)),
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("gust-lsp".to_string()),
                        message: warning.message,
                        ..Diagnostic::default()
                    });
                }
                for error in report.errors {
                    let line = error.line.saturating_sub(1) as u32;
                    let col = error.col.saturating_sub(1) as u32;
                    diags.push(Diagnostic {
                        range: Range {
                            start: Position::new(line, col),
                            end: Position::new(line, col.saturating_add(1)),
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        source: Some("gust-lsp".to_string()),
                        message: error.message,
                        ..Diagnostic::default()
                    });
                }
            }
        }
        self.client.publish_diagnostics(uri, diags, None).await;
    }
}

// --- Token / position helpers ---

fn token_at_col(line: &str, col: usize) -> Option<String> {
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

fn first_ident(s: &str) -> Option<&str> {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some(&s[..end])
    }
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// --- Display helpers ---

fn type_expr_label(ty: &TypeExpr) -> String {
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

// --- Hover helpers ---

/// Build a markdown hover from a signature line and optional doc comment.
fn make_hover(signature: &str, doc: &str) -> Hover {
    let content = if doc.is_empty() {
        format!("```gust\n{signature}\n```")
    } else {
        format!("{doc}\n\n---\n\n```gust\n{signature}\n```")
    };
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: None,
    }
}

// --- Doc comment helpers ---

/// Given the source text and a declaration name, find the declaration line and
/// walk backwards collecting contiguous `//` comment lines above it.
/// Returns the doc comment as a single string (lines joined by `\n`), or empty.
fn collect_doc_comments(text: &str, decl_name: &str) -> String {
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

// --- Source search helpers ---

/// Returns a zero-based (line, col) range for the first line that contains the
/// given identifier as a declaration keyword prefix.  Falls back to a
/// zero-width range at the document start when not found.
fn find_decl_range(text: &str, name: &str) -> Range {
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
            let start = Position::new(idx as u32, 0);
            let end = Position::new(idx as u32, line.len() as u32);
            return Range { start, end };
        }
    }
    Range {
        start: Position::new(0, 0),
        end: Position::new(0, 0),
    }
}

/// Returns the 0-based line index of the first line matching a prefix, or None.
fn find_line_index(text: &str, prefix: &str) -> Option<usize> {
    text.lines()
        .enumerate()
        .find(|(_, l)| l.trim_start().starts_with(prefix))
        .map(|(i, _)| i)
}

/// Returns all (line_index, col_start) pairs where `word` appears as a whole
/// word (surrounded by non-identifier characters or string boundaries).
fn find_all_word_occurrences(text: &str, word: &str) -> Vec<(usize, usize)> {
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
fn find_handler_insert_line(text: &str, machine: &gust_lang::ast::MachineDecl) -> usize {
    // Try last handler
    if let Some(last_handler) = machine.handlers.last() {
        // Find the closing brace of this handler by looking for the `on <name>` line
        // then scanning forward for a `}` at the handler indentation level.
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
fn find_closing_brace_line(text: &str, start_line: usize) -> Option<usize> {
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
fn find_let_line(text: &str, var_name: &str) -> Option<usize> {
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
fn find_name_end_col(line_text: &str, name: &str) -> usize {
    // Find `let <name>` and return the position right after `name`
    let pattern = format!("let {name}");
    if let Some(pos) = line_text.find(pattern.as_str()) {
        return pos + "let ".len() + name.len();
    }
    // Fallback: column 0
    0
}

/// Searches `prefix` (text on the current line up to the cursor) for the most
/// recent `perform <name>(` invocation that has not yet been closed.
/// Returns the effect name if found.
fn find_perform_effect_name(prefix: &str) -> Option<String> {
    // Walk backwards through `perform` occurrences to find the innermost one
    // whose opening paren has more opens than closes after it.
    let mut search = prefix;
    while let Some(pos) = search.rfind("perform ") {
        let after = &search[pos + "perform ".len()..];
        if let Some(name) = first_ident(after) {
            let after_name = &after[name.len()..];
            // Check that an opening paren follows (possibly with whitespace)
            if after_name.trim_start().starts_with('(') {
                // Count whether the paren is still open up to end of prefix
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
        // Shrink search range and try the next earlier occurrence
        search = &search[..pos];
    }
    None
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        docs: RwLock::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
