#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(uname -s)" != Darwin ]]; then
  echo "Packaging requires macOS and the Apple SDK." >&2
  exit 1
fi
swift build -c release --product Function
binary_dir="$(swift build -c release --show-bin-path)"
app_root="dist/Function.app"
python3 -c 'import shutil; shutil.rmtree("dist/Function.app", ignore_errors=True)'
mkdir -p "$app_root/Contents/MacOS" "$app_root/Contents/Resources"
cp "$binary_dir/Function" "$app_root/Contents/MacOS/Function"
cp assets/macos/Info.plist "$app_root/Contents/Info.plist"
cp assets/icon.png "$app_root/Contents/Resources/icon.png"
# SwiftPM's generated accessor searches alongside Bundle.main.bundleURL.
for resource_bundle in "$binary_dir"/Function_FunctionApp.bundle "$binary_dir"/Function_FunctionApp.resources; do
  if [[ -d "$resource_bundle" ]]; then cp -R "$resource_bundle" "$app_root/"; fi
done
icon_directory="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$icon_directory"
trap 'python3 -c '\''import shutil,sys; shutil.rmtree(sys.argv[1], ignore_errors=True)'\'' "${icon_directory%/AppIcon.iconset}"' EXIT
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/icon.png --out "$icon_directory/icon_${size}x${size}.png" >/dev/null
  retina_size=$((size * 2))
  sips -z "$retina_size" "$retina_size" assets/icon.png --out "$icon_directory/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$icon_directory" -o "$app_root/Contents/Resources/AppIcon.icns"
codesign --force --deep --options runtime --entitlements assets/macos/entitlements.plist --sign - "$app_root"
codesign --verify --deep --strict "$app_root"
echo "Built $app_root. Launch with: open dist/Function.app"
