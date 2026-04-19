# 2026-04-19 — CI Bootstrap

This pass added the first repository-native CI workflow now that Sextant has a healthy git repo and canonical Gitea remote.

## What Changed

- added `.gitea/workflows/rust-ci.yml`
- workflow runs:
  - `cargo check --workspace`
  - `cargo test --workspace`
- workflow triggers on:
  - push to `main`
  - pull requests
  - manual dispatch

## Why It Matters

Sextant now has a basic automated guardrail for the Rust workspace instead of relying only on local session discipline.

This is intentionally minimal. The first goal is to keep the native product path compiling and the current regression coverage running in a repeatable way.
