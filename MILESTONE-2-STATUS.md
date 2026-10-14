
## M2.2a — project index over FFI (CLOSED, 2026-10-14)

- `latex_project_index(root) -> {files:[{path,kind}], entry:str|null}`
- 4 new tests in `crates/latex-ffi/tests/project_index_ffi.rs`
- Whole crate: 9 tests green

## M2.2b — editor pane (CLOSED, 2026-10-14)

Delivered:
- File tree sidebar sourced from the project index, grouped by directory,
  entry marked with a star.
- Code editor (`flutter_code_editor` + `highlight` tex grammar) with line
  numbers, LaTeX syntax highlighting.
- Multi-file switching: click a file in the tree to open it.
- Save via Ctrl+S and toolbar button; save-on-compile so the engine sees
  the latest edit.
- Dirty indicator in the window title.
- Three responsive layouts:
    >= 1100 px:  tree | editor/PDF tabs | diagnostics
    700-1099:    tree | editor/PDF tabs, diagnostics strip below
    < 700:       four tabs (Files/Editor/PDF/Problems)

Verified manually on Linux desktop (WSLg, Flutter 3.47.7, Dart 3.13.5)
against `corpus/multifile-book`.

Not delivered (carried to M2.3 / M2.4):
- Autosave and crash recovery.
- No automated tests for the Flutter layer — manual smoke run only.
- Editor does not yet jump to a diagnostic's file:line when one is clicked.

## M2.3 — autosave and recovery (not started)
## M2.4 — templates (not started)
