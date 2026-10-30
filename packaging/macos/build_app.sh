#!/bin/sh
# Build a self-contained codwawzpc.app for Apple Silicon macOS.
#
# Usage: packaging/macos/build_app.sh [output-dir] [icon]
#   output-dir  where codwawzpc.app is written (default: target/macos)
#   icon        field-radio, mystery-box, undead or wunderwaffe (default: undead)
#
# Needs Homebrew's rustup, cmake, boost and uv. The bundle carries its own
# Python with Tk and Pillow; FFmpeg (intro movie, clip capture) is taken from
# Homebrew at runtime when installed. Saves stay in ~/.local/share/codwawzpc,
# so replacing the app keeps them.
set -eu
src=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
out=${1:-"$src/target/macos"}
icon=${2:-undead}
app="$out/codwawzpc.app"
contents="$app/Contents"
python_version=3.13

export PATH="/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:$PATH"
cd "$src"
cargo build --release --locked --bin codwawzpc

rm -rf "$app"
mkdir -p "$contents/MacOS" "$contents/Resources"

cp target/release/codwawzpc "$contents/MacOS/codwawzpc"
cp packaging/macos/Launch "$contents/MacOS/Launch"
chmod +x "$contents/MacOS/Launch"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$contents/Info.plist"

# Runtime resources and the launcher, at the paths the host looks for.
res="$contents/Resources"
cp -R touchHLE_dylibs touchHLE_fonts "$res/"
cp touchHLE_default_options.txt LICENSE NOTICE.md README.md OPTIONS_HELP.txt "$res/"
mkdir "$res/launcher"
cp launcher/*.py "$res/launcher/"
cp -R launcher/assets "$res/launcher/assets"

# A relocatable CPython (python-build-standalone, Tk included) and Pillow.
uv python install "$python_version"
python_home=$(dirname "$(dirname "$(uv python find "$python_version" --managed-python)")")
cp -R "$python_home/" "$res/python"
rm -rf "$res/python/lib/python$python_version/test" "$res/python/lib/python$python_version/idlelib"
uv pip install --quiet --python "$res/python/bin/python$python_version" \
    --target "$res/site-packages" -r launcher/requirements.txt

# App icon from the launcher's artwork.
iconset=$(mktemp -d)/AppIcon.iconset
mkdir -p "$iconset"
for size in 16 32 128 256; do
    sips -z $size $size "launcher/assets/app-icon-$icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double "launcher/assets/app-icon-$icon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$res/AppIcon.icns"
rm -rf "$(dirname "$iconset")"

find "$app" -name __pycache__ -type d -prune -exec rm -rf {} +
# Ad-hoc signature so Apple Silicon runs the copied binaries.
codesign --force --deep --sign - "$app"
echo "Built $app"
