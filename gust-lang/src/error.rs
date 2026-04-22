use colored::Colorize;

/// A hard compilation error with a source location and optional
/// advisory note/help text.
#[derive(Debug, Clone)]
pub struct GustError {
    /// Source file path for the diagnostic.
    pub file: String,
    /// 1-based line number. `0` means "no precise location."
    pub line: usize,
    /// 1-based column number. `0` paired with `line == 0` means
    /// "no precise location."
    pub col: usize,
    /// Primary diagnostic message.
    pub message: String,
    /// Optional "note:" supplementary explanation.
    pub note: Option<String>,
    /// Optional "help:" suggestion (e.g. did-you-mean prompts).
    pub help: Option<String>,
}

/// An advisory warning with a source location and optional note. Does
/// not block compilation.
#[derive(Debug, Clone)]
pub struct GustWarning {
    /// Source file path for the warning.
    pub file: String,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub col: usize,
    /// Primary warning message.
    pub message: String,
    /// Optional "note:" supplementary explanation.
    pub note: Option<String>,
    /// Optional "help:" suggestion (e.g. did-you-mean prompts).
    pub help: Option<String>,
}

impl GustError {
    /// Render this error with a source-annotated caret block, matching
    /// `rustc`'s visual style.
    pub fn render(&self, source: &str) -> String {
        render_diag(
            "error",
            &self.file,
            (self.line, self.col),
            &self.message,
            (self.note.as_deref(), self.help.as_deref()),
            source,
            true,
        )
    }
}

impl GustWarning {
    /// Render this warning with a source-annotated caret block.
    pub fn render(&self, source: &str) -> String {
        render_diag(
            "warning",
            &self.file,
            (self.line, self.col),
            &self.message,
            (self.note.as_deref(), self.help.as_deref()),
            source,
            false,
        )
    }
}

fn render_diag(
    kind: &str,
    file: &str,
    pos: (usize, usize),
    message: &str,
    note_help: (Option<&str>, Option<&str>),
    source: &str,
    is_error: bool,
) -> String {
    let (line, col) = pos;
    let (note, help) = note_help;
    let color_enabled = std::env::var_os("NO_COLOR").is_none();
    let kind_text = if color_enabled {
        if is_error {
            kind.red().bold().to_string()
        } else {
            kind.yellow().bold().to_string()
        }
    } else {
        kind.to_string()
    };

    let mut out = format!("{kind_text}: {message}\n  --> {file}:{line}:{col}\n");
    if line > 0 {
        let lines: Vec<&str> = source.lines().collect();
        let before = line.saturating_sub(1);
        let current = line;
        let after = line + 1;

        out.push_str("   |\n");
        if before >= 1 && before <= lines.len() {
            out.push_str(&format!("{:>2} | {}\n", before, lines[before - 1]));
        }
        if current >= 1 && current <= lines.len() {
            out.push_str(&format!("{:>2} | {}\n", current, lines[current - 1]));
            out.push_str(&format!("   | {}^\n", " ".repeat(col.saturating_sub(1))));
        }
        if after >= 1 && after <= lines.len() {
            out.push_str(&format!("{:>2} | {}\n", after, lines[after - 1]));
        }
        out.push_str("   |\n");
    }
    if let Some(n) = note {
        let note_text = if color_enabled {
            "note".cyan().to_string()
        } else {
            "note".to_string()
        };
        out.push_str(&format!("   = {note_text}: {n}\n"));
    }
    if let Some(h) = help {
        let help_text = if color_enabled {
            "help".cyan().to_string()
        } else {
            "help".to_string()
        };
        out.push_str(&format!("   = {help_text}: {h}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // All render tests serialize through this mutex because `colored`'s
    // override is global process state and our render function also reads
    // NO_COLOR from the environment.
    static RENDER_LOCK: Mutex<()> = Mutex::new(());

    fn no_color_var<F: FnOnce() -> String>(f: F) -> String {
        let _g = RENDER_LOCK.lock().unwrap();
        colored::control::set_override(false);
        unsafe {
            std::env::set_var("NO_COLOR", "1");
        }
        let out = f();
        unsafe {
            std::env::remove_var("NO_COLOR");
        }
        colored::control::unset_override();
        out
    }

    #[test]
    fn error_render_with_line_source_and_caret() {
        let err = GustError {
            file: "example.gu".to_string(),
            line: 2,
            col: 5,
            message: "undefined identifier".to_string(),
            note: None,
            help: None,
        };
        let source = "machine Foo {\n    state Bar\n}\n";
        let out = no_color_var(|| err.render(source));
        assert!(out.contains("error: undefined identifier"));
        assert!(out.contains("--> example.gu:2:5"));
        assert!(out.contains("state Bar"));
        assert!(out.contains("^"));
    }

    #[test]
    fn error_render_line_zero_skips_source_block() {
        let err = GustError {
            file: "example.gu".to_string(),
            line: 0,
            col: 0,
            message: "global failure".to_string(),
            note: None,
            help: None,
        };
        let out = no_color_var(|| err.render(""));
        assert!(out.contains("error: global failure"));
        assert!(!out.contains("   |"));
    }

    #[test]
    fn error_render_note_and_help_present() {
        let err = GustError {
            file: "x.gu".to_string(),
            line: 1,
            col: 1,
            message: "bad".to_string(),
            note: Some("why it broke".to_string()),
            help: Some("try this".to_string()),
        };
        let out = no_color_var(|| err.render("machine X {}\n"));
        assert!(out.contains("note: why it broke"));
        assert!(out.contains("help: try this"));
    }

    #[test]
    fn error_render_after_line_not_in_source_ok() {
        // When `line` equals the last line, `after` is out of range and must
        // be skipped gracefully.
        let err = GustError {
            file: "x.gu".to_string(),
            line: 1,
            col: 1,
            message: "eof".to_string(),
            note: None,
            help: None,
        };
        let out = no_color_var(|| err.render("only-line"));
        assert!(out.contains("only-line"));
    }

    #[test]
    fn warning_render_uses_warning_kind_and_no_help() {
        let warn = GustWarning {
            file: "x.gu".to_string(),
            line: 1,
            col: 1,
            message: "deprecated".to_string(),
            note: Some("replaced in 0.2".to_string()),
            help: None,
        };
        let out = no_color_var(|| warn.render("old_api()\n"));
        assert!(out.contains("warning: deprecated"));
        assert!(out.contains("note: replaced in 0.2"));
        assert!(!out.contains("help:"));
    }

    #[test]
    fn warning_render_help_text_when_set() {
        let warn = GustWarning {
            file: "x.gu".to_string(),
            line: 1,
            col: 1,
            message: "unknown effect".to_string(),
            note: None,
            help: Some("did you mean 'process'?".to_string()),
        };
        let out = no_color_var(|| warn.render("perform proess();\n"));
        assert!(out.contains("warning: unknown effect"));
        assert!(out.contains("help: did you mean 'process'?"));
        assert!(!out.contains("note:"));
    }

    #[test]
    fn render_with_color_enabled_contains_ansi_escape() {
        let _g = RENDER_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("NO_COLOR");
        }
        colored::control::set_override(true);
        let err = GustError {
            file: "x.gu".to_string(),
            line: 1,
            col: 1,
            message: "colored".to_string(),
            note: Some("n".to_string()),
            help: Some("h".to_string()),
        };
        let out = err.render("line one\n");
        colored::control::unset_override();
        assert!(
            out.contains('\u{1b}'),
            "expected ANSI escape in colored output: {:?}",
            out
        );
    }
}
