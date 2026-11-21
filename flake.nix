{
  description = "AST-driven public API manifest generator and drift detector for Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };
        apiManifestPackage = rustPlatform.buildRustPackage {
          pname = "api-manifest";
          version = "0.1.0";
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./README.md
              ./src
              ./crates
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
        };
        devShell =
          with pkgs;
          mkShell {
            buildInputs = [
              toolchain
              cargo-audit
              taplo
              prettier
            ];
          };
      in
      {
        devShells.default = devShell;

        checks = {
          inherit devShell;
          envrc =
            pkgs.runCommand "envrc-check"
              {
                nativeBuildInputs = [
                  pkgs.bash
                  pkgs.shellcheck
                  toolchain
                  pkgs.cargo-audit
                  pkgs.taplo
                  pkgs.prettier
                ];
              }
              ''
                shellcheck --shell=bash ${./.envrc}
                bash -n ${./.envrc}
                expected_channel=$(sed -n 's/^channel = "\(.*\)"/\1/p' ${./rust-toolchain.toml})
                [ -n "$expected_channel" ]
                rustc --version | grep -E "^rustc ''${expected_channel}(\.| )"
                cargo --version
                clippy-driver --version
                rustfmt --version
                rust-analyzer --version
                cargo-audit --version
                taplo --version
                prettier --version
                touch "$out"
              '';
        };

        packages = {
          default = apiManifestPackage;
          api-manifest = apiManifestPackage;
        };

        apps = {
          default = flake-utils.lib.mkApp {
            drv = apiManifestPackage;
          };
          api-manifest = flake-utils.lib.mkApp {
            drv = apiManifestPackage;
          };
        };
      }
    );
}
