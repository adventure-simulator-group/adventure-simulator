{
  inputs = {
    nixpkgs = {
      url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    };
    utils = {
      url = "github:numtide/flake-utils";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      utils,
      rust-overlay,
    }:
    utils.lib.eachSystem [ "aarch64-linux" "x86_64-linux" ] (
      system:
      let
        # Use rust overlay to override rust from nixpkgs to the latest nightly rust
        # (and also tools: cargo + clippy)
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
          config.allowUnfree = true;
        };

        rustBuildInputs = with pkgs; [
          (rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
          mold
          pkg-config
        ];

        requiredLibsLinux = with pkgs; [
          libudev-zero
          alsa-lib
          vulkan-loader

          libX11
          libXcursor
          libXrandr
          libXi

          libxkbcommon

          freetype
          fontconfig
          expat
          libGL
          wayland
          openssl
        ];

        # IDE/shell dependencies
        developPrograms =
          (with pkgs; [
            cargo-edit
            clippy
            rust-analyzer-unwrapped
            just
            python3
            wasm-bindgen-cli_0_2_106
            binaryen
          ]);

        mkLinuxLdLibraryPathExport = libs: ''
          FLAKE_LIBDIR="${pkgs.lib.makeLibraryPath libs}"
          RUST_LIBDIR=$(rustc --print target-libdir)
          export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$FLAKE_LIBDIR:$RUST_LIBDIR:target/debug/deps:target/debug:${pkgs.stdenv.cc.cc.lib}/lib"
        '';
      in
      {
        # `nix develop`
        devShells.default = pkgs.mkShell rec {
          nativeBuildInputs = developPrograms ++ rustBuildInputs;
          buildInputs = requiredLibsLinux;

          shellHook = ''
            ${mkLinuxLdLibraryPathExport buildInputs}
          '';
        };
        devShells.mdbook = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            mdbook
            mdbook-linkcheck
          ];
        };
      }
    );
}
