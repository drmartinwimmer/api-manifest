use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

/// Output format for reports and manifests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OutputFormat {
    /// Human-readable console output
    #[default]
    Console,
    /// Machine-readable JSON output
    Json,
    /// GitHub-Flavored Markdown output
    Markdown,
}

/// Severity level for a diagnostic finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Hint,
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hint => write!(f, "hint"),
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Source code location span.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Span {
    pub file: PathBuf,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Span {
    /// Creates a new `Span`.
    pub(crate) fn new(
        file: impl Into<PathBuf>,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Self {
        Self {
            file: file.into(),
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }
}

/// A structured diagnostic finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
}

impl Diagnostic {
    /// Creates a new `Diagnostic`.
    pub(crate) fn new(
        rule: impl Into<String>,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule: rule.into(),
            severity,
            message: message.into(),
            span: None,
            suggested_fix: None,
        }
    }

    /// Attaches a source span to the diagnostic.
    pub(crate) fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Attaches a suggested fix to the diagnostic.
    pub(crate) fn with_suggested_fix(mut self, fix: impl Into<String>) -> Self {
        self.suggested_fix = Some(fix.into());
        self
    }
}

/// Summary counts for a diagnostic report.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total_errors: usize,
    pub total_warnings: usize,
    pub total_info: usize,
    pub total_hints: usize,
}

/// Aggregated report of diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub diagnostics: Vec<Diagnostic>,
    pub summary: ReportSummary,
}

impl DiagnosticReport {
    /// Creates a new `DiagnosticReport` from a list of diagnostics.
    pub(crate) fn new(diagnostics: Vec<Diagnostic>) -> Self {
        let mut summary = ReportSummary::default();
        for d in &diagnostics {
            match d.severity {
                Severity::Error => summary.total_errors += 1,
                Severity::Warning => summary.total_warnings += 1,
                Severity::Info => summary.total_info += 1,
                Severity::Hint => summary.total_hints += 1,
            }
        }
        Self {
            diagnostics,
            summary,
        }
    }

    /// Renders the diagnostic report to a writer in the requested format.
    pub(crate) fn render<W: Write>(
        &self,
        format: OutputFormat,
        writer: &mut W,
    ) -> std::io::Result<()> {
        match format {
            OutputFormat::Console => self.render_console(writer),
            OutputFormat::Json => {
                let s = serde_json::to_string_pretty(self)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                writeln!(writer, "{s}")
            }
            OutputFormat::Markdown => self.render_markdown(writer),
        }
    }

    fn render_console<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        if self.diagnostics.is_empty() {
            return writeln!(writer, "No issues found.");
        }
        for d in &self.diagnostics {
            writeln!(writer, "{}[{}]: {}", d.severity, d.rule, d.message)?;
            if let Some(ref s) = d.span {
                writeln!(
                    writer,
                    " --> {}:{}:{}",
                    s.file.display(),
                    s.start_line,
                    s.start_col
                )?;
            }
            if let Some(ref fix) = d.suggested_fix {
                writeln!(writer, "  = help: {fix}")?;
            }
        }
        writeln!(
            writer,
            "{} error(s), {} warning(s), {} info, {} hint(s) found.",
            self.summary.total_errors,
            self.summary.total_warnings,
            self.summary.total_info,
            self.summary.total_hints
        )
    }

    fn render_markdown<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "# Diagnostic Report\n")?;
        if self.diagnostics.is_empty() {
            return writeln!(writer, "No issues found.");
        }
        writeln!(
            writer,
            "| Severity | Rule | Location | Message | Suggestion |"
        )?;
        writeln!(writer, "| --- | --- | --- | --- | --- |")?;
        for d in &self.diagnostics {
            let loc = match &d.span {
                Some(s) => format!("`{}:{}:{}`", s.file.display(), s.start_line, s.start_col),
                None => "-".to_string(),
            };
            let fix = match &d.suggested_fix {
                Some(f) => format!("`{}`", f.replace('|', "\\|").replace('\n', " ")),
                None => "-".to_string(),
            };
            let msg = d.message.replace('|', "\\|").replace('\n', " ");
            writeln!(
                writer,
                "| {:?} | `{}` | {} | {} | {} |",
                d.severity, d.rule, loc, msg, fix
            )?;
        }
        Ok(())
    }
}
