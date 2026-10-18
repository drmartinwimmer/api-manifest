extern crate self as api_manifest;

mod cli_api;
mod commands;
mod config_file;
mod diagnostics;
mod diff_report;
mod extractors;
mod http_api;
mod library_api;
mod manifest;
mod markdown_parser;
mod schema_api;

pub use commands::{ApiManifestCommand, ApiManifestError};
pub use config_file::ConfigFile;
pub use diff_report::ApiDriftReport;
pub use manifest::{ApiManifest, ManifestError};

#[cfg(test)]
mod tests {
    use crate::extractors::CargoPackageMetadata;
    use googletest::prelude::*;
    use std::fs;
    use std::path::Path;

    #[googletest::test]
    fn verify_msrv_badge_consistency_succeeds() -> Result<(), Box<dyn std::error::Error>> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let metadata = CargoPackageMetadata::from_crate_root(manifest_dir)?;
        let rust_version = metadata
            .rust_version
            .ok_or("rust-version not found in Cargo.toml")?;

        let mut parts = rust_version.split('.');
        let major = parts.next().ok_or("Missing major version component")?;
        let minor = parts.next().ok_or("Missing minor version component")?;
        let badge_target = format!("https://img.shields.io/badge/MSRV-{major}.{minor}%2B-");

        let readme_content = fs::read_to_string(manifest_dir.join("README.md"))?;
        assert_that!(readme_content.contains(&badge_target), is_true());

        Ok(())
    }
}
