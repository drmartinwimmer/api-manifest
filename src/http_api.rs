use crate::diff_report::{ApiDriftReport, DriftItem, DriftSurface};
use crate::manifest::ApiManifest;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A single HTTP service endpoint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct HttpEndpointItem {
    /// HTTP method (`GET`, `POST`, `PUT`, `DELETE`, `PATCH`, etc.)
    pub(crate) method: String,
    /// Route path pattern (e.g. `/api/v1/users/:id`)
    pub(crate) path: String,
    /// Handler function name
    pub(crate) handler: String,
    /// Optional request body / extractor type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_type: Option<String>,
    /// Optional response type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response_type: Option<String>,
    /// Optional doc comment on handler
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

/// Public HTTP API surface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HttpApi {
    pub(crate) endpoints: Vec<HttpEndpointItem>,
}

impl HttpApi {
    pub(crate) fn new(mut endpoints: Vec<HttpEndpointItem>) -> Self {
        endpoints.sort();
        Self { endpoints }
    }

    pub(crate) fn format_markdown(&self, out: &mut String, section_idx: usize) {
        if self.endpoints.is_empty() {
            return;
        }
        out.push_str(&format!("## {section_idx}. HTTP Endpoints\n\n"));
        let headers = ["Method", "Path", "Handler", "Description"];
        let rows: Vec<Vec<String>> = self
            .endpoints
            .iter()
            .map(|ep| {
                vec![
                    format!("`{}`", ep.method),
                    format!("`{}`", ep.path),
                    format!("`{}`", ep.handler),
                    ep.doc.as_deref().unwrap_or("-").to_string(),
                ]
            })
            .collect();
        ApiManifest::format_aligned_table(out, &headers, &rows);
    }

    pub(crate) fn diff(&self, baseline: &Self) -> ApiDriftReport {
        let mut report = ApiDriftReport::default();
        let active_endpoints: HashSet<(&str, &str)> = self
            .endpoints
            .iter()
            .map(|e| (e.method.as_str(), e.path.as_str()))
            .collect();
        let base_endpoints: HashSet<(&str, &str)> = baseline
            .endpoints
            .iter()
            .map(|e| (e.method.as_str(), e.path.as_str()))
            .collect();

        for (method, path) in &base_endpoints {
            if !active_endpoints.contains(&(*method, *path)) {
                report.add_removal(DriftItem::new(
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
                report.add_addition(DriftItem::new(
                    DriftSurface::Http,
                    format!("{method} {path}"),
                    "New HTTP route introduced".to_string(),
                    false,
                    None,
                    None,
                ));
            }
        }

        let base_map: HashMap<(&str, &str), &HttpEndpointItem> = baseline
            .endpoints
            .iter()
            .map(|e| ((e.method.as_str(), e.path.as_str()), e))
            .collect();
        for active_ep in &self.endpoints {
            if let Some(base_ep) =
                base_map.get(&(active_ep.method.as_str(), active_ep.path.as_str()))
                && base_ep.request_type != active_ep.request_type
                && base_ep.request_type.is_some()
            {
                report.add_modification(DriftItem::new(
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

        report
    }
}
