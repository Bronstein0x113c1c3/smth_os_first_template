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

    # 2. Add the required component for OS dev (sysroot compilation)
    rustup component add rust-src



    echo "--- Environment ready! ---"
  '';
}
