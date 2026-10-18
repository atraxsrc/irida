# Contributing to Irida

Bug reports, fixes and new apps are all welcome. Irida is tested on Pop!_OS
24.04 with COSMIC, so reports from other distributions help a lot.

## Reporting a bug

Open an issue with:

- your distribution and COSMIC version
- the Irida version (`irida --version`, or the .deb you installed)
- what you did, what you expected and what happened
- for an app that came out wrong: the app's version and the file Irida wrote

## Adding an app

Each app Irida colours is a *target*: one folder under `irida-core/targets/`
with a `target.toml` and its templates. `irida-core/build.rs` embeds every
folder there, so a new target needs no engine code.

The quickest start is to copy the closest existing target:

| If the app... | Start from |
|---------------|------------|
| reads a theme file it includes from its config | `kitty`, `alacritty`, `foot` |
| takes a theme name from one config line | `helix`, `bat`, `delta` |
| keeps colours in a block inside its main config | `tmux`, `starship` |
| loads a theme or flavor from its own folder | `zellij`, `yazi` |

A `target.toml` has:

- `name`, `display` and `detect` (a binary and, optionally, a config folder)
- one or more `[[file]]` writes, each with a `path`, a `write` kind and a
  `template`:
  - `file`: Irida owns the whole file
  - `key`: one line in the user's config, set to a value
  - `block`: a marked block inside the user's config
- optional `[reload]`: a signal, a command, or just a `hint` telling the user
  how to reload
- `[[rice]]` entries for what a rice export includes, usually the theme file
  and an `install.sh`

Templates see the 13 palette roles (`background`, `pane`, `container`,
`text`, `muted`, `comment`, `accent`, `success`, `warning`, `error`, `blue`,
`magenta`, `cyan`), the terminal colours (`term.normal.red`,
`term.bright.red`, `term.foreground`, ...), `name`, `slug`, `mode`, and
helpers such as `mix(a, b, amount)`. Read an existing template for the exact
use.

Please:

- Start `target.toml` with a comment that says where the config format was
  checked (docs or changelog, and the date). The existing targets show the
  style.
- Never overwrite a file the user wrote by hand: use `key` or `block` for the
  user's own config, and `file` only for a file Irida owns.
- Add `irida-core/tests/builtin_<name>.rs` with snapshot tests for the
  ember and emberday starters, plus an apply and undo round trip.
  `builtin_kitty.rs` is a good model. Expected output lives in
  `irida-core/tests/fixtures/expected/<name>/`.
- Add a row to the Supported apps table in the README.

## Before you open a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Keep one change per pull request and describe how you tested it. Use plain
hyphens rather than long dashes in code, docs and commit messages.

By contributing you agree that your work is released under the MIT License.
