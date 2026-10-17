use crate::extractors::library_format::extract_doc_comment;
use crate::http_api::{HttpApi, HttpEndpointItem};
use crate::manifest::ManifestError;
use quote::ToTokens;
use std::fs;
use std::path::Path;
use syn::visit::{Visit, visit_expr_method_call, visit_item_fn};
use syn::{Attribute, Expr, ExprCall, ExprMethodCall, File, FnArg, ItemFn, Lit, Meta};

/// Extractor for HTTP service endpoints (Axum router patterns and route attribute macros).
#[derive(Debug, Default)]
pub(crate) struct HttpExtractor;

impl HttpExtractor {
    /// Creates a new `HttpExtractor`.
    pub(crate) fn new() -> Self {
        Self
    }

    /// Extracts HTTP endpoints from Rust files in a crate.
    pub(crate) fn extract_from_crate(
        &self,
        crate_root: &Path,
    ) -> Result<Option<HttpApi>, ManifestError> {
        let src_dir = crate_root.join("src");
        if !src_dir.exists() {
            return Ok(None);
        }

        let mut endpoints = Vec::new();
        self.scan_dir_recursive(&src_dir, &mut endpoints)?;

        if endpoints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(HttpApi::new(endpoints)))
        }
    }

    fn scan_dir_recursive(
        &self,
        dir: &Path,
        output: &mut Vec<HttpEndpointItem>,
    ) -> Result<(), ManifestError> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::path);

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir_recursive(&path, output)?;
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                let content = fs::read_to_string(&path)?;
                let file = syn::parse_file(&content)?;
                let file_api = self.extract_from_file(&file);
                output.extend(file_api.endpoints);
            }
        }
        Ok(())
    }

    /// Extracts HTTP endpoints from a parsed `syn::File`.
    pub(crate) fn extract_from_file(&self, file: &File) -> HttpApi {
        let mut visitor = RouteVisitor::default();
        visitor.visit_file(file);
        HttpApi::new(visitor.endpoints)
    }
}

struct RouteAttribute {
    method: String,
    path: String,
}

impl RouteAttribute {
    fn from_attr(attr: &Attribute) -> Option<Self> {
        let path = attr.path();
        let method_candidate = path.get_ident().map(|i| i.to_string().to_uppercase());

        if let Some(m) = &method_candidate
            && RouteVisitor::is_http_method(m)
            && let Meta::List(list) = &attr.meta
        {
            let token_str = list.tokens.to_string();
            let cleaned = token_str.trim().trim_matches('"');
            return Some(Self {
                method: m.clone(),
                path: cleaned.to_string(),
            });
        }

        // utoipa::path pattern: #[utoipa::path(get, path = "/...")]
        if path.segments.iter().any(|s| s.ident == "utoipa")
            && let Meta::List(list) = &attr.meta
        {
            let tokens = list.tokens.to_string();
            let mut detected_method = "GET".to_string();
            let mut detected_path = None;

            for part in tokens.split(',') {
                let part = part.trim();
                let upper = part.to_uppercase();
                if RouteVisitor::is_http_method(&upper) {
                    detected_method = upper;
                } else if let Some(path_val) = part.strip_prefix("path =") {
                    detected_path = Some(path_val.trim().trim_matches('"').to_string());
                }
            }

            if let Some(p) = detected_path {
                return Some(Self {
                    method: detected_method,
                    path: p,
                });
            }
        }

        None
    }
}

#[derive(Default)]
struct RouteVisitor {
    endpoints: Vec<HttpEndpointItem>,
}

impl<'ast> Visit<'ast> for RouteVisitor {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if node.method == "route" && node.args.len() >= 2 {
            let mut iter = node.args.iter();
            let path_arg = iter.next();
            let handler_arg = iter.next();

            if let (Some(path_expr), Some(handler_expr)) = (path_arg, handler_arg)
                && let Some(path_str) = Self::extract_string_literal(path_expr)
            {
                self.extract_axum_handlers(&path_str, handler_expr);
            }
        }

        visit_expr_method_call(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        for attr in &node.attrs {
            if let Some(route_attr) = RouteAttribute::from_attr(attr) {
                let handler = node.sig.ident.to_string();
                let doc = extract_doc_comment(&node.attrs);
                let request_type = Self::extract_request_payload(&node.sig);
                let response_type = Self::extract_return_type(&node.sig);

                self.endpoints.push(HttpEndpointItem::new(
                    route_attr.method,
                    route_attr.path,
                    handler,
                    request_type,
                    response_type,
                    doc,
                ));
            }
        }

        visit_item_fn(self, node);
    }
}

impl RouteVisitor {
    fn extract_axum_handlers(&mut self, path: &str, expr: &Expr) {
        match expr {
            Expr::Call(ExprCall { func, args, .. }) => {
                if let Expr::Path(path_expr) = &**func
                    && let Some(ident) = path_expr.path.get_ident()
                {
                    let method = ident.to_string().to_uppercase();
                    if Self::is_http_method(&method) {
                        let handler_name = args
                            .first()
                            .and_then(Self::extract_ident_string)
                            .unwrap_or_else(|| "anonymous".to_string());
                        self.endpoints.push(HttpEndpointItem::new(
                            method,
                            path.to_string(),
                            handler_name,
                            None,
                            None,
                            None,
                        ));
                    }
                }
            }
            Expr::MethodCall(ExprMethodCall {
                receiver,
                method,
                args,
                ..
            }) => {
                let method_str = method.to_string().to_uppercase();
                if Self::is_http_method(&method_str) {
                    let handler_name = args
                        .first()
                        .and_then(Self::extract_ident_string)
                        .unwrap_or_else(|| "anonymous".to_string());
                    self.endpoints.push(HttpEndpointItem::new(
                        method_str,
                        path.to_string(),
                        handler_name,
                        None,
                        None,
                        None,
                    ));
                }
                self.extract_axum_handlers(path, receiver);
            }
            _ => {}
        }
    }

    fn is_http_method(s: &str) -> bool {
        matches!(
            s,
            "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS"
        )
    }

    fn extract_string_literal(expr: &Expr) -> Option<String> {
        if let Expr::Lit(syn::ExprLit {
            lit: Lit::Str(s), ..
        }) = expr
        {
            Some(s.value())
        } else {
            None
        }
    }

    fn extract_ident_string(expr: &Expr) -> Option<String> {
        if let Expr::Path(path_expr) = expr {
            path_expr
                .path
                .get_ident()
                .map(std::string::ToString::to_string)
        } else {
            None
        }
    }

    fn extract_request_payload(sig: &syn::Signature) -> Option<String> {
        for input in &sig.inputs {
            if let FnArg::Typed(pat_type) = input {
                let ty_str = pat_type.ty.to_token_stream().to_string().replace(' ', "");
                if ty_str.contains("Json<") {
                    return Some(ty_str);
                }
            }
        }
        None
    }

    fn extract_return_type(sig: &syn::Signature) -> Option<String> {
        if let syn::ReturnType::Type(_, ty) = &sig.output {
            let ty_str = ty.to_token_stream().to_string().replace(' ', "");
            Some(ty_str)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn extract_http_endpoints_from_axum_router() -> Result<(), Box<dyn std::error::Error>> {
        let code = r#"
            pub fn app_router() -> Router {
                Router::new()
                    .route("/health", get(health_check))
                    .route("/users", post(create_user).get(list_users))
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = HttpExtractor::new();
        let api = extractor.extract_from_file(&file);

        expect_that!(api.endpoints.len(), eq(3));

        let health = api.endpoints.iter().find(|e| e.path == "/health");
        assert_that!(health, some(anything()));
        let h = health.ok_or("health route missing")?;
        expect_that!(h.method, eq(&"GET".to_string()));
        expect_that!(h.handler, eq(&"health_check".to_string()));

        let user_post = api
            .endpoints
            .iter()
            .find(|e| e.path == "/users" && e.method == "POST");
        assert_that!(user_post, some(anything()));
        let up = user_post.ok_or("user post route missing")?;
        expect_that!(up.handler, eq(&"create_user".to_string()));

        let user_get = api
            .endpoints
            .iter()
            .find(|e| e.path == "/users" && e.method == "GET");
        assert_that!(user_get, some(anything()));
        let ug = user_get.ok_or("user get route missing")?;
        expect_that!(ug.handler, eq(&"list_users".to_string()));

        Ok(())
    }

    #[googletest::test]
    fn extract_http_endpoints_from_attribute_macros() -> Result<(), Box<dyn std::error::Error>> {
        let code = r#"
            /// Get current status
            #[get("/status")]
            pub async fn get_status() -> String {
                "ok".to_string()
            }

            /// Create item
            #[post("/items")]
            pub async fn create_item(payload: Json<CreateItemRequest>) -> StatusCode {
                StatusCode::CREATED
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = HttpExtractor::new();
        let api = extractor.extract_from_file(&file);

        expect_that!(api.endpoints.len(), eq(2));

        let status = api.endpoints.iter().find(|e| e.path == "/status");
        assert_that!(status, some(anything()));
        let s = status.ok_or("status route missing")?;
        expect_that!(s.method, eq(&"GET".to_string()));
        expect_that!(s.doc, eq(&Some("Get current status".to_string())));

        let item = api.endpoints.iter().find(|e| e.path == "/items");
        assert_that!(item, some(anything()));
        let i = item.ok_or("items route missing")?;
        expect_that!(i.method, eq(&"POST".to_string()));
        expect_that!(
            i.request_type,
            eq(&Some("Json<CreateItemRequest>".to_string()))
        );

        Ok(())
    }
}
