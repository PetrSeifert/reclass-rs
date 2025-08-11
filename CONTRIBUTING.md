# Contributing to reclass-rs

Thanks for your interest in improving reclass-rs! This guide explains how to get set up, make changes, and submit a great PR.

## Prerequisites
- Windows 10/11 x64
- Rust nightly (the project uses some nightly features)
- A working `vtd-libum` driver environment (required for process memory access)

## Project layout (workspace)
- `main/`: app binary (`re-class`), egui/eframe UI and app state
- `handle/`: process handle, module listing, memory reading, signature resolving
- `main/src/memory/`: core data model (definitions, instances, enums)

## Build and run
- Build: `cargo build --release`
- Run: `cargo run --release`
- Tests: `cargo test`

## Development workflow
1. Fork and create a feature branch:
   - `git checkout -b feat/my-change`
2. Make focused, incremental changes.
3. Run locally:
   - `cargo check` (fast type check)
   - `cargo fmt --all` (format)
   - `cargo test` (unit tests)
   - `cargo clippy --all-targets --all-features -D warnings` (lint; fix all warnings)
4. Commit using Conventional Commits (see below).
5. Open a Pull Request with a clear description and, for UI changes, screenshots where useful.

## Commit message conventions (Conventional Commits)
Use the following types to make changelogs clear:
- `feat`: new feature
- `fix`: bug fix
- `docs`: documentation only
- `refactor`: code change that neither fixes a bug nor adds a feature
- `perf`: performance improvement
- `test`: add or update tests
- `build`: build system or dependencies
- `ci`: CI configuration
- `chore`: maintenance tasks

Examples:
- `feat(ui): signatures window with auto-resolve and $name expressions`
- `fix(memory): initialize nested instances for pointer targets`

## Coding style and guidelines
- Rust style: run `cargo fmt --all` before committing (repo contains `rustfmt.toml`).
- Lint with Clippy: run `cargo clippy --all-targets --all-features` and address all findings.
- Prefer clear names and self-explanatory methods.
- Keep control flow simple (early returns over deep nesting).
- UI (egui):
  - Avoid state flicker; prefer buffering user input and committing on blur/Enter or reading live buffers when appropriate.
  - Keep UI responsive; avoid long blocking operations in the frame.
- Tests:
  - Prefer exercising the same code paths as the app (avoid test-only shortcuts), to reflect real behavior.

## Feature notes (helpful specifics)
- Signatures
  - UI auto-resolves every frame; offset and instLen accept decimal or hex (`0x..`).
  - `$SignatureName` can be used in expressions.
  - Signatures are saved with memory to JSON in a wrapper: `{ "memory": ..., "signatures": [...] }`.
- Expressions (root address)
  - Support: decimal, hex (`0x..`), `+`, `-`, parentheses `()`, deref `[expr]`, `<module.dll>`, `$SignatureName`.

## Adding/Changing data model or persistence
- Update serde structs and ensure backward compatibility when possible.
- When changing JSON layout, keep legacy loaders if feasible (as done for `{ memory }` only files).
- Add/adjust tests when modifying core behavior.

## Submitting a Pull Request
- Keep PRs focused and as small as practical.
- Describe the problem and how your change fixes it.
- Note any UX changes and include screenshots for UI when helpful.
- Ensure `cargo fmt --all`, `cargo clippy --all-targets --all-features` and `cargo test` pass.

## Code of conduct
Please be respectful and constructive in all interactions. Be mindful when working with code that reads other processes’ memory; only test on systems where you have permission.

---
Thank you for contributing!