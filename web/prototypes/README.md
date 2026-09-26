# Web frontend prototypes

Five clickable design directions for a browser UI for reclass-rs. They are plain HTML/JS with no build step and no dependencies.

```sh
cd web/prototypes
python -m http.server 8765   # then open http://localhost:8765
```

Opening `index.html` straight from disk also works (the clipboard may be blocked on `file://`).

| # | Name | Idea |
|---|------|------|
| 1 | Classic | ReClass.NET-style menubar, toolbar and dense monospace tree |
| 2 | Workbench | VS Code-style IDE with tabs, inspector, watch panel and command palette |
| 3 | Atlas | Light theme, drill-down navigation, hex dump with drag-to-define |
| 4 | Terminal | Keyboard-first TUI with vim keys and a `:` command line |
| 5 | Constellation | Node-graph canvas where pointers spawn linked instance cards |

## Shared code

- `shared/model.js` is a mock backend. It simulates an attached process with byte-level memory (values change live at 4 Hz), the class/enum registries, signatures, the root-address expression language (`[$GWorld] + 0x10`, `<module.dll>`, `[deref]`) and layout mutations (change type with hex padding, insert and remove bytes, rename). It mirrors the data model in `main/src/memory`, so it doubles as a sketch of the API a real backend would expose (for example over a WebSocket served by the Rust side).
- `shared/ui.js` holds unstyled helpers: context menu, the standard field menu, tree flattening, inline rename and change tracking. Each prototype styles them its own way.
