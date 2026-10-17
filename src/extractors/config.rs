use crate::extractors::cli_args::{extract_doc_comment, has_derive_attribute, to_kebab_case};
use crate::manifest::ManifestError;
use crate::schema_api::{ConfigApi, ConfigOptionItem, ConfigSectionItem};
use quote::ToTokens;
use std::fs;
use std::path::Path;
use syn::visit::{Visit, visit_field, visit_item_mod, visit_item_struct};
use syn::{Attribute, Field, Fields, File, ItemMod, ItemStruct, Lit};

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

        let mut sections = Vec::new();
        self.scan_dir_for_config(&src_dir, &mut sections)?;

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
        sections: &mut Vec<ConfigSectionItem>,
    ) -> Result<(), ManifestError> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::path);

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir_for_config(&path, sections)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let content = fs::read_to_string(&path)?;
                let file = syn::parse_file(&content)?;
                self.extract_from_file(&file, sections);
            }
        }
        Ok(())
    }

    pub(crate) fn extract_from_file(&self, file: &File, sections: &mut Vec<ConfigSectionItem>) {
        let mut visitor = ConfigSectionVisitor { sections };
        visitor.visit_file(file);
    }
}

struct ConfigSectionVisitor<'a> {
    sections: &'a mut Vec<ConfigSectionItem>,
}

impl ConfigSectionVisitor<'_> {
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

    fn extract_section_from_struct(item_struct: &ItemStruct) -> Option<ConfigSectionItem> {
        if !matches!(item_struct.fields, Fields::Named(_)) {
            return None;
        }

        let file_fmt = Self::parse_config_file_path(&item_struct.attrs)
            .unwrap_or_else(|| "Cargo.toml".to_string());
        let sec_name = Self::parse_config_section_name(&item_struct.attrs).unwrap_or_else(|| {
            let kebab = to_kebab_case(&item_struct.ident.to_string());
            format!("[{kebab}]")
        });

        let doc = extract_doc_comment(&item_struct.attrs);

        let mut field_visitor = ConfigFieldVisitor::default();
        field_visitor.visit_item_struct(item_struct);

        Some(ConfigSectionItem::new(
            sec_name,
            file_fmt,
            doc,
            field_visitor.options,
        ))
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
                let val = s.value();
                return Some(if val.starts_with('[') {
                    val
                } else {
                    format!("[{val}]")
                });
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
                    return Some(if s.starts_with('[') {
                        s
                    } else {
                        format!("[{s}]")
                    });
                }
            }
        }
        None
    }
}

impl<'ast> Visit<'ast> for ConfigSectionVisitor<'_> {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if Self::is_cfg_test(&node.attrs) {
            return;
        }
        visit_item_mod(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        if Self::is_config_struct(node)
            && let Some(sec) = Self::extract_section_from_struct(node)
        {
            self.sections.push(sec);
        }
        visit_item_struct(self, node);
    }
}

#[derive(Default)]
struct FieldAttributeConfig {
    renamed_key: Option<String>,
    default_value: Option<String>,
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

#[derive(Default)]
struct ConfigFieldVisitor {
    options: Vec<ConfigOptionItem>,
}

impl ConfigFieldVisitor {
    fn format_field_type(ty: &syn::Type) -> String {
        ty.to_token_stream()
            .to_string()
            .replace(" < ", "<")
            .replace(" >", ">")
            .replace("< ", "<")
    }
}

impl<'ast> Visit<'ast> for ConfigFieldVisitor {
    fn visit_field(&mut self, field: &'ast Field) {
        if let Some(ident) = field.ident.as_ref() {
            let field_meta = FieldAttributeConfig::from_attrs(&field.attrs);
            let key = field_meta.renamed_key.unwrap_or_else(|| ident.to_string());
            let val_type = Self::format_field_type(&field.ty);
            let opt_doc = extract_doc_comment(&field.attrs);
            self.options.push(ConfigOptionItem::new(
                key,
                val_type,
                field_meta.default_value,
                opt_doc,
            ));
        }
        visit_field(self, field);
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
}
