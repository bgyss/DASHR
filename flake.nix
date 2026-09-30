{
  description = "DASHR native Rust/wgpu development environment";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  outputs = { nixpkgs, ... }:
    let
      systems = [ "aarch64-darwin" "x86_64-darwin" "x86_64-linux" "aarch64-linux" ];
    in {
      devShells = nixpkgs.lib.genAttrs systems (system:
        let pkgs = import nixpkgs { inherit system; };
        in { default = pkgs.mkShell {
          packages = with pkgs; [ rustup mise pkg-config git ]
            ++ lib.optionals stdenv.hostPlatform.isLinux [ vulkan-loader libxkbcommon wayland libX11 libXcursor libXi libXrandr ];
          shellHook = ''
            export DASHR_NIX=1
            unset RUSTUP_TOOLCHAIN RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RUSTC RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
            export XDG_CACHE_HOME="$PWD/.dev/cache"
            export XDG_STATE_HOME="$PWD/.dev/state"
            export MISE_STATE_DIR="$PWD/.dev/mise-state"
            export CARGO_HOME="$PWD/.dev/cargo"
            export RUSTUP_HOME="$PWD/.dev/rustup"
            export CARGO_TARGET_DIR="$PWD/target"
            export MISE_CACHE_DIR="$PWD/.dev/mise-cache"
            export MISE_DATA_DIR="$PWD/.dev/mise-data"
            unset LIBRARY_PATH CPATH C_INCLUDE_PATH CPLUS_INCLUDE_PATH
            export CC="${pkgs.stdenv.cc}/bin/cc"
            export CXX="${pkgs.stdenv.cc}/bin/c++"
            ${pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isDarwin ''
              export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="$CC"
              export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER="$CC"
            ''}
            ${pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
              export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [ pkgs.vulkan-loader pkgs.libxkbcommon pkgs.wayland pkgs.libX11 pkgs.libXcursor pkgs.libXi pkgs.libXrandr ]}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            ''}
          '';
        }; });
    };
}
