# Rust Operating System

A custom x86_64 operating system written in Rust. This project runs in a bare-metal environment using QEMU.

## Prerequisites

This project uses the **Nix package manager** to ensure a reproducible development environment with all necessary dependencies (GCC, QEMU, and Rustup).

If you don't have Nix installed, follow the installation instructions at [nixos.org](https://nixos.org/download.html).

---

## Getting Started

### 1. Enter the Development Shell
Clone the repository and navigate into the project directory. Then, spin up the Nix shell. This will automatically pull in QEMU, GCC, and configure your Rust nightly toolchain, `rust-src`, and `bootimage`.

```bash
nix-shell
