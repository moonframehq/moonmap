# How Apple Maps zooms, and what Moonmap took from it

Working notes. Not meant to be committed.

Source: MapKit, VectorKit and GeoServices, extracted from this Mac's dyld
shared cache (macOS on Darwin 27.0.0, arm64e). MapKit sha256
`dfc39ab06458bbc0e283f7a43784b42752c74f29f8bd1e179b47c7eaf5a35bd0`. Maps.app
itself holds none of this: it lives in the frameworks.

Each finding is marked:

- **Static**: read from disassembly or decompilation. Shows what the code says,
  not that it was seen running.
- **Runtime**: read from the frameworks loaded in a small probe process.
- **Measured**: timed here.

REA evidence ids for the Hopper pseudocode: `ev_71d546d2…` (`handlePinch:`),
`ev_b4e38a61…` (`handleDoubleClick:`), `ev_a926d709…` (`smartMagnifyWithEvent:`),
`ev_6288a172…` (`zoomIn:`). Selector names and constants were resolved by
decoding the shared cache's stubs and constant pools.

## What Maps does

### Scrolling (`-[MKMapGestureController scrollWithEvent:]`, static)

A two-finger trackpad scroll **pans**; it does not zoom. What zooms depends on
the event:

| Input | Effect | Formula |
| --- | --- | --- |
| Scroll with no gesture phase, precise deltas | zoom | scale × `1 + 0.01 × deltaY` |
| Scroll with no gesture phase, wheel notches | zoom | scale × `1 + 0.05 × deltaY`, the change held between ±0.1 and ±0.25 |
| Shift + scroll | zoom | scale × `1 + 0.0075 × sign(d) × √\|d\|`, d held to ±100 |
| Option + vertical scroll | tilt | |
| Option + horizontal scroll | rotate | |
| Plain scroll with a phase | pan | |

The modifiers were read from the masks `0x20000` (Shift) and `0x80000` (Option).

### Pinch (`handlePinch:`, static)

- Each event multiplies the scale by `1 + Δmagnification`. Moonframe already did
  the same.
- The rate is smoothed: `rate = new / 3 + 2 × rate / 3`, if the last real change
  was within 0.15 s. Otherwise it restarts from the new value.
- A real change is `|Δ| > 0.01`.
- Let go within 0.15 s of the last real change, it runs a **0.5 s momentum
  animation**. Each frame multiplies the scale by
  `1 + 0.25 × (rate − 1) × (1 − t²)`.

### Clicks (`handleDoubleClick:`, `smartMagnifyWithEvent:`, static)

- Double click zooms one level in; with Option, one level out.
- Both go through `zoom:withFocusPoint:completionHandler:`, then
  `-[VKMapCameraController zoom:withPoint:completionHandler:]`, which is a
  `VKTimedAnimation` of **0.25 s**. A new zoom stops the previous one first.
- The curve is the animation's default, `VKAnimationCurveEaseInOut`. Nothing in
  this path sets another. (Static.)
- The focus point stays fixed: each step calls `zoomToLevel:withPoint:`.

### Zoom buttons and keys (`handleArrowZoom:`, static)

Not a fixed step: a continuous zoom. Each frame multiplies the scale by
`1 + dt × speed × 60`, smoothed the same way as a pinch, and a press lasts at
least 0.3 s. The speed comes from user defaults whose key names were not
recovered, so **how far one click goes is unknown**.

### Tile loading

- `GEOTileRequestBalancer.maxRunningOperationsCount` = **60**. (Runtime.)
- `GEOTileLoaderConfiguration`: memory cache of 512 tiles, cost limit 4 MiB,
  backpressure limit 50. (Runtime, on a new configuration; Maps may set others.)
- Fallback tiles and preloading exist (`gdc::FallbackCollector`,
  `gdc::LayerDataManager::preloadLayerDataKeys`). Only their names were read.

Apple runs its own tile servers and draws vector tiles, so none of these numbers
carry over directly.

## What was wrong in Moonmap

### 1. Every gesture lost half its motion (the main bug)

Moonmap passes `glide = true` and hands each `move` back as `center` and `zoom`.
Moonframe read that echo as a new order from the view: it dropped where the user
had left the map and began a 600 ms glide from the last drawn frame. The next
event then started from that barely moved frame.

Measured in a unit test: 40 steps of 0.05 from zoom 14, at 8 ms apart, should
reach 16. It reached **15.0**. Now it reaches 16.

### 2. Tiles came two at a time

Moonframe used two connections for every provider. Measured, 40 fresh `@2x`
CARTO Voyager tiles over HTTP/1.1, three rounds:

| Connections | Time |
| --- | --- |
| 2 | 1.06 to 1.10 s |
| 8 | 0.26 to 0.28 s |

### 3. Clicks were slow or abrupt

The zoom buttons glided for 600 ms where Maps takes 250 ms. A double click jumped
with no animation at all.

### 4. Each event did double work

Every `move` republished the whole `app.maps` snapshot, rebuilding the Luau view
a second time. The save to disk was already debounced.

## What changed

All uncommitted. In `~/code/moonframe-fast-zoom` (branch `map-fast-zoom`):

| Change | From | To | Basis |
| --- | --- | --- | --- |
| The view's echo of `move` is no longer an order | half of a gesture lost | all of it kept | bug |
| Zoom glide, staying over one place | 600 ms, cubic out | 250 ms, ease in-out | Maps |
| Double click and smart zoom | instant jump | 250 ms glide | Maps |
| Second click during a glide | | adds to the first | Maps stacks its own way |
| Glide around a focus point | point drifted | point stays fixed | Maps |
| Pinch release | stops dead | coasts 0.5 s | Maps |
| Trackpad scroll zoom rate | 1/120 step per pixel | scale × `1 + 0.01 × d`, about 1.7× faster | Maps' precise rate |
| Touching the map during a glide | jumped to its end | stops where it shows | Maps' `stopDynamicAnimations` |
| Tile connections | 2 | 8; OpenStreetMap stays at 2 | measured |
| Tile order | row by row | from the middle out | design choice |
| Tiles for a glide's destination | asked on arrival | asked as it sets off | design choice |

Trips to another place (locate, open a place) keep their 600 ms. Their Maps
timing was not looked at.

In Moonmap: `crates/moonmap-app/src/provider.rs` no longer publishes on `moved`.
`Cargo.toml` and `Cargo.lock` carry a temporary `[patch]` onto the worktree.

### Left as it was, on purpose

- **Mouse wheel:** one notch stays 1/3 of a step. Maps' is slower (0.14 to 0.32
  of a step), and the complaint was about speed.
- **Two-finger scroll still zooms.** Maps pans. Changing it is a behavior choice
  for every `ui.map`, not a speed fix.
- **Shift-scroll curve:** Moonmap has no such mode.
- **Option + double click to zoom out:** not a speed matter.

## Second pass: measured in the running app

The screen was unlocked for this pass. A throwaway module fed scroll events
through GPUI's own `Window::dispatch_event` and timed frames; `sample` profiled
the main thread. It is not in the tree. All figures are the dev build (`just
run`: dependencies at `opt-level = 2`), the demo in fresh folders, a 60 Hz
display. Synthetic events sent to the process from outside did not reach the map,
so nothing here comes from a real trackpad.

### What the profile said

The tile code was not the problem: drawing the map took 0.5 ms a frame, so locks
and file checks per tile were left alone. During a scroll the main thread was 91%
busy. 43% of that was handling the event: each one ran the Luau `update` and
`view` and re-read the whole tree, only to report the position. 49% was GPUI
drawing the window.

### Changes

| Change | Before | After | Basis |
| --- | --- | --- | --- |
| `move` at most ten times a second, always ending with the latest | every event | 100 ms apart | measured |
| Echo compared with what was told, not the latest position | | needed by the line above | correctness |
| Scroll or pinch asks for the next level's tiles ahead of the ring | on arrival | while zooming | measured |
| Pulsing pin paces its own frames | every frame | 30 fps | measured |

Scrolling, warm tiles (the "after" is three runs):

| | Before | After |
| --- | --- | --- |
| Handling an input event, mean | 4.34 ms | 0.02 to 0.03 ms |
| Main thread busy | 91% | 63 to 65% |
| Frame gap p99 | 41 ms | 24 to 32 ms |
| Frame gap worst | 74 ms | 27 to 56 ms |

"Busy" counts the wait for a drawable as work, so real work is lower.

Blurred frames in a one-second zoom of about two levels, empty tile cache, of
about 60, five interleaved runs on the real network:

| Look-ahead | Runs | Mean |
| --- | --- | --- |
| Off | 43 51 26 32 31 | 36.6 |
| On | 27 27 27 17 23 | 24.2 |

Idle with the frame throttle for inactive windows off, as a window in front has
it, three runs each, measured 16 to 28 s after launch:

| Pulse | Frame rate | CPU of one core |
| --- | --- | --- |
| Every frame | 60 fps | 18 to 26% |
| 20 ms timer (kept) | 30 fps | 10 to 18% |
| 33 ms timer | 20 fps | 7 to 8% |

The 20 ms timer gives 30 fps because the frame then waits for the screen. 8 ms did
not slow it at all, and 25 and 33 ms gave 20 fps. Windows in the background were
already held to about 20 fps by GPUI.

The end-to-end check: a one-second zoom from 15.8, then a pause, saved zoom 19.13
and the new center to `library.json`. The save debounces on `moved`, so a position
is only written once the map has rested for 800 ms.

### Tried and dropped

- **Loading Writing Tools on a side thread.** The sampler put `setMainMenu` at
  0.8 s, loading it. Measured without the sampler it saved about 3% of the launch
  (2,560 ms against about 2,645 ms), not worth an unsafe `dlopen` of a private
  framework path.
- **An early idle figure of 44% against 7%** was noise: the window overlapped work
  left over from launch. The table above replaces it.

### Left, and why

- **Launch takes about 2.5 s to the first map, warm or cold.** About half is
  `gpui_component::init` listing every font (this Mac has 5,161 faces, so most
  Macs will be quicker). That is Longbridge's crate; it needs a fix upstream.
  The rest is AppKit's menu setup, loading the workspace and the Dock icon.
- **GPUI draws the whole window each frame:** now the largest cost in a gesture.
  Not drawing the sidebar again would mean splitting `MoonframeApp` into views.
- **Stopping the pulse in the background** would save more, but is a visible
  change.

## Third pass: launch and flicker

### Launch

Stamped phases, no sampler, dev build, tiles cached:

| Phase | Before | After |
| --- | --- | --- |
| `gpui_kit::init` | 850 ms | about 950 ms |
| Dock icon | 95 ms | after the window |
| Workspace and Luau | 375 ms | 375 ms |
| Window to first frame | 900 ms | 130 ms |
| First frame | about 2,400 ms | about 1,650 ms |

The 900 ms was Moonframe's own `fonts::default_mono` listing every font a second
time. Listing on a thread did not help: the two scans serialize on a CoreText
lock, and `gpui_kit::init` doubled to 1,625 ms. Family names alone
(`CTFontManagerCopyAvailableFontFamilyNames`) take 92 ms here.

**The sampler misled twice.** It put `setMainMenu` at 0.8 s; stamped, it is 18 ms.
That time was dyld notifying the sampler itself.

**What is left is upstream.** `gpui_kit::init` spends its time in
`gpui-pre-apple`'s `all_font_names`, whose `create_for_all_families` removes
duplicates: 850 ms against about 100 ms for a plain
`CTFontCollectionCreateFromAvailableFonts` on this Mac. `gpui-component` lists all
families only to check two names. Not avoidable from Moonframe: `Theme::change`
reaches it again on any appearance change.

### Flicker

Reported by hand: tiles flicker when zooming in and out fast. First guess, tiles
drawn with nothing decoded, was mostly wrong: a count of such holes found them in
1 to 6 of 359 frames, old logic or new. Frames from `render_to_image` showed the
real thing: at each level change every tile swapped at once, sharp to washed out,
with no fade as they were already decoded.

Fixed with a 200 ms cross-fade on every level change, and 0.12 steps of hysteresis
where levels meet. Tiles also go by real decode state now, not "the file exists"
and "600 ms passed". 7 captures a second cannot show smoothness: it needs eyes.
Levels of a raster map still look different; only vector tiles avoid that.

### `just benchmark`

Drives the map on screen through real events: zoom, fast zoom, pinch, drag, idle.
Release build, loaded machine (load average near 30), first run:

| | fps | p99 ms | max ms | input ms |
| --- | --- | --- | --- | --- |
| Zoom in and out | 60 | 18.3 | 19.2 | 0.02 |
| Fast zoom | 60 | 18.8 | 19.1 | 0.01 |
| Pinch | 60 | 18.2 | 18.3 | 0.01 |
| Drag | 60 | 17.6 | 17.7 | 0.02 |
| Idle | 30 | 34.3 | 34.3 | |

Idle took 9 to 12% of a core.

## Fourth pass: how the sheets move

Maps.app's cards are real `UISheetPresentationController` sheets
(`-[CardPresentationController configureForSheetPresentationWithContainer:layout:]`,
detents). `ContainerTransitionManager` queues a card behind the one in progress
(`WaitingTransition`, `dequeueWaitingContainees`). (Static.)

UIKit's `-[_UISheetAnimationController interruptibleAnimatorForTransition:]` builds a
`UIViewPropertyAnimator` from two private functions. (Static, then called in a
Mac Catalyst probe.)

| | |
| --- | --- |
| `_UISheetTransitionDuration` | 0.4 s |
| `_UISheetTransitionTimingCurve` | `UISpringTimingParameters`, mass 1, stiffness 333.333, damping 36.515, velocity 0 |
| `_UISheetPresentationMetrics` spring | damping ratio 1, response 0.3441 |
| The same, high speed | damping ratio 0.8, response 0.3441 |

Damping ratio 1 is critically damped: no overshoot, starting from rest and settling
slowly, so it reads as ease in and out. Position at time t, with w = 18.257, is
`1 - (1 + w t) e^(-w t)`: 17% at a tenth of the way, 64% at 0.3, 88% at half, 99.4%
at the end.

`cubic-bezier(0.22, 0.18, 0.14, 1)` over 400 ms follows it to within 2%, never past 1.

`AnimationSlideParameters` in Maps.app holds `{0.5 s, mass 3, stiffness 1000, damping
500}`, but nothing calls it: dead code on this build. Do not use it.

Not proven: that a Mac card goes through `_UISheetAnimationController` rather than
`_UISheetHostManagerMac`, or that the high-speed spring is not used on some path. The
probe had no window, so values come from `_UIFallbackSheetMetrics`, and no live sheet
was traced.

## Not verified

- **Nothing was tried with a real trackpad or by hand.** The running app was only
  driven by synthetic events.
- **The first pass was not compared with the old build** in the running app.
- **Look-ahead's extra requests were not counted.** A zoom that stops short of the
  next level has fetched up to a screen of tiles it did not show.
- **120 Hz displays were not measured.** The pulse's saving should be larger there.
- **The 15.0 figure comes from a model of the event loop,** one event and one frame
  per 8 ms. The real loss will vary with event and frame timing.
- **The pinch coast assumes 60 frames a second.** Maps adds to the scale per frame,
  so `COAST_STEPS = 7.2` is 0.25 × 2/3 × 30 frames ÷ ln 2. Its feel needs a real
  trackpad.
- **Eight connections were only timed against CARTO.** Esri, Mapbox and the others
  were not, and their limits were not checked.
