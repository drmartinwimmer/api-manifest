use crate::extractors::CargoPackageConfig;
use crate::extractors::cli_args::is_cfg_test;
use crate::extractors::library_format::{
    collect_use_tree, extract_doc_comment, extract_exported_macro, extract_impl_method,
    format_const, format_enum, format_item_fn_kind_name_sig, format_static, format_struct,
    format_trait, format_type_alias, is_public, normalize_tokens,
};
use crate::library_api::{ItemKind, LibraryApi, LibraryItem};
use crate::manifest::ManifestError;
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{
    File, ImplItemFn, ItemConst, ItemEnum, ItemFn, ItemImpl, ItemMacro, ItemMod, ItemStatic,
    ItemStruct, ItemTrait, ItemType, ItemUse,
};

const MAX_REEXPORT_DEPTH: usize = 5;

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

        let (submodules, private_submodules) =
            self.extract_file(&file, &crate_name, Some(crate_root), &mut items);
        self.extract_submodules(crate_root, &submodules, &crate_name, &mut items)?;
        self.extract_private_macro_submodules(
            crate_root,
            &private_submodules,
            &crate_name,
            &mut items,
        )?;

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
    ) -> (Vec<String>, Vec<String>) {
        let mut visitor = LibraryItemVisitor::new(module_path.to_string(), crate_root);
        visitor.visit_file(file);
        items.extend(visitor.items);
        (visitor.external_submodules, visitor.private_submodules)
    }

    fn extract_submodules(
        &self,
        crate_root: &Path,
        submodules: &[String],
        module_path: &str,
        items: &mut Vec<LibraryItem>,
    ) -> Result<(), ManifestError> {
        for mod_name in submodules {
            if let Some(path) = resolve_submodule_file(crate_root, mod_name) {
                let content = fs::read_to_string(&path)?;
                let sub_file = syn::parse_file(&content)?;
                let nested_path = format!("{module_path}::{mod_name}");
                self.extract_file(&sub_file, &nested_path, Some(crate_root), items);
            }
        }
        Ok(())
    }

    fn extract_private_macro_submodules(
        &self,
        crate_root: &Path,
        submodules: &[String],
        crate_root_module: &str,
        items: &mut Vec<LibraryItem>,
    ) -> Result<(), ManifestError> {
        for mod_name in submodules {
            if let Some(path) = resolve_submodule_file(crate_root, mod_name) {
                let content = fs::read_to_string(&path)?;
                let sub_file = syn::parse_file(&content)?;
                let mut macro_visitor = MacroExportVisitor::new(crate_root_module);
                macro_visitor.visit_file(&sub_file);
                items.extend(macro_visitor.items);
            }
        }
        Ok(())
    }
}

fn resolve_submodule_file(crate_root: &Path, mod_name: &str) -> Option<PathBuf> {
    let src_dir = crate_root.join("src");
    let candidate_file = src_dir.join(format!("{mod_name}.rs"));
    let candidate_mod_dir = src_dir.join(mod_name).join("mod.rs");

    if candidate_file.exists() {
        Some(candidate_file)
    } else if candidate_mod_dir.exists() {
        Some(candidate_mod_dir)
    } else {
        None
    }
}

fn resolve_dependency_crate_root(crate_root: &Path, dep_ident: &str) -> Option<PathBuf> {
    let cargo_toml_path = crate_root.join("Cargo.toml");
    let content = fs::read_to_string(cargo_toml_path).ok()?;
    let table: toml::Table = toml::from_str(&content).ok()?;
    let deps = table.get("dependencies")?.as_table()?;
    for (dep_name, dep_val) in deps {
        if dep_name.replace('-', "_") == dep_ident
            && let Some(path_str) = dep_val.get("path").and_then(|v| v.as_str())
        {
            let candidate = crate_root.join(path_str);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

fn resolve_reexport_items(
    crate_root: &Path,
    reexport: &str,
    module_path: &str,
    depth: usize,
) -> Vec<LibraryItem> {
    if depth >= MAX_REEXPORT_DEPTH {
        return Vec::new();
    }

    let segments: Vec<&str> = reexport.split("::").collect();
    let start_idx = usize::from(segments.first() == Some(&"crate"));
    if start_idx >= segments.len() {
        return Vec::new();
    }

    let Some(&target_name) = segments.last() else {
        return Vec::new();
    };
    let mod_segments = segments
        .get(start_idx..segments.len().saturating_sub(1))
        .unwrap_or(&[]);

    let candidates = if mod_segments.is_empty() {
        vec![crate_root.join("src").join("lib.rs")]
    } else {
        let mut mod_file_path = crate_root.join("src");
        for seg in mod_segments {
            mod_file_path = mod_file_path.join(seg);
        }
        vec![
            mod_file_path.with_extension("rs"),
            mod_file_path.join("mod.rs"),
        ]
    };

    for c in &candidates {
        if c.is_file()
            && let Ok(content) = fs::read_to_string(c)
            && let Ok(file) = syn::parse_file(&content)
        {
            let mut target_visitor =
                ReexportTargetVisitor::new(target_name, module_path, Some(crate_root), depth);
            target_visitor.visit_file(&file);
            if target_visitor.matched {
                return target_visitor.items;
            }
        }
    }

    if start_idx == 0
        && let Some(&first_seg) = mod_segments.first()
        && let Some(dep_root) = resolve_dependency_crate_root(crate_root, first_seg)
        && let Some(remaining) = segments.get(1..)
    {
        return resolve_reexport_items(&dep_root, &remaining.join("::"), module_path, depth + 1);
    }

    Vec::new()
}

struct MacroExportVisitor<'a> {
    crate_root_module: &'a str,
    items: Vec<LibraryItem>,
}

impl<'a> MacroExportVisitor<'a> {
    fn new(crate_root_module: &'a str) -> Self {
        Self {
            crate_root_module,
            items: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for MacroExportVisitor<'_> {
    fn visit_item_macro(&mut self, item_macro: &'ast ItemMacro) {
        if let Some(item) = extract_exported_macro(item_macro, self.crate_root_module) {
            self.items.push(item);
        }
    }

    fn visit_item_mod(&mut self, item_mod: &'ast ItemMod) {
        if !is_cfg_test(&item_mod.attrs) {
            visit::visit_item_mod(self, item_mod);
        }
    }
}

struct LibraryItemVisitor<'a> {
    crate_root_module: String,
    module_path: String,
    crate_root: Option<&'a Path>,
    current_impl_ty: Option<String>,
    items: Vec<LibraryItem>,
    external_submodules: Vec<String>,
    private_submodules: Vec<String>,
}

impl<'a> LibraryItemVisitor<'a> {
    fn new(module_path: String, crate_root: Option<&'a Path>) -> Self {
        let crate_root_module = module_path
            .split("::")
            .next()
            .unwrap_or(&module_path)
            .to_string();
        Self {
            crate_root_module,
            module_path,
            crate_root,
            current_impl_ty: None,
            items: Vec::new(),
            external_submodules: Vec::new(),
            private_submodules: Vec::new(),
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
        let resolved = resolve_reexport_items(crate_root, reexport, &self.module_path, 0);
        if resolved.is_empty() {
            false
        } else {
            self.items.extend(resolved);
            true
        }
    }
}

impl<'ast> Visit<'ast> for LibraryItemVisitor<'_> {
    fn visit_item_struct(&mut self, item_struct: &'ast ItemStruct) {
        if is_cfg_test(&item_struct.attrs) || !is_public(&item_struct.vis) {
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
        if is_cfg_test(&item_enum.attrs) || !is_public(&item_enum.vis) {
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
        if is_cfg_test(&item_fn.attrs) || !is_public(&item_fn.vis) {
            return;
        }
        let (kind, name, sig) = format_item_fn_kind_name_sig(item_fn);
        let path = format!("{}::{name}", self.module_path);
        let doc = extract_doc_comment(&item_fn.attrs);
        self.items
            .push(LibraryItem::new(kind, name, path, sig, doc));
    }

    fn visit_item_macro(&mut self, item_macro: &'ast ItemMacro) {
        if let Some(item) = extract_exported_macro(item_macro, &self.crate_root_module) {
            self.items.push(item);
        }
    }

    fn visit_item_trait(&mut self, item_trait: &'ast ItemTrait) {
        if is_cfg_test(&item_trait.attrs) || !is_public(&item_trait.vis) {
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
        if is_cfg_test(&item_type.attrs) || !is_public(&item_type.vis) {
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
        if is_cfg_test(&item_const.attrs) || !is_public(&item_const.vis) {
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
        if is_cfg_test(&item_static.attrs) || !is_public(&item_static.vis) {
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
        if is_cfg_test(&item_use.attrs) || !is_public(&item_use.vis) {
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
        if is_cfg_test(&item_mod.attrs) {
            return;
        }
        let mod_name = item_mod.ident.to_string();
        if !is_public(&item_mod.vis) {
            if item_mod.content.is_some() {
                let mut macro_visitor = MacroExportVisitor::new(&self.crate_root_module);
                macro_visitor.visit_item_mod(item_mod);
                self.items.extend(macro_visitor.items);
            } else {
                self.private_submodules.push(mod_name);
            }
            return;
        }
        if item_mod.content.is_some() {
            self.with_module_scope(&mod_name, |this| {
                visit::visit_item_mod(this, item_mod);
            });
        } else {
            self.external_submodules.push(mod_name);
        }
    }

    fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
        if is_cfg_test(&item_impl.attrs) || item_impl.trait_.is_some() {
            return;
        }
        let self_ty = normalize_tokens(&item_impl.self_ty);
        self.with_impl_ty(self_ty, |this| {
            visit::visit_item_impl(this, item_impl);
        });
    }

    fn visit_impl_item_fn(&mut self, method: &'ast ImplItemFn) {
        if let Some(ref self_ty) = self.current_impl_ty
            && let Some(item) = extract_impl_method(method, self_ty, &self.module_path)
        {
            self.items.push(item);
        }
    }
}

struct ReexportTargetVisitor<'a> {
    target_name: &'a str,
    module_path: &'a str,
    crate_root: Option<&'a Path>,
    depth: usize,
    current_impl_ty: Option<String>,
    items: Vec<LibraryItem>,
    matched: bool,
}

impl<'a> ReexportTargetVisitor<'a> {
    fn new(
        target_name: &'a str,
        module_path: &'a str,
        crate_root: Option<&'a Path>,
        depth: usize,
    ) -> Self {
        Self {
            target_name,
            module_path,
            crate_root,
            depth,
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
        if is_cfg_test(&item_struct.attrs) || item_struct.ident != self.target_name {
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
        if is_cfg_test(&item_enum.attrs) || item_enum.ident != self.target_name {
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
        if is_cfg_test(&item_trait.attrs) || item_trait.ident != self.target_name {
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
        if is_cfg_test(&item_fn.attrs) {
            return;
        }
        let (kind, name, sig) = format_item_fn_kind_name_sig(item_fn);
        if name != self.target_name {
            return;
        }
        let path = format!("{}::{name}", self.module_path);
        let doc = extract_doc_comment(&item_fn.attrs);
        self.items
            .push(LibraryItem::new(kind, name, path, sig, doc));
        self.matched = true;
    }

    fn visit_item_macro(&mut self, item_macro: &'ast ItemMacro) {
        if let Some(item) = extract_exported_macro(item_macro, self.module_path)
            && item.name == self.target_name
        {
            self.items.push(item);
            self.matched = true;
        }
    }

    fn visit_item_use(&mut self, item_use: &'ast ItemUse) {
        if is_cfg_test(&item_use.attrs) || !is_public(&item_use.vis) {
            return;
        }
        let Some(crate_root) = self.crate_root else {
            return;
        };
        let mut reexports = Vec::new();
        collect_use_tree(&item_use.tree, "", &mut reexports);
        for reexport in reexports {
            let exported_name = reexport.split("::").last().unwrap_or(&reexport);
            if exported_name == self.target_name {
                let resolved =
                    resolve_reexport_items(crate_root, &reexport, self.module_path, self.depth + 1);
                if !resolved.is_empty() {
                    self.items.extend(resolved);
                    self.matched = true;
                }
            }
        }
    }

    fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
        if is_cfg_test(&item_impl.attrs) || item_impl.trait_.is_some() {
            return;
        }
        let self_ty = normalize_tokens(&item_impl.self_ty);
        let base_ty = self_ty.split('<').next().map(str::trim).unwrap_or(&self_ty);
        if base_ty == self.target_name {
            self.with_impl_ty(self_ty, |this| {
                visit::visit_item_impl(this, item_impl);
            });
        }
    }

    fn visit_impl_item_fn(&mut self, method: &'ast ImplItemFn) {
        if let Some(ref self_ty) = self.current_impl_ty
            && let Some(item) = extract_impl_method(method, self_ty, self.module_path)
        {
            self.items.push(item);
        }
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

    #[googletest::test]
    fn extract_library_items_ignores_cfg_test_items_and_methods()
    -> Result<(), Box<dyn std::error::Error>> {
        let code = r#"
            pub struct SandboxBuilder<'a> {
                pub name: &'a str,
                #[cfg(test)]
                pub test_only_field: bool,
            }

            #[cfg(test)]
            pub struct TestHelper;

            #[cfg(test)]
            pub fn test_helper_fn() {}

            impl<'a> SandboxBuilder<'a> {
                pub fn build_command(&self) -> Result<(), String> {
                    Ok(())
                }

                #[cfg(test)]
                pub fn with_shared_config_dir(&self) {}
            }

            #[cfg(test)]
            impl<'a> SandboxBuilder<'a> {
                pub fn extra_test_method(&self) {}
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = LibraryExtractor::new();
        let api = extractor.extract_from_file(&file, "demo");

        expect_that!(api.items.len(), eq(2));
        let names: Vec<String> = api.items.iter().map(|i| i.name.clone()).collect();
        expect_that!(names.contains(&"SandboxBuilder".to_string()), is_true());
        expect_that!(
            names.contains(&"SandboxBuilder < 'a >::build_command".to_string()),
            is_true()
        );

        let sb = api
            .items
            .iter()
            .find(|i| i.name == "SandboxBuilder")
            .ok_or("SandboxBuilder missing")?;
        expect_that!(sb.signature, not(contains_substring("test_only_field")));

        Ok(())
    }

    #[googletest::test]
    fn extract_library_items_extracts_public_macros_and_derives()
    -> Result<(), Box<dyn std::error::Error>> {
        let code = r#"
            /// Exported declarative macro
            #[macro_export]
            macro_rules! exported_macro {
                () => {};
            }

            macro_rules! internal_macro {
                () => {};
            }

            mod private_mod {
                /// Exported macro inside a private module
                #[macro_export]
                macro_rules! nested_exported_macro {
                    ($x:expr) => { $x };
                }

                macro_rules! nested_private_macro {
                    () => {};
                }
            }

            /// Function-like procedural macro
            #[proc_macro]
            pub fn sql(input: TokenStream) -> TokenStream {
                input
            }

            /// Attribute procedural macro
            #[proc_macro_attribute]
            pub fn route(attr: TokenStream, item: TokenStream) -> TokenStream {
                item
            }

            /// Derive macro with helper attributes
            #[proc_macro_derive(ConfigFile, attributes(config_file, config_section))]
            pub fn derive_config_file(input: TokenStream) -> TokenStream {
                input
            }

            /// Simple derive macro without helper attributes
            #[proc_macro_derive(SimpleBean)]
            pub fn derive_simple_bean(input: TokenStream) -> TokenStream {
                input
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = LibraryExtractor::new();
        let api = extractor.extract_from_file(&file, "demo");

        expect_that!(api.items.len(), eq(6));

        let exported_macro = api
            .items
            .iter()
            .find(|i| i.name == "exported_macro")
            .ok_or("exported_macro missing")?;
        expect_that!(exported_macro.kind, eq(ItemKind::Macro));
        expect_that!(exported_macro.path, eq("demo::exported_macro"));
        expect_that!(exported_macro.signature, eq("macro_rules! exported_macro"));
        expect_that!(
            exported_macro.doc,
            eq(&Some("Exported declarative macro".to_string()))
        );

        let nested_macro = api
            .items
            .iter()
            .find(|i| i.name == "nested_exported_macro")
            .ok_or("nested_exported_macro missing")?;
        expect_that!(nested_macro.kind, eq(ItemKind::Macro));
        expect_that!(nested_macro.path, eq("demo::nested_exported_macro"));
        expect_that!(
            nested_macro.signature,
            eq("macro_rules! nested_exported_macro")
        );

        let sql_macro = api
            .items
            .iter()
            .find(|i| i.name == "sql")
            .ok_or("sql missing")?;
        expect_that!(sql_macro.kind, eq(ItemKind::Macro));
        expect_that!(sql_macro.signature, eq("#[proc_macro] pub fn sql"));

        let route_macro = api
            .items
            .iter()
            .find(|i| i.name == "route")
            .ok_or("route missing")?;
        expect_that!(route_macro.kind, eq(ItemKind::Macro));
        expect_that!(
            route_macro.signature,
            eq("#[proc_macro_attribute] pub fn route")
        );

        let config_derive = api
            .items
            .iter()
            .find(|i| i.name == "ConfigFile")
            .ok_or("ConfigFile missing")?;
        expect_that!(config_derive.kind, eq(ItemKind::DeriveMacro));
        expect_that!(
            config_derive.signature,
            eq("#[proc_macro_derive(ConfigFile, attributes(config_file, config_section))]")
        );

        let simple_derive = api
            .items
            .iter()
            .find(|i| i.name == "SimpleBean")
            .ok_or("SimpleBean missing")?;
        expect_that!(simple_derive.kind, eq(ItemKind::DeriveMacro));
        expect_that!(
            simple_derive.signature,
            eq("#[proc_macro_derive(SimpleBean)]")
        );

        Ok(())
    }

    #[googletest::test]
    fn extract_from_crate_resolves_workspace_derive_reexport_and_trait_with_same_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let extractor = LibraryExtractor::new();
        let api = extractor
            .extract_from_crate(manifest_dir)?
            .ok_or("Expected LibraryApi for workspace root crate")?;

        let trait_item = api
            .items
            .iter()
            .find(|i| i.name == "ConfigFile" && i.kind == ItemKind::Trait)
            .ok_or("ConfigFile trait missing")?;
        expect_that!(
            trait_item.signature,
            contains_substring("pub trait ConfigFile")
        );

        let derive_item = api
            .items
            .iter()
            .find(|i| i.name == "ConfigFile" && i.kind == ItemKind::DeriveMacro)
            .ok_or("ConfigFile derive macro missing")?;
        expect_that!(
            derive_item.signature,
            eq("#[proc_macro_derive(ConfigFile, attributes(config_file, config_section))]")
        );

        Ok(())
    }
}
