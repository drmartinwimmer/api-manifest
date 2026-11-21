use crate::diagnostics::{ApiManifestDiagnostic, Diagnostic, DiagnosticReport, Severity};
use annotate_snippets::{Group, Level, Origin, Renderer};
use anstyle::Style;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// Output format for diagnostic reports and manifests.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Human-readable console output
    #[default]
    Console,
    /// Machine-readable JSON output
    Json,
    /// GitHub-Flavored Markdown output
    Markdown,
}

/// Renders a diagnostic report to the specified writer in the chosen format.
/// Automatically detects color support based on standard environment and terminal conventions.
pub fn render_report(
    report: &DiagnosticReport,
    format: OutputFormat,
    writer: &mut dyn io::Write,
) -> io::Result<()> {
    let use_color = auto_detect_color();
    render_report_with_options(report, format, writer, use_color)
}

fn auto_detect_color() -> bool {
    if anstyle_query::no_color() {
        return false;
    }
    if anstyle_query::clicolor_force() {
        return true;
    }
    if anstyle_query::clicolor() == Some(false) {
        return false;
    }
    true
}

/// Renders a diagnostic report with explicit control over ANSI colorization.
pub(crate) fn render_report_with_options(
    report: &DiagnosticReport,
    format: OutputFormat,
    writer: &mut dyn io::Write,
    use_color: bool,
) -> io::Result<()> {
    match format {
        OutputFormat::Console => render_console(report, writer, use_color),
        OutputFormat::Json => render_json(report, writer),
        OutputFormat::Markdown => render_markdown(report, writer),
    }
}

fn render_console(
    report: &DiagnosticReport,
    writer: &mut dyn io::Write,
    use_color: bool,
) -> io::Result<()> {
    if report.is_empty() {
        writeln!(writer, "No issues found.")?;
        return Ok(());
    }

    let renderer = if use_color {
        Renderer::styled()
    } else {
        Renderer::plain()
    };

    // Group diagnostics by file path
    let mut by_file: BTreeMap<&Path, Vec<&ApiManifestDiagnostic>> = BTreeMap::new();
    for diag in &report.diagnostics {
        by_file
            .entry(diag.span().file.as_path())
            .or_default()
            .push(diag);
    }

    for (file, diags) in by_file {
        let path_str = file.display().to_string();
        for diag in diags {
            let level = match diag.severity().unwrap_or(Severity::Warning) {
                Severity::Error => Level::ERROR,
                Severity::Warning => Level::WARNING,
                Severity::Advice => Level::HELP,
            };

            let message = diag.to_string();
            let rule = diag
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "api::unknown".to_string());
            let help_str = diag.help().map(|h| h.to_string());
            let span = diag.span();

            let title = level.primary_title(&message).id(&rule);
            let mut group = Group::with_title(title);

            if !path_str.is_empty() {
                let origin = Origin::path(&path_str)
                    .line(span.start_line)
                    .char_column(span.start_col);
                group = group.element(origin);
            }

            if let Some(fix) = &help_str {
                group = group.element(Level::HELP.message(fix));
            }

            let rendered = renderer.render(&[group]);
            writeln!(writer, "{rendered}")?;
        }
    }

    let summary_line = format!(
        "{} error(s), {} warning(s), 0 info, {} hint(s) found.",
        report.error_count(),
        report.warning_count(),
        report.advice_count()
    );

    if use_color {
        let bold = Style::new().bold();
        writeln!(writer, "{bold}{summary_line}{bold:#}")?;
    } else {
        writeln!(writer, "{summary_line}")?;
    }

    Ok(())
}

fn render_json(report: &DiagnosticReport, writer: &mut dyn io::Write) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut *writer, report).map_err(io::Error::other)?;
    writeln!(writer)?;
    Ok(())
}

fn render_markdown(report: &DiagnosticReport, writer: &mut dyn io::Write) -> io::Result<()> {
    writeln!(writer, "# Diagnostic Report\n")?;

    if report.is_empty() {
        writeln!(writer, "No issues found.")?;
        return Ok(());
    }

    writeln!(
        writer,
        "| Severity | Rule | Location | Message | Suggestion |"
    )?;
    writeln!(writer, "| --- | --- | --- | --- | --- |")?;

    for diag in &report.diagnostics {
        let span = diag.span();
        let location = if span.file.as_os_str().is_empty() {
            "-".to_string()
        } else {
            format!(
                "`{}:{}:{}`",
                span.file.display(),
                span.start_line,
                span.start_col
            )
        };

        let suggestion = match diag.help().map(|h| h.to_string()) {
            Some(fix) => {
                let sanitized_fix = fix.replace('|', "\\|").replace('\n', " ");
                format!("`{sanitized_fix}`")
            }
            None => "-".to_string(),
        };

        let sanitized_msg = diag.to_string().replace('|', "\\|").replace('\n', " ");
        let rule = diag
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "api::unknown".to_string());
        let severity = diag.severity().unwrap_or(Severity::Warning);

        writeln!(
            writer,
            "| {:?} | `{}` | {} | {} | {} |",
            severity, rule, location, sanitized_msg, suggestion
        )?;
    }

    writeln!(
        writer,
        "\n**Summary**: {} error(s), {} warning(s), 0 info, {} hint(s)",
        report.error_count(),
        report.warning_count(),
        report.advice_count()
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Span;
    use googletest::prelude::*;

    #[googletest::test]
    fn render_console_empty_report_reports_no_issues() -> Result<(), Box<dyn std::error::Error>> {
        let report = DiagnosticReport::default();
        let mut buffer = Vec::new();
        render_report_with_options(&report, OutputFormat::Console, &mut buffer, false)?;
        let output = String::from_utf8(buffer)?;
        expect_that!(output, contains_substring("No issues found."));
        Ok(())
    }

    #[googletest::test]
    fn render_console_diagnostics_formats_grouped_by_file_and_summary()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = DiagnosticReport::new(vec![
            ApiManifestDiagnostic::Removal {
                surface: "Library".to_string(),
                name: "old_fn".to_string(),
                detail: "Removed from manifest".to_string(),
                is_breaking: true,
                span: Span::new("API.md", 10, 1, 10, 20),
            },
            ApiManifestDiagnostic::Addition {
                surface: "Library".to_string(),
                name: "new_fn".to_string(),
                detail: "Added to manifest".to_string(),
                span: Span::new("API.md", 12, 1, 12, 20),
            },
        ]);

        let mut buffer = Vec::new();
        render_report_with_options(&report, OutputFormat::Console, &mut buffer, false)?;
        let output = String::from_utf8(buffer)?;

        expect_that!(output, contains_substring("--> API.md:10:1"));
        expect_that!(output, contains_substring("--> API.md:12:1"));
        expect_that!(output, contains_substring("error[api::drift::removal]"));
        expect_that!(output, contains_substring("warning[api::drift::addition]"));
        expect_that!(output, contains_substring("api-manifest dump"));
        expect_that!(output, contains_substring("1 error(s)"));
        expect_that!(output, contains_substring("1 warning(s)"));
        Ok(())
    }

    #[googletest::test]
    fn render_console_with_color_includes_ansi_escape_codes()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = DiagnosticReport::new(vec![ApiManifestDiagnostic::Removal {
            surface: "Library".to_string(),
            name: "old_fn".to_string(),
            detail: "Removed".to_string(),
            is_breaking: true,
            span: Span::new("API.md", 1, 1, 1, 2),
        }]);

        let mut buffer = Vec::new();
        render_report_with_options(&report, OutputFormat::Console, &mut buffer, true)?;
        let output = String::from_utf8(buffer)?;

        expect_that!(output, contains_substring("\x1b["));
        expect_that!(output, contains_substring("\x1b[0m"));
        Ok(())
    }

    #[googletest::test]
    fn render_json_outputs_valid_pretty_json_with_summary() -> Result<(), Box<dyn std::error::Error>>
    {
        let report = DiagnosticReport::new(vec![ApiManifestDiagnostic::Addition {
            surface: "Library".to_string(),
            name: "new_fn".to_string(),
            detail: "Added".to_string(),
            span: Span::new("API.md", 1, 1, 1, 5),
        }]);
        let mut buffer = Vec::new();
        render_report(&report, OutputFormat::Json, &mut buffer)?;
        let parsed: serde_json::Value = serde_json::from_slice(&buffer)?;

        let rule = parsed
            .get("diagnostics")
            .and_then(|d| d.get(0))
            .and_then(|e| e.get("rule"))
            .and_then(|r| r.as_str());
        expect_that!(rule, eq(Some("api::drift::addition")));

        let total_warnings = parsed
            .get("summary")
            .and_then(|s| s.get("total_warnings"))
            .and_then(|w| w.as_u64());
        expect_that!(total_warnings, eq(Some(1)));
        Ok(())
    }

    #[googletest::test]
    fn render_markdown_empty_report_outputs_clean_markdown()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = DiagnosticReport::default();
        let mut buffer = Vec::new();
        render_report(&report, OutputFormat::Markdown, &mut buffer)?;
        let output = String::from_utf8(buffer)?;
        expect_that!(output, contains_substring("# Diagnostic Report"));
        expect_that!(output, contains_substring("No issues found."));
        Ok(())
    }

    #[googletest::test]
    fn render_markdown_diagnostics_outputs_table_and_summary()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = DiagnosticReport::new(vec![ApiManifestDiagnostic::Removal {
            surface: "Library".to_string(),
            name: "old_fn".to_string(),
            detail: "Removed".to_string(),
            is_breaking: true,
            span: Span::new("API.md", 1, 1, 1, 5),
        }]);
        let mut buffer = Vec::new();
        render_report(&report, OutputFormat::Markdown, &mut buffer)?;
        let output = String::from_utf8(buffer)?;
        expect_that!(
            output,
            contains_substring("| Severity | Rule | Location | Message | Suggestion |")
        );
        expect_that!(
            output,
            contains_substring(
                "| Error | `api::drift::removal` | `API.md:1:1` | Public Library item 'old_fn' was removed: Removed |"
            )
        );
        expect_that!(
            output,
            contains_substring("**Summary**: 1 error(s), 0 warning(s)")
        );
        Ok(())
    }

    #[googletest::test]
    fn render_report_renders_console_successfully() -> Result<(), Box<dyn std::error::Error>> {
        let report = DiagnosticReport::new(vec![ApiManifestDiagnostic::Modification {
            surface: "Library".to_string(),
            name: "compute".to_string(),
            detail: "Signature changed".to_string(),
            is_breaking: true,
            span: Span::new("API.md", 5, 1, 5, 10),
        }]);
        let mut buffer = Vec::new();
        render_report(&report, OutputFormat::Console, &mut buffer)?;
        let output = String::from_utf8(buffer)?;
        expect_that!(output, contains_substring("api::drift::modification"));
        expect_that!(output, contains_substring("compute"));
        expect_that!(output, contains_substring("API.md:5:1"));
        Ok(())
    }
}
