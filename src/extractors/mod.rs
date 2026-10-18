use crate::ConfigFile;
use crate::manifest::ManifestError;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer};
use std::fs;
use std::path::Path;

pub(crate) mod cli;
pub(crate) mod cli_args;
pub(crate) mod config;
pub(crate) mod http;
pub(crate) mod library;
pub(crate) mod library_format;

pub(crate) use cli::CliExtractor;
pub(crate) use config::ConfigExtractor;
pub(crate) use http::HttpExtractor;
pub(crate) use library::LibraryExtractor;

#[derive(Debug, Default, Deserialize, ConfigFile)]
#[config_file("Cargo.toml")]
struct CargoManifestConfig {
    package: Option<CargoPackageConfig>,
    workspace: Option<CargoWorkspaceConfig>,
}

/// Workspace configuration extracted from `Cargo.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct CargoWorkspaceConfig {
    /// Shared workspace package metadata
    pub(crate) package: Option<CargoWorkspacePackageConfig>,
}

/// Shared workspace package metadata extracted from `[workspace.package]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct CargoWorkspacePackageConfig {
    /// Package version
    pub(crate) version: Option<String>,
    /// Minimum supported Rust version
    #[serde(rename = "rust-version")]
    pub(crate) rust_version: Option<String>,
}

/// Package metadata extracted from `Cargo.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct CargoPackageConfig {
    /// Package name
    pub(crate) name: String,
    /// Package version
    #[serde(default, deserialize_with = "deserialize_workspace_field")]
    pub(crate) version: Option<String>,
    /// Minimum supported Rust version
    #[serde(
        default,
        rename = "rust-version",
        deserialize_with = "deserialize_workspace_field"
    )]
    pub(crate) rust_version: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WorkspaceField<T> {
    Value(T),
    Workspace { workspace: bool },
}

fn deserialize_workspace_field<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<WorkspaceField<String>>::deserialize(deserializer)? {
        Some(WorkspaceField::Value(val)) => Ok(Some(val)),
        Some(WorkspaceField::Workspace { workspace: true }) | None => Ok(None),
        Some(WorkspaceField::Workspace { workspace: false }) => Err(DeError::custom(
            "expected `workspace = true` for workspace-inherited field",
        )),
    }
}

impl CargoPackageConfig {
    /// Extracts `[package]` metadata from `Cargo.toml` in `crate_root`.
    pub(crate) fn from_crate_root(crate_root: &Path) -> Result<Self, ManifestError> {
        let cargo_toml = crate_root.join(CargoManifestConfig::config_file_path());
        let content = fs::read_to_string(cargo_toml)?;
        let manifest: CargoManifestConfig = toml::from_str(&content)?;
        let mut pkg = manifest
            .package
            .ok_or(ManifestError::MissingPackageSection)?;
        if pkg.version.is_none() || pkg.rust_version.is_none() {
            let ws_pkg = manifest
                .workspace
                .and_then(|ws| ws.package)
                .or_else(|| Self::find_workspace_package(crate_root));
            if let Some(ws_pkg) = ws_pkg {
                if pkg.version.is_none() {
                    pkg.version = ws_pkg.version;
                }
                if pkg.rust_version.is_none() {
                    pkg.rust_version = ws_pkg.rust_version;
                }
            }
        }
        Ok(pkg)
    }

    fn find_workspace_package(crate_root: &Path) -> Option<CargoWorkspacePackageConfig> {
        let start = crate_root
            .canonicalize()
            .unwrap_or_else(|_| crate_root.to_path_buf());
        let mut current = start.parent();
        while let Some(dir) = current {
            let candidate = dir.join(CargoManifestConfig::config_file_path());
            if candidate.is_file()
                && let Ok(content) = fs::read_to_string(&candidate)
                && let Ok(parsed) = toml::from_str::<CargoManifestConfig>(&content)
                && let Some(ws) = parsed.workspace
            {
                return ws.package;
            }
            current = dir.parent();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ApiManifest;
    use googletest::prelude::*;
    use std::path::PathBuf;

    struct TempDirGuard(PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.0));
        }
    }

    #[googletest::test]
    fn extract_cargo_metadata_reads_name_and_version() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_extract_cargo_meta_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir)?;
        let _guard = TempDirGuard(temp_dir.clone());

        let cargo_toml = r#"
            [package]
            name = "demo-pkg"
            version = "1.2.3"
        "#;
        fs::write(temp_dir.join("Cargo.toml"), cargo_toml)?;

        let manifest = ApiManifest::extract_from_crate(&temp_dir)?;
        expect_that!(manifest.name, eq("demo-pkg"));
        expect_that!(manifest.version, eq(&Some("1.2.3".to_string())));
        Ok(())
    }

    #[googletest::test]
    fn extract_cargo_metadata_returns_error_when_name_is_missing()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_extract_cargo_meta_missing_name_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir)?;
        let _guard = TempDirGuard(temp_dir.clone());

        let cargo_toml = r#"
            [package]
            version = "1.2.3"
        "#;
        fs::write(temp_dir.join("Cargo.toml"), cargo_toml)?;

        let result = ApiManifest::extract_from_crate(&temp_dir);
        expect_that!(result, err(anything()));
        Ok(())
    }

    #[googletest::test]
    fn extract_cargo_metadata_resolves_workspace_inherited_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_extract_cargo_ws_inherit_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let member_dir = temp_dir.join("crates").join("member-pkg");
        fs::create_dir_all(&member_dir)?;
        let _guard = TempDirGuard(temp_dir.clone());

        let ws_cargo_toml = r#"
            [workspace]
            members = ["crates/member-pkg"]

            [workspace.package]
            version = "2.4.0"
            rust-version = "1.88.0"
        "#;
        fs::write(temp_dir.join("Cargo.toml"), ws_cargo_toml)?;

        let member_cargo_toml = r#"
            [package]
            name = "member-pkg"
            version.workspace = true
            rust-version.workspace = true
        "#;
        fs::write(member_dir.join("Cargo.toml"), member_cargo_toml)?;

        let metadata = CargoPackageConfig::from_crate_root(&member_dir)?;
        expect_that!(metadata.name, eq("member-pkg"));
        expect_that!(metadata.version, eq(&Some("2.4.0".to_string())));
        expect_that!(metadata.rust_version, eq(&Some("1.88.0".to_string())));

        let manifest = ApiManifest::extract_from_crate(&member_dir)?;
        expect_that!(manifest.name, eq("member-pkg"));
        expect_that!(manifest.version, eq(&Some("2.4.0".to_string())));

        Ok(())
    }

    #[googletest::test]
    fn extract_cargo_metadata_handles_workspace_fields_without_workspace_root()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_extract_cargo_ws_no_root_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir)?;
        let _guard = TempDirGuard(temp_dir.clone());

        let cargo_toml = r#"
            [package]
            name = "standalone-ws-pkg"
            version.workspace = true
            rust-version.workspace = true
        "#;
        fs::write(temp_dir.join("Cargo.toml"), cargo_toml)?;

        let metadata = CargoPackageConfig::from_crate_root(&temp_dir)?;
        expect_that!(metadata.name, eq("standalone-ws-pkg"));
        expect_that!(metadata.version, none());
        expect_that!(metadata.rust_version, none());

        Ok(())
    }
}
