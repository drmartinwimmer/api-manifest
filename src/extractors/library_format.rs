use quote::ToTokens;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, ExprLit, Field, Fields, FnArg, ItemConst, ItemEnum, ItemStatic, ItemStruct,
    ItemTrait, ItemType, Lit, Meta, ReturnType, TraitItemConst, TraitItemFn, TraitItemType,
    UseTree, Variant, Visibility,
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

#[derive(Default)]
struct StructFieldFormatter {
    fields: Vec<String>,
}

impl<'ast> Visit<'ast> for StructFieldFormatter {
    fn visit_field(&mut self, f: &'ast Field) {
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
