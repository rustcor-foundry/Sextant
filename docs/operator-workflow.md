# Operator Workflow

This is the normal development flow for working in Sextant without losing the thread of the product.

## Primary Rule

Follow [next-work.md](next-work.md) as the active execution guide unless the team explicitly decides to re-prioritize.

## Read Order At Session Start

1. [current-state.md](current-state.md)
2. [next-work.md](next-work.md)
3. [architecture.md](architecture.md)
4. [development-guardrails.md](development-guardrails.md)
5. [completed.md](completed.md)

## Normal Development Loop

1. Confirm which lane in [next-work.md](next-work.md) you are advancing.
2. Inspect the relevant Rust crates before changing anything.
3. Prefer the native Rust hull and supporting Rust crates over the TypeScript simulator for real product work.
4. Make the smallest coherent change that advances the actual product path.
5. Verify with the most relevant command:
   - `cargo check -p sextant-hull`
   - `cargo test -p sextant-pilot --lib`
   - `cargo test -p sextant-engine --lib`
   - `cargo run -p sextant-hull`
6. Update docs if the current state, workflow, or recovery story changed.
7. Commit in coherent units and push to the canonical Gitea remote.

## Preferred Verification Commands

### Fast Native Sanity Check

```bash
cd rust
cargo check -p sextant-hull
```

### Pilot Regression Coverage

```bash
cd rust
cargo test -p sextant-pilot --lib
```

### Engine Regression Coverage

```bash
cd rust
cargo test -p sextant-engine --lib
```

### Real Hull Launch

```bash
cd rust
cargo run -p sextant-hull
```

## Documentation Rules

Update docs when any of these changed:

- the real product status
- the recommended work order
- the system shape
- the repo workflow
- the recovery/onboarding path

Usually that means touching one or more of:

- [current-state.md](current-state.md)
- [next-work.md](next-work.md)
- [architecture.md](architecture.md)
- [completed.md](completed.md)
- [archive](archive)

## What Counts As The Product Path

The product path is:

- user enters intent
- Pilot reasons about it
- engine navigates and distills
- Wake records it
- Captain's Log reflects it
- hull shows truthful state

Prefer changes that make this loop more real, more observable, or more reliable.

## What To Avoid

- polishing one panel too long while the core loop is still incomplete
- hiding failures behind fake success states
- growing giant inline Xilem view trees
- putting orchestration logic directly inside widget callbacks
- treating the TypeScript simulator as the main product path

## Commit And Push

The canonical remote is `rustcor/Sextant` on Gitea.

Normal finish:

```bash
git status
git add -A
git commit -m "Your concise message"
git push
```
