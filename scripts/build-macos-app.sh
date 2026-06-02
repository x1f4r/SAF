#!/usr/bin/env bash
# Build the SAF Dashboard macOS app from the Swift package, assemble a signed
# .app bundle, and (optionally) install it to /Applications.
#
#   scripts/build-macos-app.sh [--install] [--run] [--debug]
#
# No third-party tools required: swift, sips, iconutil, codesign ship with Xcode.
set -Eeuo pipefail

REPO_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
PKG_DIR="$REPO_DIR/SAFDashboard"
APP_NAME="SAF Dashboard"
BUNDLE_ID="com.x1f4r.safdashboard"
ICON_SRC="$REPO_DIR/assets/saf-icon.png"

CONFIG="release"
DO_INSTALL=0
DO_RUN=0
for arg in "$@"; do
    case "$arg" in
        --install) DO_INSTALL=1 ;;
        --run) DO_RUN=1 ;;
        --debug) CONFIG="debug" ;;
        *) echo "unknown flag: $arg" >&2; exit 2 ;;
    esac
done

echo "==> swift build ($CONFIG)"
( cd "$PKG_DIR" && swift build -c "$CONFIG" )
BIN_PATH="$( cd "$PKG_DIR" && swift build -c "$CONFIG" --show-bin-path )"
EXE="$BIN_PATH/SAFDashboard"
[[ -x "$EXE" ]] || { echo "build did not produce $EXE" >&2; exit 1; }

STAGE="$REPO_DIR/SAFDashboard/.build/bundle"
APP="$STAGE/$APP_NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$EXE" "$APP/Contents/MacOS/SAFDashboard"
# Copy any SPM-generated resource bundles next to the executable so
# Bundle.module resolves inside the app bundle.
shopt -s nullglob
for b in "$BIN_PATH"/*.bundle; do
    cp -R "$b" "$APP/Contents/MacOS/"
done
shopt -u nullglob

# Bundle the web dashboard's Docker build context so the app can launch the
# self-hosted web server with `docker compose`.
if [[ -d "$REPO_DIR/web" ]] && command -v rsync >/dev/null 2>&1; then
    echo "==> bundling web/ (Docker context)"
    mkdir -p "$APP/Contents/Resources/web"
    rsync -a --delete \
        --exclude node_modules --exclude dist --exclude .build --exclude .git \
        --exclude '.env' --exclude '*.log' \
        "$REPO_DIR/web/" "$APP/Contents/Resources/web/"
fi

echo "==> generating AppIcon.icns"
if [[ -f "$ICON_SRC" ]]; then
    ICONSET="$STAGE/AppIcon.iconset"
    rm -rf "$ICONSET"; mkdir -p "$ICONSET"
    for size in 16 32 64 128 256 512; do
        sips -z "$size" "$size"     "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}.png"   >/dev/null
        sips -z $((size*2)) $((size*2)) "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
    done
    iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
    rm -rf "$ICONSET"
fi

VERSION="$(cd "$REPO_DIR" && git rev-list --count HEAD 2>/dev/null || echo 1)"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$APP_NAME</string>
    <key>CFBundleDisplayName</key><string>$APP_NAME</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleExecutable</key><string>SAFDashboard</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0.$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.finance</string>
</dict>
</plist>
PLIST

echo "==> ad-hoc codesign"
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || \
    codesign --force --sign - "$APP" >/dev/null 2>&1 || true

echo "==> built: $APP"

if [[ "$DO_INSTALL" == "1" ]]; then
    DEST="/Applications/$APP_NAME.app"
    echo "==> installing to $DEST"
    # Quit a running instance so the copy is not blocked.
    osascript -e "tell application \"$APP_NAME\" to quit" >/dev/null 2>&1 || true
    pkill -f "/Applications/$APP_NAME.app/Contents/MacOS/SAFDashboard" 2>/dev/null || true
    sleep 1
    rm -rf "$DEST"
    cp -R "$APP" "$DEST"
    echo "installed: $DEST"
    APP="$DEST"
fi

if [[ "$DO_RUN" == "1" ]]; then
    echo "==> launching"
    open "$APP"
fi
