{
  description = "AST-driven public API manifest generator and drift detector for Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    rust-overlay.url = "github:oxalica/rust-overlay";
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
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };
      in
      {
        devShells.default =
          with pkgs;
          mkShell {
            buildInputs = [
              toolchain
              (cargo-audit.override { inherit rustPlatform; })
              taplo
              prettier
            ];
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
