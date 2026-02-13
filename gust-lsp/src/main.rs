use gust_lang::{ast::TypeExpr, parse_program_with_errors, validate_program};
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
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![" ".to_string(), ":".to_string()]),
                    ..CompletionOptions::default()
                }),
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

    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
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
                return Ok(Some(GotoDefinitionResponse::Scalar(Location { uri, range })));
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
                    return Ok(Some(Hover {
                        contents: HoverContents::Scalar(MarkedString::String(format!(
                            "state `{}` ({fields})",
                            state.name
                        ))),
                        range: None,
                    }));
                }
                if let Some(effect) = machine.effects.iter().find(|e| e.name == token) {
                    let params = effect
                        .params
                        .iter()
                        .map(|p| format!("{}: {}", p.name, type_expr_label(&p.ty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Ok(Some(Hover {
                        contents: HoverContents::Scalar(MarkedString::String(format!(
                            "{}effect `{}`({}) -> {}",
                            if effect.is_async { "async " } else { "" },
                            effect.name,
                            params,
                            type_expr_label(&effect.return_type)
                        ))),
                        range: None,
                    }));
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
                if let Some(rest) = t.strip_prefix("effect ").or_else(|| t.strip_prefix("async effect ")) {
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

fn type_expr_label(ty: &TypeExpr) -> String {
    match ty {
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
