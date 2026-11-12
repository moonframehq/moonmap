#!/usr/bin/env bash
# Packages a Linux or Windows release in OUT_DIR from BINARY, a release
# build, through `moonmap bundle`:
#
# - Linux: Moonmap-VERSION-TARGET.tar.gz, the bundle's prefix (bin/, share/
#   with the app, its icon and .desktop file) to copy under /usr/local or
#   ~/.local.
# - Windows: Moonmap-VERSION-TARGET.zip, one folder holding moonmap.exe with
#   its app/ and toolbox/ beside it, where the executable looks for them.
#
# Each also carries the license and readme.
set -euo pipefail
[[ $# == 3 ]] || { echo "usage: package-archive.sh TARGET BINARY OUT_DIR" >&2; exit 2; }
target=$1
binary=$2
out=$3
version=$(scripts/release/version.sh)
name="Moonmap-$version-$target"
exe=$(basename "$binary")
mkdir -p "$out"
out=$(cd "$out" && pwd)

work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
"$binary" bundle "$work/bundle" >/dev/null
root="$work/$name"
case $target in
*-windows-*)
    # The bundle is laid out as on Linux; Windows keeps it flat.
    mkdir -p "$root"
    cp "$work/bundle/Moonmap/bin/$exe" "$root/"
    cp -R "$work/bundle/Moonmap/share/$exe/app" "$work/bundle/Moonmap/share/$exe/toolbox" "$root/"
    ;;
*)
    mv "$work/bundle/Moonmap" "$root"
    ;;
esac
cp LICENSE README.md "$root/"

# The packaged executable must find its app and load every module of it,
# headlessly.
case $target in
*-windows-*) "$root/$exe" --check --no-user ;;
*) "$root/bin/$exe" --check --no-user ;;
esac

case $target in
*-windows-*)
    # Git Bash on the Windows runners has Python but no zip.
    python=$(command -v python3 || command -v python)
    (cd "$work" && "$python" -c 'import shutil, sys; shutil.make_archive(sys.argv[1], "zip", ".", sys.argv[2])' "$out/$name" "$name")
    echo "$out/$name.zip"
    ;;
*)
    tar -C "$work" -czf "$out/$name.tar.gz" "$name"
    echo "$out/$name.tar.gz"
    ;;
esac
