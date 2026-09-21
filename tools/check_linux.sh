#!/usr/bin/env bash
# Requires a binary built with --features visual-check and an active X11 or
# Wayland display. CI supplies Xvfb; a desktop can use its existing display.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
BINARY=$(realpath "${1:-${CARGO_TARGET_DIR:-target}/debug/todora}")
OUT="$ROOT/dist/linux-check/${2:-x11}"
mkdir -p "$OUT"

# Launch outside the checkout, from a package path containing spaces, with
# assets beside the executable. This exercises the Linux asset lookup.
PACKAGE=$(mktemp -d -t 'Todora Linux.XXXXXXXX')
trap 'rm -rf "$PACKAGE"' EXIT
cp "$BINARY" "$PACKAGE/todora"
cp -R assets "$PACKAGE/assets"
if [[ -z "${XDG_RUNTIME_DIR:-}" ]]; then
  export XDG_RUNTIME_DIR="$PACKAGE/runtime"
  mkdir -m 700 "$XDG_RUNTIME_DIR"
fi
unset BEVY_ASSET_ROOT CARGO_MANIFEST_DIR
cd /tmp

for circuit in suzuka monza monaco; do
  rm -f "$OUT/$circuit.png"
  TODORA_CAPTURE="$OUT/$circuit.png" TODORA_CIRCUIT="$circuit" TODORA_SMALL=1 \
    timeout 180 "$PACKAGE/todora" > "$OUT/$circuit.log" 2>&1
  python3 - "$OUT/$circuit.png" "$OUT/$circuit.log" <<'PY'
import pathlib
import sys
from PIL import Image, ImageStat

picture = Image.open(sys.argv[1]).convert("RGB")
assert picture.size == (800, 600), picture.size
# Fail for blank frames, including cases where a window opens but no scene
# renders. Keep the screenshots for human review of the road and HUD.
assert sum(ImageStat.Stat(picture).var) > 100, "blank capture"
assert len(picture.getcolors(picture.width * picture.height)) > 100, "empty scene"
log = pathlib.Path(sys.argv[2]).read_text()
assert "ERROR" not in log and "panicked" not in log, log
print(f"Rendered {pathlib.Path(sys.argv[1]).stem}: {picture.size}")
PY
done
