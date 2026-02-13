use colored::Colorize;

#[derive(Debug, Clone)]
pub struct GustError {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub note: Option<String>,
    pub help: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GustWarning {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub note: Option<String>,
}

impl GustError {
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
