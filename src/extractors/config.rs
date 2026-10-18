use crate::extractors::cli_args::{
    extract_doc_comment, extract_type_ident, has_derive_attribute, to_kebab_case,
};
use crate::manifest::ManifestError;
use crate::schema_api::{ConfigApi, ConfigOptionItem, ConfigSectionItem};
use quote::ToTokens;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use syn::visit::{Visit, visit_item_mod, visit_item_struct};
use syn::{Attribute, Fields, ItemMod, ItemStruct, Lit};

/// Extractor for configuration file formats (e.g. `Cargo.toml` tables) defined via `ConfigFile`.
#[derive(Debug, Default)]
pub(crate) struct ConfigExtractor;

impl ConfigExtractor {
    /// Creates a new `ConfigExtractor`.
    pub(crate) fn new() -> Self {
        Self
    }

    /// Extracts configuration format specifications from a crate root by dynamically inspecting source files.
    pub(crate) fn extract_from_crate(
        &self,
        crate_root: &Path,
    ) -> Result<Option<ConfigApi>, ManifestError> {
        let src_dir = crate_root.join("src");
        if !src_dir.exists() {
            return Ok(None);
        }

        let mut visitor = ConfigSectionVisitor::default();
        self.scan_dir_for_config(&src_dir, &mut visitor)?;
        let mut sections = visitor.into_sections();

        if sections.is_empty() {
            Ok(None)
        } else {
            sections.sort_by(|a, b| a.section.cmp(&b.section));
            sections.dedup_by(|a, b| a.section == b.section);
            Ok(Some(ConfigApi::new(sections)))
        }
    }

    fn scan_dir_for_config(
        &self,
        dir: &Path,
        visitor: &mut ConfigSectionVisitor,
    ) -> Result<(), ManifestError> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::path);

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir_for_config(&path, visitor)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let content = fs::read_to_string(&path)?;
                let file = syn::parse_file(&content)?;
                visitor.visit_file(&file);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn extract_from_file(
        &self,
        file: &syn::File,
        sections: &mut Vec<ConfigSectionItem>,
    ) {
        let mut visitor = ConfigSectionVisitor::default();
        visitor.visit_file(file);
        sections.extend(visitor.into_sections());
    }
}

#[derive(Default)]
struct ConfigSectionVisitor {
    structs: HashMap<String, ItemStruct>,
    root_structs: Vec<String>,
    visited: HashSet<String>,
    sections: Vec<ConfigSectionItem>,
}

impl ConfigSectionVisitor {
    fn is_cfg_test(attrs: &[Attribute]) -> bool {
        attrs.iter().any(|attr| {
            attr.path().is_ident("cfg") && attr.meta.to_token_stream().to_string().contains("test")
        })
    }

    fn is_config_struct(item_struct: &ItemStruct) -> bool {
        has_derive_attribute(&item_struct.attrs, "ConfigFile")
            || Self::has_config_file_attribute(&item_struct.attrs)
    }

    fn has_config_file_attribute(attrs: &[Attribute]) -> bool {
        attrs
            .iter()
            .any(|a| a.path().is_ident("config_file") || a.path().is_ident("config_section"))
    }

    fn into_sections(mut self) -> Vec<ConfigSectionItem> {
        let root_names = self.root_structs.clone();
        for root_name in root_names {
            if let Some(root_struct) = self.structs.get(&root_name).cloned() {
                let file_fmt = Self::parse_config_file_path(&root_struct.attrs)
                    .unwrap_or_else(|| "Cargo.toml".to_string());
                let explicit_sec = Self::parse_config_section_name(&root_struct.attrs);
                let doc = extract_doc_comment(&root_struct.attrs);
                self.extract_struct_sections(&root_struct, explicit_sec.as_deref(), &file_fmt, doc);
            }
        }
        self.sections
    }

    fn extract_struct_sections(
        &mut self,
        item_struct: &ItemStruct,
        section_prefix: Option<&str>,
        file_format: &str,
        section_doc: Option<String>,
    ) {
        if !matches!(item_struct.fields, Fields::Named(_)) {
            return;
        }
        let struct_name = item_struct.ident.to_string();
        if !self.visited.insert(struct_name.clone()) {
            return;
        }

        let mut leaf_options = Vec::new();
        let has_nested_sections =
            self.collect_struct_fields(item_struct, section_prefix, file_format, &mut leaf_options);

        if !leaf_options.is_empty() || !has_nested_sections {
            let sec_name = match section_prefix {
                Some(prefix) => Self::format_section_header(prefix),
                None => {
                    let kebab = to_kebab_case(&item_struct.ident.to_string());
                    format!("[{kebab}]")
                }
            };
            self.sections.push(ConfigSectionItem::new(
                sec_name,
                file_format,
                section_doc,
                leaf_options,
            ));
        }

        self.visited.remove(&struct_name);
    }

    fn collect_struct_fields(
        &mut self,
        item_struct: &ItemStruct,
        section_prefix: Option<&str>,
        file_format: &str,
        leaf_options: &mut Vec<ConfigOptionItem>,
    ) -> bool {
        let Fields::Named(ref named_fields) = item_struct.fields else {
            return false;
        };

        let mut has_nested_sections = false;

        for field in &named_fields.named {
            if Self::is_cfg_test(&field.attrs) {
                continue;
            }
            let Some(ident) = field.ident.as_ref() else {
                continue;
            };
            let field_meta = FieldAttributeConfig::from_attrs(&field.attrs);
            let key = field_meta.renamed_key.unwrap_or_else(|| ident.to_string());

            if let Some(type_ident) = extract_type_ident(&field.ty)
                && let Some(nested_struct) = self.structs.get(&type_ident).cloned()
            {
                if field_meta.flatten {
                    has_nested_sections |= self.collect_flattened_fields(
                        &nested_struct,
                        section_prefix,
                        file_format,
                        leaf_options,
                    );
                } else {
                    has_nested_sections = true;
                    let nested_prefix =
                        Self::resolve_nested_prefix(&nested_struct.attrs, section_prefix, &key);
                    let nested_doc = extract_doc_comment(&field.attrs)
                        .or_else(|| extract_doc_comment(&nested_struct.attrs));
                    self.extract_struct_sections(
                        &nested_struct,
                        Some(&nested_prefix),
                        file_format,
                        nested_doc,
                    );
                }
                continue;
            }

            let val_type = Self::format_field_type(&field.ty);
            let opt_doc = extract_doc_comment(&field.attrs);
            leaf_options.push(ConfigOptionItem::new(
                key,
                val_type,
                field_meta.default_value,
                opt_doc,
            ));
        }

        has_nested_sections
    }

    fn collect_flattened_fields(
        &mut self,
        nested_struct: &ItemStruct,
        section_prefix: Option<&str>,
        file_format: &str,
        leaf_options: &mut Vec<ConfigOptionItem>,
    ) -> bool {
        let nested_name = nested_struct.ident.to_string();
        if !self.visited.insert(nested_name.clone()) {
            return false;
        }
        let has_nested =
            self.collect_struct_fields(nested_struct, section_prefix, file_format, leaf_options);
        self.visited.remove(&nested_name);
        has_nested
    }

    fn resolve_nested_prefix(
        attrs: &[Attribute],
        section_prefix: Option<&str>,
        key: &str,
    ) -> String {
        Self::parse_config_section_name(attrs).unwrap_or_else(|| match section_prefix {
            Some(prefix) => {
                let clean = prefix.trim_start_matches('[').trim_end_matches(']');
                format!("{clean}.{key}")
            }
            None => key.to_string(),
        })
    }

    fn format_section_header(section: &str) -> String {
        if section.starts_with('[') {
            section.to_string()
        } else {
            format!("[{section}]")
        }
    }

    fn format_field_type(ty: &syn::Type) -> String {
        ty.to_token_stream()
            .to_string()
            .replace(" < ", "<")
            .replace(" >", ">")
            .replace("< ", "<")
    }

    fn parse_config_file_path(attrs: &[Attribute]) -> Option<String> {
        for attr in attrs {
            if !attr.path().is_ident("config_file") {
                continue;
            }

            if let Ok(Lit::Str(s)) = attr.parse_args::<Lit>() {
                return Some(s.value());
            }

            let mut path_str = None;
            drop(attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("path") || meta.path.is_ident("file") {
                    let val: syn::LitStr = meta.value()?.parse()?;
                    path_str = Some(val.value());
                } else if meta.input.peek(syn::Token![=]) {
                    drop(meta.value()?.parse::<syn::Expr>());
                }
                Ok(())
            }));

            if path_str.is_some() {
                return path_str;
            }
        }
        None
    }

    fn parse_config_section_name(attrs: &[Attribute]) -> Option<String> {
        for attr in attrs {
            if attr.path().is_ident("config_section")
                && let Ok(Lit::Str(s)) = attr.parse_args::<Lit>()
            {
                return Some(Self::format_section_header(&s.value()));
            }

            if attr.path().is_ident("config_file") {
                let mut sec = None;
                drop(attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("section") {
                        let val: syn::LitStr = meta.value()?.parse()?;
                        sec = Some(val.value());
                    } else if meta.input.peek(syn::Token![=]) {
                        drop(meta.value()?.parse::<syn::Expr>());
                    }
                    Ok(())
                }));
                if let Some(s) = sec {
                    return Some(Self::format_section_header(&s));
                }
            }
        }
        None
    }
}

impl<'ast> Visit<'ast> for ConfigSectionVisitor {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if Self::is_cfg_test(&node.attrs) {
            return;
        }
        visit_item_mod(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        if Self::is_cfg_test(&node.attrs) {
            return;
        }
        let name = node.ident.to_string();
        if Self::is_config_struct(node) && !self.root_structs.contains(&name) {
            self.root_structs.push(name.clone());
        }
        self.structs.insert(name, node.clone());
        visit_item_struct(self, node);
    }
}

#[derive(Default)]
struct FieldAttributeConfig {
    renamed_key: Option<String>,
    default_value: Option<String>,
    flatten: bool,
}

impl FieldAttributeConfig {
    fn from_attrs(attrs: &[Attribute]) -> Self {
        let mut meta_out = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("serde") && !attr.path().is_ident("config") {
                continue;
            }
            drop(attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    let val: syn::LitStr = meta.value()?.parse()?;
                    meta_out.renamed_key = Some(val.value());
                } else if meta.path.is_ident("flatten") {
                    meta_out.flatten = true;
                } else if meta.path.is_ident("default") {
                    if meta.input.peek(syn::Token![=]) {
                        let val: syn::LitStr = meta.value()?.parse()?;
                        meta_out.default_value = Some(val.value());
                    } else {
                        meta_out.default_value = Some("default".to_string());
                    }
                } else if meta.input.peek(syn::Token![=]) {
                    drop(meta.value()?.parse::<syn::Expr>());
                }
                Ok(())
            }));
        }

        meta_out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn extract_from_file_extracts_custom_config_file_derive()
    -> Result<(), Box<dyn std::error::Error>> {
        let code = r#"
            /// Custom application settings
            #[derive(ConfigFile)]
            #[config_file("app.toml")]
            #[config_section("settings")]
            pub struct AppConfig {
                /// Request timeout in milliseconds
                #[config(default = "5000")]
                pub timeout_ms: u64,
                pub enabled: Option<bool>,
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = ConfigExtractor::new();
        let mut sections = Vec::new();
        extractor.extract_from_file(&file, &mut sections);

        expect_that!(sections.len(), eq(1));
        let sec = sections.first().ok_or("custom section missing")?;
        expect_that!(sec.section, eq("[settings]"));
        expect_that!(sec.file_format, eq("app.toml"));
        expect_that!(sec.options.len(), eq(2));
        expect_that!(
            sec.doc,
            eq(&Some("Custom application settings".to_string()))
        );

        let first_opt = sec.options.first().ok_or("missing first option")?;
        expect_that!(first_opt.key, eq("enabled"));
        expect_that!(first_opt.value_type, eq("Option<bool>"));

        let second_opt = sec.options.get(1).ok_or("missing second option")?;
        expect_that!(second_opt.key, eq("timeout_ms"));
        expect_that!(second_opt.value_type, eq("u64"));
        expect_that!(second_opt.default_value, eq(&Some("5000".to_string())));
        expect_that!(
            second_opt.doc,
            eq(&Some("Request timeout in milliseconds".to_string()))
        );

        Ok(())
    }

    #[googletest::test]
    fn extract_from_file_infers_default_section_and_cargo_toml()
    -> Result<(), Box<dyn std::error::Error>> {
        let code = r#"
            #[derive(ConfigFile)]
            pub struct WorkspaceRulesConfig {
                #[serde(rename = "max-lines", default)]
                pub max_lines: usize,
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = ConfigExtractor::new();
        let mut sections = Vec::new();
        extractor.extract_from_file(&file, &mut sections);

        expect_that!(sections.len(), eq(1));
        let sec = sections.first().ok_or("section missing")?;
        expect_that!(sec.section, eq("[workspace-rules-config]"));
        expect_that!(sec.file_format, eq("Cargo.toml"));
        expect_that!(sec.options.len(), eq(1));
        let opt = sec.options.first().ok_or("option missing")?;
        expect_that!(opt.key, eq("max-lines"));
        expect_that!(opt.default_value, eq(&Some("default".to_string())));

        Ok(())
    }

    #[googletest::test]
    fn extract_from_file_resolves_nested_section_structs() -> Result<(), Box<dyn std::error::Error>>
    {
        let code = r#"
            #[derive(ConfigFile)]
            #[config_file("Cargo.toml")]
            struct CargoManifest {
                package: Option<CargoPackageMetadata>,
            }

            /// Package metadata extracted from Cargo.toml.
            struct CargoPackageMetadata {
                /// Package name
                name: String,
                /// Package version
                version: Option<String>,
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = ConfigExtractor::new();
        let mut sections = Vec::new();
        extractor.extract_from_file(&file, &mut sections);

        expect_that!(sections.len(), eq(1));
        let sec = sections.first().ok_or("nested section missing")?;
        expect_that!(sec.section, eq("[package]"));
        expect_that!(sec.file_format, eq("Cargo.toml"));
        expect_that!(
            sec.doc,
            eq(&Some(
                "Package metadata extracted from Cargo.toml.".to_string()
            ))
        );
        expect_that!(sec.options.len(), eq(2));

        let first_opt = sec.options.first().ok_or("missing name option")?;
        expect_that!(first_opt.key, eq("name"));
        expect_that!(first_opt.value_type, eq("String"));

        let second_opt = sec.options.get(1).ok_or("missing version option")?;
        expect_that!(second_opt.key, eq("version"));
        expect_that!(second_opt.value_type, eq("Option<String>"));

        Ok(())
    }
}
