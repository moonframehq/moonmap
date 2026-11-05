nightly_toolchain := "nightly-2026-06-20"

default:
    @just --list

# Runs the debug build from target/dev/Moonmap.app: macOS gives your
# location only to an app bundle. Luau still reloads from the source tree.
run *args: dev-bundle
    target/dev/Moonmap.app/Contents/MacOS/moonmap {{args}}

# Opens at Union Square on a sample library of public San Francisco places.
demo: dev-bundle
    MOONMAP_DEMO=1 target/dev/Moonmap.app/Contents/MacOS/moonmap

dev-bundle:
    cargo build --locked -p moonmap-app
    mkdir -p target/dev/Moonmap.app/Contents/MacOS
    cp crates/moonmap-app/Info.plist target/dev/Moonmap.app/Contents/Info.plist
    cp target/debug/moonmap target/dev/Moonmap.app/Contents/MacOS/moonmap
    if [ {{quote(os())}} = macos ]; then codesign --force --sign - target/dev/Moonmap.app 2>/dev/null; fi

# A standalone release bundle in target/bundle, with the location keys.
app:
    cargo build --locked -p moonmap-app --release
    target/release/moonmap bundle target/bundle
    /usr/libexec/PlistBuddy -c "Merge crates/moonmap-app/Info.plist" target/bundle/Moonmap.app/Contents/Info.plist
    codesign --force --deep --sign - target/bundle/Moonmap.app
    @echo "target/bundle/Moonmap.app"

build:
    cargo build --locked -p moonmap-app --release

check:
    cargo run --locked -p moonmap-app -- --check

# Capture the real GPUI frame with the demo library in an isolated folder.
# A process id as the view captures that running window instead: the sparkles'
# agent checks its change that way.
screenshot output="target/screenshots/moonmap.png" view="home":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ {{quote(view)}} =~ ^[0-9]+$ ]]; then
        exec scripts/window.sh {{quote(output)}} {{quote(view)}}
    fi
    cargo build --locked -p moonmap-app --features capture
    python3 scripts/capture.py {{quote(output)}} {{quote(view)}}

# Retake the README's screenshots at the demo's position. Each runs twice:
# the first run fills the tile cache. Needs ImageMagick.
screenshots:
    #!/usr/bin/env bash
    set -euo pipefail
    if ioreg -n Root -d1 -a | grep -A1 CGSSessionScreenIsLocked | grep -q true; then
        echo "The screen is locked: GPUI draws only its first frame." >&2
        exit 1
    fi
    cargo build --locked -p moonmap-app --features capture
    export MOONFRAME_CAPTURE_DELAY=9000
    for pass in 1 2; do
        for view in home place directions styles results look; do
            python3 scripts/capture.py "docs/screenshots/$view.png" "$view" --appearance light
        done
        python3 scripts/capture.py docs/screenshots/dark.png place --appearance dark
    done
    magick mogrify -resize 1800x -colors 256 docs/screenshots/*.png

# Opens the demo, drives its map on screen (zooming, pinching, dragging, then
# idle) and prints what each cost. Tiles are kept in target/benchmark, so the
# first run also fetches them: run it twice for figures that compare.
benchmark:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --locked -p moonmap-app --release --features benchmark
    root=target/benchmark
    rm -rf "$root/data" "$root/config" "$root/state"
    mkdir -p "$root/data" "$root/config" "$root/state" "$root/cache"
    MOONMAP_DEMO=1 \
    MOONMAP_DATA_DIR="$PWD/$root/data" \
    XDG_CONFIG_HOME="$PWD/$root/config" \
    XDG_STATE_HOME="$PWD/$root/state" \
    XDG_CACHE_HOME="$PWD/$root/cache" \
    MOONFRAME_MAP_BENCHMARK=1 \
    MOONFRAME_MAP_BENCHMARK_SINCE="$(python3 -c 'import time; print(time.time() * 1000)')" \
    RUST_LOG=off \
    target/release/moonmap

# Render the app icon, which Moonframe finds in the app folder.
icon:
    rsvg-convert -w 1024 -h 1024 assets/icon.svg -o crates/moonmap-app/app/icon.png

# Render the glyphs Moonframe's icon set lacks: blue for the sidebar,
# ink for the map controls in light and dark appearances.
icons:
    for name in assets/icons/*.svg; do \
        base=$(basename "$name" .svg); \
        for tone in blue:#0a84ff ink:#3c3c43 night:#e5e5ea white:#ffffff; do \
            sed "s/currentColor/${tone#*:}/g" "$name" | rsvg-convert -w 64 -h 64 -o "crates/moonmap-app/app/icons/$base-${tone%%:*}.png" || exit; \
        done; \
    done

format:
    cargo +{{nightly_toolchain}} fmt --all
    taplo fmt

format-check:
    cargo +{{nightly_toolchain}} fmt --all -- --check
    taplo fmt --check

ci: format-check
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo test --locked --workspace
