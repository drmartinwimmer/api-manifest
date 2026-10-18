use crate::cli_api::{CliApi, CliArgItem, CliCommandItem};
use crate::http_api::{HttpApi, HttpEndpointItem};
use crate::library_api::{ItemKind, LibraryApi, LibraryItem};
use crate::manifest::ApiManifest;
use crate::schema_api::{ConfigApi, ConfigOptionItem, ConfigSectionItem};

#[derive(Default)]
pub(crate) struct MarkdownParseState {
    name: String,
    version: Option<String>,
    lib_items: Vec<LibraryItem>,
    cli_commands: Vec<CliCommandItem>,
    http_endpoints: Vec<HttpEndpointItem>,
    config_sections: Vec<ConfigSectionItem>,
    current_section: &'static str,
    current_subsection: String,
    current_cmd: Option<CliCommandItem>,
    current_cfg_sec: Option<ConfigSectionItem>,
}

impl MarkdownParseState {
    fn new() -> Self {
        Self {
            name: "unnamed".to_string(),
            ..Self::default()
        }
    }

    pub(crate) fn parse_str(content: &str) -> ApiManifest {
        let mut state = Self::new();
        for line in content.lines() {
            state.parse_line(line);
        }
        state.into_manifest()
    }

    fn flush_pending(&mut self) {
        if let Some(cmd) = self.current_cmd.take() {
            self.cli_commands.push(cmd);
        }
        if let Some(sec) = self.current_cfg_sec.take() {
            self.config_sections.push(sec);
        }
    }

    fn parse_line(&mut self, line: &str) {
        let trimmed = line.trim();

        if let Some(header) = trimmed.strip_prefix("# Public API Manifest:") {
            self.parse_header(header);
        } else if trimmed.contains(". Library API") {
            self.flush_pending();
            self.current_section = "lib";
        } else if trimmed.contains(". CLI API") {
            self.flush_pending();
            self.current_section = "cli";
        } else if trimmed.contains(". HTTP Endpoints") {
            self.flush_pending();
            self.current_section = "http";
        } else if trimmed.contains(". Configuration Formats") {
            self.flush_pending();
            self.current_section = "config";
        } else {
            self.parse_section_line(trimmed);
        }
    }

    fn parse_header(&mut self, header: &str) {
        let parts: Vec<&str> = header.split_whitespace().collect();
        if let Some(crate_name) = parts.first() {
            self.name = (*crate_name).to_string();
        }
        if let Some(ver) = parts.get(1) {
            let v = ver
                .trim_matches('(')
                .trim_matches(')')
                .trim_start_matches('v');
            self.version = Some(v.to_string());
        }
    }

    fn parse_section_line(&mut self, trimmed: &str) {
        match self.current_section {
            "lib" => self.parse_lib_line(trimmed),
            "cli" => self.parse_cli_line(trimmed),
            "http" => self.parse_http_line(trimmed),
            "config" => self.parse_config_line(trimmed),
            _ => {}
        }
    }

    fn parse_lib_line(&mut self, trimmed: &str) {
        if let Some(sub) = trimmed.strip_prefix("### ") {
            self.current_subsection = sub.trim().to_string();
        } else if trimmed.starts_with("- `")
            && let Some(sig) = extract_code_span(trimmed)
        {
            let kind = ItemKind::from_markdown_section(&self.current_subsection, &sig);
            let item_name = Self::parse_lib_item_name(&sig);
            let mod_prefix = self.name.replace('-', "_");
            let path = format!("{mod_prefix}::{item_name}");
            self.lib_items
                .push(LibraryItem::new(kind, item_name, path, sig, None));
        }
    }

    fn parse_lib_item_name(sig: &str) -> String {
        if let Some(rest) = sig.strip_prefix("pub use ") {
            return rest
                .strip_suffix(';')
                .and_then(|s| s.split("::").last())
                .unwrap_or("unknown")
                .to_string();
        }
        if let Some(rest) = sig.strip_prefix("impl ") {
            return rest
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string();
        }
        let word = sig
            .split_whitespace()
            .find(|w| {
                ![
                    "pub", "fn", "async", "const", "unsafe", "struct", "enum", "trait", "type",
                    "static", "mut",
                ]
                .contains(w)
            })
            .unwrap_or("unknown");
        word.split('(')
            .next()
            .unwrap_or(word)
            .split('<')
            .next()
            .unwrap_or(word)
            .trim_end_matches('{')
            .trim_end_matches(';')
            .trim_end_matches(':')
            .to_string()
    }

    fn parse_cli_line(&mut self, trimmed: &str) {
        if trimmed.starts_with("### `") {
            if let Some(cmd) = self.current_cmd.take() {
                self.cli_commands.push(cmd);
            }
            if let Some(cmd_name) = extract_code_span(trimmed) {
                self.current_cmd =
                    Some(CliCommandItem::new(cmd_name, None, Vec::new(), Vec::new()));
            }
        } else if (trimmed.starts_with("| `")
            || (trimmed.starts_with('|')
                && !trimmed.starts_with("| ---")
                && !trimmed.starts_with("| Flag")))
            && let Some(ref mut cmd) = self.current_cmd
            && let Some(arg) = CliArgItem::from_markdown_row(trimmed)
        {
            cmd.args.push(arg);
        } else if trimmed.starts_with("- `")
            && let Some(ref mut cmd) = self.current_cmd
            && let Some(sub) = extract_code_span(trimmed)
        {
            cmd.subcommands.push(sub);
        }
    }

    fn parse_http_line(&mut self, trimmed: &str) {
        if !trimmed.starts_with('|')
            || trimmed.starts_with("| ---")
            || trimmed.starts_with("| Method")
        {
            return;
        }
        let cols = split_markdown_cells(trimmed);
        if cols.len() < 4 {
            return;
        }
        let method = cols
            .get(1)
            .map(|s| s.trim_matches('`'))
            .unwrap_or_default()
            .to_string();
        let path = cols
            .get(2)
            .map(|s| s.trim_matches('`'))
            .unwrap_or_default()
            .to_string();
        let handler = cols
            .get(3)
            .map(|s| s.trim_matches('`'))
            .unwrap_or_default()
            .to_string();
        if !method.is_empty() && !path.is_empty() {
            self.http_endpoints.push(HttpEndpointItem::new(
                method, path, handler, None, None, None,
            ));
        }
    }

    fn parse_config_line(&mut self, trimmed: &str) {
        if trimmed.starts_with("### `") {
            if let Some(sec) = self.current_cfg_sec.take() {
                self.config_sections.push(sec);
            }
            let sec_name = extract_code_span(trimmed).unwrap_or_default();
            let file_fmt = trimmed
                .split("(_")
                .nth(1)
                .and_then(|s| s.split("_)").next())
                .or_else(|| {
                    trimmed
                        .split("(*")
                        .nth(1)
                        .and_then(|s| s.split("*)").next())
                })
                .unwrap_or("Cargo.toml");
            self.current_cfg_sec =
                Some(ConfigSectionItem::new(sec_name, file_fmt, None, Vec::new()));
        } else if (trimmed.starts_with("| `") || trimmed.starts_with('|'))
            && !trimmed.starts_with("| ---")
            && !trimmed.starts_with("| Key")
            && let Some(ref mut sec) = self.current_cfg_sec
            && let Some(opt) = ConfigOptionItem::from_markdown_row(trimmed)
        {
            sec.options.push(opt);
        } else if !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && !trimmed.starts_with('|')
            && !trimmed.starts_with("<!--")
            && let Some(ref mut sec) = self.current_cfg_sec
            && sec.doc.is_none()
            && sec.options.is_empty()
        {
            sec.doc = Some(trimmed.to_string());
        }
    }

    fn into_manifest(mut self) -> ApiManifest {
        self.flush_pending();
        let mut manifest = ApiManifest::new(self.name, self.version);
        if !self.lib_items.is_empty() {
            manifest = manifest.with_library(LibraryApi::new(self.lib_items));
        }
        if !self.cli_commands.is_empty() {
            manifest = manifest.with_cli(CliApi::new(self.cli_commands));
        }
        if !self.http_endpoints.is_empty() {
            manifest = manifest.with_http(HttpApi::new(self.http_endpoints));
        }
        if !self.config_sections.is_empty() {
            manifest = manifest.with_config(ConfigApi::new(self.config_sections));
        }
        manifest
    }
}

pub(crate) fn format_aligned_table(out: &mut String, headers: &[&str], rows: &[Vec<String>]) {
    let col_count = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();

    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = widths.get_mut(i) {
                let len = cell.chars().count();
                if len > *w {
                    *w = len;
                }
            }
        }
    }

    out.push('|');
    for (i, h) in headers.iter().enumerate() {
        let w = widths.get(i).copied().unwrap_or(3).max(3);
        out.push_str(&format!(" {h:<w$} |"));
    }
    out.push('\n');

    out.push('|');
    for i in 0..col_count {
        let w = widths.get(i).copied().unwrap_or(3).max(3);
        let dashes = "-".repeat(w);
        out.push_str(&format!(" {dashes} |"));
    }
    out.push('\n');

    for row in rows {
        out.push('|');
        for i in 0..col_count {
            let cell = row.get(i).map(String::as_str).unwrap_or_default();
            let w = widths.get(i).copied().unwrap_or(3).max(3);
            out.push_str(&format!(" {cell:<w$} |"));
        }
        out.push('\n');
    }
    out.push('\n');
}

pub(crate) fn split_markdown_cells(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = row.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&'|') = chars.peek() {
                current.push('|');
                chars.next();
                continue;
            }
            current.push('\\');
        } else if ch == '|' {
            cells.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    cells.push(current.trim().to_string());
    cells
}

fn extract_code_span(line: &str) -> Option<String> {
    let mut parts = line.split('`');
    parts.next()?;
    let span = parts.next()?;
    Some(span.to_string())
}
