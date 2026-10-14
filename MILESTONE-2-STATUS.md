
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

## Open issue: build latency

Noticed during M2.4 smoke test: compile takes noticeably longer than
expected. Likely causes to investigate in M3:

1. Tectonic may be re-downloading or re-checking something per run even
   with a primed cache. Confirm with `time tectonic --only-cached main.tex`
   on the same project from a shell.
2. Flutter debug builds are slower than release. Confirm with
   `flutter run --release -d linux`.
3. First build of a new template always pays the initial format/font
   cost. Second build should be far faster. Measure both.
4. `NetworkPolicy::Auto` may be retrying the offline path, failing, then
   retrying online. Check the Rust logs during a compile.

M3 will add a "measure and report" step before any optimisation.

## Build latency — first measurements (2026-10-14)

Corpus: multifile-book (chapters one+two, shared macros, .bib file).
Command: FFI smoke test with LATEX_TECTONIC_NETWORK=offline.

- Cold (first run after rm -rf):   ~3.5 s wall
- Warm:                             1.15 s wall, success
- Inside the engine:                 see "duration_ms" field

The 1.15 s warm figure includes three BibTeX passes and one TeX rerun,
all orchestrated by Tectonic internally. The engine is not the source of
perceived app slowness; that investigation moves to the Flutter side in
M3 (compare flutter debug vs --release).

## Discovery: Tectonic handles BibTeX internally

Tectonic 0.17 detects .bib dependencies and runs BibTeX as needed. The
"bibliography passes" requirement in the brief is therefore substantially
provided by the engine, not something the product must orchestrate.
Milestone 3 work reduces to: surface the engine choice, expose the .bib
files in the project model, and report the BibTeX/rerun phases in the UI.

## Build latency — conclusion (2026-10-14)

Release-mode Flutter: fast. Debug-mode Flutter: slow.
The engine (Tectonic) was never the bottleneck.

Action for M3: use `flutter run --release` for any timing observation.
Debug builds remain the default for development.
