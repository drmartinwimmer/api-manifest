use serde::{Deserialize, Serialize};

/// Kind of public library item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
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
pub struct LibraryItem {
    pub kind: ItemKind,
    pub name: String,
    pub path: String,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

impl LibraryItem {
    /// Creates a new `LibraryItem`.
    pub fn new(
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
pub struct LibraryApi {
    pub items: Vec<LibraryItem>,
}

impl LibraryApi {
    /// Creates a new `LibraryApi` with items sorted deterministically.
    pub fn new(mut items: Vec<LibraryItem>) -> Self {
        items.sort();
        Self { items }
    }
}

/// A command line argument or flag.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CliArgItem {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short: Option<char>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_name: Option<String>,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    pub is_positional: bool,
}

/// A command line subcommand or entry point.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CliCommandItem {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    pub args: Vec<CliArgItem>,
    pub subcommands: Vec<String>,
}

impl CliCommandItem {
    /// Creates a new `CliCommandItem` with sorted arguments and subcommands.
    pub fn new(
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
pub struct CliApi {
    pub commands: Vec<CliCommandItem>,
}

impl CliApi {
    /// Creates a new `CliApi` with sorted commands.
    pub fn new(mut commands: Vec<CliCommandItem>) -> Self {
        commands.sort();
        Self { commands }
    }
}

/// An HTTP web service endpoint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HttpEndpointItem {
    pub method: String,
    pub path: String,
    pub handler: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

impl HttpEndpointItem {
    /// Creates a new `HttpEndpointItem`.
    pub fn new(
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
pub struct HttpApi {
    pub endpoints: Vec<HttpEndpointItem>,
}

impl HttpApi {
    /// Creates a new `HttpApi` with sorted endpoints.
    pub fn new(mut endpoints: Vec<HttpEndpointItem>) -> Self {
        endpoints.sort();
        Self { endpoints }
    }
}

/// A configuration option or key within a configuration section.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConfigOptionItem {
    pub key: String,
    pub value_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

impl ConfigOptionItem {
    /// Creates a new `ConfigOptionItem`.
    pub fn new(
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
pub struct ConfigSectionItem {
    pub section: String,
    pub file_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub options: Vec<ConfigOptionItem>,
}

impl ConfigSectionItem {
    /// Creates a new `ConfigSectionItem` with sorted options.
    pub fn new(
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
pub struct ConfigApi {
    pub sections: Vec<ConfigSectionItem>,
}

impl ConfigApi {
    /// Creates a new `ConfigApi` with sorted sections.
    pub fn new(mut sections: Vec<ConfigSectionItem>) -> Self {
        sections.sort();
        Self { sections }
    }
}

/// Comprehensive public API manifest representing library, CLI, HTTP, and config surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiManifest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<LibraryApi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<CliApi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpApi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ConfigApi>,
}

impl ApiManifest {
    /// Creates a new empty `ApiManifest` with a crate name.
    pub fn new(name: impl Into<String>, version: Option<String>) -> Self {
        Self {
            name: name.into(),
            version,
            library: None,
            cli: None,
            http: None,
            config: None,
        }
    }

    /// Sets the library surface.
    pub fn with_library(mut self, library: LibraryApi) -> Self {
        self.library = Some(library);
        self
    }

    /// Sets the CLI surface.
    pub fn with_cli(mut self, cli: CliApi) -> Self {
        self.cli = Some(cli);
        self
    }

    /// Sets the HTTP service surface.
    pub fn with_http(mut self, http: HttpApi) -> Self {
        self.http = Some(http);
        self
    }

    /// Sets the configuration file surface.
    pub fn with_config(mut self, config: ConfigApi) -> Self {
        self.config = Some(config);
        self
    }

    /// Returns true if all surfaces are empty or None.
    pub fn is_empty(&self) -> bool {
        let lib_empty = self.library.as_ref().is_none_or(|l| l.items.is_empty());
        let cli_empty = self.cli.as_ref().is_none_or(|c| c.commands.is_empty());
        let http_empty = self.http.as_ref().is_none_or(|h| h.endpoints.is_empty());
        let config_empty = self.config.as_ref().is_none_or(|c| c.sections.is_empty());
        lib_empty && cli_empty && http_empty && config_empty
    }
}
