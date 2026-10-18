use crate::diff_report::{ApiDriftReport, DriftItem, DriftSurface};
use crate::markdown_parser::{format_aligned_table, split_markdown_cells};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single CLI argument or flag.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct CliArgItem {
    /// Argument name / ID
    pub(crate) id: String,
    /// Optional short flag (e.g. `q`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) short: Option<char>,
    /// Optional long flag (e.g. `quiet`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) long: Option<String>,
    /// Value placeholder / type hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value_name: Option<String>,
    /// Whether this argument is required
    pub(crate) required: bool,
    /// Default value if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_value: Option<String>,
    /// Help text description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) help: Option<String>,
    /// Whether this is a positional argument
    pub(crate) is_positional: bool,
}

impl CliArgItem {
    pub(crate) fn format_flag_col(&self) -> String {
        let mut parts = Vec::new();
        if let Some(s) = self.short {
            parts.push(format!("`-{s}`"));
        }
        if let Some(ref l) = self.long {
            parts.push(format!("`--{l}`"));
        }
        if parts.is_empty() {
            format!("`<{}>`", self.id.to_uppercase())
        } else {
            parts.join(", ")
        }
    }

    pub(crate) fn from_markdown_row(trimmed: &str) -> Option<Self> {
        let cols = split_markdown_cells(trimmed);
        if cols.len() < 6 {
            return None;
        }
        let flag_col = cols.get(1).map(String::as_str).unwrap_or_default();
        let type_col = cols.get(2).map(|s| s.trim_matches('`')).unwrap_or_default();
        let req_col = cols.get(3).map(String::as_str) == Some("Yes");
        let def_col = match cols.get(4).map(String::as_str) {
            Some("-" | "") | None => None,
            Some(s) => Some(s.trim_matches('`').to_string()),
        };
        let desc_col = match cols.get(5).map(String::as_str) {
            Some("-" | "") | None => None,
            Some(s) => Some(s.to_string()),
        };

        let mut short = None;
        let mut long = None;
        let is_positional = !flag_col.contains('-');
        for part in flag_col.split(',').map(|p| p.trim().trim_matches('`')) {
            if let Some(l) = part.strip_prefix("--") {
                let l_name = l.split_whitespace().next().unwrap_or(l);
                long = Some(l_name.to_string());
            } else if let Some(s) = part.strip_prefix('-') {
                short = s.chars().next();
            }
        }
        let id = long
            .clone()
            .or_else(|| short.map(|c| c.to_string()))
            .unwrap_or_else(|| flag_col.trim_matches('<').trim_matches('>').to_string());

        Some(Self {
            id,
            short,
            long,
            value_name: if type_col == "-" {
                None
            } else {
                Some(type_col.to_string())
            },
            required: req_col,
            default_value: def_col,
            help: desc_col,
            is_positional,
        })
    }
}

/// A CLI command or subcommand entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct CliCommandItem {
    /// Full command invocation path (e.g. `api-manifest check`)
    pub(crate) name: String,
    /// Optional command description (`about`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) about: Option<String>,
    /// Arguments and flags accepted by this command
    pub(crate) args: Vec<CliArgItem>,
    /// Subcommand names nested under this command
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

    pub(crate) fn diff(&self, base_cmd: &Self) -> ApiDriftReport {
        let mut report = ApiDriftReport::default();
        let active_args: HashMap<&str, &CliArgItem> =
            self.args.iter().map(|a| (a.id.as_str(), a)).collect();
        let base_args: HashMap<&str, &CliArgItem> =
            base_cmd.args.iter().map(|a| (a.id.as_str(), a)).collect();

        for (id, base_arg) in &base_args {
            if let Some(active_arg) = active_args.get(id) {
                if !base_arg.required && active_arg.required && active_arg.default_value.is_none() {
                    report.add_modification(DriftItem::new(
                        DriftSurface::Cli,
                        format!("{} --{}", base_cmd.name, id),
                        "Previously optional argument is now required without a default"
                            .to_string(),
                        true,
                        None,
                        None,
                    ));
                }
            } else {
                report.add_removal(DriftItem::new(
                    DriftSurface::Cli,
                    format!("{} --{}", base_cmd.name, id),
                    "CLI argument or flag removed".to_string(),
                    true,
                    None,
                    None,
                ));
            }
        }

        for (id, active_arg) in &active_args {
            if !base_args.contains_key(id) {
                if active_arg.required && active_arg.default_value.is_none() {
                    report.add_modification(DriftItem::new(
                        DriftSurface::Cli,
                        format!("{} --{}", base_cmd.name, id),
                        "New required CLI argument without a default is a breaking change"
                            .to_string(),
                        true,
                        None,
                        None,
                    ));
                } else {
                    report.add_addition(DriftItem::new(
                        DriftSurface::Cli,
                        format!("{} --{}", base_cmd.name, id),
                        "New CLI argument or flag added".to_string(),
                        false,
                        None,
                        None,
                    ));
                }
            }
        }

        report
    }
}

/// Public CLI API surface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CliApi {
    pub(crate) commands: Vec<CliCommandItem>,
}

impl CliApi {
    pub(crate) fn new(mut commands: Vec<CliCommandItem>) -> Self {
        commands.sort();
        Self { commands }
    }

    pub(crate) fn format_markdown(&self, out: &mut String, section_idx: usize) {
        if self.commands.is_empty() {
            return;
        }
        out.push_str(&format!("## {section_idx}. CLI API\n\n"));

        for cmd in &self.commands {
            out.push_str(&format!("### `{}`\n\n", cmd.name));
            if let Some(ref about) = cmd.about {
                out.push_str(&format!("{about}\n\n"));
            }

            if !cmd.args.is_empty() {
                let headers = [
                    "Flag / Argument",
                    "Type",
                    "Required",
                    "Default",
                    "Description",
                ];
                let rows: Vec<Vec<String>> = cmd
                    .args
                    .iter()
                    .map(|arg| {
                        let flag_col = arg.format_flag_col();
                        let type_col = format!("`{}`", arg.value_name.as_deref().unwrap_or("-"));
                        let req_col = if arg.required { "Yes" } else { "No" }.to_string();
                        let def_col = format!("`{}`", arg.default_value.as_deref().unwrap_or("-"));
                        let desc_col = arg.help.as_deref().unwrap_or("-").to_string();
                        vec![flag_col, type_col, req_col, def_col, desc_col]
                    })
                    .collect();
                format_aligned_table(out, &headers, &rows);
            }

            if !cmd.subcommands.is_empty() {
                out.push_str("**Subcommands:**\n\n");
                for sub in &cmd.subcommands {
                    out.push_str(&format!("- `{sub}`\n"));
                }
                out.push('\n');
            }
        }
    }

    pub(crate) fn diff(&self, baseline: &Self) -> ApiDriftReport {
        let mut report = ApiDriftReport::default();
        let active_cmds: HashMap<&str, &CliCommandItem> =
            self.commands.iter().map(|c| (c.name.as_str(), c)).collect();
        let base_cmds: HashMap<&str, &CliCommandItem> = baseline
            .commands
            .iter()
            .map(|c| (c.name.as_str(), c))
            .collect();

        for (name, base_cmd) in &base_cmds {
            if let Some(active_cmd) = active_cmds.get(name) {
                report.extend(active_cmd.diff(base_cmd));
            } else {
                report.add_removal(DriftItem::new(
                    DriftSurface::Cli,
                    (*name).to_string(),
                    "CLI command removed".to_string(),
                    true,
                    None,
                    None,
                ));
            }
        }

        for (name, active_cmd) in &active_cmds {
            if !base_cmds.contains_key(name) {
                report.add_addition(DriftItem::new(
                    DriftSurface::Cli,
                    active_cmd.name.clone(),
                    "New CLI command introduced".to_string(),
                    false,
                    None,
                    None,
                ));
            }
        }

        report
    }
}
