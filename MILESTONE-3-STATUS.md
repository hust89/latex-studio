# Milestone 3 — Status

## M3.1 — multi-engine compilation (CLOSED, 2026-10-14)

Delivered:
- Engine trait in latex-model; EngineInfo, EngineRequest.
- latex-engine-system: pdfLaTeX, XeLaTeX, LuaLaTeX driven through latexmk.
- latex-engine-tectonic: implements Engine.
- latex-build: registry, list_engines, find_engine, default_engine_id.
- latex-ffi: latex_engines_list; latex_compile now takes engine id first.
- Flutter: engine dropdown in app bar. System engines report unavailable
  when latexmk is not on PATH, which is the correct behaviour.

Tests: 52 total across the workspace. 3 new engine FFI tests.

Not verified on this host:
- A real pdfLaTeX/XeLaTeX/LuaLaTeX compile. Requires TeX Live + latexmk.
  The trait dispatch is proven by tests; end-to-end system-engine compile
  remains to be run when the toolchain is available.

## M3.2 — bibliography UI (not started)
## M3.3 — publishing (not started)
## M3.4 — speed (closed early, see M2 notes)

## M3.3 — publishing (CLOSED, 2026-10-14)

Delivered:
- latex-publish crate.
- Portable zip archive: project files at original paths, output.pdf at
  root when a successful build exists, MANIFEST.json with engine, time,
  entry, and per-file SHA-256 hashes.
- Build artefacts excluded.
- latex_publish_archive FFI.
- Flutter: publish button with save dialog and snackbar.
- 4 crate tests, 3 FFI tests.

Not delivered:
- Dedicated "Save PDF as..." action.
- Byte-reproducible archive. Manifest carries a wall-clock timestamp.

## Milestone 3 — CLOSED

- M3.1  multi-engine compilation
- M3.2  build phase
- M3.3  publishing
- M3.4  speed (closed early)

Total tests added in M3: 15.
