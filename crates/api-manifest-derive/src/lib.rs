//! Procedural derive macro for `#[derive(ConfigFile)]`.

mod derive;

use derive::impl_config_file;
use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Derives the `ConfigFile` trait, requiring a `#[config_file("path/to/file")]` attribute.
#[proc_macro_derive(ConfigFile, attributes(config_file, config_section))]
pub fn derive_config_file(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_config_file(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
