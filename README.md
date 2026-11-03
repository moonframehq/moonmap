# Moonmap

A desktop map app on [Moonframe](https://moonframe.dev), laid out like Apple
Maps: a sidebar of places and recents, Maps' control stack down the map's
right edge, Look Around bottom-left and the weather bottom-right. The map is
Moonframe's `ui.map`, and the tiles come from whichever provider you pick.

```sh
just demo        # a sample library of public places around Cupertino
just run         # your own library, in ~/Library/Application Support/moonmap
just app         # a standalone release bundle in target/bundle
just screenshot target/screenshots/home.png home   # home, place, directions, styles, results, dark
just ci
```

## What it does

- **My Location**: CoreLocation at its best accuracy, with no distance
  filter. If you granted only an approximate location, it asks once for
  full accuracy. The blue dot breathes while the fix is live and turns grey
  when it only shows where you were last seen. Hover the location button
  to see the accuracy.

- **Search**: Nominatim (OpenStreetMap). It looks near the map first, nearest
  first, then tries a category word ("coffee" becomes cafe), then searches
  anywhere.
- **Places**: Pinned, Saved Places, Guides (your own collections), Routes,
  Visited Places (places you opened) and Recently Added, kept in
  `library.json`.
- **Siri Suggestions**: drive times from My Location to your first two pinned
  places, from OSRM at routing.openstreetmap.de.
- **Directions**: car, walk or bike, routed along streets by `ui.map`. You
  can save a route.
- **Map Styles**: the map button switches tile providers. Styles that need
  an API key take one in the picker. It is stored in the library file, in
  plain text.
- **Weather**: Open-Meteo temperature and US AQI at the map's center.

## Make it yours

The sparkles under the map controls open a dialog: say what should change,
optionally pick an element with Inspect, and a coding agent (Claude by default)
changes the app's files. It asks before each change, and the window reloads
with the result. Replies go under **Feedback & fixes**. It needs an agent with
an ACP mode, or Node for `npx`.

## Moonframe patch

The breathing halo is `ui.map`'s new `pulse` pin option, on the local
Moonframe branch `map-pin-pulse` in `../moonframe-map-pulse`. The root
`Cargo.toml` patches Moonframe to that path; drop the patch once the
option lands upstream.

## Limitations

- Sparkles in a packaged app: outside a repository the agent works in
  `~/.config/moonframe`, while Moonmap reads `~/.config/moonmap`. Use a debug
  build from this repository until `cwd` in `providers/acp` covers it.
- macOS gives a location only to an app bundle. `just run` and `just demo`
  run the debug build from `target/dev/Moonmap.app`. A bare `cargo run`
  shows a notice instead, and so does a terminal launch on first use: there
  the terminal, not Moonmap, gets the permission request. Launch with
  `open target/dev/Moonmap.app` the first time so macOS asks Moonmap.
  `MOONMAP_DEBUG_LOCATION=1` prints what CoreLocation reports.
- A Mac locates itself by Wi-Fi, usually to within 20 to 60 m. Maps.app
  reads the same service, so it isn't more precise; it may only have had
  a fix longer.
- The tiles are raster, so **3D** and the **compass** can't rotate or tilt
  anything. They are shown for parity and explain this in a tooltip.
- **Look Around** opens Mapillary's street-level imagery in your browser.
- CARTO's basemaps now return an "API key required" tile to every keyless
  request. Moonframe's `carto-*` providers have no key slot, so Moonmap
  leaves them out until Moonframe adds one.
- Apple's points of interest come from Apple's vector data. Here, what you
  see is whatever the chosen tile style draws.
