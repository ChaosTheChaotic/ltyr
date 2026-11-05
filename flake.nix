{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    gitignore = {
      url = "github:hercules-ci/gitignore.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      gitignore,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pname = "ltyr";
        version = "0.1.0";
        overlays = [ (import rust-overlay) ];
        pkgs = (import nixpkgs) { inherit system overlays; };

        toolchain = (
          pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" ];
          }
        );
        platform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

        src = pkgs.lib.cleanSourceWith {
          name = "${pname}-clean-src";
          src = ./.;
          filter = gitignore.lib.gitignoreFilterWith {
            basePath = ./.;
            extraRules = ''
              flake.*
              LICENSE.md
              README.md
            '';
          };
        };
        drv = platform.buildRustPackage {
          inherit src pname version;

          doCheck = false;

          cargoLock.lockFile = ./Cargo.lock;
        };
      in
      {
        packages = {
          default = self.packages.${system}.${pname};
          ${pname} = drv;
        };
        devShell = pkgs.mkShell {
          nativeBuildInputs = [
            toolchain
          ];
          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
        };
      }
    );
}
