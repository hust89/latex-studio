
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

## M2.3 — autosave and recovery (CLOSED, 2026-10-14)

Delivered:
- Atomic writes (temp + fsync + rename); no temp files leak on success.
- Autosave mirror at .latex-studio-build/autosave/<rel>, cleared on real save.
- Debounced autosave, 2 s idle.
- Save-on-file-switch and save-on-compile.
- Recovery dialog on project load listing each recoverable file with its
  autosave time; Recover / Discard.
- Path sanitization: .. and absolute paths rejected.
- Rust: 5 unit tests + 5 FFI tests.

Verification: manual smoke run on Linux desktop. Individual edge cases
not individually exercised; see M4 test plan when formal verification of
the correction workflow is added.

Not delivered:
- Versioned history (multiple restore points). Single rolling autosave
  per file. Recommended for v1.5, not v1.0.
- No automated Flutter tests.

## M2.4 — templates (not started)
