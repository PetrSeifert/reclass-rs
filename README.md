## reclass-rs

ReClass-style memory exploration for Windows, written in Rust.

### What it does

- Attach to a process and browse loaded modules
- Build class layouts and view live memory in an interactive tree
- Edit class/field names and the root address inline
- Evaluate expressions in the root address field:
  - numbers (decimal or hex `0x..`), `+`, `-`, parentheses, deref `[expr]`
  - module refs `<module.dll>`
  - signature refs `$SignatureName`
- Define signatures in a dedicated window:
  - name, module, pattern, offset, instLen (hex accepted for numbers)
  - auto‑resolves each frame and shows the last value/error
  - use `$SignatureName` in expressions
- Save/Load to JSON
  - New format: `{ memory: ..., signatures: [...] }`
  - Legacy files with only `memory` are still supported

### Build and run

- Requirements: Windows 10/11 x64, Rust (nightly), working `vtd-libum` driver interface
- Build: `cargo build --release`
- Run: `cargo run --release`

### Tips

- Double‑click a class in the left panel to set it as root
- Right‑click fields for quick actions (insert bytes, remove, change type, copy)
- Unreferenced classes can be removed via context menu; “Delete unused” helps clean up

### Safety

Reads memory of other processes. Use only where you have permission.

### License

[MIT](LICENSE)


