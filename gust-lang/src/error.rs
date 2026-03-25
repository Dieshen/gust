//! Diagnostic types for Gust compiler errors and warnings.
//!
//! Both [`GustError`] and [`GustWarning`] carry source location information
//! (file, line, column) and can be rendered into rustc-style diagnostic
//! strings with [`GustError::render`] / [`GustWarning::render`].

use colored::Colorize;

/// A compile-time error produced by the parser or validator.
///
/// Each error carries the source location and an explanatory message. Optional
/// `note` and `help` fields provide extra context, such as listing valid names
/// or suggesting a typo correction (powered by Levenshtein distance).
#[derive(Debug, Clone)]
pub struct GustError {
    /// Source file path (as provided by the caller).
    pub file: String,
    /// 1-based line number in the source file.
    pub line: usize,
    /// 1-based column number in the source file.
    pub col: usize,
    /// Human-readable description of the error.
    pub message: String,
    /// Optional additional context (e.g. "declared states: Idle, Running").
    pub note: Option<String>,
    /// Optional fix suggestion (e.g. "did you mean 'state'?").
    pub help: Option<String>,
}

/// A compile-time warning produced by the validator.
///
/// Warnings indicate potential issues that do not prevent code generation
/// (e.g. unreachable states, unused effects, handlers with missing `goto`).
#[derive(Debug, Clone)]
pub struct GustWarning {
    /// Source file path (as provided by the caller).
    pub file: String,
    /// 1-based line number in the source file.
    pub line: usize,
    /// 1-based column number in the source file.
    pub col: usize,
    /// Human-readable description of the warning.
    pub message: String,
    /// Optional additional context.
    pub note: Option<String>,
}

impl GustError {
    /// Render this error as a rustc-style diagnostic string.
    ///
    /// The output includes the error message, a source location pointer with
    /// surrounding context lines, and optional note/help annotations.
    /// Coloring is controlled by the `NO_COLOR` environment variable.
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
    /// Render this warning as a rustc-style diagnostic string.
    ///
    /// Same format as [`GustError::render`] but with a yellow "warning" label
    /// instead of a red "error" label.
    pub fn render(&self, source: &str) -> String {
        render_diag(
            "warning",
            &self.file,
            (self.line, self.col),
            &self.message,
            (self.note.as_deref(), None),
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
