use crate::diff_report::ApiDriftReport;
use crate::manifest::ManifestError;
use serde::{Deserialize, Serialize};
use std::path::Path;

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

/// A public item exported by a Rust library.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct LibraryItem {
    pub(crate) kind: ItemKind,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) signature: String,
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

/// Public API surface for a library crate.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct LibraryApi {
    pub(crate) items: Vec<LibraryItem>,
}

impl LibraryApi {
    pub(crate) fn new(mut items: Vec<LibraryItem>) -> Self {
        items.sort();
        Self { items }
    }
}

/// A command line argument or flag.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct CliArgItem {
    pub(crate) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) short: Option<char>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) long: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value_name: Option<String>,
    pub(crate) required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) help: Option<String>,
    pub(crate) is_positional: bool,
}

/// A command line subcommand or entry point.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct CliCommandItem {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) about: Option<String>,
    pub(crate) args: Vec<CliArgItem>,
    pub(crate) subcommands: Vec<String>,
}

impl CliCommandItem {
    pub(crate) fn new(
        name: impl Into<String>,
        about: Option<String>,
        mut args: Vec<CliArgItem>,
        mut subcommands: Vec<String>,
    ) -> Self {
        args.sort();
        subcommands.sort();
        Self {
            name: name.into(),
            about,
            args,
            subcommands,
        }
    }
}

/// Public API surface for CLI binary targets.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct CliApi {
    pub(crate) commands: Vec<CliCommandItem>,
}

impl CliApi {
    pub(crate) fn new(mut commands: Vec<CliCommandItem>) -> Self {
        commands.sort();
        Self { commands }
    }
}

/// An HTTP web service endpoint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct HttpEndpointItem {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) handler: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) doc: Option<String>,
}

impl HttpEndpointItem {
    pub(crate) fn new(
        method: impl Into<String>,
        path: impl Into<String>,
        handler: impl Into<String>,
        request_type: Option<String>,
        response_type: Option<String>,
        doc: Option<String>,
    ) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            handler: handler.into(),
            request_type,
            response_type,
            doc,
        }
    }
}

/// Public API surface for HTTP web service targets.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct HttpApi {
    pub(crate) endpoints: Vec<HttpEndpointItem>,
}

impl HttpApi {
    pub(crate) fn new(mut endpoints: Vec<HttpEndpointItem>) -> Self {
        endpoints.sort();
        Self { endpoints }
    }
}

/// A configuration option or key within a configuration section.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct ConfigOptionItem {
    pub(crate) key: String,
    pub(crate) value_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) doc: Option<String>,
}

impl ConfigOptionItem {
    pub(crate) fn new(
        key: impl Into<String>,
        value_type: impl Into<String>,
        default_value: Option<String>,
        doc: Option<String>,
    ) -> Self {
        Self {
            key: key.into(),
            value_type: value_type.into(),
            default_value,
            doc,
        }
    }
}

/// A configuration section or table (e.g. `[lints.purist]` in Cargo.toml).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct ConfigSectionItem {
    pub(crate) section: String,
    pub(crate) file_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) doc: Option<String>,
    pub(crate) options: Vec<ConfigOptionItem>,
}

impl ConfigSectionItem {
    pub(crate) fn new(
        section: impl Into<String>,
        file_format: impl Into<String>,
        doc: Option<String>,
        mut options: Vec<ConfigOptionItem>,
    ) -> Self {
        options.sort();
        Self {
            section: section.into(),
            file_format: file_format.into(),
            doc,
            options,
        }
    }
}

/// Public API surface for configuration file formats.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct ConfigApi {
    pub(crate) sections: Vec<ConfigSectionItem>,
}

impl ConfigApi {
    pub(crate) fn new(mut sections: Vec<ConfigSectionItem>) -> Self {
        sections.sort();
        Self { sections }
    }
}

/// Comprehensive public API manifest representing library, CLI, HTTP, and config surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiManifest {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) library: Option<LibraryApi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cli: Option<CliApi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) http: Option<HttpApi>,
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
    pub fn extract_from_crate(crate_root: &Path) -> std::io::Result<Self> {
        Self::extract_from_crate_inner(crate_root)
    }

    /// Reads and parses an `ApiManifest` from disk (supports Markdown or JSON).
    pub fn read_from_file(path: &Path) -> Result<Self, ManifestError> {
        Self::read_from_file_inner(path)
    }

    /// Formats this `ApiManifest` as GitHub Flavored Markdown (`API.md`).
    pub fn format_markdown(&self) -> String {
        self.format_markdown_inner()
    }

    /// Diffs this active `ApiManifest` against a checked-in baseline manifest.
    pub fn diff(&self, baseline: &Self) -> ApiDriftReport {
        self.diff_inner(baseline)
    }
}
