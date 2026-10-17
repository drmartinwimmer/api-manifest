use crate::diagnostics::{Diagnostic, Severity, Span};

/// Surface category where drift occurred.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub(crate) enum DriftSurface {
    Library,
    Cli,
    Http,
    Config,
}

impl std::fmt::Display for DriftSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Library => write!(f, "Library"),
            Self::Cli => write!(f, "CLI"),
            Self::Http => write!(f, "HTTP"),
            Self::Config => write!(f, "Config"),
        }
    }
}

/// A specific API drift finding between active code and checked-in manifest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub(crate) struct DriftItem {
    pub(crate) surface: DriftSurface,
    pub(crate) name: String,
    pub(crate) detail: String,
    pub(crate) is_breaking: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) old_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) new_signature: Option<String>,
}

impl DriftItem {
    /// Creates a new `DriftItem`.
    pub(crate) fn new(
        surface: DriftSurface,
        name: impl Into<String>,
        detail: impl Into<String>,
        is_breaking: bool,
        old_signature: Option<String>,
        new_signature: Option<String>,
    ) -> Self {
        Self {
            surface,
            name: name.into(),
            detail: detail.into(),
            is_breaking,
            old_signature,
            new_signature,
        }
    }
}

/// Aggregated report of API drift between active code and manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApiDriftReport {
    pub(crate) additions: Vec<DriftItem>,
    pub(crate) removals: Vec<DriftItem>,
    pub(crate) modifications: Vec<DriftItem>,
}

impl ApiDriftReport {
    /// Returns true if there is no drift detected.
    pub fn is_clean(&self) -> bool {
        self.additions.is_empty() && self.removals.is_empty() && self.modifications.is_empty()
    }

    /// Returns true if any breaking changes were detected.
    pub fn has_breaking_changes(&self) -> bool {
        self.removals.iter().any(|i| i.is_breaking)
            || self.modifications.iter().any(|i| i.is_breaking)
    }

    /// Returns the number of breaking changes.
    pub fn breaking_count(&self) -> usize {
        let rem_breaking = self.removals.iter().filter(|i| i.is_breaking).count();
        let mod_breaking = self.modifications.iter().filter(|i| i.is_breaking).count();
        rem_breaking + mod_breaking
    }

    /// Returns the total number of drifted items.
    pub fn total_drift_count(&self) -> usize {
        self.additions.len() + self.removals.len() + self.modifications.len()
    }

    /// Converts drift items into standard [`Diagnostic`] findings.
    pub(crate) fn to_diagnostics(&self, manifest_file: &str) -> Vec<Diagnostic> {
        let mut diags = Vec::new();

        for item in &self.removals {
            let severity = if item.is_breaking {
                Severity::Error
            } else {
                Severity::Warning
            };
            diags.push(
                Diagnostic::new(
                    "api::drift::removal",
                    severity,
                    format!(
                        "Public {} item '{}' was removed: {}",
                        item.surface, item.name, item.detail
                    ),
                )
                .with_span(Span::new(manifest_file, 1, 1, 1, 1))
                .with_suggested_fix(
                    "Update API manifest via 'api-manifest dump' if this removal was intentional",
                ),
            );
        }

        for item in &self.modifications {
            let severity = if item.is_breaking {
                Severity::Error
            } else {
                Severity::Warning
            };
            diags.push(
                Diagnostic::new(
                    "api::drift::modification",
                    severity,
                    format!(
                        "Public {} item '{}' modified: {}",
                        item.surface, item.name, item.detail
                    ),
                )
                .with_span(Span::new(manifest_file, 1, 1, 1, 1))
                .with_suggested_fix(
                    "Update API manifest via 'api-manifest dump' to reflect signature changes",
                ),
            );
        }

        for item in &self.additions {
            diags.push(
                Diagnostic::new(
                    "api::drift::addition",
                    Severity::Warning,
                    format!(
                        "New public {} item '{}' added: {}",
                        item.surface, item.name, item.detail
                    ),
                )
                .with_span(Span::new(manifest_file, 1, 1, 1, 1))
                .with_suggested_fix(
                    "Run 'api-manifest dump' to commit newly introduced API items into API.md",
                ),
            );
        }

        diags
    }
}
