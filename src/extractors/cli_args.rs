use crate::cli_api::CliArgItem;
use quote::ToTokens;
use syn::meta::ParseNestedMeta;
use syn::{Attribute, Expr, ExprLit, Lit, Meta};

/// Command name and description extracted from `#[command(...)]` and doc attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandMetadata {
    pub(crate) name: String,
    pub(crate) about: Option<String>,
}

impl CommandMetadata {
    pub(crate) fn from_attrs(attrs: &[Attribute], default_name: &str) -> Self {
        let mut name = default_name.to_string();
        let mut about = extract_doc_comment(attrs);

        for attr in attrs {
            if attr.path().is_ident("command") {
                drop(attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("name") {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        name = value.value();
                    } else if meta.path.is_ident("about") {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        about = Some(value.value());
                    }
                    Ok(())
                }));
            }
        }

        Self { name, about }
    }
}

#[derive(Default)]
pub(crate) struct ParsedArgAttributes {
    pub(crate) short: Option<char>,
    pub(crate) long: Option<String>,
    pub(crate) value_name: Option<String>,
    pub(crate) default_value: Option<String>,
    pub(crate) is_action_count: bool,
}

impl ParsedArgAttributes {
    pub(crate) fn parse(field_name: &str, attrs: &[Attribute]) -> Self {
        let mut parsed = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("arg") {
                continue;
            }
            drop(attr.parse_nested_meta(|meta| {
                parsed.parse_single_meta(&meta, field_name)?;
                Ok(())
            }));
        }

        parsed
    }

    pub(crate) fn extract_cli_arg(field: &syn::Field, field_name: String) -> CliArgItem {
        let help = extract_doc_comment(&field.attrs);
        let attrs = Self::parse(&field_name, &field.attrs);
        let is_bool = is_boolean_type(&field.ty);
        let mut required = !is_optional_or_boolean_type(&field.ty);

        if attrs.default_value.is_some() || attrs.is_action_count {
            required = false;
        }

        let is_positional = attrs.short.is_none() && attrs.long.is_none();

        let value_name = if is_bool || attrs.is_action_count {
            None
        } else if attrs.value_name.is_some() {
            attrs.value_name
        } else {
            Some(infer_value_name(&field_name, &field.ty))
        };

        let id = attrs
            .long
            .clone()
            .or_else(|| attrs.short.map(|c| c.to_string()))
            .unwrap_or_else(|| field_name.replace('_', "-"));

        CliArgItem {
            id,
            short: attrs.short,
            long: attrs.long,
            value_name,
            required,
            default_value: attrs.default_value,
            help,
            is_positional,
        }
    }

    fn parse_single_meta(&mut self, meta: &ParseNestedMeta, field_name: &str) -> syn::Result<()> {
        if meta.path.is_ident("short") {
            self.short = parse_short_meta(meta, field_name)?;
        } else if meta.path.is_ident("long") {
            self.long = parse_long_meta(meta, field_name)?;
        } else if meta.path.is_ident("value_name") {
            self.value_name = parse_str_meta(meta)?;
        } else if meta.path.is_ident("default_value") {
            self.default_value = parse_str_meta(meta)?;
        } else if meta.path.is_ident("default_value_t") {
            self.default_value = parse_default_value_t(meta)?;
        } else if meta.path.is_ident("action") {
            self.is_action_count = parse_is_action_count(meta)?;
        } else if meta.input.peek(syn::Token![=]) {
            drop(meta.value()?.parse::<syn::Expr>());
        }
        Ok(())
    }
}

fn parse_short_meta(meta: &ParseNestedMeta, field_name: &str) -> syn::Result<Option<char>> {
    if meta.input.peek(syn::Token![=]) {
        let val = meta.value()?.parse::<syn::LitChar>()?;
        Ok(Some(val.value()))
    } else {
        Ok(field_name.chars().next())
    }
}

fn parse_long_meta(meta: &ParseNestedMeta, field_name: &str) -> syn::Result<Option<String>> {
    if meta.input.peek(syn::Token![=]) {
        let val = meta.value()?.parse::<syn::LitStr>()?;
        Ok(Some(val.value()))
    } else {
        Ok(Some(field_name.replace('_', "-")))
    }
}

fn parse_str_meta(meta: &ParseNestedMeta) -> syn::Result<Option<String>> {
    let val = meta.value()?.parse::<syn::LitStr>()?;
    Ok(Some(val.value()))
}

fn parse_default_value_t(meta: &ParseNestedMeta) -> syn::Result<Option<String>> {
    if !meta.input.peek(syn::Token![=]) {
        return Ok(Some("default".to_string()));
    }

    let expr = meta.value()?.parse::<syn::Expr>()?;
    let val_str = match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => s.value(),
        Expr::Path(ref path_expr) => path_expr
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string().to_lowercase())
            .unwrap_or_else(|| expr.to_token_stream().to_string()),
        other => other.to_token_stream().to_string().replace(' ', ""),
    };
    Ok(Some(val_str))
}

fn parse_is_action_count(meta: &ParseNestedMeta) -> syn::Result<bool> {
    if !meta.input.peek(syn::Token![=]) {
        return Ok(false);
    }
    let expr = meta.value()?.parse::<syn::Expr>()?;
    Ok(expr.to_token_stream().to_string().contains("Count"))
}

pub(crate) fn infer_value_name(field_name: &str, ty: &syn::Type) -> String {
    if let Some(ident) = extract_type_ident(ty)
        && (ident == "PathBuf" || ident == "Path")
    {
        return "PATH".to_string();
    }
    field_name.to_uppercase()
}

pub(crate) fn is_boolean_type(ty: &syn::Type) -> bool {
    let ty_str = ty.clone().into_token_stream().to_string().replace(' ', "");
    ty_str == "bool" || ty_str == "Option<bool>"
}

pub(crate) fn is_optional_or_boolean_type(ty: &syn::Type) -> bool {
    let ty_str = ty.clone().into_token_stream().to_string().replace(' ', "");
    ty_str.starts_with("Option<") || ty_str == "bool"
}

pub(crate) fn extract_type_ident(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(type_path) => {
            let last_segment = type_path.path.segments.last()?;
            if (last_segment.ident == "Option" || last_segment.ident == "Vec")
                && let syn::PathArguments::AngleBracketed(ref args) = last_segment.arguments
                && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
            {
                return extract_type_ident(inner_ty);
            }
            Some(last_segment.ident.to_string())
        }
        _ => None,
    }
}

pub(crate) fn to_kebab_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch == '_' {
            result.push('-');
        } else if ch.is_uppercase() {
            if i > 0 && !result.ends_with('-') {
                result.push('-');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

pub(crate) fn has_derive_attribute(attrs: &[Attribute], trait_name: &str) -> bool {
    for attr in attrs {
        if attr.path().is_ident("derive")
            && let Meta::List(ref list) = attr.meta
        {
            let tokens = list.tokens.to_string();
            for part in tokens.split(',') {
                if part.trim() == trait_name {
                    return true;
                }
            }
        }
    }
    false
}

pub(crate) fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let mut docs = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc")
            && let Meta::NameValue(ref nv) = attr.meta
            && let Expr::Lit(ExprLit {
                lit: Lit::Str(ref s),
                ..
            }) = nv.value
        {
            docs.push(s.value().trim().to_string());
        }
    }
    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

pub(crate) fn is_flatten_field(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("command") {
            let mut is_flatten = false;
            drop(attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("flatten") {
                    is_flatten = true;
                }
                Ok(())
            }));
            if is_flatten {
                return true;
            }
        }
    }
    false
}

pub(crate) fn is_subcommand_field(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("command") {
            let mut is_sub = false;
            drop(attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("subcommand") {
                    is_sub = true;
                }
                Ok(())
            }));
            if is_sub {
                return true;
            }
        }
    }
    false
}
