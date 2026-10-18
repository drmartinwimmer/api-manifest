use crate::cli_api::CliApi;
use crate::diff_report::ApiDriftReport;
use crate::extractors::{
    CargoPackageConfig, CliExtractor, ConfigExtractor, HttpExtractor, LibraryExtractor,
};
use crate::http_api::HttpApi;
use crate::library_api::LibraryApi;
use crate::markdown_parser::MarkdownParseState;
use crate::schema_api::ConfigApi;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use toml::de;

/// Manifest output format.
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
pub(crate) enum ManifestFormat {
    /// GitHub Flavored Markdown (API.md)
    #[default]
    Markdown,
    /// Machine-readable structured JSON
    Json,
}

/// Manifest formatting, parsing, and extraction errors.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("Failed to parse JSON manifest: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Failed to parse Cargo.toml manifest: {0}")]
    Toml(#[from] de::Error),

    #[error("Failed to parse Rust source syntax: {0}")]
    Syn(#[from] syn::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Missing [package] section in Cargo.toml")]
    MissingPackageSection,
}

/// Comprehensive public API manifest representing library, CLI, HTTP, and config surfaces.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiManifest {
    /// Crate or project name
    pub(crate) name: String,
    /// Optional crate version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
    /// Rust library API items
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) library: Option<LibraryApi>,
    /// CLI command and argument items
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cli: Option<CliApi>,
    /// HTTP service endpoints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) http: Option<HttpApi>,
    /// Configuration file format specifications
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) config: Option<ConfigApi>,
}

impl ApiManifest {
    pub(crate) fn new(name: impl Into<String>, version: Option<String>) -> Self {
        Self {
            name: name.into(),
            version,
            library: None,
            cli: None,
            http: None,
            config: None,
        }
    }

    pub(crate) fn with_library(mut self, library: LibraryApi) -> Self {
        self.library = Some(library);
        self
    }

    pub(crate) fn with_cli(mut self, cli: CliApi) -> Self {
        self.cli = Some(cli);
        self
    }

    pub(crate) fn with_http(mut self, http: HttpApi) -> Self {
        self.http = Some(http);
        self
    }

    pub(crate) fn with_config(mut self, config: ConfigApi) -> Self {
        self.config = Some(config);
        self
    }

    /// Auto-detects targets and extracts the public API manifest from a crate root.
    pub fn extract_from_crate(crate_root: &Path) -> Result<Self, ManifestError> {
        let metadata = CargoPackageConfig::from_crate_root(crate_root)?;
        let mut manifest = Self::new(metadata.name, metadata.version);

        let lib_extractor = LibraryExtractor::new();
        if let Some(lib_api) = lib_extractor.extract_from_crate(crate_root)? {
            manifest = manifest.with_library(lib_api);
        }

        let cli_extractor = CliExtractor::new();
        if let Some(cli_api) = cli_extractor.extract_from_crate(crate_root)? {
            manifest = manifest.with_cli(cli_api);
        }

        let http_extractor = HttpExtractor::new();
        if let Some(http_api) = http_extractor.extract_from_crate(crate_root)? {
            manifest = manifest.with_http(http_api);
        }

        let config_extractor = ConfigExtractor::new();
        if let Some(config_api) = config_extractor.extract_from_crate(crate_root)? {
            manifest = manifest.with_config(config_api);
        }

        Ok(manifest)
    }

    /// Reads and parses an `ApiManifest` from disk (supports Markdown or JSON).
    pub fn read_from_file(path: &Path) -> Result<Self, ManifestError> {
        let content = fs::read_to_string(path)?;
        Self::parse_str(&content)
    }

    pub(crate) fn parse_str(content: &str) -> Result<Self, ManifestError> {
        let trimmed = content.trim();

        if trimmed.starts_with('{') {
            return serde_json::from_str(trimmed).map_err(ManifestError::Json);
        }

        Ok(MarkdownParseState::parse_str(content))
    }

    /// Formats this `ApiManifest` as GitHub Flavored Markdown (`API.md`).
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        let version_suffix = self
            .version
            .as_ref()
            .map(|v| format!(" (v{v})"))
            .unwrap_or_default();

        out.push_str(&format!(
            "# Public API Manifest: {}{}\n\n",
            self.name, version_suffix
        ));
        out.push_str("<!-- Generated by api-manifest. Do not edit manually. -->\n\n");

        let mut section_idx = 1;

        if let Some(ref lib) = self.library {
            lib.format_markdown(&mut out, section_idx);
            section_idx += 1;
        }

        if let Some(ref cli) = self.cli {
            cli.format_markdown(&mut out, section_idx);
            section_idx += 1;
        }

        if let Some(ref http) = self.http {
            http.format_markdown(&mut out, section_idx);
            section_idx += 1;
        }

        if let Some(ref config) = self.config {
            config.format_markdown(&mut out, section_idx);
        }

        let trimmed = out.trim_end();
        format!("{trimmed}\n")
    }

    /// Diffs this active `ApiManifest` against a checked-in baseline manifest.
    pub fn diff(&self, baseline: &Self) -> ApiDriftReport {
        let mut report = ApiDriftReport::default();

        let empty_lib = LibraryApi::default();
        let active_lib = self.library.as_ref().unwrap_or(&empty_lib);
        let base_lib = baseline.library.as_ref().unwrap_or(&empty_lib);
        report.extend(active_lib.diff(base_lib));

        let empty_cli = CliApi::default();
        let active_cli = self.cli.as_ref().unwrap_or(&empty_cli);
        let base_cli = baseline.cli.as_ref().unwrap_or(&empty_cli);
        report.extend(active_cli.diff(base_cli));

        let empty_http = HttpApi::default();
        let active_http = self.http.as_ref().unwrap_or(&empty_http);
        let base_http = baseline.http.as_ref().unwrap_or(&empty_http);
        report.extend(active_http.diff(base_http));

        let empty_cfg = ConfigApi::default();
        let active_cfg = self.config.as_ref().unwrap_or(&empty_cfg);
        let base_cfg = baseline.config.as_ref().unwrap_or(&empty_cfg);
        report.extend(active_cfg.diff(base_cfg));

        report.sort();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Severity;
    use crate::library_api::{ItemKind, LibraryApi, LibraryItem};
    use googletest::prelude::*;

    #[googletest::test]
    fn format_and_parse_roundtrip_preserves_manifest() -> Result<(), Box<dyn std::error::Error>> {
        let mut manifest = ApiManifest::new("demo-crate", Some("1.0.0".to_string()));
        let lib_item = LibraryItem::new(
            ItemKind::Function,
            "add",
            "demo_crate::add",
            "pub fn add(a: i32, b: i32) -> i32",
            Some("Add two numbers".to_string()),
        );
        manifest = manifest.with_library(LibraryApi::new(vec![lib_item]));

        let md = manifest.format_markdown();
        expect_that!(
            md,
            contains_substring("# Public API Manifest: demo-crate (v1.0.0)")
        );
        expect_that!(md, contains_substring("pub fn add(a: i32, b: i32) -> i32"));

        let parsed = ApiManifest::parse_str(&md)?;
        expect_that!(parsed.name, eq("demo-crate"));
        expect_that!(parsed.version, eq(&Some("1.0.0".to_string())));

        let lib = parsed.library.ok_or("Library API missing")?;
        expect_that!(lib.items.len(), eq(1));
        let item = lib.items.first().ok_or("Item missing")?;
        expect_that!(item.name, eq("add"));
        expect_that!(item.signature, eq("pub fn add(a: i32, b: i32) -> i32"));

        Ok(())
    }

    #[googletest::test]
    fn parse_json_manifest_succeeds() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = ApiManifest::new("my-tool", Some("0.2.0".to_string()));
        let json = serde_json::to_string_pretty(&manifest)?;
        let parsed = ApiManifest::parse_str(&json)?;
        expect_that!(parsed.name, eq("my-tool"));
        expect_that!(parsed.version, eq(&Some("0.2.0".to_string())));
        Ok(())
    }

    #[googletest::test]
    fn format_and_parse_member_functions_preserves_struct_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut manifest = ApiManifest::new("demo-crate", Some("1.0.0".to_string()));
        let method_item = LibraryItem::new(
            ItemKind::Method,
            "Greeter::new",
            "demo_crate::Greeter::new",
            "pub fn Greeter::new(msg: String) -> Self",
            Some("Constructs a greeter".to_string()),
        );
        manifest = manifest.with_library(LibraryApi::new(vec![method_item]));

        let md = manifest.format_markdown();
        expect_that!(md, contains_substring("### Methods"));
        expect_that!(
            md,
            contains_substring("pub fn Greeter::new(msg: String) -> Self")
        );

        let parsed = ApiManifest::parse_str(&md)?;
        let lib = parsed.library.ok_or("Library API missing")?;
        expect_that!(lib.items.len(), eq(1));
        let item = lib.items.first().ok_or("Item missing")?;
        expect_that!(item.name, eq("Greeter::new"));
        expect_that!(item.kind, eq(ItemKind::Method));
        expect_that!(
            item.signature,
            eq("pub fn Greeter::new(msg: String) -> Self")
        );

        Ok(())
    }

    #[googletest::test]
    fn diff_identical_manifests_is_clean() {
        let manifest = ApiManifest::new("demo", Some("1.0.0".to_string()));
        let report = manifest.diff(&manifest);
        expect_that!(report.is_clean(), is_true());
        expect_that!(report.has_breaking_changes(), is_false());
        expect_that!(report.total_drift_count(), eq(0));
    }

    #[googletest::test]
    fn diff_detects_library_item_removal_as_breaking() -> Result<(), Box<dyn std::error::Error>> {
        let old_item = LibraryItem::new(
            ItemKind::Function,
            "old_fn",
            "demo::old_fn",
            "pub fn old_fn()",
            None,
        );
        let baseline = ApiManifest::new("demo", Some("1.0.0".to_string()))
            .with_library(LibraryApi::new(vec![old_item]));
        let active =
            ApiManifest::new("demo", Some("1.0.0".to_string())).with_library(LibraryApi::default());

        let report = active.diff(&baseline);
        expect_that!(report.is_clean(), is_false());
        expect_that!(report.has_breaking_changes(), is_true());
        expect_that!(report.removals.len(), eq(1));

        let removal = report.removals.first().ok_or("removal missing")?;
        expect_that!(removal.name, eq("old_fn"));
        expect_that!(removal.is_breaking, is_true());

        let diags = report.to_diagnostics("API.md");
        expect_that!(diags.len(), eq(1));
        let diag = diags.first().ok_or("diag missing")?;
        expect_that!(diag.severity, eq(Severity::Error));
        expect_that!(diag.rule, eq("api::drift::removal"));

        Ok(())
    }

    #[googletest::test]
    fn diff_detects_library_item_addition_as_non_breaking() -> Result<(), Box<dyn std::error::Error>>
    {
        let new_item = LibraryItem::new(
            ItemKind::Function,
            "new_fn",
            "demo::new_fn",
            "pub fn new_fn()",
            None,
        );
        let baseline =
            ApiManifest::new("demo", Some("1.0.0".to_string())).with_library(LibraryApi::default());
        let active = ApiManifest::new("demo", Some("1.0.0".to_string()))
            .with_library(LibraryApi::new(vec![new_item]));

        let report = active.diff(&baseline);
        expect_that!(report.is_clean(), is_false());
        expect_that!(report.has_breaking_changes(), is_false());
        expect_that!(report.additions.len(), eq(1));

        let addition = report.additions.first().ok_or("addition missing")?;
        expect_that!(addition.name, eq("new_fn"));
        expect_that!(addition.is_breaking, is_false());

        let diags = report.to_diagnostics("API.md");
        expect_that!(diags.len(), eq(1));
        let diag = diags.first().ok_or("diag missing")?;
        expect_that!(diag.severity, eq(Severity::Warning));
        expect_that!(diag.rule, eq("api::drift::addition"));

        Ok(())
    }

    #[googletest::test]
    fn diff_detects_signature_modification_as_breaking() -> Result<(), Box<dyn std::error::Error>> {
        let base_item = LibraryItem::new(
            ItemKind::Function,
            "compute",
            "demo::compute",
            "pub fn compute(x: i32) -> i32",
            None,
        );
        let active_item = LibraryItem::new(
            ItemKind::Function,
            "compute",
            "demo::compute",
            "pub fn compute(x: i32, y: i32) -> i32",
            None,
        );

        let baseline = ApiManifest::new("demo", Some("1.0.0".to_string()))
            .with_library(LibraryApi::new(vec![base_item]));
        let active = ApiManifest::new("demo", Some("1.0.0".to_string()))
            .with_library(LibraryApi::new(vec![active_item]));

        let report = active.diff(&baseline);
        expect_that!(report.is_clean(), is_false());
        expect_that!(report.has_breaking_changes(), is_true());
        expect_that!(report.modifications.len(), eq(1));

        let mod_item = report.modifications.first().ok_or("mod missing")?;
        expect_that!(mod_item.is_breaking, is_true());
        expect_that!(mod_item.name, eq("compute"));

        Ok(())
    }
}
