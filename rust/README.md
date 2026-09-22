# Sextant Native Build (Rust)

This directory contains the native implementation of the Sextant Browser as described in the Architecture Design Document.

## Project Structure

- `sextant-vault`: Cryptographic security core (Ed25519, TPM 2.0 simulation).
- `sextant-hull`: Hardware-accelerated UI shell built with Xilem and Vello.
- `sextant-engine`: Parallelized DOM layout and distillation engine based on Servo.

## Local Build Instructions

To build the native browser on your local machine:

1.  **Install Rust**: Ensure you have the latest stable Rust toolchain installed.
2.  **Install Dependencies**:
    -   Linux: `libdbus-1-dev`, `libvulkan-dev`, `pkg-config`.
    -   macOS: Xcode Command Line Tools.
    -   Windows: Visual Studio Build Tools with the C++ toolset. Build from a
        **Developer Command Prompt** (or run `vcvars64.bat` first) so `INCLUDE`,
        `LIB` and `link.exe` are on the environment. The repository does not
        pin a toolchain location -- `cargo build` from a normal shell will fail
        to link.
3.  **Build the Workspace**:
    ```bash
    cargo build --release
    ```
4.  **Run the Hull**:
    ```bash
    cargo run -p sextant-hull
    ```

## Product Path

The Rust workspace is the product path. The older TypeScript simulator/web preview has been removed.

## Servo Patch Helper

Sextant currently carries one local Servo checkout patch for the Bing `NodeList` live-search panic. The patch is stored in `../docs/patches/servo-nodelist-bing-panic.patch`.

After a fresh Cargo Servo checkout, or whenever `Cargo.lock` moves Servo to a new revision, run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\apply-servo-patches.ps1
```

Use `-CheckOnly` to verify whether the patch is already applied. The helper cleans `servo-script` by default after applying so the next browser build uses the patched source.
