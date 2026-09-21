#!/usr/bin/env bash
# Build Todora as a Linux app directory, and optionally install it for this user
# so Omarchy's launcher can find it.
#
#   tools/package_linux.sh            -> dist/Todora/
#   tools/package_linux.sh --install  -> also ~/.local/share/todora, a desktop
#                                        entry, and an icon
set -euo pipefail
cd "$(dirname "$0")/.."

INSTALL=0
for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=1 ;;
    *)
      echo "usage: tools/package_linux.sh [--install]" >&2
      exit 2
      ;;
  esac
done

if [[ "$(uname -s)" != Linux ]]; then
  echo "Build the Linux package on Linux (a Linux container also works)." >&2
  exit 1
fi

NAME=Todora
BIN=todora
ID=com.lukschander.todora
OUT="dist/$NAME"
ICON_SRC=art/icon/todora.svg
ICON_PNG="$OUT/todora.png"

# Rust can come from rustup, the distribution, or a container image.
if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then
  . "$HOME/.cargo/env"
fi
command -v rsvg-convert >/dev/null 2>&1 || {
  echo "Install rsvg-convert first (librsvg2-bin on Debian/Ubuntu; librsvg on Arch)." >&2
  exit 1
}

cargo build --release --locked

rm -rf "$OUT"
mkdir -p "$OUT"
cp "${CARGO_TARGET_DIR:-target}/release/$BIN" "$OUT/$BIN"
cp -R assets "$OUT/assets"
rsvg-convert -w 512 -h 512 "$ICON_SRC" -o "$ICON_PNG"

cat > "$OUT/$BIN.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Version=1.5
Name=$NAME
Comment=A 3rd-person arcade racer around forty scaled real circuits
Exec="$PWD/$OUT/$BIN"
Path=$PWD/$OUT
Icon=$PWD/$ICON_PNG
Terminal=false
Categories=Game;SportsGame;
StartupNotify=true
StartupWMClass=$BIN
X-Desktop-File-Install-Version=0.1
DESKTOP

echo "built $OUT ($(du -sh "$OUT" | cut -f1))"

if [[ "$INSTALL" -eq 1 ]]; then
  SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"
  APPDIR="$SHARE/todora"
  APP_DESKTOP="$SHARE/applications/${ID}.desktop"
  ICON_DIR="$SHARE/icons/hicolor/512x512/apps"
  mkdir -p "$APPDIR" "$SHARE/applications" "$ICON_DIR"
  cp "$OUT/$BIN" "$APPDIR/$BIN"
  rm -rf "$APPDIR/assets"
  cp -R "$OUT/assets" "$APPDIR/assets"
  cp "$ICON_PNG" "$ICON_DIR/todora.png"
  cat > "$APP_DESKTOP" <<DESKTOP
[Desktop Entry]
Type=Application
Version=1.5
Name=$NAME
Comment=A 3rd-person arcade racer around forty scaled real circuits
Exec="$APPDIR/$BIN"
Path=$APPDIR
Icon=todora
Terminal=false
Categories=Game;SportsGame;
StartupNotify=true
StartupWMClass=$BIN
DESKTOP
  update-desktop-database "$SHARE/applications" >/dev/null 2>&1 || true
  gtk-update-icon-cache -f -t "$SHARE/icons/hicolor" >/dev/null 2>&1 || true
  echo "installed $APPDIR and $APP_DESKTOP"
fi
