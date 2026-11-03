"""Capture Moonmap's actual GPUI frame with the demo library.

The library lives in a temporary folder; map tiles come from the network
(and Moonframe's tile cache), so a first capture can show tiles still
loading. Run it twice for a settled map.
"""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

VIEWS = ("home", "place", "directions", "styles", "results", "dark", "hidden")


def patch(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f"Update the capture harness: {old!r} not found once")
    return source.replace(old, new, 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("view", choices=VIEWS, default="home", nargs="?")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    binary = repo / "target/debug/moonmap"
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="moonmap-capture-") as directory:
        root = Path(directory)
        env = dict(os.environ, MOONMAP_DEMO="1", MOONMAP_DATA_DIR=str(root / "data"),
                   XDG_CONFIG_HOME=str(root / "config"), XDG_STATE_HOME=str(root / "state"))
        env.pop("MOONFRAME_CAPTURE", None)
        overlay = root / "overlay"
        overlay.mkdir()
        source = (repo / "crates/moonmap-app/app/main.luau").read_text()
        hook = "\t\tif msg.topic == \"provider/maps\" then\n"
        if args.view in ("place", "directions"):
            source = patch(source, hook, hook + f'''\t\t\tif not state.captured and msg.value and msg.value.saved then
\t\t\t\tstate.captured = true
\t\t\t\tstate.pending_directions = {'"demo:apple-park"' if args.view == "directions" else "nil"}
\t\t\t\treturn state, {{ send("open", {{ id = "demo:apple-park" }}) }}
\t\t\tend
''')
        if args.view == "results":
            source = patch(source, hook, hook + '''\t\t\tif not state.captured and msg.value and msg.value.saved then
\t\t\t\tstate.captured = true
\t\t\t\tstate.query = "coffee"
\t\t\t\treturn state, { send("search", { query = "coffee" }) }
\t\t\tend
''')
        if args.view == "dark":
            source = patch(source, hook, hook + '''\t\t\tif not state.captured then
\t\t\t\tstate.captured = true
\t\t\t\tstate.maps = msg.value or {}
\t\t\t\treturn state, { { to = "app:appearance", msg = "dark" }, send("style", { style = "openstreetmap" }) }
\t\t\tend
''')
        if args.view == "hidden":
            source = patch(source, hook, hook + '''\t\t\tif not state.captured and msg.value and msg.value.saved then
\t\t\t\tstate.captured = true
\t\t\t\treturn state, { send("sidebar", { open = false }) }
\t\t\tend
''')
        if args.view == "styles":
            source = patch(source, 'popup = nil, focus = false,', 'popup = "styles", focus = false,')
        (overlay / "main.luau").write_text(source)
        # Images resolve beside main.luau, which is now the overlay's.
        shutil.copytree(repo / "crates/moonmap-app/app/icons", overlay / "icons")
        env["MOONFRAME_CAPTURE"] = str(output)
        # Long enough for the provider, the routing and the tiles to arrive.
        env.setdefault("MOONFRAME_CAPTURE_DELAY", "3000")
        subprocess.run([binary, "--user", overlay], env=env, check=True, timeout=90)
    if not output.is_file():
        raise RuntimeError("No capture produced; build with --features capture")
    print(f"GPUI capture: {output}")


if __name__ == "__main__":
    main()
