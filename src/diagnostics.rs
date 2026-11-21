pub use miette::{Diagnostic, Severity};
use miette::{LabeledSpan, MietteError, MietteSpanContents, SourceCode, SourceSpan, SpanContents};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

/// Source code location span with optional byte offsets and source text for [`miette::SourceCode`] rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub file: PathBuf,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) start_byte: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) end_byte: Option<usize>,
    #[serde(skip)]
    pub(crate) source: Option<Arc<str>>,
}

impl Span {
    /// Creates a new source code span without byte offsets.
    pub fn new(
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
            start_byte: None,
            end_byte: None,
            source: None,
        }
    }

    /// Attaches byte offsets to the span.
    pub fn with_byte_offsets(mut self, start_byte: usize, end_byte: usize) -> Self {
        self.start_byte = Some(start_byte);
        self.end_byte = Some(end_byte);
        self
    }

    /// Attaches shared source code text to the span.
    pub fn with_source(mut self, source: Arc<str>) -> Self {
        self.source = Some(source);
        self
    }
}

impl From<&Span> for SourceSpan {
    fn from(span: &Span) -> Self {
        if let (Some(start), Some(end)) = (span.start_byte, span.end_byte) {
            Self::new(start.into(), end.saturating_sub(start))
        } else {
            Self::new(0.into(), 0)
        }
    }
}

impl From<Span> for SourceSpan {
    fn from(span: Span) -> Self {
        Self::from(&span)
    }
}

impl SourceCode for Span {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, MietteError> {
        let name = self.file.display().to_string();
        if let Some(src) = &self.source
            && !src.is_empty()
            && self.start_byte.is_some()
            && span.offset() <= src.len()
            && let Ok(inner) =
                src.as_ref()
                    .read_span(span, context_lines_before, context_lines_after)
        {
            let contents = MietteSpanContents::new_named(
                name,
                inner.data(),
                *inner.span(),
                self.start_line.saturating_sub(1),
                self.start_col.saturating_sub(1),
                inner.line_count(),
            );
            return Ok(Box::new(contents));
        }

        let line_count = self.end_line.saturating_sub(self.start_line).max(1);
        let contents = MietteSpanContents::new_named(
            name,
            &[],
            *span,
            self.start_line.saturating_sub(1),
            self.start_col.saturating_sub(1),
            line_count,
        );
        Ok(Box::new(contents))
    }
}

/// Concrete [`miette::Diagnostic`] enum for API drift findings produced by `api-manifest`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiManifestDiagnostic {
    /// A public API item was removed relative to the checked-in manifest.
    #[error("Public {surface} item '{name}' was removed: {detail}")]
    Removal {
        surface: String,
        name: String,
        detail: String,
        is_breaking: bool,
        span: Span,
    },

    /// A public API item was modified relative to the checked-in manifest.
    #[error("Public {surface} item '{name}' modified: {detail}")]
    Modification {
        surface: String,
        name: String,
        detail: String,
        is_breaking: bool,
        span: Span,
    },

    /// A new public API item was added relative to the checked-in manifest.
    #[error("New public {surface} item '{name}' added: {detail}")]
    Addition {
        surface: String,
        name: String,
        detail: String,
        span: Span,
    },
}

impl ApiManifestDiagnostic {
    /// Returns the source [`Span`] associated with this diagnostic.
    pub const fn span(&self) -> &Span {
        match self {
            Self::Removal { span, .. }
            | Self::Modification { span, .. }
            | Self::Addition { span, .. } => span,
        }
    }
}

impl Diagnostic for ApiManifestDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        let code = match self {
            Self::Removal { .. } => "api::drift::removal",
            Self::Modification { .. } => "api::drift::modification",
            Self::Addition { .. } => "api::drift::addition",
        };
        Some(Box::new(code))
    }

    fn severity(&self) -> Option<Severity> {
        let severity = match self {
            Self::Removal { is_breaking, .. } | Self::Modification { is_breaking, .. } => {
                if *is_breaking {
                    Severity::Error
                } else {
                    Severity::Warning
                }
            }
            Self::Addition { .. } => Severity::Warning,
        };
        Some(severity)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        let help = match self {
            Self::Removal { .. } => {
                "Update API manifest via 'api-manifest dump' if this removal was intentional"
            }
            Self::Modification { .. } => {
                "Update API manifest via 'api-manifest dump' to reflect signature changes"
            }
            Self::Addition { .. } => {
                "Run 'api-manifest dump' to commit newly introduced API items into API.md"
            }
        };
        Some(Box::new(help))
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        Some(self.span())
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(LabeledSpan::at(
            SourceSpan::from(self.span()),
            "",
        ))))
    }
}

/// Summary metrics aggregated for a diagnostic report.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct ReportSummary {
    pub(crate) total_errors: usize,
    pub(crate) total_warnings: usize,
    pub(crate) total_advice: usize,
}

#[derive(Serialize)]
struct SerializedDiagnostic<'a> {
    rule: String,
    severity: Severity,
    message: String,
    span: &'a Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
}

/// An aggregated container of all [`ApiManifestDiagnostic`] findings produced during a check run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiagnosticReport {
    pub diagnostics: Vec<ApiManifestDiagnostic>,
    pub(crate) summary: ReportSummary,
}

impl Serialize for DiagnosticReport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let serialized_diags: Vec<SerializedDiagnostic<'_>> = self
            .diagnostics
            .iter()
            .map(|d| {
                let rule = d
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "api::unknown".to_string());
                let severity = d.severity().unwrap_or(Severity::Warning);
                let suggested_fix = d.help().map(|h| h.to_string());
                SerializedDiagnostic {
                    rule,
                    severity,
                    message: d.to_string(),
                    span: d.span(),
                    suggested_fix,
                }
            })
            .collect();

        let mut state = serializer.serialize_struct("DiagnosticReport", 2)?;
        state.serialize_field("diagnostics", &serialized_diags)?;
        state.serialize_field("summary", &self.summary)?;
        state.end()
    }
}

impl DiagnosticReport {
    /// Creates a new diagnostic report with the given diagnostics, automatically computing summary counts.
    pub fn new(diagnostics: Vec<ApiManifestDiagnostic>) -> Self {
        let mut total_errors = 0;
        let mut total_warnings = 0;
        let mut total_advice = 0;

        for d in &diagnostics {
            match d.severity().unwrap_or(Severity::Warning) {
                Severity::Error => total_errors += 1,
                Severity::Warning => total_warnings += 1,
                Severity::Advice => total_advice += 1,
            }
        }

        Self {
            diagnostics,
            summary: ReportSummary {
                total_errors,
                total_warnings,
                total_advice,
            },
        }
    }

    /// Returns true if any diagnostic has error severity.
    pub fn has_errors(&self) -> bool {
        self.summary.total_errors > 0
    }

    /// Returns the total count of error diagnostics.
    pub fn error_count(&self) -> usize {
        self.summary.total_errors
    }

    /// Returns the total count of warning diagnostics.
    pub fn warning_count(&self) -> usize {
        self.summary.total_warnings
    }

    /// Returns the total count of advice/hint diagnostics.
    pub fn advice_count(&self) -> usize {
        self.summary.total_advice
    }

    /// Returns true if there are no diagnostics in the report.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn check_empty_report_returns_empty_and_zero_counts() -> Result<(), Box<dyn std::error::Error>>
    {
        let report = DiagnosticReport::default();
        expect_that!(report.is_empty(), is_true());
        expect_that!(report.has_errors(), is_false());
        expect_that!(report.error_count(), eq(0));
        expect_that!(report.warning_count(), eq(0));
        expect_that!(report.advice_count(), eq(0));
        Ok(())
    }

    #[googletest::test]
    fn create_report_computes_error_and_warning_counts() -> Result<(), Box<dyn std::error::Error>> {
        let span = Span::new("API.md", 1, 1, 1, 10);
        let report = DiagnosticReport::new(vec![
            ApiManifestDiagnostic::Removal {
                surface: "Library".to_string(),
                name: "old_fn".to_string(),
                detail: "Removed from markdown manifest".to_string(),
                is_breaking: true,
                span: span.clone(),
            },
            ApiManifestDiagnostic::Modification {
                surface: "Library".to_string(),
                name: "demo".to_string(),
                detail: "Header drifted".to_string(),
                is_breaking: false,
                span: span.clone(),
            },
            ApiManifestDiagnostic::Addition {
                surface: "Library".to_string(),
                name: "new_fn".to_string(),
                detail: "Added to markdown manifest".to_string(),
                span,
            },
        ]);

        expect_that!(report.is_empty(), is_false());
        expect_that!(report.has_errors(), is_true());
        expect_that!(report.error_count(), eq(1));
        expect_that!(report.warning_count(), eq(2));
        expect_that!(report.advice_count(), eq(0));
        expect_that!(report.diagnostics.len(), eq(3));
        Ok(())
    }

    #[googletest::test]
    fn create_span_stores_coordinates_byte_offsets_and_source()
    -> Result<(), Box<dyn std::error::Error>> {
        let source: Arc<str> = Arc::from("pub fn demo() {}\n");
        let span = Span::new("API.md", 1, 1, 1, 16)
            .with_byte_offsets(0, 16)
            .with_source(source);
        expect_that!(span.file, eq(&PathBuf::from("API.md")));
        expect_that!(span.start_line, eq(1));
        expect_that!(span.start_col, eq(1));
        expect_that!(span.end_line, eq(1));
        expect_that!(span.end_col, eq(16));
        expect_that!(span.start_byte, eq(Some(0)));
        expect_that!(span.end_byte, eq(Some(16)));

        let source_span = SourceSpan::from(&span);
        expect_that!(source_span.offset(), eq(0));
        expect_that!(source_span.len(), eq(16));

        let contents = span.read_span(&source_span, 0, 0)?;
        expect_that!(contents.name(), some(eq("API.md")));
        expect_that!(contents.data().is_empty(), is_false());
        Ok(())
    }

    #[googletest::test]
    fn api_manifest_diagnostic_exposes_miette_metadata() -> Result<(), Box<dyn std::error::Error>> {
        let span = Span::new("API.md", 1, 1, 1, 1);
        let removal = ApiManifestDiagnostic::Removal {
            surface: "Library".to_string(),
            name: "old_fn".to_string(),
            detail: "Removed".to_string(),
            is_breaking: true,
            span: span.clone(),
        };
        expect_that!(
            removal.code().map(|c| c.to_string()),
            some(eq("api::drift::removal"))
        );
        expect_that!(removal.severity(), some(eq(Severity::Error)));
        expect_that!(
            removal.help().map(|h| h.to_string()),
            some(contains_substring("api-manifest dump"))
        );
        expect_that!(removal.span(), eq(&span));
        let source_code = removal.source_code().ok_or("source_code missing")?;
        let contents = source_code.read_span(&SourceSpan::from(&span), 0, 0)?;
        expect_that!(contents.name(), some(eq("API.md")));
        let labels: Vec<_> = removal.labels().ok_or("labels missing")?.collect();
        expect_that!(labels.len(), eq(1));

        let non_breaking_mod = ApiManifestDiagnostic::Modification {
            surface: "CLI".to_string(),
            name: "--flag".to_string(),
            detail: "Description updated".to_string(),
            is_breaking: false,
            span: span.clone(),
        };
        expect_that!(
            non_breaking_mod.code().map(|c| c.to_string()),
            some(eq("api::drift::modification"))
        );
        expect_that!(non_breaking_mod.severity(), some(eq(Severity::Warning)));

        let addition = ApiManifestDiagnostic::Addition {
            surface: "HTTP".to_string(),
            name: "GET /health".to_string(),
            detail: "New endpoint".to_string(),
            span,
        };
        expect_that!(
            addition.code().map(|c| c.to_string()),
            some(eq("api::drift::addition"))
        );
        expect_that!(addition.severity(), some(eq(Severity::Warning)));
        Ok(())
    }

    #[googletest::test]
    fn serialize_report_outputs_diagnostics_and_summary() -> Result<(), Box<dyn std::error::Error>>
    {
        let span = Span::new("API.md", 1, 1, 1, 1);
        let report = DiagnosticReport::new(vec![ApiManifestDiagnostic::Removal {
            surface: "Library".to_string(),
            name: "old_fn".to_string(),
            detail: "Removed".to_string(),
            is_breaking: true,
            span,
        }]);

        let serialized = serde_json::to_value(&report)?;
        expect_that!(
            serialized
                .get("diagnostics")
                .and_then(|d| d.get(0))
                .and_then(|e| e.get("rule"))
                .and_then(|r| r.as_str()),
            some(eq("api::drift::removal"))
        );
        expect_that!(
            serialized
                .get("summary")
                .and_then(|s| s.get("total_errors"))
                .and_then(|v| v.as_u64()),
            some(eq(1))
        );
        expect_that!(
            serialized
                .get("summary")
                .and_then(|s| s.get("total_warnings"))
                .and_then(|v| v.as_u64()),
            some(eq(0))
        );
        expect_that!(
            serialized
                .get("summary")
                .and_then(|s| s.get("total_advice"))
                .and_then(|v| v.as_u64()),
            some(eq(0))
        );
        Ok(())
    }
}
