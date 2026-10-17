use crate::diagnostics::{DiagnosticReport, OutputFormat};
use crate::manifest::{ManifestError, ManifestFormat};
use crate::model::ApiManifest;
use clap::{Args, Subcommand, ValueEnum};
use std::fs;
use std::path::{Path, PathBuf};

/// Drift threshold triggering non-zero exit code.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ApiFailOn {
    /// Fail if any drift (additions, removals, or modifications) is detected.
    #[default]
    Any,
    /// Fail only if breaking changes (removals or signature modifications) are detected.
    Breaking,
}

/// Error type for API manifest inspection execution.
#[derive(Debug, thiserror::Error)]
pub enum ApiManifestError {
    /// Target path was not found on disk.
    #[error("Target path '{0}' was not found")]
    PathNotFound(PathBuf),

    /// Expected API manifest file was not found.
    #[error("API manifest not found at '{0}'")]
    ManifestNotFound(PathBuf),

    /// I/O error during API manifest operation.
    #[error("I/O error during API inspection: {0}")]
    Io(#[from] std::io::Error),

    /// Manifest parsing or formatting error.
    #[error("Manifest error: {0}")]
    Manifest(#[from] ManifestError),

    /// JSON serialization error.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Drift detected between active code and checked-in manifest.
    #[error("API drift detected ({drift_count} issue(s), {breaking_count} breaking)")]
    DriftDetected {
        drift_count: usize,
        breaking_count: usize,
    },
}

/// Arguments for `api-manifest dump`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ApiDumpArgs {
    /// Path to target crate or workspace directory
    #[arg(long)]
    path: Option<PathBuf>,

    /// Output manifest file path
    #[arg(short, long, default_value = "API.md")]
    output: PathBuf,

    /// Manifest serialization format (markdown or json)
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// Silence non-essential logging output
    #[arg(short, long)]
    quiet: bool,
}

impl ApiDumpArgs {
    /// Creates a new `ApiDumpArgs` instance.
    #[cfg(test)]
    pub(crate) fn new(output: PathBuf, format: Option<OutputFormat>, quiet: bool) -> Self {
        Self {
            path: None,
            output,
            format,
            quiet,
        }
    }

    /// Sets the target directory path.
    pub(crate) fn with_path(mut self, path: Option<PathBuf>) -> Self {
        self.path = path;
        self
    }

    /// Sets quiet logging flag.
    pub(crate) fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Executes the API manifest dump command.
    pub(crate) fn run(self) -> Result<(), ApiManifestError> {
        let target_dir = self.path.unwrap_or_else(|| PathBuf::from("."));
        if !target_dir.exists() {
            return Err(ApiManifestError::PathNotFound(target_dir));
        }

        let manifest_format = match self.format {
            Some(OutputFormat::Json) => ManifestFormat::Json,
            Some(OutputFormat::Markdown) | Some(OutputFormat::Console) | None => {
                ManifestFormat::Markdown
            }
        };

        let manifest = ApiManifest::extract_from_crate(&target_dir)?;
        let content = match manifest_format {
            ManifestFormat::Markdown => manifest.format_markdown(),
            ManifestFormat::Json => serde_json::to_string_pretty(&manifest)?,
        };

        let target_output = if self.output.is_absolute() || self.output.starts_with(&target_dir) {
            self.output
        } else {
            target_dir.join(self.output)
        };

        if let Some(parent) = target_output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target_output, content)?;

        Ok(())
    }
}

/// Arguments for `api-manifest check`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ApiCheckArgs {
    /// Path to target crate or workspace directory
    #[arg(long)]
    path: Option<PathBuf>,

    /// Path to checked-in API manifest file
    #[arg(short, long, default_value = "API.md")]
    manifest: PathBuf,

    /// Severity threshold triggering non-zero exit: any (default) or breaking
    #[arg(long, value_enum, default_value_t = ApiFailOn::Any)]
    fail_on: ApiFailOn,

    /// Output format for diagnostics
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// Silence non-essential logging output
    #[arg(short, long)]
    quiet: bool,
}

impl ApiCheckArgs {
    /// Creates a new `ApiCheckArgs` instance.
    pub(crate) fn new(manifest: PathBuf, fail_on: ApiFailOn, quiet: bool) -> Self {
        Self {
            path: None,
            manifest,
            fail_on,
            format: None,
            quiet,
        }
    }

    /// Sets the target directory path.
    pub(crate) fn with_path(mut self, path: Option<PathBuf>) -> Self {
        self.path = path;
        self
    }

    /// Sets quiet logging flag.
    pub(crate) fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Executes the API manifest drift check command.
    pub(crate) fn run(self) -> Result<(), ApiManifestError> {
        let target_dir = self.path.unwrap_or_else(|| PathBuf::from("."));
        if !target_dir.exists() {
            return Err(ApiManifestError::PathNotFound(target_dir));
        }

        let target_manifest = if self.manifest.is_absolute() {
            self.manifest.clone()
        } else {
            target_dir.join(&self.manifest)
        };

        let manifests_to_check = if target_manifest.exists() {
            vec![(target_dir.clone(), target_manifest)]
        } else if self.manifest == Path::new("API.md") {
            let found = Self::find_api_manifests(&target_dir);
            if found.is_empty() {
                return Ok(());
            }
            found
                .into_iter()
                .map(|m| {
                    let crate_dir = m.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
                    (crate_dir, m)
                })
                .collect()
        } else {
            return Err(ApiManifestError::ManifestNotFound(target_manifest));
        };

        let mut total_drift = 0;
        let mut total_breaking = 0;
        let mut any_drift = false;
        let mut any_breaking = false;

        for (crate_dir, manifest_path) in manifests_to_check {
            let baseline = ApiManifest::read_from_file(&manifest_path)?;
            let active = ApiManifest::extract_from_crate(&crate_dir)?;
            let report = active.diff(&baseline);

            if !self.quiet && !report.is_clean() {
                let output_format = self.format.unwrap_or(OutputFormat::Console);
                let diags = report.to_diagnostics(&manifest_path.display().to_string());
                let diag_report = DiagnosticReport::new(diags);
                diag_report.render(output_format, &mut std::io::stderr())?;
            }

            if !report.is_clean() {
                any_drift = true;
                total_drift += report.total_drift_count();
            }
            if report.has_breaking_changes() {
                any_breaking = true;
                total_breaking += report.breaking_count();
            }
        }

        let fail = match self.fail_on {
            ApiFailOn::Any => any_drift,
            ApiFailOn::Breaking => any_breaking,
        };

        if fail {
            return Err(ApiManifestError::DriftDetected {
                drift_count: total_drift,
                breaking_count: total_breaking,
            });
        }

        Ok(())
    }

    fn find_api_manifests(root: &Path) -> Vec<PathBuf> {
        if root.is_file() {
            if root.file_name().and_then(|s| s.to_str()) == Some("API.md") {
                return vec![root.to_path_buf()];
            }
            return Vec::new();
        }

        let mut manifests = Vec::new();
        Self::crawl_api_manifests(root, &mut manifests);
        manifests.sort();
        manifests
    }

    fn crawl_api_manifests(dir: &Path, manifests: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };

            if path.is_dir() {
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                Self::crawl_api_manifests(&path, manifests);
            } else if path.is_file() && name == "API.md" {
                manifests.push(path);
            }
        }
    }
}

/// Subcommands for the API manifest and drift inspection tool.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub(crate) enum ApiSubcommand {
    /// Inspect codebase and generate or update the API manifest (API.md)
    Dump(ApiDumpArgs),

    /// Compare active codebase against the checked-in API manifest and detect drift
    Check(ApiCheckArgs),
}

/// Command-line arguments for `api-manifest`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct ApiManifestCommand {
    #[command(subcommand)]
    command: Option<ApiSubcommand>,

    /// Path to target workspace or crate directory
    #[arg(long, global = true)]
    path: Option<PathBuf>,

    /// Silence non-essential logging output
    #[arg(short, long, global = true)]
    quiet: bool,
}

impl ApiManifestCommand {
    /// Creates a new `ApiManifestCommand`.
    #[cfg(test)]
    pub(crate) fn new(path: Option<PathBuf>, quiet: bool) -> Self {
        Self {
            command: None,
            path,
            quiet,
        }
    }

    /// Sets the subcommand.
    #[cfg(test)]
    pub(crate) fn with_subcommand(mut self, subcommand: ApiSubcommand) -> Self {
        self.command = Some(subcommand);
        self
    }

    /// Runs the API command (defaults to check if no subcommand is given).
    pub fn run(self) -> Result<(), ApiManifestError> {
        let global_path = self.path;
        let global_quiet = self.quiet;

        match self.command {
            Some(ApiSubcommand::Dump(mut args)) => {
                if args.path.is_none() {
                    args = args.with_path(global_path);
                }
                if global_quiet {
                    args = args.with_quiet(true);
                }
                args.run()
            }
            Some(ApiSubcommand::Check(mut args)) => {
                if args.path.is_none() {
                    args = args.with_path(global_path);
                }
                if global_quiet {
                    args = args.with_quiet(true);
                }
                args.run()
            }
            None => {
                let target_dir = global_path.unwrap_or_else(|| PathBuf::from("."));
                let manifest_file = PathBuf::from("API.md");
                let check_args = ApiCheckArgs::new(manifest_file, ApiFailOn::Any, global_quiet)
                    .with_path(Some(target_dir));
                check_args.run()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    struct TempDirGuard(PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.0));
        }
    }

    #[googletest::test]
    fn run_api_dump_and_check_clean_manifest_succeeds() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_api_dump_check_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir)?;
        let _guard = TempDirGuard(temp_dir.clone());

        let cargo_toml = r#"
            [package]
            name = "sample-crate"
            version = "0.1.0"
        "#;
        fs::write(temp_dir.join("Cargo.toml"), cargo_toml)?;

        let dump_args =
            ApiDumpArgs::new(PathBuf::from("API.md"), None, true).with_path(Some(temp_dir.clone()));
        let dump_cmd =
            ApiManifestCommand::new(None, true).with_subcommand(ApiSubcommand::Dump(dump_args));
        dump_cmd.run()?;

        expect_that!(temp_dir.join("API.md").exists(), is_true());

        let check_args = ApiCheckArgs::new(PathBuf::from("API.md"), ApiFailOn::Any, true)
            .with_path(Some(temp_dir));
        let check_cmd =
            ApiManifestCommand::new(None, true).with_subcommand(ApiSubcommand::Check(check_args));
        check_cmd.run()?;

        Ok(())
    }

    #[googletest::test]
    fn run_api_check_detects_drift_and_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_api_drift_err_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir)?;
        let _guard = TempDirGuard(temp_dir.clone());

        let cargo_toml = r#"
            [package]
            name = "drift-crate"
            version = "0.1.0"
        "#;
        fs::write(temp_dir.join("Cargo.toml"), cargo_toml)?;

        let modified_manifest = r#"# Public API Manifest: drift-crate (v0.1.0)

## 1. Library API

### Functions

- `pub fn missing_fn()`
"#;
        fs::write(temp_dir.join("API.md"), modified_manifest)?;

        let check_args = ApiCheckArgs::new(PathBuf::from("API.md"), ApiFailOn::Any, true)
            .with_path(Some(temp_dir));
        let check_cmd =
            ApiManifestCommand::new(None, true).with_subcommand(ApiSubcommand::Check(check_args));

        let result = check_cmd.run();
        assert_that!(result, err(anything()));

        Ok(())
    }

    #[googletest::test]
    fn run_api_check_workspace_discovery_checks_nested_manifests()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_api_ws_disc_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir)?;
        let _guard = TempDirGuard(temp_dir.clone());

        let subcrate_dir = temp_dir.join("crates").join("sub");
        fs::create_dir_all(&subcrate_dir)?;
        let cargo_toml = r#"
            [package]
            name = "sub-crate"
            version = "0.1.0"
        "#;
        fs::write(subcrate_dir.join("Cargo.toml"), cargo_toml)?;

        let dump_args = ApiDumpArgs::new(PathBuf::from("API.md"), None, true)
            .with_path(Some(subcrate_dir.clone()));
        let dump_cmd =
            ApiManifestCommand::new(None, true).with_subcommand(ApiSubcommand::Dump(dump_args));
        dump_cmd.run()?;

        let check_args = ApiCheckArgs::new(PathBuf::from("API.md"), ApiFailOn::Any, true)
            .with_path(Some(temp_dir.clone()));
        let check_cmd =
            ApiManifestCommand::new(None, true).with_subcommand(ApiSubcommand::Check(check_args));
        assert_that!(check_cmd.run(), ok(anything()));

        let modified_manifest = r#"# Public API Manifest: sub-crate (v0.1.0)

## 1. Library API

### Functions

- `pub fn missing_fn()`
"#;
        fs::write(subcrate_dir.join("API.md"), modified_manifest)?;
        let check_args2 = ApiCheckArgs::new(PathBuf::from("API.md"), ApiFailOn::Any, true)
            .with_path(Some(temp_dir));
        let check_cmd2 =
            ApiManifestCommand::new(None, true).with_subcommand(ApiSubcommand::Check(check_args2));
        assert_that!(check_cmd2.run(), err(anything()));

        Ok(())
    }
}
