#!/usr/bin/env bash
# Capture just the SAF Dashboard window (no desktop clutter) to a PNG.
#   scripts/screenshot-app.sh [output.png]
# Relies on Screen Recording permission for the controlling terminal, which is
# already required for screencapture to see other apps' windows.
set -Eeuo pipefail

OUT="${1:-/tmp/saf-shots/shot.png}"
mkdir -p "$(dirname "$OUT")"
# CGWindowOwnerName is the app's display name, not the binary name.
OWNER="SAF Dashboard"

# Bring the app forward so its window is on-screen and unobscured.
osascript -e 'tell application "SAF Dashboard" to activate' >/dev/null 2>&1 || true
sleep 0.8

# Find the largest on-screen window owned by the app via CoreGraphics.
WID="$(/usr/bin/swift - "$OWNER" <<'SWIFT' 2>/dev/null || true
import CoreGraphics
import Foundation
let owner = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "SAFDashboard"
guard let infos = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { exit(0) }
var best: (Int, CGFloat) = (-1, -1)
for w in infos {
    guard let name = w[kCGWindowOwnerName as String] as? String, name == owner,
          let num = w[kCGWindowNumber as String] as? Int,
          let b = w[kCGWindowBounds as String] as? [String: CGFloat] else { continue }
    let area = (b["Width"] ?? 0) * (b["Height"] ?? 0)
    if area > best.1 { best = (num, area) }
}
if best.0 >= 0 { print(best.0) }
SWIFT
)"

if [[ -n "${WID:-}" ]]; then
    screencapture -o -x -l"$WID" "$OUT"
else
    # Fallback: whole main display.
    screencapture -o -x "$OUT"
fi
echo "$OUT"
