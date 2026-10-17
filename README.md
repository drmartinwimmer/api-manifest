# api-manifest

[![CI](https://github.com/drmartinwimmer/api-manifest/actions/workflows/ci.yml/badge.svg)](https://github.com/drmartinwimmer/api-manifest/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/api-manifest.svg)](https://crates.io/crates/api-manifest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![MSRV: 1.88+](https://img.shields.io/badge/MSRV-1.88%2B-blue.svg?logo=rust)](https://www.rust-lang.org/)

AST-driven public API manifest generator (`API.md`) and drift detector for Rust crates and workspaces.

## Overview

`api-manifest` statically inspects your Rust codebase to generate human-readable `API.md` manifests and detect unintended public API changes or breaking drift across:

- **Library API**: Public structs, enums, traits, functions, methods, type aliases, constants, statics, and re-exports.
- **CLI API**: Clap commands, subcommands, flags, options, and positional arguments.
- **HTTP Service API**: Axum and attribute-macro HTTP endpoints.
- **Configuration File Schemas**: Configuration structs implementing `ConfigFile`.

## Installation

### From Crates.io

```bash
cargo install api-manifest
```

### From Source

```bash
cargo install --path .
```

## Usage

Check the current workspace or crate against checked-in `API.md` manifests:

```bash
api-manifest
```

Generate or update an `API.md` manifest for a crate:

```bash
api-manifest dump
```

Compare active code against a checked-in `API.md` manifest:

```bash
api-manifest check
```

## License

MIT
