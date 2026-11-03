# Moonmap's live UI

- `main.luau`: the shell (sidebar, map, controls, place card, Map Styles).
- `style.luau`: colors and rules. Classes are prefixed `side-`, `map-`,
  `place-` and `style-`.
- `providers/maps`: connects to the Rust provider in `../src/provider.rs`.
- `providers/acp`: the coding agent behind the sparkles, Moonframe's built-in
  `feedback` mod. It stays here so agents work in this repository.
- `icons/`: PNGs rendered by `just icons`. Don't edit them by hand.

Luau edits reload in the running debug app. Rust edits need a rebuild.
