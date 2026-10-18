use crate::diff_report::{ApiDriftReport, DriftItem, DriftSurface};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Kind of public library item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ItemKind {
    Struct,
    Enum,
    Function,
    Method,
    Trait,
    TypeAlias,
    Constant,
    Static,
    Field,
    Reexport,
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Struct => write!(f, "struct"),
            Self::Enum => write!(f, "enum"),
            Self::Function => write!(f, "function"),
            Self::Method => write!(f, "method"),
            Self::Trait => write!(f, "trait"),
            Self::TypeAlias => write!(f, "type_alias"),
            Self::Constant => write!(f, "constant"),
            Self::Static => write!(f, "static"),
            Self::Field => write!(f, "field"),
            Self::Reexport => write!(f, "reexport"),
        }
    }
}

/// A single public library item in a Rust crate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct LibraryItem {
    /// Kind of item (struct, enum, function, method, etc.)
    pub(crate) kind: ItemKind,
    /// Item identifier (e.g. `LintConfig` or `LintConfig::new`)
    pub(crate) name: String,
    /// Fully qualified path within the crate (e.g. `api_manifest::LintConfig`)
    pub(crate) path: String,
    /// Canonical Rust signature string (e.g. `pub fn run(&self) -> Result<(), Error>`)
    pub(crate) signature: String,
    /// Optional documentation summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) doc: Option<String>,
}

impl LibraryItem {
    pub(crate) fn new(
        kind: ItemKind,
        name: impl Into<String>,
        path: impl Into<String>,
        signature: impl Into<String>,
        doc: Option<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            path: path.into(),
            signature: signature.into(),
            doc,
        }
    }
}

/// Public API surface of a Rust library.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LibraryApi {
    pub(crate) items: Vec<LibraryItem>,
}

impl LibraryApi {
    pub(crate) fn new(mut items: Vec<LibraryItem>) -> Self {
        items.sort();
        items.dedup_by(|a, b| a.kind == b.kind && a.name == b.name && a.signature == b.signature);
        Self { items }
    }

    pub(crate) fn format_markdown(&self, out: &mut String, section_idx: usize) {
        out.push_str(&format!("## {section_idx}. Library API\n\n"));

        let mut functions = Vec::new();
        let mut methods = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut traits = Vec::new();
        let mut type_aliases = Vec::new();
        let mut constants = Vec::new();
        let mut reexports = Vec::new();

        for item in &self.items {
            match item.kind {
                ItemKind::Function => functions.push(item),
                ItemKind::Method => methods.push(item),
                ItemKind::Struct => structs.push(item),
                ItemKind::Field => {}
                ItemKind::Enum => enums.push(item),
                ItemKind::Trait => traits.push(item),
                ItemKind::TypeAlias => type_aliases.push(item),
                ItemKind::Constant | ItemKind::Static => constants.push(item),
                ItemKind::Reexport => reexports.push(item),
            }
        }

        Self::format_section(out, "Structs", &structs);
        Self::format_section(out, "Enums", &enums);
        Self::format_section(out, "Functions", &functions);
        Self::format_section(out, "Methods", &methods);
        Self::format_section(out, "Traits", &traits);
        Self::format_section(out, "Type Aliases", &type_aliases);
        Self::format_section(out, "Constants & Statics", &constants);
        Self::format_section(out, "Re-exports", &reexports);
    }

    fn format_section(out: &mut String, title: &str, items: &[&LibraryItem]) {
        if items.is_empty() {
            return;
        }
        out.push_str(&format!("### {title}\n\n"));
        for item in items {
            if let Some(ref doc) = item.doc {
                let first_line = doc.lines().next().unwrap_or_default();
                out.push_str(&format!("- `{}` — _{first_line}_\n", item.signature));
            } else {
                out.push_str(&format!("- `{}`\n", item.signature));
            }
        }
        out.push('\n');
    }

    pub(crate) fn diff(&self, baseline: &Self) -> ApiDriftReport {
        let mut report = ApiDriftReport::default();
        let active_map: HashMap<&str, &LibraryItem> =
            self.items.iter().map(|i| (i.path.as_str(), i)).collect();
        let base_map: HashMap<&str, &LibraryItem> = baseline
            .items
            .iter()
            .map(|i| (i.path.as_str(), i))
            .collect();

        for (path, base_item) in &base_map {
            if let Some(active_item) = active_map.get(path) {
                if base_item.signature != active_item.signature {
                    report.add_modification(DriftItem::new(
                        DriftSurface::Library,
                        base_item.name.clone(),
                        format!(
                            "Signature changed from '{}' to '{}'",
                            base_item.signature, active_item.signature
                        ),
                        true,
                        Some(base_item.signature.clone()),
                        Some(active_item.signature.clone()),
                    ));
                }
            } else {
                report.add_removal(DriftItem::new(
                    DriftSurface::Library,
                    base_item.name.clone(),
                    format!("Public item '{}' removed", base_item.signature),
                    true,
                    Some(base_item.signature.clone()),
                    None,
                ));
            }
        }

        for (path, active_item) in &active_map {
            if !base_map.contains_key(path) {
                report.add_addition(DriftItem::new(
                    DriftSurface::Library,
                    active_item.name.clone(),
                    format!("New public item '{}'", active_item.signature),
                    false,
                    None,
                    Some(active_item.signature.clone()),
                ));
            }
        }

        report
    }
}
