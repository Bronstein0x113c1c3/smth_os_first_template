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

    # 3. Install bootimage globally via cargo if not already present
    if ! command -v bootimage &> /dev/null; then
      echo "bootimage not found. Installing..."
      cargo install bootimage
    else
      echo "bootimage is already installed."
    fi

    echo "--- Environment ready! ---"
  '';
}
