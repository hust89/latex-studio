# Milestone 1 — Status

**State: CLOSED.**

Date closed: 2026-10-14.

## Scope

Rust core + Flutter shell + one real TeX engine (Tectonic) + structured
diagnostics + PDF display, per the engineering brief §11 Milestone 1.

## Verified on two platforms

| Host                              | Parser | Real-engine | FFI contract |
|-----------------------------------|--------|-------------|--------------|
| aarch64-bionic (Termux/Android)   | 7/7    | 4/4         | 5/5          |
| x86_64-glibc (WSL2 / Ubuntu 24.04)| 7/7    | 4/4         | 5/5          |

Plus a C smoke test on each host: an external process `dlopen`s
`liblatex_ffi.so`, calls `latex_compile`, and receives valid JSON
describing a build whose PDF is confirmed on disk.

## Flutter shell verified end-to-end (Linux desktop)

- Opens a project from a path.
- Compiles it through the FFI on a worker isolate.
- Displays the resulting PDF in the right pane.
- Displays structured diagnostics with file:line in the left pane.
- Collapses to Diagnostics/PDF tabs below 900 px width.

Verified against:
- `corpus/valid-minimal` — success, no diagnostics, PDF renders.
- `corpus/broken-undefined` — one error, no PDF shown.
- Window resize — tab layout engages.

## What is implemented

### Rust

- `latex-model`         — Diagnostic, Severity, DiagnosticKind, BuildResult.
- `latex-project`       — canonicalized project root, entry discovery.
- `latex-engine-tectonic`
    - Structured log parsing: pdfTeX `! msg` + `l.NN`; Tectonic
      `error: file:line[:col]: msg`.
    - Per-invocation build directory `<out_parent>/build_<nanos>/` so a
      returned PDF cannot be a leftover. Enforced by test.
    - `NetworkPolicy` (Auto | OfflineOnly | AllowNetwork).
    - `kill_on_drop(true)` so a timeout does not orphan the child.
- `latex-build`         — thin orchestration.
- `latex-ffi`           — C ABI: `latex_compile`, `latex_free_string`.

### Flutter

- `lib/main.dart`       — FFI binding, worker-isolate compile, two-pane
  and tabbed layouts, path field, folder picker (via zenity), play button.

## Known limitations (carried forward)

1. **First-run Tectonic cache priming is minutes of silent network I/O.**
   Looks like a hang. The Flutter shell does not surface progress yet.
   Milestone 2 must add a visible "preparing TeX engine" state by
   parsing `note: downloading` lines from Tectonic's stdout (Tectonic
   0.17 writes progress to stdout, not stderr).

2. **Android compilation is deferred.** The engine calls
   `Command::new("tectonic")`; there is no such binary on Android and
   none is bundled in the APK. Bundling a per-ABI `tectonic` is a
   Milestone 2 packaging task. The Android APK will not compile
   documents until then.

3. **`file_picker` on Linux requires `zenity`.** Without it the folder
   icon silently does nothing under WSLg. The path text field is the
   fallback. Documented for users, not a defect.

4. **WSLg + `file_picker` + Flutter's GTK embedder interact poorly.**
   The folder icon behaves differently under WSLg than on native Linux.
   Not a product defect; a development-environment limitation.

5. **Log format coverage is partial.** Only the two highest-value error
   shapes are parsed. XeLaTeX, LuaLaTeX, and older pdfTeX variants are
   untested.

## Not implemented (Milestones 2–5)

Editor pane with syntax highlighting and completion, project management,
autosave, recovery, templates, multi-engine compilation, bibliography
passes, publishing, validation engine beyond log parsing, correction
engine, approval workflow, mathematical consistency checks, formal
verification. None of these are present.

## Outstanding defects

None known in the Rust core. The Flutter shell has no automated tests —
it was verified by manual smoke run only.

## Acceptance criteria for Milestone 1 (brief §11)

- [x] Project loading.
- [x] Real compilation using a supported engine.
- [x] Accurate diagnostic locations where the engine provides them.
- [x] Reproducible test results.
- [x] Flutter shell running on desktop.
- [ ] Flutter shell on Android — deferred, documented.
- [ ] VerbTeX benchmark — not attempted; needs the same projects run on both.
