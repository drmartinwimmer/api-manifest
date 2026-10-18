use crate::extractors::CargoPackageConfig;
use crate::extractors::library_format::{
    collect_use_tree, extract_doc_comment, format_const, format_enum, format_fn_sig, format_static,
    format_struct, format_trait, format_type_alias, is_public, normalize_tokens,
};
use crate::library_api::{ItemKind, LibraryApi, LibraryItem};
use crate::manifest::ManifestError;
use std::fs;
use std::path::Path;
use syn::visit::{self, Visit};
use syn::{
    File, ImplItemFn, ItemConst, ItemEnum, ItemFn, ItemImpl, ItemMod, ItemStatic, ItemStruct,
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
    pub(crate) fn extract_from_crate(
        &self,
        crate_root: &Path,
    ) -> Result<Option<LibraryApi>, ManifestError> {
        let lib_path = crate_root.join("src").join("lib.rs");
        if !lib_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&lib_path)?;
        let file = syn::parse_file(&content)?;

        let crate_name = CargoPackageConfig::from_crate_root(crate_root)?
            .name
            .replace('-', "_");
        let mut items = Vec::new();

        let submodules = self.extract_file(&file, &crate_name, Some(crate_root), &mut items);
        self.extract_submodules(crate_root, &submodules, &crate_name, &mut items)?;

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
    ) -> Vec<String> {
        let mut visitor = LibraryItemVisitor::new(module_path.to_string(), crate_root);
        visitor.visit_file(file);
        items.extend(visitor.items);
        visitor.external_submodules
    }

    fn extract_submodules(
        &self,
        crate_root: &Path,
        submodules: &[String],
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) -> Result<(), ManifestError> {
        let src_dir = crate_root.join("src");
        for mod_name in submodules {
            let candidate_file = src_dir.join(format!("{mod_name}.rs"));
            let candidate_mod_dir = src_dir.join(mod_name).join("mod.rs");

            let target_path = if candidate_file.exists() {
                Some(candidate_file)
            } else if candidate_mod_dir.exists() {
                Some(candidate_mod_dir)
            } else {
                None
            };

            if let Some(path) = target_path {
                let content = fs::read_to_string(&path)?;
                let sub_file = syn::parse_file(&content)?;
                let nested_path = format!("{module_path}::{mod_name}");
                self.extract_file(&sub_file, &nested_path, Some(crate_root), items);
            }
        }
        Ok(())
    }
}

struct LibraryItemVisitor<'a> {
    module_path: String,
    crate_root: Option<&'a Path>,
    current_impl_ty: Option<String>,
    items: Vec<LibraryItem>,
    external_submodules: Vec<String>,
}

impl<'a> LibraryItemVisitor<'a> {
    fn new(module_path: String, crate_root: Option<&'a Path>) -> Self {
        Self {
            module_path,
            crate_root,
            current_impl_ty: None,
            items: Vec::new(),
            external_submodules: Vec::new(),
        }
    }

    fn with_module_scope(&mut self, mod_name: &str, f: impl FnOnce(&mut Self)) {
        let previous = self.module_path.clone();
        self.module_path = format!("{previous}::{mod_name}");
        f(self);
        self.module_path = previous;
    }

    fn with_impl_ty(&mut self, self_ty: String, f: impl FnOnce(&mut Self)) {
        let previous = self.current_impl_ty.replace(self_ty);
        f(self);
        self.current_impl_ty = previous;
    }

    fn resolve_internal_reexport(&mut self, crate_root: &Path, reexport: &str) -> bool {
        let segments: Vec<&str> = reexport.split("::").collect();
        if segments.is_empty() {
            return false;
        }

        let start_idx = usize::from(segments.first() == Some(&"crate"));
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

        let mut target_visitor = ReexportTargetVisitor::new(target_name, &self.module_path);
        target_visitor.visit_file(&file);
        let matched = target_visitor.matched;
        self.items.extend(target_visitor.items);
        matched
    }
}

impl<'ast> Visit<'ast> for LibraryItemVisitor<'_> {
    fn visit_item_struct(&mut self, item_struct: &'ast ItemStruct) {
        if !is_public(&item_struct.vis) {
            return;
        }
        let name = item_struct.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_struct(item_struct);
        let doc = extract_doc_comment(&item_struct.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Struct, name, path, sig, doc));
    }

    fn visit_item_enum(&mut self, item_enum: &'ast ItemEnum) {
        if !is_public(&item_enum.vis) {
            return;
        }
        let name = item_enum.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_enum(item_enum);
        let doc = extract_doc_comment(&item_enum.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Enum, name, path, sig, doc));
    }

    fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
        if !is_public(&item_fn.vis) {
            return;
        }
        let name = item_fn.sig.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_fn_sig(&item_fn.sig, None);
        let doc = extract_doc_comment(&item_fn.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Function, name, path, sig, doc));
    }

    fn visit_item_trait(&mut self, item_trait: &'ast ItemTrait) {
        if !is_public(&item_trait.vis) {
            return;
        }
        let name = item_trait.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_trait(item_trait);
        let doc = extract_doc_comment(&item_trait.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Trait, name, path, sig, doc));
    }

    fn visit_item_type(&mut self, item_type: &'ast ItemType) {
        if !is_public(&item_type.vis) {
            return;
        }
        let name = item_type.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_type_alias(item_type);
        let doc = extract_doc_comment(&item_type.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::TypeAlias, name, path, sig, doc));
    }

    fn visit_item_const(&mut self, item_const: &'ast ItemConst) {
        if !is_public(&item_const.vis) {
            return;
        }
        let name = item_const.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_const(item_const);
        let doc = extract_doc_comment(&item_const.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Constant, name, path, sig, doc));
    }

    fn visit_item_static(&mut self, item_static: &'ast ItemStatic) {
        if !is_public(&item_static.vis) {
            return;
        }
        let name = item_static.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_static(item_static);
        let doc = extract_doc_comment(&item_static.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Static, name, path, sig, doc));
    }

    fn visit_item_use(&mut self, item_use: &'ast ItemUse) {
        if !is_public(&item_use.vis) {
            return;
        }
        let mut reexports = Vec::new();
        collect_use_tree(&item_use.tree, "", &mut reexports);
        for reexport in reexports {
            let handled = self
                .crate_root
                .is_some_and(|root| self.resolve_internal_reexport(root, &reexport));
            if !handled {
                let name = reexport.split("::").last().unwrap_or(&reexport).to_string();
                let path = format!("{}::{name}", self.module_path);
                let sig = format!("pub use {reexport};");
                let doc = extract_doc_comment(&item_use.attrs);
                self.items
                    .push(LibraryItem::new(ItemKind::Reexport, name, path, sig, doc));
            }
        }
    }

    fn visit_item_mod(&mut self, item_mod: &'ast ItemMod) {
        if !is_public(&item_mod.vis) {
            return;
        }
        let mod_name = item_mod.ident.to_string();
        if item_mod.content.is_some() {
            self.with_module_scope(&mod_name, |this| {
                visit::visit_item_mod(this, item_mod);
            });
        } else {
            self.external_submodules.push(mod_name);
        }
    }

    fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
        if item_impl.trait_.is_some() {
            return;
        }
        let self_ty = normalize_tokens(&item_impl.self_ty);
        self.with_impl_ty(self_ty, |this| {
            visit::visit_item_impl(this, item_impl);
        });
    }

    fn visit_impl_item_fn(&mut self, method: &'ast ImplItemFn) {
        if !is_public(&method.vis) {
            return;
        }
        let Some(ref self_ty) = self.current_impl_ty else {
            return;
        };
        let method_name = method.sig.ident.to_string();
        let qual_name = format!("{self_ty}::{method_name}");
        let path = format!("{}::{qual_name}", self.module_path);
        let sig = format_fn_sig(&method.sig, Some(self_ty));
        let doc = extract_doc_comment(&method.attrs);
        self.items.push(LibraryItem::new(
            ItemKind::Method,
            qual_name,
            path,
            sig,
            doc,
        ));
    }
}

struct ReexportTargetVisitor<'a> {
    target_name: &'a str,
    module_path: &'a str,
    current_impl_ty: Option<String>,
    items: Vec<LibraryItem>,
    matched: bool,
}

impl<'a> ReexportTargetVisitor<'a> {
    fn new(target_name: &'a str, module_path: &'a str) -> Self {
        Self {
            target_name,
            module_path,
            current_impl_ty: None,
            items: Vec::new(),
            matched: false,
        }
    }

    fn with_impl_ty(&mut self, self_ty: String, f: impl FnOnce(&mut Self)) {
        let previous = self.current_impl_ty.replace(self_ty);
        f(self);
        self.current_impl_ty = previous;
    }
}

impl<'ast> Visit<'ast> for ReexportTargetVisitor<'_> {
    fn visit_item_struct(&mut self, item_struct: &'ast ItemStruct) {
        if item_struct.ident != self.target_name {
            return;
        }
        let name = item_struct.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_struct(item_struct);
        let doc = extract_doc_comment(&item_struct.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Struct, name, path, sig, doc));
        self.matched = true;
    }

    fn visit_item_enum(&mut self, item_enum: &'ast ItemEnum) {
        if item_enum.ident != self.target_name {
            return;
        }
        let name = item_enum.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_enum(item_enum);
        let doc = extract_doc_comment(&item_enum.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Enum, name, path, sig, doc));
        self.matched = true;
    }

    fn visit_item_trait(&mut self, item_trait: &'ast ItemTrait) {
        if item_trait.ident != self.target_name {
            return;
        }
        let name = item_trait.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_trait(item_trait);
        let doc = extract_doc_comment(&item_trait.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Trait, name, path, sig, doc));
        self.matched = true;
    }

    fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
        if item_fn.sig.ident != self.target_name {
            return;
        }
        let name = item_fn.sig.ident.to_string();
        let path = format!("{}::{name}", self.module_path);
        let sig = format_fn_sig(&item_fn.sig, None);
        let doc = extract_doc_comment(&item_fn.attrs);
        self.items
            .push(LibraryItem::new(ItemKind::Function, name, path, sig, doc));
        self.matched = true;
    }

    fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
        if item_impl.trait_.is_some() {
            return;
        }
        let self_ty = normalize_tokens(&item_impl.self_ty);
        if self_ty == self.target_name {
            self.with_impl_ty(self_ty, |this| {
                visit::visit_item_impl(this, item_impl);
            });
        }
    }

    fn visit_impl_item_fn(&mut self, method: &'ast ImplItemFn) {
        if !is_public(&method.vis) {
            return;
        }
        let Some(ref self_ty) = self.current_impl_ty else {
            return;
        };
        let method_name = method.sig.ident.to_string();
        let qual_name = format!("{self_ty}::{method_name}");
        let path = format!("{}::{qual_name}", self.module_path);
        let sig = format_fn_sig(&method.sig, Some(self_ty));
        let doc = extract_doc_comment(&method.attrs);
        self.items.push(LibraryItem::new(
            ItemKind::Method,
            qual_name,
            path,
            sig,
            doc,
        ));
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
