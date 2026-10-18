use crate::cli_api::{CliApi, CliArgItem, CliCommandItem};
use crate::extractors::CargoPackageMetadata;
use crate::extractors::cli_args::{
    CommandMetadata, ParsedArgAttributes, extract_doc_comment, extract_type_ident,
    has_derive_attribute, is_flatten_field, is_subcommand_field, to_kebab_case,
};
use crate::manifest::ManifestError;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{Fields, File, Item, ItemEnum, ItemStruct, Variant};

/// Extractor for CLI subcommands, flags, and arguments.
#[derive(Debug, Default)]
pub(crate) struct CliExtractor;

impl CliExtractor {
    /// Creates a new `CliExtractor`.
    pub(crate) fn new() -> Self {
        Self
    }

    /// Dynamically introspects a `clap::Command` builder instance.
    #[cfg(test)]
    pub(crate) fn extract_from_command(&self, cmd: &clap::Command) -> CliApi {
        let mut commands = Vec::new();
        self.extract_cmd_recursive(cmd, "", &mut commands);
        CliApi::new(commands)
    }

    #[cfg(test)]
    fn extract_cmd_recursive(
        &self,
        cmd: &clap::Command,
        prefix: &str,
        output: &mut Vec<CliCommandItem>,
    ) {
        let name = if prefix.is_empty() {
            cmd.get_name().to_string()
        } else {
            format!("{prefix} {}", cmd.get_name())
        };

        let about = cmd.get_about().map(|a| a.to_string());

        let mut args = Vec::new();
        for arg in cmd.get_arguments() {
            let id = arg.get_id().to_string();
            if id == "help" || id == "version" {
                continue;
            }

            let short = arg.get_short();
            let long = arg.get_long().map(|l| l.to_string());
            let value_name = arg
                .get_value_names()
                .and_then(|names| names.first().map(|v| v.to_string()));
            let required = arg.is_required_set();
            let default_value = arg
                .get_default_values()
                .first()
                .and_then(|v| v.to_str().map(|s| s.to_string()));
            let help = arg.get_help().map(|h| h.to_string());
            let is_positional = arg.is_positional();

            args.push(CliArgItem {
                id,
                short,
                long,
                value_name,
                required,
                default_value,
                help,
                is_positional,
            });
        }

        let mut subcmd_names = Vec::new();
        for subcmd in cmd.get_subcommands() {
            let sub_name = subcmd.get_name().to_string();
            subcmd_names.push(sub_name);
            self.extract_cmd_recursive(subcmd, &name, output);
        }

        output.push(CliCommandItem::new(name, about, args, subcmd_names));
    }

    /// Statically extracts CLI commands from Rust source files in a crate.
    pub(crate) fn extract_from_crate(
        &self,
        crate_root: &Path,
    ) -> Result<Option<CliApi>, ManifestError> {
        let mut local = AstRegistry::default();
        local.index_rs_files(&crate_root.join("src"));

        let mut workspace = AstRegistry::default();
        for ws_dir in Self::discover_workspace_src_dirs(crate_root) {
            workspace.index_rs_files(&ws_dir);
        }

        let ctx = CrateContext { local, workspace };
        let default_name = CargoPackageMetadata::from_crate_root(crate_root)?.name;
        let api = self.extract_with_context(&ctx, &default_name);

        if api.commands.is_empty() {
            Ok(None)
        } else {
            Ok(Some(api))
        }
    }

    /// Statically extracts CLI commands from a parsed `syn::File`.
    #[cfg(test)]
    pub(crate) fn extract_from_file(&self, file: &File, default_name: &str) -> CliApi {
        let mut local = AstRegistry::default();
        local.index_file(file);
        let ctx = CrateContext {
            local,
            workspace: AstRegistry::default(),
        };
        self.extract_with_context(&ctx, default_name)
    }

    fn extract_with_context(&self, ctx: &CrateContext, default_name: &str) -> CliApi {
        let mut commands = Vec::new();
        let mut visited_enums = HashSet::new();

        let root_structs = ctx.find_root_structs();

        for root in root_structs {
            let meta = CommandMetadata::from_attrs(&root.attrs, default_name);
            ctx.extract_command_hierarchy(
                &root,
                &meta.name,
                meta.about,
                "",
                &mut commands,
                &mut visited_enums,
            );
        }

        for (enum_name, item_enum) in &ctx.local.enums {
            if has_derive_attribute(&item_enum.attrs, "Subcommand")
                && !visited_enums.contains(enum_name)
            {
                commands.push(SubcommandVariantVisitor::extract_enum_subcommand(item_enum));
            }
        }

        CliApi::new(commands)
    }

    fn discover_workspace_src_dirs(crate_root: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        let current_canonical = crate_root
            .canonicalize()
            .unwrap_or_else(|_| crate_root.to_path_buf());

        if let Some(crates_dir) = current_canonical.parent()
            && let Ok(entries) = fs::read_dir(crates_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
                    if canon != current_canonical && path.join("src").exists() {
                        dirs.push(path.join("src"));
                    }
                }
            }
        }
        dirs
    }
}

#[derive(Default)]
struct AstRegistry {
    structs: HashMap<String, ItemStruct>,
    enums: HashMap<String, ItemEnum>,
}

impl AstRegistry {
    fn index_rs_files(&mut self, dir: &Path) {
        if !dir.exists() {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.index_rs_files(&path);
                } else if path.extension().is_some_and(|ext| ext == "rs")
                    && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(file) = syn::parse_file(&content)
                {
                    self.index_file(&file);
                }
            }
        }
    }

    fn index_file(&mut self, file: &File) {
        for item in &file.items {
            match item {
                Item::Struct(item_struct) => {
                    self.structs
                        .insert(item_struct.ident.to_string(), item_struct.clone());
                }
                Item::Enum(item_enum) => {
                    self.enums
                        .insert(item_enum.ident.to_string(), item_enum.clone());
                }
                _ => {}
            }
        }
    }
}

struct CrateContext {
    local: AstRegistry,
    workspace: AstRegistry,
}

impl CrateContext {
    fn find_struct(&self, name: &str) -> Option<&ItemStruct> {
        self.local
            .structs
            .get(name)
            .or_else(|| self.workspace.structs.get(name))
    }

    fn find_enum(&self, name: &str) -> Option<&ItemEnum> {
        self.local
            .enums
            .get(name)
            .or_else(|| self.workspace.enums.get(name))
    }

    fn find_root_structs(&self) -> Vec<ItemStruct> {
        let mut parsers = Vec::new();
        for item_struct in self.local.structs.values() {
            if has_derive_attribute(&item_struct.attrs, "Parser") {
                parsers.push(item_struct.clone());
            }
        }
        if !parsers.is_empty() {
            parsers.sort_by_key(|s| if s.ident == "Cli" { 0 } else { 1 });
            return parsers;
        }

        for item_struct in self.local.structs.values() {
            if has_derive_attribute(&item_struct.attrs, "Args")
                && (item_struct.ident == "Cli"
                    || item_struct.ident.to_string().ends_with("Command"))
            {
                return vec![item_struct.clone()];
            }
        }

        Vec::new()
    }

    fn extract_command_hierarchy(
        &self,
        struct_item: &ItemStruct,
        cmd_name: &str,
        about: Option<String>,
        prefix: &str,
        output: &mut Vec<CliCommandItem>,
        visited_enums: &mut HashSet<String>,
    ) {
        let full_name = if prefix.is_empty() {
            cmd_name.to_string()
        } else {
            format!("{prefix} {cmd_name}")
        };

        let mut args = Vec::new();
        let mut subcmd_names = Vec::new();
        let mut pending_subcommands = Vec::new();
        let mut visited_flattened = HashSet::new();

        self.extract_struct_fields(
            &struct_item.fields,
            &mut args,
            &mut subcmd_names,
            &mut pending_subcommands,
            &mut visited_flattened,
            visited_enums,
        );

        output.push(CliCommandItem::new(
            full_name.clone(),
            about,
            args,
            subcmd_names,
        ));

        for pending in pending_subcommands {
            match pending.target {
                SubcommandTarget::Struct(type_name) => {
                    if let Some(inner_struct) = self.find_struct(&type_name) {
                        let effective_about = pending
                            .about
                            .or_else(|| CommandMetadata::from_attrs(&inner_struct.attrs, "").about);
                        self.extract_command_hierarchy(
                            inner_struct,
                            &pending.name,
                            effective_about,
                            &full_name,
                            output,
                            visited_enums,
                        );
                    } else {
                        let sub_full_name = format!("{full_name} {}", pending.name);
                        output.push(CliCommandItem::new(
                            sub_full_name,
                            pending.about,
                            Vec::new(),
                            Vec::new(),
                        ));
                    }
                }
                SubcommandTarget::InlineFields(fields) => {
                    let mut sub_args = Vec::new();
                    for field in &fields {
                        let f_name = field
                            .ident
                            .as_ref()
                            .map(|i| i.to_string())
                            .unwrap_or_default();
                        sub_args.push(ParsedArgAttributes::extract_cli_arg(field, f_name));
                    }
                    let sub_full_name = format!("{full_name} {}", pending.name);
                    output.push(CliCommandItem::new(
                        sub_full_name,
                        pending.about,
                        sub_args,
                        Vec::new(),
                    ));
                }
                SubcommandTarget::Unit => {
                    let sub_full_name = format!("{full_name} {}", pending.name);
                    output.push(CliCommandItem::new(
                        sub_full_name,
                        pending.about,
                        Vec::new(),
                        Vec::new(),
                    ));
                }
            }
        }
    }

    fn extract_struct_fields(
        &self,
        fields: &Fields,
        args: &mut Vec<CliArgItem>,
        subcmd_names: &mut Vec<String>,
        pending_subcommands: &mut Vec<PendingSubcommand>,
        visited_flattened: &mut HashSet<String>,
        visited_enums: &mut HashSet<String>,
    ) {
        let Fields::Named(named) = fields else {
            return;
        };

        for field in &named.named {
            let field_name = field
                .ident
                .as_ref()
                .map(|i| i.to_string())
                .unwrap_or_default();

            if is_flatten_field(&field.attrs)
                && let Some(inner_type) = extract_type_ident(&field.ty)
                && visited_flattened.insert(inner_type.clone())
                && let Some(inner_struct) = self.find_struct(&inner_type)
            {
                self.extract_struct_fields(
                    &inner_struct.fields,
                    args,
                    subcmd_names,
                    pending_subcommands,
                    visited_flattened,
                    visited_enums,
                );
            } else if is_subcommand_field(&field.attrs) {
                self.extract_subcommand_variants(
                    field,
                    subcmd_names,
                    pending_subcommands,
                    visited_enums,
                );
            } else {
                args.push(ParsedArgAttributes::extract_cli_arg(field, field_name));
            }
        }
    }

    fn extract_subcommand_variants(
        &self,
        field: &syn::Field,
        subcmd_names: &mut Vec<String>,
        pending_subcommands: &mut Vec<PendingSubcommand>,
        visited_enums: &mut HashSet<String>,
    ) {
        let Some(enum_name) = extract_type_ident(&field.ty) else {
            return;
        };
        visited_enums.insert(enum_name.clone());
        let Some(enum_item) = self.find_enum(&enum_name) else {
            return;
        };

        let mut visitor = SubcommandVariantVisitor::default();
        visitor.visit_item_enum(enum_item);
        subcmd_names.extend(visitor.subcmd_names);
        pending_subcommands.extend(visitor.pending_subcommands);
    }
}

enum SubcommandTarget {
    Struct(String),
    InlineFields(Vec<syn::Field>),
    Unit,
}

struct PendingSubcommand {
    name: String,
    about: Option<String>,
    target: SubcommandTarget,
}

#[derive(Default)]
struct SubcommandVariantVisitor {
    subcmd_names: Vec<String>,
    pending_subcommands: Vec<PendingSubcommand>,
}

impl SubcommandVariantVisitor {
    fn extract_enum_subcommand(item: &ItemEnum) -> CliCommandItem {
        let default_name = item.ident.to_string().to_lowercase();
        let meta = CommandMetadata::from_attrs(&item.attrs, &default_name);
        let mut visitor = Self::default();
        visitor.visit_item_enum(item);

        CliCommandItem::new(meta.name, meta.about, Vec::new(), visitor.subcmd_names)
    }

    fn extract_variant_name(variant: &syn::Variant) -> String {
        for attr in &variant.attrs {
            if attr.path().is_ident("command") {
                let mut name = None;
                drop(attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("name") {
                        let val: syn::LitStr = meta.value()?.parse()?;
                        name = Some(val.value());
                    }
                    Ok(())
                }));
                if let Some(n) = name {
                    return n;
                }
            }
        }
        to_kebab_case(&variant.ident.to_string())
    }

    fn extract_variant_about(variant: &syn::Variant) -> Option<String> {
        for attr in &variant.attrs {
            if attr.path().is_ident("command") {
                let mut about = None;
                drop(attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("about") {
                        let val: syn::LitStr = meta.value()?.parse()?;
                        about = Some(val.value());
                    }
                    Ok(())
                }));
                if about.is_some() {
                    return about;
                }
            }
        }
        extract_doc_comment(&variant.attrs)
    }
}

impl<'ast> Visit<'ast> for SubcommandVariantVisitor {
    fn visit_variant(&mut self, variant: &'ast Variant) {
        let var_name = Self::extract_variant_name(variant);
        let var_about = Self::extract_variant_about(variant);
        self.subcmd_names.push(var_name.clone());

        let target = match &variant.fields {
            Fields::Unnamed(unnamed) => {
                if let Some(first_field) = unnamed.unnamed.first()
                    && let Some(t_name) = extract_type_ident(&first_field.ty)
                {
                    SubcommandTarget::Struct(t_name)
                } else {
                    SubcommandTarget::Unit
                }
            }
            Fields::Named(named_fields) => {
                SubcommandTarget::InlineFields(named_fields.named.iter().cloned().collect())
            }
            Fields::Unit => SubcommandTarget::Unit,
        };

        self.pending_subcommands.push(PendingSubcommand {
            name: var_name,
            about: var_about,
            target,
        });
        visit::visit_variant(self, variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser, Subcommand};
    use googletest::prelude::*;

    #[derive(Parser, Debug)]
    #[command(name = "test-tool", about = "A sample CLI test tool")]
    struct SampleCli {
        /// Input file path
        #[arg(short, long)]
        file: Option<String>,

        /// Output format
        #[arg(long, default_value = "table")]
        format: String,

        /// Verbose flag
        #[arg(short, long)]
        verbose: bool,

        #[command(subcommand)]
        cmd: Option<SampleSubcommand>,
    }

    #[derive(Subcommand, Debug)]
    enum SampleSubcommand {
        /// Run audit
        Audit {
            #[arg(long)]
            strict: bool,
        },
        /// Run inspect
        Inspect,
    }

    #[googletest::test]
    fn extract_from_command_introspects_clap_hierarchy() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = <SampleCli as CommandFactory>::command();
        let extractor = CliExtractor::new();
        let cli_api = extractor.extract_from_command(&cmd);

        expect_that!(cli_api.commands.len(), eq(3));

        let root_cmd = cli_api.commands.iter().find(|c| c.name == "test-tool");
        assert_that!(root_cmd, some(anything()));
        let root = root_cmd.ok_or("root missing")?;
        expect_that!(root.about, eq(&Some("A sample CLI test tool".to_string())));
        expect_that!(root.args.len(), eq(3));
        expect_that!(
            root.subcommands,
            elements_are![eq(&"audit".to_string()), eq(&"inspect".to_string())]
        );

        let format_arg = root.args.iter().find(|a| a.id == "format");
        assert_that!(format_arg, some(anything()));
        let fmt = format_arg.ok_or("format arg missing")?;
        expect_that!(fmt.default_value, eq(&Some("table".to_string())));

        Ok(())
    }

    #[googletest::test]
    fn extract_from_file_parses_static_clap_structs() -> Result<(), Box<dyn std::error::Error>> {
        let code = r#"
            #[derive(Parser, Debug)]
            #[command(name = "static-cli", about = "Statically extracted CLI")]
            pub struct StaticCli {
                /// Target path
                #[arg(short, long)]
                path: Option<String>,

                /// Required flag
                #[arg(long)]
                count: u32,
            }

            #[derive(Subcommand, Debug)]
            pub enum Commands {
                Start,
                Stop,
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = CliExtractor::new();
        let api = extractor.extract_from_file(&file, "static-cli");

        expect_that!(api.commands.len(), eq(2));

        let static_cmd = api.commands.iter().find(|c| c.name == "static-cli");
        assert_that!(static_cmd, some(anything()));
        let cmd = static_cmd.ok_or("static-cli missing")?;
        expect_that!(cmd.about, eq(&Some("Statically extracted CLI".to_string())));

        let path_arg = cmd.args.iter().find(|a| a.id == "path");
        assert_that!(path_arg, some(anything()));
        let path = path_arg.ok_or("path missing")?;
        expect_that!(path.short, eq(Some('p')));
        expect_that!(path.required, is_false());

        let count_arg = cmd.args.iter().find(|a| a.id == "count");
        assert_that!(count_arg, some(anything()));
        let count = count_arg.ok_or("count missing")?;
        expect_that!(count.required, is_true());

        Ok(())
    }

    #[googletest::test]
    fn extract_from_file_flattens_nested_commands_and_subcommands()
    -> Result<(), Box<dyn std::error::Error>> {
        let code = r#"
            #[derive(Parser, Debug)]
            #[command(name = "suite-cli", about = "CLI suite")]
            pub struct SuiteCli {
                #[command(flatten)]
                inner: InnerCommand,

                #[command(subcommand)]
                cmd: SuiteSubcommands,
            }

            #[derive(Args, Debug)]
            pub struct InnerCommand {
                /// Host address
                #[arg(long, default_value = "127.0.0.1")]
                host: String,

                /// Enable TLS
                #[arg(long)]
                tls: bool,
            }

            #[derive(Subcommand, Debug)]
            pub enum SuiteSubcommands {
                /// Deploy service
                Deploy(DeployArgs),
                /// Status check
                Status,
            }

            #[derive(Args, Debug)]
            pub struct DeployArgs {
                /// Target environment
                #[arg(short, long)]
                env: String,
            }
        "#;

        let file = syn::parse_file(code)?;
        let extractor = CliExtractor::new();
        let api = extractor.extract_from_file(&file, "suite-cli");

        expect_that!(api.commands.len(), eq(3));

        let root_cmd = api.commands.iter().find(|c| c.name == "suite-cli");
        assert_that!(root_cmd, some(anything()));
        let root = root_cmd.ok_or("root missing")?;

        let host_arg = root.args.iter().find(|a| a.id == "host");
        assert_that!(host_arg, some(anything()));
        let host = host_arg.ok_or("host missing")?;
        expect_that!(host.default_value, eq(&Some("127.0.0.1".to_string())));
        expect_that!(host.required, is_false());

        let tls_arg = root.args.iter().find(|a| a.id == "tls");
        assert_that!(tls_arg, some(anything()));
        let tls = tls_arg.ok_or("tls missing")?;
        expect_that!(tls.required, is_false());

        expect_that!(
            root.subcommands,
            elements_are![eq(&"deploy".to_string()), eq(&"status".to_string())]
        );

        let deploy_cmd = api.commands.iter().find(|c| c.name == "suite-cli deploy");
        assert_that!(deploy_cmd, some(anything()));
        let deploy = deploy_cmd.ok_or("deploy missing")?;
        expect_that!(deploy.about, eq(&Some("Deploy service".to_string())));
        let env_arg = deploy.args.iter().find(|a| a.id == "env");
        assert_that!(env_arg, some(anything()));
        let env = env_arg.ok_or("env missing")?;
        expect_that!(env.required, is_true());

        let status_cmd = api.commands.iter().find(|c| c.name == "suite-cli status");
        assert_that!(status_cmd, some(anything()));
        let status = status_cmd.ok_or("status missing")?;
        expect_that!(status.about, eq(&Some("Status check".to_string())));
        expect_that!(status.args.len(), eq(0));

        Ok(())
    }
}
