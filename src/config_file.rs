//! Configuration file abstractions and derive macros.
//!
//! Provides the [`ConfigFile`] trait and procedural derive macro to bind
//! configuration structures with their backing configuration files (such as `Cargo.toml`).

pub use api_manifest_derive::ConfigFile;

/// Trait implemented by structures representing configuration file contents.
pub trait ConfigFile {
    /// Returns the configuration file path relative to workspace or project root.
    fn config_file_path() -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[derive(Debug, PartialEq, Eq, ConfigFile)]
    #[config_file("Cargo.toml")]
    struct CargoConfig {
        version: u32,
    }

    #[derive(Debug, PartialEq, Eq, ConfigFile)]
    #[config_file(".prettierrc.json")]
    #[config_section("formatting")]
    struct PrettierConfig {
        tab_width: u32,
    }

    #[googletest::test]
    fn derives_config_file_returns_correct_path() {
        expect_that!(CargoConfig::config_file_path(), eq("Cargo.toml"));
        expect_that!(PrettierConfig::config_file_path(), eq(".prettierrc.json"));
    }
}
