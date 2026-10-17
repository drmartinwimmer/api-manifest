use crate::model::ApiManifest;
use std::fs;
use std::path::Path;

pub(crate) mod library;
pub(crate) mod library_format;

pub(crate) use library::LibraryExtractor;

/// High-level function that auto-detects targets and extracts the public API manifest from a crate root.
pub fn extract_crate_api(crate_root: &Path) -> std::io::Result<ApiManifest> {
    let (name, version) = extract_cargo_metadata(crate_root);
    let mut manifest = ApiManifest::new(name, version);

    let lib_extractor = LibraryExtractor::new();
    if let Some(lib_api) = lib_extractor.extract_from_crate(crate_root)? {
        manifest = manifest.with_library(lib_api);
    }

    Ok(manifest)
}

pub(crate) fn extract_cargo_metadata(crate_root: &Path) -> (String, Option<String>) {
    let cargo_toml = crate_root.join("Cargo.toml");
    let mut name = "unnamed".to_string();
    let mut version = None;

    if let Ok(content) = fs::read_to_string(cargo_toml) {
        let mut in_package = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_package = trimmed == "[package]";
            } else if in_package {
                if let Some(val) = trimmed.strip_prefix("name =") {
                    name = val.trim().trim_matches('"').trim_matches('\'').to_string();
                } else if let Some(val) = trimmed.strip_prefix("version =") {
                    version = Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
                }
            }
        }
    }

    (name, version)
}

#[cfg(test)]
mod tests {
    use super::*;
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

        let manifest = extract_crate_api(&temp_dir)?;
        expect_that!(manifest.name, eq("demo-pkg"));
        expect_that!(manifest.version, eq(&Some("1.2.3".to_string())));
        Ok(())
    }
}
