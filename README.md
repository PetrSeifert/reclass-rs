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

### Web UI

`reclass-server` holds the attached process, the project and an open canvas of
cards, and serves a browser UI. Each instance is a card; clicking a pointer's
port opens its target as a linked card. With *Share cards* on, a pointer to an
instance that is already open links to that card instead of opening a copy.

```sh
cd web/app && npm install && npm run build && cd ../..
cargo run --release -p reclass-server -- --project memory_structure.json
# open http://127.0.0.1:7878
```

- `--demo` runs against a built-in simulated process (no driver needed)
- `--pid <pid>` attaches on start; `--bind` changes the address (loopback by default;
  anything else exposes process memory to the network)
- `--decrypt-module <name>` sets the module holding XenuineDecrypt for encrypted pointers
  (defaults to the process image)
- The project file is the same `memory_structure.json` the egui app uses; the canvas is
  stored in an extra `web` section that the egui app ignores

For UI development run the server with `--demo` and `npm run dev` in `web/app`;
Vite proxies `/ws` and `/api` to the server.

#### API

Every client drives the same workspace, and every change is pushed to all open
browsers. Scripts and agents can use plain HTTP:

```sh
curl -s localhost:7878/api/rpc -d '{"method":"eval","params":{"expr":"[$GWorld] + 0x10"}}' -H 'content-type: application/json'
curl -s localhost:7878/api/rpc -d '{"method":"snapshot"}' -H 'content-type: application/json'
```

Commands (see `server/src/workspace.rs`): `processes`, `attach`, `detach`, `modules`,
`setLive`, `setRoot`, `eval`, `read`, `snapshot`, `state`, `retype`, `defineAt`,
`renameField`, `insertBytes`, `removeField`, `setPointerTarget`, `setEmbeddedClass`,
`setEnum`, `setArray`, `addClass`, `renameClass`, `deleteClass`, `deleteUnusedClasses`,
`setSignature`, `removeSignature`, `rescan`, `follow`, `closeCard`, `closeAll`,
`mergeCard`, `toggleExpand`, `moveCards`, `setShare`, `save`, `load`, `newProject`.
The WebSocket at `/ws` takes the same `{id, method, params}` messages and streams
`session`, `defs` and `frame` updates.

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


