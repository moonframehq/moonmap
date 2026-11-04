# Moonmap engineering guide

Moonmap is a Moonframe app modeled on Apple Maps, with switchable tile providers.

## Architecture

- `moonmap-core`: places, the library (atomic JSON file), the map-style
  catalog, and parsers for Nominatim, OSRM and Open-Meteo answers. No UI,
  network or executor dependencies.
- `moonmap-app`: the binary, the `maps` native provider (`src/provider.rs`),
  short-lived HTTP worker threads (`src/net.rs`) and the Luau UI (`app/`).
- The provider acts only on place ids it has published. Views never send
  whole places.
- `ui.map` draws its own zoom buttons and credit. The shell overhangs the map
  past both clipped edges and draws Maps-style controls and the credit
  itself. Keep the overhang symmetric so the map's center is the area's.

- `src/location.rs` is the only unsafe code: CoreLocation calls, made on
  the main thread (whose run loop CoreLocation needs). It is never started
  outside an `.app`, where CoreLocation throws.
- `src/street_view.rs` draws Look Around itself, so that it is the same on
  macOS and Linux: `ui.browser` does not embed on Linux, and MapKit is macOS
  only.

## Rules

- Same Rust conventions as moonmail: workspace-level versions and lints,
  `thiserror` in libraries, no production `unwrap`/`expect`/`unsafe`.
- Network work never runs on the provider's tick or the UI thread.
- Respect the services' usage policies: identify with the user agent, keep
  Nominatim to about one request a second, and show the tile provider's
  attribution.
- `just icons` renders `assets/icons/*.svg` into `app/icons` in four tones.
  Re-run it after editing a glyph.
- `just icon` renders `assets/icon.svg` into `app/icon.png`, the app icon.
  Keep it out of `assets/icons/`, which holds only glyphs.
- Run `just format`, `just ci` and `just check` before committing.
- A request from the sparkles dialog is checked with `just check`, not
  Moonframe's `just types` or `check-examples`. The Moonframe skills are in
  the Moonframe checkout. `just screenshot <out.png> <pid>` shows the
  user's window.
