{ pkgs ? import <nixpkgs> { } }:

with pkgs;

mkShell {
  buildInputs = [
    rustup
    gcc
    qemu
  ];

  shellHook = ''
    echo "--- Setting up Rust OS development environment ---"

    # 1. Ensure rustup is using nightly
    rustup default nightly
    rustup override set nightly-2026-06-01


    # 2. Add the required component for OS dev (sysroot compilation)
    rustup component add rust-src

    rustup component add llvm-tools-preview


    echo "--- Environment ready! ---"
  '';
}
