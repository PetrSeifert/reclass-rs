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

Paste the API token printed in the server terminal into the UI's password field.
The server generates a new 256-bit token each run. To keep a stable token, set
`RECLASS_API_TOKEN` to a cryptographically random hexadecimal string of at least
64 characters before starting the server. Empty or malformed values stop startup.
The UI keeps the token only in memory; reloads and server token changes require
entering it again. Treat the token as access to the entire shared workspace.

- `--demo` runs against a built-in simulated process (no driver needed)
- `--pid <pid>` attaches on start; `--bind` changes the address, loopback by default.
  Use a TLS reverse proxy for remote access to protect the token and process memory.
  Disable or redact proxy logging of `Sec-WebSocket-Protocol`, which carries the browser's API token.
- `--decrypt-module <name>` sets the module holding XenuineDecrypt for encrypted pointers
  (defaults to the process image)
- The project file is the same `memory_structure.json` the egui app uses; the canvas is
  stored in an extra `web` section that the egui app ignores

For UI development run the server with `--demo --allowed-origin http://localhost:5173`
and `npm run dev` in `web/app`. Vite proxies `/ws` and `/api` to the server.
Use the exact origin shown by Vite if its hostname or port differs.

#### API

Every client drives the same workspace, and every change is pushed to all open
browsers. Scripts and agents can use plain HTTP:

```sh
curl -s localhost:7878/api/rpc -H "Authorization: Bearer $RECLASS_API_TOKEN" -d '{"method":"eval","params":{"expr":"[$GWorld] + 0x10"}}' -H 'content-type: application/json'
curl -s localhost:7878/api/rpc -H "Authorization: Bearer $RECLASS_API_TOKEN" -d '{"method":"snapshot"}' -H 'content-type: application/json'
```

These shell examples assume `RECLASS_API_TOKEN` contains the configured or printed
token. Both `/api/rpc` and `/ws` require authentication before workspace access.
`POST /api/auth` uses the same authentication and origin checks and returns 204
without accessing the workspace; the UI uses it to distinguish rejection from an outage.
Native WebSocket clients can send the same Bearer header. Browser clients offer
the protocols `reclass` and `reclass-token.<token>`; the server selects only
`reclass`. Credentials in query strings and cookies are not accepted.

When an Origin header is present, it must exactly match a trusted origin.
Defaults are `http://<bound-address>:<port>` and, for loopback binds,
`http://localhost:<port>`. Wildcard binds add no default origins. Add trusted
UI origins with repeatable `--allowed-origin` arguments, including scheme and
port with no trailing slash. Other ports, `null`, and lookalike domains are
rejected. Native clients may omit Origin but still need the token. Host and
forwarded headers do not grant trust. Static UI files remain public and contain
no token or workspace data.

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


