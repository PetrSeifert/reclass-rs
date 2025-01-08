## reclass-rs

Modern, open-source ReClass-style memory exploration tool for Windows, written in Rust. It lets you attach to a process, define class layouts, visualize live memory, and iteratively build complex structures with a fast, stable GUI.

### Highlights

- **Windows GUI** built with eframe/egui
- **Attach to process** and browse loaded modules
- **Interactive memory tree** with live value reading
- **Reusable class definitions** with a side panel and double‑click to set root
- **Inline editing** of class names, field names, and root address
- **Context menu actions** on fields: add/insert hex bytes, remove, change type, create class from field, copy address/value
- **Robust structure editing** with stable IDs, cycle prevention, and commit on Enter or blur
- **Save/Load** memory structures as JSON (IDs are normalized on load)
- **Delete unused** definitions via reachability from the root

### Safety and scope

This project reads memory of other processes. Use only in environments where you have permission.

## Getting started

### Requirements

- Rust (stable)
- Windows 10/11 (x64)
- A working driver interface via `vtd-libum` (the included handle crate expects it)

### Build

```
cargo build --release
```

### Run

```
cargo run --release -p re-class
```

## Using the app

### Attach to a process

- Launch the app and click “Attach to Process”.
- Select a process (filter supported) and attach. Optionally open “Modules” to view module list.

### Class definitions panel (left)

- Lists all registered definitions as buttons.
- Double‑click a definition to set it as the root class.
- Right‑click a definition to rename it (rename is validated; references update).
- “Delete unused” removes all definitions that are not reachable from any other definition and have default layout

### Memory structure panel (center)

- Top bar: New / Save / Load JSON.
- Root header shows: root class name, base address, and total size.
  - Inline edit class name and base address; confirm with Enter or by clicking away.
  - Changes that affect structure trigger a deferred rebuild to keep the UI stable.

### Interactive tree

- Each field row shows: address, name, type, size, and for primitives a live value.
- ClassInstance fields are collapsible; expansion state remains stable across edits.

### Inline editing

- Click a field name or class name to edit; press Enter or click away to commit.
- Invalid or conflicting edits revert gracefully.

### Context menu on fields (right‑click)

- Add Hex64 at end / Insert Hex64 here
- Remove field (disabled if it’s the last field)
- Change type (switch between hex, integers, bool, float/double, text pointer, class instance)
- Create class from field (generates a unique class and converts the field to a class instance)
- Copy address / Copy value

### Changing a ClassInstance’s target class

- Inside a ClassInstance header, use the dropdown to pick an existing class.
- Cycles are prevented; invalid selections show an error dialog and are rejected.

### Save / Load

- Save: writes current structure to JSON.
- Load: reads JSON, normalizes definition/field IDs to avoid collisions, rebuilds nested instances, and applies as current structure.

## Architecture

- `main/` binary (`re-class`): egui/eframe UI, app state, process interactions
  - `re_class_app/ui.rs`: GUI and interactions
  - `re_class_app/app.rs`: process handle & memory structure orchestration
- `handle/`: process handle and memory access via `vtd-libum`
- `main/src/memory/`: core model types and logic
  - `types.rs`: `FieldType`
  - `definitions.rs`: `FieldDefinition`, `ClassDefinition`, `ClassDefinitionRegistry`
  - `nodes.rs`: `MemoryField`, `ClassInstance`, `MemoryStructure`

### Key design choices

- **Stable IDs**: `ClassDefinition` and `FieldDefinition` carry unique IDs. UI anchors and re-binding use these to prevent collapse or cross-link issues during edits.
- **Deferred rebuilds**: Structural mutations set a `needs_rebuild` flag; rebuild is executed once per frame to avoid inconsistent states.
- **Cycle prevention**: Changing a ClassInstance’s definition is checked for cycles via DFS before applying.
- **Normalization on load**: Loaded structures normalize IDs and reseed global counters so new fields/classes use fresh IDs.

## Tests

`cargo test` runs a comprehensive suite validating:

- FieldType sizing and formatting
- ClassDefinition offset/size recomputation
- MemoryStructure rebuild semantics (root change, root rebuild, address change)
- Rename propagation and cycle detection
- JSON load → normalize IDs → convert hex to class instance flow

## Shortcuts and tips

- Double‑click a class in the side panel to set it as root
- Enter or click away to commit inline edits
- Use right‑click on fields for quick structure edits

## Limitations

- Windows‑only (depends on Windows driver stack)
- Reading complex pointer chains/text beyond the basic helpers may require extending the handle layer

## License

MIT


