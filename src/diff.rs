use crate::diff_report::{ApiDriftReport, DriftItem, DriftSurface};
use crate::model::{
    ApiManifest, CliApi, CliArgItem, CliCommandItem, ConfigApi, ConfigOptionItem,
    ConfigSectionItem, HttpApi, HttpEndpointItem, LibraryApi, LibraryItem,
};
use std::collections::{HashMap, HashSet};

/// Diffs the active API manifest against the checked-in baseline manifest.
pub fn diff_manifests(active: &ApiManifest, baseline: &ApiManifest) -> ApiDriftReport {
    let mut additions = Vec::new();
    let mut removals = Vec::new();
    let mut modifications = Vec::new();

    diff_library(
        &mut additions,
        &mut removals,
        &mut modifications,
        active,
        baseline,
    );
    diff_cli(
        &mut additions,
        &mut removals,
        &mut modifications,
        active,
        baseline,
    );
    diff_http(
        &mut additions,
        &mut removals,
        &mut modifications,
        active,
        baseline,
    );
    diff_config(
        &mut additions,
        &mut removals,
        &mut modifications,
        active,
        baseline,
    );

    additions.sort();
    removals.sort();
    modifications.sort();

    ApiDriftReport {
        additions,
        removals,
        modifications,
    }
}

fn diff_library(
    additions: &mut Vec<DriftItem>,
    removals: &mut Vec<DriftItem>,
    modifications: &mut Vec<DriftItem>,
    active: &ApiManifest,
    baseline: &ApiManifest,
) {
    let empty_lib = LibraryApi::default();
    let active_lib = active.library.as_ref().unwrap_or(&empty_lib);
    let base_lib = baseline.library.as_ref().unwrap_or(&empty_lib);

    let active_map: HashMap<&str, &LibraryItem> = active_lib
        .items
        .iter()
        .map(|i| (i.path.as_str(), i))
        .collect();
    let base_map: HashMap<&str, &LibraryItem> = base_lib
        .items
        .iter()
        .map(|i| (i.path.as_str(), i))
        .collect();

    for (path, base_item) in &base_map {
        if let Some(active_item) = active_map.get(path) {
            if base_item.signature != active_item.signature {
                modifications.push(DriftItem::new(
                    DriftSurface::Library,
                    base_item.name.clone(),
                    format!(
                        "Signature changed from '{}' to '{}'",
                        base_item.signature, active_item.signature
                    ),
                    true,
                    Some(base_item.signature.clone()),
                    Some(active_item.signature.clone()),
                ));
            }
        } else {
            removals.push(DriftItem::new(
                DriftSurface::Library,
                base_item.name.clone(),
                format!("Public item '{}' removed", base_item.signature),
                true,
                Some(base_item.signature.clone()),
                None,
            ));
        }
    }

    for (path, active_item) in &active_map {
        if !base_map.contains_key(path) {
            additions.push(DriftItem::new(
                DriftSurface::Library,
                active_item.name.clone(),
                format!("New public item '{}'", active_item.signature),
                false,
                None,
                Some(active_item.signature.clone()),
            ));
        }
    }
}

fn diff_cli(
    additions: &mut Vec<DriftItem>,
    removals: &mut Vec<DriftItem>,
    modifications: &mut Vec<DriftItem>,
    active: &ApiManifest,
    baseline: &ApiManifest,
) {
    let empty_cli = CliApi::default();
    let active_cli = active.cli.as_ref().unwrap_or(&empty_cli);
    let base_cli = baseline.cli.as_ref().unwrap_or(&empty_cli);

    let active_cmds: HashMap<&str, &CliCommandItem> = active_cli
        .commands
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let base_cmds: HashMap<&str, &CliCommandItem> = base_cli
        .commands
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();

    for (name, base_cmd) in &base_cmds {
        if let Some(active_cmd) = active_cmds.get(name) {
            diff_cli_command(additions, removals, modifications, active_cmd, base_cmd);
        } else {
            removals.push(DriftItem::new(
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
            additions.push(DriftItem::new(
                DriftSurface::Cli,
                active_cmd.name.clone(),
                "New CLI command introduced".to_string(),
                false,
                None,
                None,
            ));
        }
    }
}

fn diff_cli_command(
    additions: &mut Vec<DriftItem>,
    removals: &mut Vec<DriftItem>,
    modifications: &mut Vec<DriftItem>,
    active_cmd: &CliCommandItem,
    base_cmd: &CliCommandItem,
) {
    let active_args: HashMap<&str, &CliArgItem> =
        active_cmd.args.iter().map(|a| (a.id.as_str(), a)).collect();
    let base_args: HashMap<&str, &CliArgItem> =
        base_cmd.args.iter().map(|a| (a.id.as_str(), a)).collect();

    for (id, base_arg) in &base_args {
        if let Some(active_arg) = active_args.get(id) {
            if !base_arg.required && active_arg.required && active_arg.default_value.is_none() {
                modifications.push(DriftItem::new(
                    DriftSurface::Cli,
                    format!("{} --{}", base_cmd.name, id),
                    "Previously optional argument is now required without a default".to_string(),
                    true,
                    None,
                    None,
                ));
            }
        } else {
            removals.push(DriftItem::new(
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
                modifications.push(DriftItem::new(
                    DriftSurface::Cli,
                    format!("{} --{}", base_cmd.name, id),
                    "New required CLI argument without a default is a breaking change".to_string(),
                    true,
                    None,
                    None,
                ));
            } else {
                additions.push(DriftItem::new(
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
}

fn diff_http(
    additions: &mut Vec<DriftItem>,
    removals: &mut Vec<DriftItem>,
    modifications: &mut Vec<DriftItem>,
    active: &ApiManifest,
    baseline: &ApiManifest,
) {
    let empty_http = HttpApi::default();
    let active_http = active.http.as_ref().unwrap_or(&empty_http);
    let base_http = baseline.http.as_ref().unwrap_or(&empty_http);

    let active_endpoints: HashSet<(&str, &str)> = active_http
        .endpoints
        .iter()
        .map(|e| (e.method.as_str(), e.path.as_str()))
        .collect();
    let base_endpoints: HashSet<(&str, &str)> = base_http
        .endpoints
        .iter()
        .map(|e| (e.method.as_str(), e.path.as_str()))
        .collect();

    for (method, path) in &base_endpoints {
        if !active_endpoints.contains(&(*method, *path)) {
            removals.push(DriftItem::new(
                DriftSurface::Http,
                format!("{method} {path}"),
                "HTTP route removed".to_string(),
                true,
                None,
                None,
            ));
        }
    }

    for (method, path) in &active_endpoints {
        if !base_endpoints.contains(&(*method, *path)) {
            additions.push(DriftItem::new(
                DriftSurface::Http,
                format!("{method} {path}"),
                "New HTTP route introduced".to_string(),
                false,
                None,
                None,
            ));
        }
    }

    let base_map: HashMap<(&str, &str), &HttpEndpointItem> = base_http
        .endpoints
        .iter()
        .map(|e| ((e.method.as_str(), e.path.as_str()), e))
        .collect();
    for active_ep in &active_http.endpoints {
        if let Some(base_ep) = base_map.get(&(active_ep.method.as_str(), active_ep.path.as_str()))
            && base_ep.request_type != active_ep.request_type
            && base_ep.request_type.is_some()
        {
            modifications.push(DriftItem::new(
                DriftSurface::Http,
                format!("{} {}", active_ep.method, active_ep.path),
                format!(
                    "Request payload changed from {:?} to {:?}",
                    base_ep.request_type, active_ep.request_type
                ),
                true,
                base_ep.request_type.clone(),
                active_ep.request_type.clone(),
            ));
        }
    }
}

fn diff_config(
    additions: &mut Vec<DriftItem>,
    removals: &mut Vec<DriftItem>,
    modifications: &mut Vec<DriftItem>,
    active: &ApiManifest,
    baseline: &ApiManifest,
) {
    let empty_cfg = ConfigApi::default();
    let active_cfg = active.config.as_ref().unwrap_or(&empty_cfg);
    let base_cfg = baseline.config.as_ref().unwrap_or(&empty_cfg);

    let active_sec_map: HashMap<&str, &ConfigSectionItem> = active_cfg
        .sections
        .iter()
        .map(|s| (s.section.as_str(), s))
        .collect();
    let base_sec_map: HashMap<&str, &ConfigSectionItem> = base_cfg
        .sections
        .iter()
        .map(|s| (s.section.as_str(), s))
        .collect();

    for (sec_name, base_sec) in &base_sec_map {
        if let Some(active_sec) = active_sec_map.get(sec_name) {
            diff_config_options(
                additions,
                removals,
                modifications,
                sec_name,
                active_sec,
                base_sec,
            );
        } else {
            removals.push(DriftItem::new(
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
            additions.push(DriftItem::new(
                DriftSurface::Config,
                (*sec_name).to_string(),
                format!("Configuration section '{sec_name}' added"),
                false,
                None,
                Some(format!("section {sec_name}")),
            ));
        }
    }
}

fn diff_config_options(
    additions: &mut Vec<DriftItem>,
    removals: &mut Vec<DriftItem>,
    modifications: &mut Vec<DriftItem>,
    section_name: &str,
    active_sec: &ConfigSectionItem,
    base_sec: &ConfigSectionItem,
) {
    let active_opts: HashMap<&str, &ConfigOptionItem> = active_sec
        .options
        .iter()
        .map(|o| (o.key.as_str(), o))
        .collect();
    let base_opts: HashMap<&str, &ConfigOptionItem> = base_sec
        .options
        .iter()
        .map(|o| (o.key.as_str(), o))
        .collect();

    for (key, base_opt) in &base_opts {
        let opt_name = format!("{section_name}::{key}");
        if let Some(active_opt) = active_opts.get(key) {
            if base_opt.value_type != active_opt.value_type {
                modifications.push(DriftItem::new(
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
                modifications.push(DriftItem::new(
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
            removals.push(DriftItem::new(
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
            additions.push(DriftItem::new(
                DriftSurface::Config,
                opt_name,
                format!("Configuration option '{key}' added to {section_name}"),
                false,
                None,
                Some(format!("{}: {}", active_opt.key, active_opt.value_type)),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Severity;
    use crate::model::{ItemKind, LibraryApi, LibraryItem};
    use googletest::prelude::*;

    #[googletest::test]
    fn diff_identical_manifests_is_clean() {
        let manifest = ApiManifest::new("demo", Some("1.0.0".to_string()));
        let report = diff_manifests(&manifest, &manifest);
        expect_that!(report.is_clean(), is_true());
        expect_that!(report.has_breaking_changes(), is_false());
        expect_that!(report.total_drift_count(), eq(0));
    }

    #[googletest::test]
    fn diff_detects_library_item_removal_as_breaking() -> Result<(), Box<dyn std::error::Error>> {
        let old_item = LibraryItem::new(
            ItemKind::Function,
            "old_fn",
            "demo::old_fn",
            "pub fn old_fn()",
            None,
        );
        let baseline = ApiManifest::new("demo", Some("1.0.0".to_string()))
            .with_library(LibraryApi::new(vec![old_item]));
        let active =
            ApiManifest::new("demo", Some("1.0.0".to_string())).with_library(LibraryApi::default());

        let report = diff_manifests(&active, &baseline);
        expect_that!(report.is_clean(), is_false());
        expect_that!(report.has_breaking_changes(), is_true());
        expect_that!(report.removals.len(), eq(1));

        let removal = report.removals.first().ok_or("removal missing")?;
        expect_that!(removal.name, eq("old_fn"));
        expect_that!(removal.is_breaking, is_true());

        let diags = report.to_diagnostics("API.md");
        expect_that!(diags.len(), eq(1));
        let diag = diags.first().ok_or("diag missing")?;
        expect_that!(diag.severity, eq(Severity::Error));
        expect_that!(diag.rule, eq("api::drift::removal"));

        Ok(())
    }

    #[googletest::test]
    fn diff_detects_library_item_addition_as_non_breaking() -> Result<(), Box<dyn std::error::Error>>
    {
        let new_item = LibraryItem::new(
            ItemKind::Function,
            "new_fn",
            "demo::new_fn",
            "pub fn new_fn()",
            None,
        );
        let baseline =
            ApiManifest::new("demo", Some("1.0.0".to_string())).with_library(LibraryApi::default());
        let active = ApiManifest::new("demo", Some("1.0.0".to_string()))
            .with_library(LibraryApi::new(vec![new_item]));

        let report = diff_manifests(&active, &baseline);
        expect_that!(report.is_clean(), is_false());
        expect_that!(report.has_breaking_changes(), is_false());
        expect_that!(report.additions.len(), eq(1));

        let addition = report.additions.first().ok_or("addition missing")?;
        expect_that!(addition.name, eq("new_fn"));
        expect_that!(addition.is_breaking, is_false());

        let diags = report.to_diagnostics("API.md");
        expect_that!(diags.len(), eq(1));
        let diag = diags.first().ok_or("diag missing")?;
        expect_that!(diag.severity, eq(Severity::Warning));
        expect_that!(diag.rule, eq("api::drift::addition"));

        Ok(())
    }

    #[googletest::test]
    fn diff_detects_signature_modification_as_breaking() -> Result<(), Box<dyn std::error::Error>> {
        let base_item = LibraryItem::new(
            ItemKind::Function,
            "compute",
            "demo::compute",
            "pub fn compute(x: i32) -> i32",
            None,
        );
        let active_item = LibraryItem::new(
            ItemKind::Function,
            "compute",
            "demo::compute",
            "pub fn compute(x: i32, y: i32) -> i32",
            None,
        );

        let baseline = ApiManifest::new("demo", Some("1.0.0".to_string()))
            .with_library(LibraryApi::new(vec![base_item]));
        let active = ApiManifest::new("demo", Some("1.0.0".to_string()))
            .with_library(LibraryApi::new(vec![active_item]));

        let report = diff_manifests(&active, &baseline);
        expect_that!(report.is_clean(), is_false());
        expect_that!(report.has_breaking_changes(), is_true());
        expect_that!(report.modifications.len(), eq(1));

        let mod_item = report.modifications.first().ok_or("mod missing")?;
        expect_that!(mod_item.is_breaking, is_true());
        expect_that!(mod_item.name, eq("compute"));

        Ok(())
    }
}
