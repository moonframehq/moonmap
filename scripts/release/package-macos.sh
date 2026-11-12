#!/usr/bin/env bash
# Packages the macOS release in OUT_DIR from one executable per
# architecture: a universal Moonmap.app (`moonmap bundle`, with Moonmap's
# location keys merged into its Info.plist, signed ad hoc), Moonmap.app.tar
# for the signing job, and the disk image users download,
# Moonmap-VERSION-universal-apple-darwin.dmg. Developer ID signing and
# notarization happen later, in scripts/release/sign-macos.sh.
set -euo pipefail
[[ $# -ge 2 ]] || { echo "usage: package-macos.sh OUT_DIR BINARY..." >&2; exit 2; }
[[ $(uname -s) == Darwin ]] || { echo "macOS required" >&2; exit 1; }
out=$1
shift
version=$(scripts/release/version.sh)
mkdir -p "$out"
out=$(cd "$out" && pwd)

work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
lipo -create "$@" -output "$work/moonmap"
lipo -info "$work/moonmap"

# The bundle is made from the running binary, so it is the universal one.
"$work/moonmap" bundle "$work/bundle" >/dev/null
app="$work/bundle/Moonmap.app"
plist="$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Merge crates/moonmap-app/Info.plist" "$plist" >/dev/null
[[ $(plutil -extract CFBundleShortVersionString raw "$plist") == "$version" ]] \
    || { echo "the bundle's version is not Cargo.toml's $version" >&2; exit 1; }
plutil -extract NSLocationWhenInUseUsageDescription raw "$plist" >/dev/null
codesign --force --deep --sign - "$app"
codesign --verify --strict "$app"
# The bundled executable must load every module of its app, headlessly, on
# each architecture this Mac can run.
for arch in $(lipo -archs "$app/Contents/MacOS/moonmap"); do
    if arch "-$arch" /usr/bin/true 2>/dev/null; then
        arch "-$arch" "$app/Contents/MacOS/moonmap" --check --no-user
    else
        echo "skipping the $arch check: this Mac cannot run it" >&2
    fi
done

tar -C "$work/bundle" -cf "$out/Moonmap.app.tar" Moonmap.app
scripts/release/make-dmg.sh "$app" "$out/Moonmap-$version-universal-apple-darwin.dmg"
