use crate::extractors::cli_args::is_cfg_test;
use crate::library_api::{ItemKind, LibraryItem};
use quote::ToTokens;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, ExprLit, Field, Fields, FnArg, Ident, ImplItemFn, ItemConst, ItemEnum, ItemFn,
    ItemMacro, ItemStatic, ItemStruct, ItemTrait, ItemType, Lit, Meta, ReturnType, TraitItemConst,
    TraitItemFn, TraitItemType, UseTree, Variant, Visibility,
};

pub(crate) fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

pub(crate) fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

pub(crate) fn format_macro_rules(ident: &Ident) -> String {
    format!("macro_rules! {ident}")
}

pub(crate) fn extract_exported_macro(
    item_macro: &ItemMacro,
    module_path: &str,
) -> Option<LibraryItem> {
    if is_cfg_test(&item_macro.attrs) || !has_attribute(&item_macro.attrs, "macro_export") {
        return None;
    }
    let ident = item_macro.ident.as_ref()?;
    let name = ident.to_string();
    let path = format!("{module_path}::{name}");
    let sig = format_macro_rules(ident);
    let doc = extract_doc_comment(&item_macro.attrs);
    Some(LibraryItem::new(ItemKind::Macro, name, path, sig, doc))
}

pub(crate) fn extract_impl_method(
    method: &ImplItemFn,
    self_ty: &str,
    module_path: &str,
) -> Option<LibraryItem> {
    if is_cfg_test(&method.attrs) || !is_public(&method.vis) {
        return None;
    }
    let method_name = method.sig.ident.to_string();
    let qual_name = format!("{self_ty}::{method_name}");
    let path = format!("{module_path}::{qual_name}");
    let sig = format_fn_sig(&method.sig, Some(self_ty));
    let doc = extract_doc_comment(&method.attrs);
    Some(LibraryItem::new(
        ItemKind::Method,
        qual_name,
        path,
        sig,
        doc,
    ))
}

pub(crate) fn extract_proc_macro_derive(attrs: &[Attribute]) -> Option<(String, String)> {
    for attr in attrs {
        if !attr.path().is_ident("proc_macro_derive") {
            continue;
        }
        let mut derive_name: Option<String> = None;
        let mut helper_attrs: Vec<String> = Vec::new();
        let parse_res = attr.parse_nested_meta(|meta| {
            if derive_name.is_none() {
                if let Some(ident) = meta.path.get_ident() {
                    derive_name = Some(ident.to_string());
                }
                return Ok(());
            }
            if meta.path.is_ident("attributes") {
                meta.parse_nested_meta(|nested| {
                    if let Some(ident) = nested.path.get_ident() {
                        helper_attrs.push(ident.to_string());
                    }
                    Ok(())
                })?;
            }
            Ok(())
        });
        if parse_res.is_ok()
            && let Some(name) = derive_name
        {
            let sig = if helper_attrs.is_empty() {
                format!("#[proc_macro_derive({name})]")
            } else {
                format!(
                    "#[proc_macro_derive({name}, attributes({}))]",
                    helper_attrs.join(", ")
                )
            };
            return Some((name, sig));
        }
    }
    None
}

pub(crate) fn format_item_fn_kind_name_sig(item_fn: &ItemFn) -> (ItemKind, String, String) {
    if let Some((derive_name, sig)) = extract_proc_macro_derive(&item_fn.attrs) {
        return (ItemKind::DeriveMacro, derive_name, sig);
    }
    let name = item_fn.sig.ident.to_string();
    if has_attribute(&item_fn.attrs, "proc_macro") {
        let sig = format!("#[proc_macro] pub fn {name}");
        return (ItemKind::Macro, name, sig);
    }
    if has_attribute(&item_fn.attrs, "proc_macro_attribute") {
        let sig = format!("#[proc_macro_attribute] pub fn {name}");
        return (ItemKind::Macro, name, sig);
    }
    let sig = format_fn_sig(&item_fn.sig, None);
    (ItemKind::Function, name, sig)
}

pub(crate) fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let mut docs = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc")
            && let Meta::NameValue(nv) = &attr.meta
            && let Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) = &nv.value
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

pub(crate) fn normalize_tokens(tokens: &impl ToTokens) -> String {
    let raw = tokens.to_token_stream().to_string();
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn format_fn_sig(sig: &syn::Signature, prefix: Option<&str>) -> String {
    let mut s = String::new();
    if sig.constness.is_some() {
        s.push_str("const ");
    }
    if sig.asyncness.is_some() {
        s.push_str("async ");
    }
    if matches!(sig.safety, syn::Safety::Unsafe(_)) {
        s.push_str("unsafe ");
    }
    s.push_str("pub fn ");
    if let Some(p) = prefix {
        s.push_str(p);
        s.push_str("::");
    }
    s.push_str(&sig.ident.to_string());

    if !sig.generics.params.is_empty() {
        s.push_str(&normalize_tokens(&sig.generics));
    }

    s.push('(');
    let mut first = true;
    for input in &sig.inputs {
        if !first {
            s.push_str(", ");
        }
        first = false;
        match input {
            FnArg::Receiver(r) => {
                s.push_str(&normalize_tokens(r));
            }
            FnArg::Typed(pat_type) => {
                let pat = normalize_tokens(&pat_type.pat);
                let ty = normalize_tokens(&pat_type.ty);
                s.push_str(&format!("{pat}: {ty}"));
            }
        }
    }
    s.push(')');

    if let ReturnType::Type(_, ref ty) = sig.output {
        s.push_str(" -> ");
        s.push_str(&normalize_tokens(ty));
    }

    if let Some(ref where_clause) = sig.generics.where_clause {
        s.push(' ');
        s.push_str(&normalize_tokens(where_clause));
    }

    s
}

#[derive(Default)]
struct StructFieldFormatter {
    fields: Vec<String>,
}

impl<'ast> Visit<'ast> for StructFieldFormatter {
    fn visit_field(&mut self, f: &'ast Field) {
        if is_cfg_test(&f.attrs) {
            return;
        }
        if let Some(ident) = f.ident.as_ref() {
            if is_public(&f.vis) {
                let f_ty = normalize_tokens(&f.ty);
                self.fields.push(format!("pub {ident}: {f_ty}"));
            }
        } else if is_public(&f.vis) {
            self.fields.push(format!("pub {}", normalize_tokens(&f.ty)));
        } else {
            self.fields.push("/* private */".to_string());
        }
    }
}

pub(crate) fn format_struct(item: &ItemStruct) -> String {
    let mut s = format!("pub struct {}", item.ident);
    if !item.generics.params.is_empty() {
        s.push_str(&normalize_tokens(&item.generics));
    }

    let mut formatter = StructFieldFormatter::default();
    visit::visit_item_struct(&mut formatter, item);

    match &item.fields {
        Fields::Named(_) => {
            if formatter.fields.is_empty() {
                s.push_str(" { /* no public fields */ }");
            } else {
                s.push_str(&format!(" {{ {} }}", formatter.fields.join(", ")));
            }
        }
        Fields::Unnamed(_) => {
            s.push_str(&format!("({});", formatter.fields.join(", ")));
        }
        Fields::Unit => {
            s.push(';');
        }
    }
    s
}

#[derive(Default)]
struct VariantFieldFormatter {
    fields: Vec<String>,
}

impl<'ast> Visit<'ast> for VariantFieldFormatter {
    fn visit_field(&mut self, f: &'ast Field) {
        let f_ty = normalize_tokens(&f.ty);
        if let Some(ident) = f.ident.as_ref() {
            self.fields.push(format!("{ident}: {f_ty}"));
        } else {
            self.fields.push(f_ty);
        }
    }
}

#[derive(Default)]
struct EnumVariantFormatter {
    variants: Vec<String>,
}

impl<'ast> Visit<'ast> for EnumVariantFormatter {
    fn visit_variant(&mut self, v: &'ast Variant) {
        let v_name = v.ident.to_string();
        let mut field_formatter = VariantFieldFormatter::default();
        visit::visit_variant(&mut field_formatter, v);
        let fields = field_formatter.fields.join(", ");

        let formatted = match &v.fields {
            Fields::Named(_) => format!("{v_name} {{ {fields} }}"),
            Fields::Unnamed(_) => format!("{v_name}({fields})"),
            Fields::Unit => v_name,
        };
        self.variants.push(formatted);
    }
}

pub(crate) fn format_enum(item: &ItemEnum) -> String {
    let mut s = format!("pub enum {}", item.ident);
    if !item.generics.params.is_empty() {
        s.push_str(&normalize_tokens(&item.generics));
    }
    s.push_str(" { ");
    let mut formatter = EnumVariantFormatter::default();
    formatter.visit_item_enum(item);
    s.push_str(&formatter.variants.join(", "));
    s.push_str(" }");
    s
}

#[derive(Default)]
struct TraitItemFormatter {
    items: Vec<String>,
}

impl<'ast> Visit<'ast> for TraitItemFormatter {
    fn visit_trait_item_fn(&mut self, fn_item: &'ast TraitItemFn) {
        let mut method_sig = format_fn_sig(&fn_item.sig, None);
        if method_sig.starts_with("pub fn ") {
            method_sig = method_sig.replacen("pub fn ", "fn ", 1);
        }
        self.items.push(format!("{method_sig};"));
    }

    fn visit_trait_item_type(&mut self, ty_item: &'ast TraitItemType) {
        self.items.push(format!("type {};", ty_item.ident));
    }

    fn visit_trait_item_const(&mut self, c_item: &'ast TraitItemConst) {
        self.items.push(format!(
            "const {}: {};",
            c_item.ident,
            normalize_tokens(&c_item.ty)
        ));
    }
}

pub(crate) fn format_trait(item: &ItemTrait) -> String {
    let mut s = format!("pub trait {}", item.ident);
    if !item.generics.params.is_empty() {
        s.push_str(&normalize_tokens(&item.generics));
    }
    if !item.supertraits.is_empty() {
        s.push_str(": ");
        s.push_str(&normalize_tokens(&item.supertraits));
    }
    s.push_str(" { ");
    let mut formatter = TraitItemFormatter::default();
    formatter.visit_item_trait(item);
    s.push_str(&formatter.items.join(" "));
    s.push_str(" }");
    s
}

pub(crate) fn format_type_alias(item: &ItemType) -> String {
    let mut s = format!("pub type {}", item.ident);
    if !item.generics.params.is_empty() {
        s.push_str(&normalize_tokens(&item.generics));
    }
    s.push_str(" = ");
    s.push_str(&normalize_tokens(&item.ty));
    s.push(';');
    s
}

pub(crate) fn format_const(item: &ItemConst) -> String {
    format!(
        "pub const {}: {} = {};",
        item.ident,
        normalize_tokens(&item.ty),
        normalize_tokens(&item.expr)
    )
}

pub(crate) fn format_static(item: &ItemStatic) -> String {
    let mutability = if let syn::StaticMutability::Mut(_) = item.mutability {
        "mut "
    } else {
        ""
    };
    format!(
        "pub static {}{}: {};",
        mutability,
        item.ident,
        normalize_tokens(&item.ty)
    )
}

pub(crate) fn collect_use_tree(tree: &UseTree, prefix: &str, output: &mut Vec<String>) {
    match tree {
        UseTree::Path(path) => {
            let next_prefix = if prefix.is_empty() {
                path.ident.to_string()
            } else {
                format!("{prefix}::{}", path.ident)
            };
            collect_use_tree(&path.tree, &next_prefix, output);
        }
        UseTree::Name(name) => {
            let item = if prefix.is_empty() {
                name.ident.to_string()
            } else {
                format!("{prefix}::{}", name.ident)
            };
            output.push(item);
        }
        UseTree::Rename(rename) => {
            let item = if prefix.is_empty() {
                rename.ident.to_string()
            } else {
                format!("{prefix}::{}", rename.ident)
            };
            output.push(item);
        }
        UseTree::Glob(_) => {
            let item = if prefix.is_empty() {
                "*".to_string()
            } else {
                format!("{prefix}::*")
            };
            output.push(item);
        }
        UseTree::Group(group) => {
            for tree in &group.items {
                collect_use_tree(tree, prefix, output);
            }
        }
    }
}
