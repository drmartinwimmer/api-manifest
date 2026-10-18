use crate::manifest::ManifestError;
use serde::Deserialize;
use std::fs;
use std::path::Path;

pub(crate) mod cli;
pub(crate) mod cli_args;
pub(crate) mod library;
pub(crate) mod library_format;

pub(crate) use cli::CliExtractor;
pub(crate) use library::LibraryExtractor;

#[derive(Debug, Default, Deserialize)]
struct CargoManifest {
    package: Option<CargoPackageMetadata>,
}

/// Package metadata extracted from `Cargo.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct CargoPackageMetadata {
    pub(crate) name: String,
    pub(crate) version: Option<String>,
    #[serde(rename = "rust-version")]
    pub(crate) rust_version: Option<String>,
}

impl CargoPackageMetadata {
    /// Extracts `[package]` metadata from `Cargo.toml` in `crate_root`.
    pub(crate) fn from_crate_root(crate_root: &Path) -> Result<Self, ManifestError> {
        let cargo_toml = crate_root.join("Cargo.toml");
        let content = fs::read_to_string(cargo_toml)?;
        let manifest: CargoManifest = toml::from_str(&content)?;
        manifest.package.ok_or(ManifestError::MissingPackageSection)
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
}
