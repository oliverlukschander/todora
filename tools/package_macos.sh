#!/usr/bin/env bash
# Build Todora as a macOS app and wrap it in a disk image.
#
#   tools/package_macos.sh      ->  dist/Todora.app, dist/Todora.dmg
#
# Only what macOS ships with: QuickLook renders the icon from the SVG, sips
# scales it, iconutil packs it, diskutil (or hdiutil, before macOS 26) makes
# the image. The app is signed ad
# hoc — enough for this Mac; another will ask for right-click > Open once.
set -euo pipefail
cd "$(dirname "$0")/.."

NAME=Todora
BIN=todora
ID=com.lukschander.todora
VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
APP="dist/$NAME.app"
RES="$APP/Contents/Resources"

cargo build --release

rm -rf "$APP" dist/dmg "dist/$NAME.dmg" dist/AppIcon.iconset dist/todora.svg.png
mkdir -p "$APP/Contents/MacOS" "$RES"
cp "target/release/$BIN" "$APP/Contents/MacOS/$BIN"
cp -R assets "$RES/assets"

# The icon. iconutil wants exactly these ten files.
qlmanage -t -s 1024 -o dist art/icon/todora.svg >/dev/null 2>&1
mkdir -p dist/AppIcon.iconset
for pair in 16:icon_16x16 32:icon_16x16@2x 32:icon_32x32 64:icon_32x32@2x \
            128:icon_128x128 256:icon_128x128@2x 256:icon_256x256 \
            512:icon_256x256@2x 512:icon_512x512 1024:icon_512x512@2x; do
  size=${pair%%:*}; name=${pair##*:}
  sips -z "$size" "$size" dist/todora.svg.png --out "dist/AppIcon.iconset/$name.png" >/dev/null
done
iconutil -c icns dist/AppIcon.iconset -o "$RES/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>$ID</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>$BIN</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.racing-games</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP"

mkdir -p dist/dmg
cp -R "$APP" dist/dmg/
ln -s /Applications dist/dmg/Applications
# macOS 26 moved disk images under diskutil and deprecated hdiutil create;
# older systems only have the latter.
if diskutil image create from >/dev/null 2>&1 || [ "$(diskutil image create from 2>&1 | head -1)" != "" ] && diskutil image 2>&1 | /usr/bin/grep -q create; then
  diskutil image create from --format UDZO --volumeName "$NAME" dist/dmg "dist/$NAME.dmg" >/dev/null
else
  hdiutil create -volname "$NAME" -srcfolder dist/dmg -ov -format UDZO "dist/$NAME.dmg" >/dev/null
fi
rm -rf dist/dmg dist/AppIcon.iconset dist/todora.svg.png

echo "built $APP and dist/$NAME.dmg ($(du -h "dist/$NAME.dmg" | cut -f1))"
