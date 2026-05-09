{
  description = "murmur - decentralized P2P chat, native GUI, no browser tech";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # Native toolchain
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # Windows cross-compilation toolchain (runs on Linux, targets Windows)
        rustToolchainWindows = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" ];
          targets = [ "x86_64-pc-windows-gnu" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        craneLibWindows = (crane.mkLib pkgs).overrideToolchain rustToolchainWindows;

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          strictDeps = true;

          nativeBuildInputs = with pkgs; [
            pkg-config
            protobuf
            cmake
          ];

          buildInputs = with pkgs; [
            openssl
            alsa-lib       # ALSA for cpal audio
            libopus        # Opus codec
            libxkbcommon   # screen capture deps
            xorg.libxcb    # screen capture
            xorg.libXrandr # screen capture
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        murmurUnwrapped = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });

        # Wrap the binary with runtime library paths for non-NixOS Linux
        murmur = pkgs.runCommand "murmur-wrapped" {
          nativeBuildInputs = [ pkgs.makeWrapper ];
        } ''
          mkdir -p $out/bin
          makeWrapper ${murmurUnwrapped}/bin/murmur $out/bin/murmur \
            --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath (with pkgs; [
              vulkan-loader
              libxkbcommon
              wayland
              libGL
              xorg.libX11
              xorg.libXcursor
              xorg.libXrandr
              xorg.libXi
              xorg.libxcb
              alsa-lib
            ])}
        '';

        # Cross-compilation for Windows using mingw from nixpkgs
        pkgsCrossWin = pkgs.pkgsCross.mingwW64;

        windowsArgs = {
          inherit src;
          strictDeps = true;

          CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";

          # Tell cargo where the linker is
          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${pkgsCrossWin.stdenv.cc}/bin/${pkgsCrossWin.stdenv.cc.targetPrefix}cc";

          # Tell rustc where to find the CRT and Windows libs
          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L native=${pkgsCrossWin.windows.pthreads}/lib -L native=${pkgsCrossWin.windows.mcfgthreads}/lib";

          # Cross-compiler env vars for build scripts (ring, opus, etc)
          CC_x86_64_pc_windows_gnu = "${pkgsCrossWin.stdenv.cc}/bin/${pkgsCrossWin.stdenv.cc.targetPrefix}cc";
          CXX_x86_64_pc_windows_gnu = "${pkgsCrossWin.stdenv.cc}/bin/${pkgsCrossWin.stdenv.cc.targetPrefix}c++";
          AR_x86_64_pc_windows_gnu = "${pkgsCrossWin.stdenv.cc}/bin/${pkgsCrossWin.stdenv.cc.targetPrefix}ar";
          RANLIB_x86_64_pc_windows_gnu = "${pkgsCrossWin.stdenv.cc}/bin/${pkgsCrossWin.stdenv.cc.targetPrefix}ranlib";
          # Host CC for build scripts that run on the build machine
          HOST_CC = "${pkgs.stdenv.cc}/bin/cc";
          # cmake-rs uses these to find the cross toolchain
          CMAKE_C_COMPILER_x86_64_pc_windows_gnu = "${pkgsCrossWin.stdenv.cc}/bin/${pkgsCrossWin.stdenv.cc.targetPrefix}cc";
          CMAKE_CXX_COMPILER_x86_64_pc_windows_gnu = "${pkgsCrossWin.stdenv.cc}/bin/${pkgsCrossWin.stdenv.cc.targetPrefix}c++";
          CMAKE_SYSTEM_NAME = "Windows";
          # audiopus_sys bundles an old CMakeLists.txt that doesn't meet cmake 4.x policy
          CMAKE_POLICY_VERSION_MINIMUM = "3.5";

          # Use vendored OpenSSL (ring handles its own crypto, openssl-sys can vendor)
          OPENSSL_STATIC = "1";
          OPENSSL_NO_VENDOR = "0";

          nativeBuildInputs = with pkgs; [
            pkg-config
            protobuf
            cmake
            pkgsCrossWin.stdenv.cc
          ];

          buildInputs = [
            pkgsCrossWin.windows.pthreads
            pkgsCrossWin.windows.mcfgthreads
          ];

          # Don't run tests for cross-compiled target
          doCheck = false;
        };

        cargoArtifactsWindows = craneLibWindows.buildDepsOnly windowsArgs;

        murmurWindows = craneLibWindows.buildPackage (windowsArgs // {
          cargoArtifacts = cargoArtifactsWindows;
        });

        # Zip the Windows build
        murmurWindowsZip = pkgs.runCommand "murmur-windows-zip" {
          nativeBuildInputs = [ pkgs.zip ];
        } ''
          mkdir -p $out murmur-windows
          cp ${murmurWindows}/bin/murmur.exe murmur-windows/ || true
          # Copy any DLLs that might be needed
          find ${murmurWindows} -name "*.dll" -exec cp {} murmur-windows/ \; 2>/dev/null || true
          cd murmur-windows/..
          zip -r $out/murmur-windows.zip murmur-windows/
        '';
      in
      {
        packages = {
          default = murmur;
          murmur = murmur;
          windows = murmurWindows;
          windows-zip = murmurWindowsZip;
        };

        devShells.default = craneLib.devShell {
          inputsFrom = [ murmurUnwrapped ];
          packages = with pkgs; [
            rustToolchain
            cargo-watch
            cargo-edit
          ];
        };
      }
    );
}
