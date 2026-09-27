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
- 32-bit (WOW64) processes work too: attaching reads the image's PE header and converts
  the project to 4-byte pointers, keeping every other field at its offset
  (`reclass pointer-size` shows or changes it)
- Values can be written into the process when the driver supports it (it reports the
  `MemoryWrite` feature; the user-mode driver does not). Double-click a value or press
  Enter on a selected field, type the new value and press Enter. With a read-only
  driver, editing is disabled and `write` calls fail with an error.
- `--decrypt-module <name>` sets the module holding XenuineDecrypt for encrypted pointers
  (defaults to the process image)
- The project file is the same `memory_structure.json` the egui app uses; the canvas is
  stored in an extra `web` section that the egui app ignores

For UI development run the server with `--demo --allowed-origin http://localhost:5173`
and `npm run dev` in `web/app`. Vite proxies `/ws` and `/api` to the server.
Use the exact origin shown by Vite if its hostname or port differs.

#### Command line

`reclass` drives a running server from a terminal, a script or a coding agent.
Its edits show up live in the browser.

```sh
cargo install --path cli        # or: cargo run -p reclass-cli -- <command>
export RECLASS_API_TOKEN=...    # the token the server printed
reclass status                  # attached process, root address, project
reclass view                    # the root class, decoded live
reclass view 0x1F3A8D12600 -s 0x100          # raw qwords with pointer/float hints
reclass define Player 0x1C f32 health        # type and name the bytes at an offset
reclass view '[$GWorld]+0x28' -c Player -f 1  # follow class pointers one level
```

To find where a value lives, scan for it and narrow the results as it changes:

```sh
reclass scan i32 100            # every address holding 100 (or: f32, text, bytes, …)
reclass next 93                 # keep those that now hold 93
reclass next decreased          # or compare with the last scan
reclass scan f32 unknown        # snapshot everything when the value is not shown
reclass results                 # addresses with their current values
reclass write 0x1F3A8D12460 i32 100   # set it, if the driver can write memory
```

Watches keep an eye on values: each is re-read every tick with a minute of
history, and can be frozen when the driver can write. A value the game computes
from others, like a health bar's fill ratio, snaps back when changed; a related
scan finds what it is computed from. It follows a watch while you play, dropping
addresses that change while the watch holds still and those that hold still
when it changes:

```sh
reclass watch add 0x1F3A8D12470 f32 -l bar  # or: right-click a field, "Watch value"
reclass watch                               # values, sparklines and ranges
reclass related bar TYPE                  # the type you guess the inputs have; then play
reclass results
reclass related stop
```

The web UI has both under *Watch* (`W`) and the scanner's *Changes with a watch*.

The driver cannot list memory regions, so scans find readable memory by probing:
a 32-bit process is covered whole, a 64-bit one through its modules and the heap
memory their pointers lead to. `--module` or `--range` narrows a scan, and
read-only parts of module images are skipped unless `--read-only` is given.
The web UI's *Scan* panel (`S`) runs the same scans and shows the one in progress,
whichever client started it.

`reclass --help` documents address expressions, field references
(`Player.health`, `Player+0x1C`) and the type syntax (`Player*`, `f32[4]`,
`enc Camera*`, …). Add `--json` to any command for the server's raw reply.
`RECLASS_URL` selects a server other than `http://127.0.0.1:7878`.

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
`mergeCard`, `toggleExpand`, `moveCards`, `setShare`, `save`, `load`, `newProject`,
`scan`, `scanNext`, `scanResults`, `scanClear`, `scanRelated`, `scanRelatedStop`, `write`,
`watches`, `watchAdd`, `watchUpdate`, `watchRemove`, `watchFreeze`, `watchSet`.
The WebSocket at `/ws` takes the same `{id, method, params}` messages and streams
`session`, `defs`, `watches` and `frame` updates.

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


