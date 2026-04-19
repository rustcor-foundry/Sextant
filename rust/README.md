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
    -   Windows: Visual Studio Build Tools.
3.  **Build the Workspace**:
    ```bash
    cargo build --release
    ```
4.  **Run the Hull**:
    ```bash
    cargo run -p sextant-hull
    ```

## Note on Web Preview
The web preview in AI Studio continues to run the **TypeScript Simulator** to provide an interactive experience. The Rust code here is for the production native binary.
