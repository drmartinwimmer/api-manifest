use crate::extractors::library_format::{
    collect_use_tree, extract_doc_comment, format_const, format_enum, format_fn_sig, format_static,
    format_struct, format_trait, format_type_alias, is_public, normalize_tokens,
};
use crate::model::{ItemKind, LibraryApi, LibraryItem};
use std::fs;
use std::path::Path;
use syn::{
    File, ImplItem, Item, ItemConst, ItemEnum, ItemFn, ItemImpl, ItemMod, ItemStatic, ItemStruct,
    ItemTrait, ItemType, ItemUse,
};

/// Extractor for public library items.
#[derive(Debug, Default)]
pub(crate) struct LibraryExtractor;

impl LibraryExtractor {
    /// Creates a new `LibraryExtractor`.
    pub(crate) fn new() -> Self {
        Self
    }

    /// Extracts the public API from a Rust library crate given its root directory.
    pub fn extract_from_crate(&self, crate_root: &Path) -> std::io::Result<Option<LibraryApi>> {
        let lib_path = crate_root.join("src").join("lib.rs");
        if !lib_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&lib_path)?;
        let file = syn::parse_file(&content)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

        let crate_name =
            Self::extract_crate_name(crate_root).unwrap_or_else(|| "crate".to_string());
        let mut items = Vec::new();

        self.extract_file(&file, &crate_name, Some(crate_root), &mut items);
        self.extract_submodules(crate_root, &file, &crate_name, &mut items)?;

        Ok(Some(LibraryApi::new(items)))
    }

    /// Extracts public API items from a single parsed `syn::File`.
    #[cfg(test)]
    pub(crate) fn extract_from_file(&self, file: &File, module_path: &str) -> LibraryApi {
        let mut items = Vec::new();
        self.extract_file(file, module_path, None, &mut items);
        LibraryApi::new(items)
    }

    fn extract_file(
        &self,
        file: &File,
        module_path: &str,
        crate_root: Option<&Path>,
        items: &mut Vec<LibraryItem>,
    ) {
        for item in &file.items {
            self.extract_item(item, module_path, crate_root, items);
        }
    }

    fn extract_item(
        &self,
        item: &Item,
        module_path: &str,
        crate_root: Option<&Path>,
        items: &mut Vec<LibraryItem>,
    ) {
        match item {
            Item::Struct(item_struct) => self.extract_struct_item(item_struct, module_path, items),
            Item::Enum(item_enum) => self.extract_enum_item(item_enum, module_path, items),
            Item::Fn(item_fn) => self.extract_fn_item(item_fn, module_path, items),
            Item::Trait(item_trait) => self.extract_trait_item(item_trait, module_path, items),
            Item::Type(item_type) => self.extract_type_item(item_type, module_path, items),
            Item::Const(item_const) => self.extract_const_item(item_const, module_path, items),
            Item::Static(item_static) => self.extract_static_item(item_static, module_path, items),
            Item::Use(item_use) => self.extract_use_item(item_use, module_path, crate_root, items),
            Item::Impl(item_impl) => self.extract_impl(item_impl, module_path, items),
            Item::Mod(item_mod) => self.extract_mod_item(item_mod, module_path, crate_root, items),
            _ => {}
        }
    }

    fn extract_struct_item(
        &self,
        item_struct: &ItemStruct,
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) {
        if !is_public(&item_struct.vis) {
            return;
        }
        let name = item_struct.ident.to_string();
        let path = format!("{module_path}::{name}");
        let sig = format_struct(item_struct);
        let doc = extract_doc_comment(&item_struct.attrs);
        items.push(LibraryItem::new(ItemKind::Struct, name, path, sig, doc));
    }

    fn extract_enum_item(
        &self,
        item_enum: &ItemEnum,
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) {
        if !is_public(&item_enum.vis) {
            return;
        }
        let name = item_enum.ident.to_string();
        let path = format!("{module_path}::{name}");
        let sig = format_enum(item_enum);
        let doc = extract_doc_comment(&item_enum.attrs);
        items.push(LibraryItem::new(ItemKind::Enum, name, path, sig, doc));
    }

    fn extract_fn_item(&self, item_fn: &ItemFn, module_path: &str, items: &mut Vec<LibraryItem>) {
        if !is_public(&item_fn.vis) {
            return;
        }
        let name = item_fn.sig.ident.to_string();
        let path = format!("{module_path}::{name}");
        let sig = format_fn_sig(&item_fn.sig, None);
        let doc = extract_doc_comment(&item_fn.attrs);
        items.push(LibraryItem::new(ItemKind::Function, name, path, sig, doc));
    }

    fn extract_trait_item(
        &self,
        item_trait: &ItemTrait,
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) {
        if !is_public(&item_trait.vis) {
            return;
        }
        let name = item_trait.ident.to_string();
        let path = format!("{module_path}::{name}");
        let sig = format_trait(item_trait);
        let doc = extract_doc_comment(&item_trait.attrs);
        items.push(LibraryItem::new(ItemKind::Trait, name, path, sig, doc));
    }

    fn extract_type_item(
        &self,
        item_type: &ItemType,
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) {
        if !is_public(&item_type.vis) {
            return;
        }
        let name = item_type.ident.to_string();
        let path = format!("{module_path}::{name}");
        let sig = format_type_alias(item_type);
        let doc = extract_doc_comment(&item_type.attrs);
        items.push(LibraryItem::new(ItemKind::TypeAlias, name, path, sig, doc));
    }

    fn extract_const_item(
        &self,
        item_const: &ItemConst,
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) {
        if !is_public(&item_const.vis) {
            return;
        }
        let name = item_const.ident.to_string();
        let path = format!("{module_path}::{name}");
        let sig = format_const(item_const);
        let doc = extract_doc_comment(&item_const.attrs);
        items.push(LibraryItem::new(ItemKind::Constant, name, path, sig, doc));
    }

    fn extract_static_item(
        &self,
        item_static: &ItemStatic,
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) {
        if !is_public(&item_static.vis) {
            return;
        }
        let name = item_static.ident.to_string();
        let path = format!("{module_path}::{name}");
        let sig = format_static(item_static);
        let doc = extract_doc_comment(&item_static.attrs);
        items.push(LibraryItem::new(ItemKind::Static, name, path, sig, doc));
    }

    fn extract_use_item(
        &self,
        item_use: &ItemUse,
        module_path: &str,
        crate_root: Option<&Path>,
        items: &mut Vec<LibraryItem>,
    ) {
        if !is_public(&item_use.vis) {
            return;
        }
        let mut reexports = Vec::new();
        collect_use_tree(&item_use.tree, "", &mut reexports);
        for reexport in reexports {
            let handled = crate_root.is_some_and(|root| {
                self.resolve_internal_reexport(root, &reexport, module_path, items)
            });
            if !handled {
                let name = reexport.split("::").last().unwrap_or(&reexport).to_string();
                let path = format!("{module_path}::{name}");
                let sig = format!("pub use {reexport};");
                let doc = extract_doc_comment(&item_use.attrs);
                items.push(LibraryItem::new(ItemKind::Reexport, name, path, sig, doc));
            }
        }
    }

    fn extract_mod_item(
        &self,
        item_mod: &ItemMod,
        module_path: &str,
        crate_root: Option<&Path>,
        items: &mut Vec<LibraryItem>,
    ) {
        if !is_public(&item_mod.vis) {
            return;
        }
        let Some((_, ref mod_items)) = item_mod.content else {
            return;
        };
        let mod_name = item_mod.ident.to_string();
        let nested_path = format!("{module_path}::{mod_name}");
        for nested in mod_items {
            self.extract_item(nested, &nested_path, crate_root, items);
        }
    }

    fn extract_impl(&self, item_impl: &ItemImpl, module_path: &str, items: &mut Vec<LibraryItem>) {
        if item_impl.trait_.is_some() {
            return;
        }
        let self_ty = normalize_tokens(&item_impl.self_ty);
        for impl_item in &item_impl.items {
            if let ImplItem::Fn(method) = impl_item
                && is_public(&method.vis)
            {
                let method_name = method.sig.ident.to_string();
                let qual_name = format!("{self_ty}::{method_name}");
                let path = format!("{module_path}::{qual_name}");
                let sig = format_fn_sig(&method.sig, Some(&self_ty));
                let doc = extract_doc_comment(&method.attrs);
                items.push(LibraryItem::new(
                    ItemKind::Method,
                    qual_name,
                    path,
                    sig,
                    doc,
                ));
            }
        }
    }

    fn resolve_internal_reexport(
        &self,
        crate_root: &Path,
        reexport: &str,
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) -> bool {
        let segments: Vec<&str> = reexport.split("::").collect();
        if segments.is_empty() {
            return false;
        }

        let start_idx = if segments.first() == Some(&"crate") {
            1
        } else {
            0
        };
        if start_idx >= segments.len() {
            return false;
        }

        let Some(&target_name) = segments.last() else {
            return false;
        };
        let mod_segments = segments
            .get(start_idx..segments.len().saturating_sub(1))
            .unwrap_or(&[]);

        let mut mod_file_path = crate_root.join("src");
        for seg in mod_segments {
            mod_file_path = mod_file_path.join(seg);
        }

        let candidates = [
            mod_file_path.with_extension("rs"),
            mod_file_path.join("mod.rs"),
        ];

        let mut file_content = None;
        for c in &candidates {
            if c.is_file()
                && let Ok(content) = fs::read_to_string(c)
            {
                file_content = Some(content);
                break;
            }
        }

        let Some(content) = file_content else {
            return false;
        };
        let Ok(file) = syn::parse_file(&content) else {
            return false;
        };

        let mut matched = false;
        for item in &file.items {
            match item {
                Item::Struct(item_struct) if item_struct.ident == target_name => {
                    let name = item_struct.ident.to_string();
                    let path = format!("{module_path}::{name}");
                    let sig = format_struct(item_struct);
                    let doc = extract_doc_comment(&item_struct.attrs);
                    items.push(LibraryItem::new(ItemKind::Struct, name, path, sig, doc));
                    matched = true;
                }
                Item::Enum(item_enum) if item_enum.ident == target_name => {
                    let name = item_enum.ident.to_string();
                    let path = format!("{module_path}::{name}");
                    let sig = format_enum(item_enum);
                    let doc = extract_doc_comment(&item_enum.attrs);
                    items.push(LibraryItem::new(ItemKind::Enum, name, path, sig, doc));
                    matched = true;
                }
                Item::Trait(item_trait) if item_trait.ident == target_name => {
                    let name = item_trait.ident.to_string();
                    let path = format!("{module_path}::{name}");
                    let sig = format_trait(item_trait);
                    let doc = extract_doc_comment(&item_trait.attrs);
                    items.push(LibraryItem::new(ItemKind::Trait, name, path, sig, doc));
                    matched = true;
                }
                Item::Fn(item_fn) if item_fn.sig.ident == target_name => {
                    let name = item_fn.sig.ident.to_string();
                    let path = format!("{module_path}::{name}");
                    let sig = format_fn_sig(&item_fn.sig, None);
                    let doc = extract_doc_comment(&item_fn.attrs);
                    items.push(LibraryItem::new(ItemKind::Function, name, path, sig, doc));
                    matched = true;
                }
                Item::Impl(item_impl) => {
                    let self_ty_str = normalize_tokens(&item_impl.self_ty);
                    if self_ty_str == *target_name {
                        self.extract_impl(item_impl, module_path, items);
                    }
                }
                _ => {}
            }
        }

        matched
    }

    fn extract_submodules(
        &self,
        crate_root: &Path,
        file: &File,
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) -> std::io::Result<()> {
        let src_dir = crate_root.join("src");
        for item in &file.items {
            if let Item::Mod(item_mod) = item
                && is_public(&item_mod.vis)
                && item_mod.content.is_none()
            {
                let mod_name = item_mod.ident.to_string();
                let candidate_file = src_dir.join(format!("{mod_name}.rs"));
                let candidate_mod_dir = src_dir.join(&mod_name).join("mod.rs");

                let target_path = if candidate_file.exists() {
                    Some(candidate_file)
                } else if candidate_mod_dir.exists() {
                    Some(candidate_mod_dir)
                } else {
                    None
                };

                if let Some(path) = target_path {
                    let content = fs::read_to_string(&path)?;
                    if let Ok(sub_file) = syn::parse_file(&content) {
                        let nested_path = format!("{module_path}::{mod_name}");
                        self.extract_file(&sub_file, &nested_path, Some(crate_root), items);
                    }
                }
            }
        }
        Ok(())
    }

    fn extract_crate_name(crate_root: &Path) -> Option<String> {
        let cargo_toml = crate_root.join("Cargo.toml");
        if let Ok(content) = fs::read_to_string(cargo_toml) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("name =") {
                    let name = rest.trim().trim_matches('"').trim_matches('\'');
                    return Some(name.replace('-', "_"));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn extract_library_items_from_file_extracts_public_items_and_ignores_private()
    -> Result<(), Box<dyn std::error::Error>> {
        let code = r#"
            /// A documented public structure
            pub struct Greeter {
                pub message: String,
                private_field: i32,
            }

            struct InternalDetails {
                x: u32,
            }

            pub enum Status {
                Active,
                Pending(u32),
                Failed { reason: String },
            }

            pub fn greet(target: &str) -> String {
                format!("Hello, {}", target)
            }

            fn internal_helper() {}

            pub trait Runner: Send {
                fn run(&self) -> bool;
            }

            pub type Outcome = Result<String, u32>;
            pub const MAX_ATTEMPTS: usize = 3;
            pub use std::path::PathBuf;

            impl Greeter {
                pub fn new(message: String) -> Self {
                    Self { message, private_field: 0 }
                }

                fn secret(&self) {}
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = LibraryExtractor::new();
        let api = extractor.extract_from_file(&file, "demo");

        expect_that!(api.items.len(), eq(8));

        let names: Vec<String> = api.items.iter().map(|i| i.name.clone()).collect();
        for expected in [
            "Greeter",
            "Status",
            "greet",
            "Runner",
            "Outcome",
            "MAX_ATTEMPTS",
            "PathBuf",
            "Greeter::new",
        ] {
            expect_that!(names.iter().any(|n| n == expected), is_true());
        }

        let greeter = api.items.iter().find(|i| i.name == "Greeter");
        assert_that!(greeter, some(anything()));
        let greeter_item = greeter.ok_or("Greeter missing")?;
        expect_that!(
            greeter_item.signature,
            contains_substring("pub message: String")
        );
        let method = api.items.iter().find(|i| i.name == "Greeter::new");
        assert_that!(method, some(anything()));
        let method_item = method.ok_or("Greeter::new missing")?;
        expect_that!(method_item.kind, eq(ItemKind::Method));
        expect_that!(
            method_item.signature,
            contains_substring("pub fn Greeter::new(message: String) -> Self")
        );

        Ok(())
    }
}
