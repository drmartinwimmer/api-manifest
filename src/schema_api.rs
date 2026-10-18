use crate::diff_report::{ApiDriftReport, DriftItem, DriftSurface};
use crate::manifest::ApiManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single configuration option or key within a configuration section.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct ConfigOptionItem {
    /// Option key name (e.g. `max_lines_per_file`)
    pub(crate) key: String,
    /// Value type (e.g. `usize`, `bool`, `String`, `Vec<String>`)
    pub(crate) value_type: String,
    /// Default value representation if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_value: Option<String>,
    /// Description of what the option controls
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

/// A configuration file section / table.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct ConfigSectionItem {
    /// Table or section header (e.g. `[workspace.metadata.purist]`)
    pub(crate) section: String,
    /// File format / location (e.g. `Cargo.toml`)
    pub(crate) file_format: String,
    /// Optional section description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) doc: Option<String>,
    /// Options belonging to this section
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

    pub(crate) fn diff_options(&self, base_sec: &Self, section_name: &str) -> ApiDriftReport {
        let mut report = ApiDriftReport::default();
        let active_opts: HashMap<&str, &ConfigOptionItem> =
            self.options.iter().map(|o| (o.key.as_str(), o)).collect();
        let base_opts: HashMap<&str, &ConfigOptionItem> = base_sec
            .options
            .iter()
            .map(|o| (o.key.as_str(), o))
            .collect();

        for (key, base_opt) in &base_opts {
            let opt_name = format!("{section_name}::{key}");
            if let Some(active_opt) = active_opts.get(key) {
                if base_opt.value_type != active_opt.value_type {
                    report.add_modification(DriftItem::new(
                        DriftSurface::Config,
                        opt_name,
                        format!(
                            "Configuration option type changed from '{}' to '{}'",
                            base_opt.value_type, active_opt.value_type
                        ),
                        true,
                        Some(base_opt.value_type.clone()),
                        Some(active_opt.value_type.clone()),
                    ));
                } else if base_opt.default_value != active_opt.default_value {
                    report.add_modification(DriftItem::new(
                        DriftSurface::Config,
                        opt_name,
                        format!(
                            "Configuration option default value changed from '{:?}' to '{:?}'",
                            base_opt.default_value, active_opt.default_value
                        ),
                        false,
                        base_opt.default_value.clone(),
                        active_opt.default_value.clone(),
                    ));
                }
            } else {
                report.add_removal(DriftItem::new(
                    DriftSurface::Config,
                    opt_name,
                    format!("Configuration option '{key}' removed from {section_name}"),
                    true,
                    Some(format!("{}: {}", base_opt.key, base_opt.value_type)),
                    None,
                ));
            }
        }

        for (key, active_opt) in &active_opts {
            if !base_opts.contains_key(key) {
                let opt_name = format!("{section_name}::{key}");
                report.add_addition(DriftItem::new(
                    DriftSurface::Config,
                    opt_name,
                    format!("Configuration option '{key}' added to {section_name}"),
                    false,
                    None,
                    Some(format!("{}: {}", active_opt.key, active_opt.value_type)),
                ));
            }
        }

        report
    }
}

/// Public configuration format surface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConfigApi {
    pub(crate) sections: Vec<ConfigSectionItem>,
}

impl ConfigApi {
    pub(crate) fn new(mut sections: Vec<ConfigSectionItem>) -> Self {
        sections.sort();
        Self { sections }
    }

    pub(crate) fn format_markdown(&self, out: &mut String, section_idx: usize) {
        if self.sections.is_empty() {
            return;
        }
        out.push_str(&format!("## {section_idx}. Configuration Formats\n\n"));

        for sec in &self.sections {
            out.push_str(&format!(
                "### `{}` (_{}_)\n\n",
                sec.section, sec.file_format
            ));
            if let Some(ref desc) = sec.doc {
                out.push_str(&format!("{desc}\n\n"));
            }

            if !sec.options.is_empty() {
                let headers = ["Key", "Value Type", "Default", "Description"];
                let rows: Vec<Vec<String>> = sec
                    .options
                    .iter()
                    .map(|opt| {
                        let def_col = opt.default_value.as_deref().unwrap_or("-");
                        let desc_col = opt.doc.as_deref().unwrap_or("-").to_string();
                        let safe_val_type = opt.value_type.replace('|', "\\|");
                        vec![
                            format!("`{}`", opt.key),
                            format!("`{safe_val_type}`"),
                            format!("`{def_col}`"),
                            desc_col,
                        ]
                    })
                    .collect();
                ApiManifest::format_aligned_table(out, &headers, &rows);
            }
        }
    }

    pub(crate) fn diff(&self, baseline: &Self) -> ApiDriftReport {
        let mut report = ApiDriftReport::default();
        let active_sec_map: HashMap<&str, &ConfigSectionItem> = self
            .sections
            .iter()
            .map(|s| (s.section.as_str(), s))
            .collect();
        let base_sec_map: HashMap<&str, &ConfigSectionItem> = baseline
            .sections
            .iter()
            .map(|s| (s.section.as_str(), s))
            .collect();

        for (sec_name, base_sec) in &base_sec_map {
            if let Some(active_sec) = active_sec_map.get(sec_name) {
                report.extend(active_sec.diff_options(base_sec, sec_name));
            } else {
                report.add_removal(DriftItem::new(
                    DriftSurface::Config,
                    (*sec_name).to_string(),
                    format!("Configuration section '{sec_name}' removed"),
                    true,
                    Some(format!("section {sec_name}")),
                    None,
                ));
            }
        }

        for sec_name in active_sec_map.keys() {
            if !base_sec_map.contains_key(sec_name) {
                report.add_addition(DriftItem::new(
                    DriftSurface::Config,
                    (*sec_name).to_string(),
                    format!("Configuration section '{sec_name}' added"),
                    false,
                    None,
                    Some(format!("section {sec_name}")),
                ));
            }
        }

        report
    }
}
