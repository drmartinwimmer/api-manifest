use quote::ToTokens;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, ExprLit, Fields, FnArg, ItemConst, ItemEnum, ItemStatic, ItemStruct,
    ItemTrait, ItemType, Lit, Meta, ReturnType, TraitItem, UseTree, Variant, Visibility,
};

pub(crate) fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
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

pub(crate) fn format_struct(item: &ItemStruct) -> String {
    let mut s = format!("pub struct {}", item.ident);
    if !item.generics.params.is_empty() {
        s.push_str(&normalize_tokens(&item.generics));
    }

    match &item.fields {
        Fields::Named(named) => {
            let mut pub_fields = Vec::new();
            for f in &named.named {
                if is_public(&f.vis) {
                    let f_name = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
                    let f_ty = normalize_tokens(&f.ty);
                    pub_fields.push(format!("pub {f_name}: {f_ty}"));
                }
            }
            if pub_fields.is_empty() {
                s.push_str(" { /* no public fields */ }");
            } else {
                s.push_str(&format!(" {{ {} }}", pub_fields.join(", ")));
            }
        }
        Fields::Unnamed(unnamed) => {
            let mut pub_fields = Vec::new();
            for f in &unnamed.unnamed {
                if is_public(&f.vis) {
                    pub_fields.push(format!("pub {}", normalize_tokens(&f.ty)));
                } else {
                    pub_fields.push("/* private */".to_string());
                }
            }
            s.push_str(&format!("({});", pub_fields.join(", ")));
        }
        Fields::Unit => {
            s.push(';');
        }
    }
    s
}

#[derive(Default)]
struct EnumVariantFormatter {
    variants: Vec<String>,
}

impl<'ast> Visit<'ast> for EnumVariantFormatter {
    fn visit_variant(&mut self, v: &'ast Variant) {
        let v_name = v.ident.to_string();
        let formatted = match &v.fields {
            Fields::Named(named) => {
                let fields = named
                    .named
                    .iter()
                    .map(|f| {
                        let f_name = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
                        let f_ty = normalize_tokens(&f.ty);
                        format!("{f_name}: {f_ty}")
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{v_name} {{ {fields} }}")
            }
            Fields::Unnamed(unnamed) => {
                let fields = unnamed
                    .unnamed
                    .iter()
                    .map(|f| normalize_tokens(&f.ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{v_name}({fields})")
            }
            Fields::Unit => v_name,
        };
        self.variants.push(formatted);
        visit::visit_variant(self, v);
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
    let mut items = Vec::new();
    for trait_item in &item.items {
        match trait_item {
            TraitItem::Fn(fn_item) => {
                let mut method_sig = format_fn_sig(&fn_item.sig, None);
                if method_sig.starts_with("pub fn ") {
                    method_sig = method_sig.replacen("pub fn ", "fn ", 1);
                }
                items.push(format!("{method_sig};"));
            }
            TraitItem::Type(ty_item) => {
                items.push(format!("type {};", ty_item.ident));
            }
            TraitItem::Const(c_item) => {
                items.push(format!(
                    "const {}: {};",
                    c_item.ident,
                    normalize_tokens(&c_item.ty)
                ));
            }
            _ => {}
        }
    }
    s.push_str(&items.join(" "));
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
