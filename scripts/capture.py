"""Capture Moonmap's actual GPUI frame with the demo library.

The library lives in a temporary folder; map tiles come from the network
(and Moonframe's tile cache), so a first capture can show tiles still
loading. Run it twice for a settled map.
"""

import argparse
import os
from pathlib import Path
import shutil
import json
import subprocess
import urllib.request
import tempfile

VIEWS = ("home", "place", "directions", "styles", "results", "dark", "hidden", "look", "loading")


def patch(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f"Update the capture harness: {old!r} not found once")
    return source.replace(old, new, 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("view", choices=VIEWS, default="home", nargs="?")
    parser.add_argument("--appearance", choices=("light", "dark"), help="default: the system's")
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
        if args.view in ("place", "directions", "loading"):
            source = patch(source, hook, hook + f'''\t\t\tif not state.captured and msg.value and msg.value.saved then
\t\t\t\tstate.captured = true
\t\t\t\tstate.pending_directions = {'"demo:palace"' if args.view == "directions" else "nil"}
\t\t\t\treturn state, {{ send("open", {{ id = "demo:palace" }}) }}
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
        if args.view == "loading":
            # The sheet's first frame, before anything has arrived.
            source = patch(source, "local info = place.details or {}", "local info = {}; place.gathering = true")
        if args.view == "look":
            # A real panorama, so the viewer is captured with an image.
            found = json.load(urllib.request.urlopen(urllib.request.Request(
                "https://api.panoramax.xyz/api/search?place_position=2.3376,48.8606&place_distance=0-150&limit=30",
                headers={"User-Agent": "Moonmap capture"}), timeout=30))
            shot = next(f for f in found["features"] if f["properties"]["pers:interior_orientation"].get("field_of_view") == 360)
            lon, lat = shot["geometry"]["coordinates"]
            look = (f'{{ id = "{shot["id"]}", lat = {lat}, lon = {lon}, heading = {shot["properties"]["view:azimuth"]}, '
                    f'url = "{shot["assets"]["sd"]["href"]}", hd = "{shot["assets"]["sd"]["href"]}", credit = "Panoramax contributors" }}')
            source = patch(source, 'popup = nil, focus = false,', f'popup = nil, focus = false, look = {look},')
        if args.view == "styles":
            source = patch(source, 'popup = nil, focus = false,', 'popup = "styles", focus = false,')
        if args.appearance:
            source = patch(source, hook, hook + f'''\t\t\tif not state.appearance then
\t\t\t\tstate.appearance = true
\t\t\t\tstate.maps = msg.value or {{}}
\t\t\t\treturn state, {{ {{ to = "app:appearance", msg = "{args.appearance}" }} }}
\t\t\tend
''')
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
