#!/usr/bin/env bash
# Populate the Tectonic cache completely. Safe to re-run; Tectonic skips files
# it already has. Run under nohup so an interrupted session does not kill it:
#
#     nohup scripts/prime-tectonic.sh > prime.log 2>&1 &
#     tail -f prime.log
#
set -uo pipefail   # no -e: we want to reach the verification step even on failure

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/main.tex" <<'TEX'
\documentclass{article}
\begin{document}
Priming the Tectonic cache.
\end{document}
TEX

echo "TMPDIR:     ${TMPDIR:-/tmp}"
echo "Cache dir:  ${TECTONIC_CACHE_DIR:-$HOME/.cache/Tectonic}"
echo "Started:    $(date)"
echo
echo "===== priming pass (network enabled, no timeout) ====="
time tectonic --outdir "$TMP" "$TMP/main.tex"
rc=$?
echo
echo "===== priming exit code: $rc ====="
echo "Finished:   $(date)"
echo
if [[ $rc -eq 0 && -f "$TMP/main.pdf" ]]; then
  echo "OK: cache primed, PDF produced at $TMP/main.pdf"
  echo
  echo "===== verification pass (--only-cached) ====="
  time tectonic --only-cached --outdir "$TMP" "$TMP/main.tex"
else
  echo "FAIL: priming did not complete. Re-run this script; Tectonic will"
  echo "      resume from wherever it got to."
fi
