# Irida design

Date: 2026-10-16
Status: draft for review

Irida is a native COSMIC app for designing a colour palette and applying it to
the desktop and the apps around it. It is named after Iris, the Greek goddess
of the rainbow. The command, the repository and the install folder are all
`irida`.

## 1. Purpose

Irida does two jobs from one palette:

1. **For COSMIC users:** design or pick a palette, preview it, and apply it to
   the live desktop and installed apps, with one-click undo.
2. **For rice authors:** export the palette as a complete rice repository,
   matching the layout of the existing `cosmic-*-theme` repos (Ember, Ember
   Day, Noir, Dossier and the others).

Success means going from an idea to a finished, contrast-checked rice in an
afternoon instead of days, and letting any COSMIC user theme their terminal
tools as easily as their desktop.

### Design principles

Minimal, beautiful, intuitive, easy to use. When a feature adds a mode, a
setting or a screen, it has to earn it.

### Relation to the COSMIC roadmap

System76's Epoch 2 and 3 roadmap covers COSMIC's own parts: a COSMIC Terminal
colour scheme editor (Epoch 2), themes with bundled wallpapers in the COSMIC
Store, and auto-matching the accent to the wallpaper (Epoch 3). Irida does not
compete with those. Its value is everything outside COSMIC: btop, Neovim,
fastfetch, lsd, zsh, cava and any app a user adds. Once System76 defines a
theme package format, it becomes one more export target.

### Reference

Aether (github.com/omacom/aether) does this for Omarchy: wallpaper extraction,
templates per app, apply with reload hooks. Irida differs in three ways: it is
palette first (the wallpaper is recoloured to fit the palette, not the other
way round), it is native COSMIC rather than a web view, and contrast is part of
the editor, not an extra.

## 2. Decisions

| Topic | Decision |
|---|---|
| Audience | COSMIC users, plus a rice repo export for rice authors |
| Palette source | Hand design first; wallpaper extraction later as a first draft |
| Unit of design | One palette, dark or light; two palettes can be linked as a pair |
| Live tweaks | In-app previews first; "try it live" on the real desktop last |
| Comparison | Pairs side by side, and before and after (saved version or snapshot) |
| Roles | Fixed set of 13 |
| Toolkit | Rust and libcosmic |
| Targets | Data files (TOML plus a Jinja template), built-in and user |
| Version one apps | COSMIC desktop, COSMIC Terminal, btop, Neovim, fastfetch, lsd, zsh colours, cava |
| Distribution | Native .deb on GitHub releases first; Flatpak later |
| Main window | Role list on the left, preview on the right (layout B) |

## 3. Palette model

A palette has a name, a mode (`dark` or `light`), the 13 roles, optional
design notes (free text shown under the preview and in the exported README),
and optional pins that override derived colours. Two palettes can be linked as
a pair: one dark, one light. A pair shows side by side in the preview and fills
both COSMIC slots on apply. The halves are independent: editing a role in one
never changes the other, because each half is tuned for contrast on its own
background (Ember's accent `#E8713A` and Ember Day's `#A4441A` differ on
purpose).

### Roles

| Group | Roles |
|---|---|
| Surfaces | `background`, `pane`, `container` |
| Text | `text`, `muted`, `comment` |
| Accent | `accent` |
| Status | `success`, `warning`, `error` |
| Hues | `blue`, `magenta`, `cyan` |

`background` is the window background. `pane` is the terminal and editor
surface. `container` is cards and panels.

### Derived colours

Brights and dims mix in gamma-encoded sRGB, which is what the existing Ember
and Ember Day schemes use. `mix(a, b, t)` moves `a` toward `b` by the fraction
`t`, per channel, rounded to the nearest integer.

**Dark mode** (reproduces Ember):

| Slot | Rule |
|---|---|
| normal black | `mix(container, text, 0.05)` |
| normal red, green, yellow | `error`, `success`, `warning` |
| normal blue, magenta, cyan | `blue`, `magenta`, `cyan` |
| normal white | `text` |
| bright red to cyan | `mix(normal, #FFFFFF, 0.12)` |
| bright black | `muted` |
| bright white | `mix(text, #FFFFFF, 0.29)` |
| dim, all eight | `mix(normal, pane, 0.40)` |
| foreground, background, cursor | `text`, `pane`, `accent` |
| bright foreground, dim foreground | bright white, `muted` |

**Light mode** (reproduces Ember Day):

| Slot | Rule |
|---|---|
| normal black | `text` |
| normal red to cyan | as dark mode |
| normal white | `muted` |
| bright, all eight | `mix(normal, #000000, 0.15)` |
| dim, all eight | `mix(normal, pane, 0.40)` |
| foreground, background, cursor | `text`, `pane`, `accent` |
| bright foreground, dim foreground | bright black, `muted` |

**Ramps** for gradients (cava bars, btop meters) interpolate in OKLab, so steps
look even. Templates ask for them by name: `ramp("comment", "accent", 8)`.

**Pins:** any derived slot can be set by hand. A pinned slot keeps its value
when roles change and shows a marker in the editor. Pins are stored in the
palette file as a `[pins]` table keyed by slot name:

```toml
[pins]
"bright.red" = "#DD6E6E"
"normal.black" = "#2A2F38"
```

Slot names: `normal.<colour>`, `bright.<colour>` and `dim.<colour>` for the
eight ANSI colours (`black`, `red`, `green`, `yellow`, `blue`, `magenta`,
`cyan`, `white`), plus `foreground`, `background`, `cursor`,
`bright_foreground` and `dim_foreground`.

### Contrast

WCAG 2 contrast ratio, measured against both `background` and `pane`.

| Roles | Minimum |
|---|---|
| `text`, `accent`, `success`, `warning`, `error`, `blue`, `magenta`, `cyan` | 4.5:1 |
| `muted` | 3:1 |
| `comment`, surfaces | none |

A role under its minimum shows its ratio in red. Nothing is ever blocked. The
rice export summary and `irida check` report failures. Pairs are not compared
with each other: each half is checked against its own surfaces only.

### Versions

Ctrl+S saves a version. The last saved version is "before" in the comparison
view. Any version can be pinned as a named snapshot. Unsaved changes show as a
dot after the palette name.

### Storage

```
$XDG_DATA_HOME/irida/palettes/<slug>/palette.toml      current palette
$XDG_DATA_HOME/irida/palettes/<slug>/versions/<n>.toml saved versions
$XDG_CONFIG_HOME/irida/settings.toml                   target toggles, last palette
$XDG_CONFIG_HOME/irida/targets/<name>/                 user targets
$XDG_STATE_HOME/irida/applies/<n>/                     apply records
```

Import reads an existing rice repo: roles from `scripts/contrast.py` and the
terminal slots from `cosmic-term/*.ron`, so Ember, Noir and the others open as
palettes. Base16 import comes later.

## 4. Targets

A target is a folder holding `target.toml` and one or more templates. Built-in
targets are compiled into the binary. User targets are read from
`$XDG_CONFIG_HOME/irida/targets/`. Both go through the same engine. A user
target with the same name as a built-in replaces it.

### Example

```toml
name = "cava"
detect = { binary = "cava", config = "~/.config/cava" }

[[file]]
path = "~/.config/cava/config"
write = "block"
template = "cava.tmpl"
comment = "#"

[reload]
signal = { process = "cava", signal = "USR2" }

[rice]
path = "cava/config.colors"
install = "block"
```

### Write kinds

| Kind | Meaning | Examples |
|---|---|---|
| `file` | Irida owns the whole file | btop theme, Neovim colorscheme |
| `block` | Irida writes only between its begin and end marker comments | cava colours, zsh colours |
| `key` | Irida sets one key's value and touches nothing else | btop's `color_theme` |
| `native` | Built-in only, written through COSMIC's config library | COSMIC desktop, COSMIC Terminal |

`key` exists for apps that rewrite their own config and would delete marker
comments. btop rewrites `btop.conf` on exit.

Block markers:

```
# >>> irida >>>
...
# <<< irida <<<
```

### Templates

Jinja syntax through `minijinja`. Every role, every derived slot and the
palette's name, slug and mode are available. Filters: `hex` (default,
`#E8713A`), `strip` (`E8713A`), `rgb` (`232,113,58`), `rgba(alpha)`,
`ansi` (`38;2;232;113;58`). Function: `ramp(from, to, steps)`.

### Detection

A target is detected when its binary is on `PATH` or its config folder exists,
including the flatpak config location under `~/.var/app/<id>/config/` when the
target lists a flatpak id. Undetected targets are hidden under "Not installed".

### Native targets

- **COSMIC desktop.** Build a `cosmic_theme::ThemeBuilder` from the palette,
  write it with `write_entry` (generated by the `CosmicConfigEntry` derive) to
  `ThemeBuilder::dark_config()` or `light_config()`, then write the built
  `Theme` to `Theme::dark_config()` or `light_config()`. This is what Settings'
  Import does. A pair fills both slots.
- **COSMIC mode.** Applying a single palette switches COSMIC to that palette's
  mode (`com.system76.CosmicTheme.Mode`, `is_dark`), as `switch.sh` does today.
  Applying a pair leaves the mode alone, since both slots are filled.
- **COSMIC Terminal.** COSMIC Terminal stores schemes as maps in its own config
  (`com.system76.CosmicTerm`, keys `color_schemes_dark` and
  `color_schemes_light`) and selects one by name (`syntax_theme_dark` and
  `syntax_theme_light`). Its types are not a published crate, so Irida mirrors
  the `ColorScheme` layout in its own serde types. **Risk:** an upstream change
  to that layout breaks this target. The test suite pins the layout, and the
  target fails with a clear error rather than writing a malformed entry.

### Apply

Apply is one transaction:

1. Render every enabled target. If any template fails, nothing is written and
   the failing target is named.
2. Create an apply record: copy every file about to change, and for `block` and
   `key` writes record the previous block or value. For `native` targets, read
   the current state back through COSMIC's config library and store it: the
   current `ThemeBuilder` for each slot written, the COSMIC mode, and for COSMIC
   Terminal the scheme maps and the selected scheme names. If that read fails,
   copy the raw config files for those keys instead.
3. Write each file atomically (write a temporary file, then rename).
4. Reload each target by command or signal. Targets that need a restart are
   named in the result.

If a write fails, Irida restores everything already written from the record
and reports the file and the reason.

### Undo

Undo restores the newest apply record. `block` and `key` writes restore only
their own part, so edits the user made elsewhere in the file survive. `native`
targets write the stored `ThemeBuilder` back, rebuild the `Theme` from it and
write that too, then restore the mode and the terminal entries. The last
ten records are kept; older ones are deleted. No backup files are left next to
the user's configs.

### Try it live

A header toggle. While on, edits are applied when a slider is released or 400 ms
after the last keystroke in a text field, and only to targets that reload live (COSMIC
desktop, COSMIC Terminal, cava). The session is one apply record from the
start. Turning the toggle off asks Keep or Revert. If Irida exits with a live
session open, it offers to revert on next start.

## 5. App

### Main window

- **Header:** palette name with a menu (switch, new, duplicate, import, export
  as rice repo, delete), the mode chip, the pair chip, Compare, the try-live
  toggle, Apply.
- **Left, role list:** grouped as in the palette model. Each row shows the
  swatch, name, hex and the lower of its two contrast ratios, red when under
  its minimum. Clicking a row opens it in place: colour picker, hex field, both
  ratios, its derived bright and dim colours, and a pin for each.
- **Right, preview scene:** the panel bar, a terminal session (fastfetch box,
  `lsd -l`, Neovim with code, a mistyped git command, a btop meter, cava bars)
  and a COSMIC settings card. Drawn with libcosmic widgets.

### Compare

The preview splits into two columns. The pair chip shows the dark and light
halves. Compare shows the last saved version (or a pinned snapshot, chosen
from the column label) next to the working copy. Only one split at a time.

### Apply sheet

Lists detected targets with a toggle each, remembered per user. Not installed
targets fold under one line. A broken target shows its error and stays off.
After applying, a toast reports the result, names any app that needs a
restart, and offers Undo.

Mockups: the three layouts and the chosen layout's states were reviewed as
claude.ai artifacts on 2026-10-16.

## 6. Rice repo export

Each target's `[rice]` section gives `path`, where its output goes in the repo,
and `install`, the write kind (`file`, `block` or `key`) the generated
`install.sh` uses on the user's machine. Export walks the same targets as Apply, so a template
exists once.

Export writes, matching the current repos:

- `cosmic/<Name>.ron` and `cosmic-term/<Name>-term.ron`, importable as today
- `btop/<Name>.theme`, `nvim/`, `fastfetch/`, `lsd/`, `zsh/<slug>.zsh`, `cava/`,
  each with its `install.sh`
- `scripts/switch.sh`, the shared script with its settings block filled in
- `README.md` with the palette table and contrast ratios, and
  `docs/showcase.md` as a skeleton

Exporting into an existing rice repo replaces only generated files. In
`README.md` and `docs/showcase.md`, Irida fills only its marked blocks. The
summary lists what is still done by hand (wallpaper, icons, cursor, Firefox
add-on, screenshot) and any contrast failures.

## 7. Command line

The same binary. With no arguments it opens the app.

```
irida check <palette>                    contrast table; exit 1 on a failure
irida apply <palette> [--only btop,nvim]
irida undo
irida export <palette> <folder>
irida list
```

`check` replaces the rice repos' `contrast.py`.

## 8. Architecture

A Cargo workspace with two crates:

- **`irida-core`**, no GUI: colour types and maths (OKLab through the `palette`
  crate), contrast, derivation, palette and version storage, target loading,
  templating (`minijinja`), detection, writers, apply records, undo, reload,
  rice export. Everything here is testable without a display.
- **`irida`**, the binary: the libcosmic app and the `clap` command line, both
  thin layers over `irida-core`.

Built-in targets live in `irida-core/targets/<name>/` and are embedded at build
time.

## 9. Errors

| Situation | Behaviour |
|---|---|
| Template or target file broken | Target shows the error in the Apply sheet and stays off; others still apply |
| Write fails partway | Roll back from the apply record; toast names the file and reason |
| Markers missing in a `block` file | Skip that file and say so; never guess where to write |
| Palette file unreadable | Open read-only with the error shown; never reset it |
| Reload fails | Files stay written; toast says to restart the app |
| COSMIC Terminal layout mismatch | Target fails with a clear error; nothing written |

## 10. Testing

- **Contrast:** known WCAG pairs, and `irida check` gives the same verdicts as
  `contrast.py` on Ember, Ember Day, Noir and Dossier.
- **Derivation:** the derived terminal colours reproduce `Ember-term.ron` and
  `EmberDay-term.ron` within 2 per channel.
- **Templates:** every built-in template rendered against Ember and Ember Day,
  with snapshot tests.
- **Engine:** apply, undo, `block`, `key`, rollback after a failed write, and
  missing markers, all against a temporary home folder. Native undo round-trips
  a `ThemeBuilder` and the terminal scheme maps through a temporary COSMIC
  config folder.
- **Command line:** end-to-end runs of each subcommand.
- **Built-in targets:** each checked once by hand against the real app before
  it counts as built-in.
- **App:** checked by hand; the preview compared against the Ember pair
  artifact.

## 11. Milestones (14 days)

1. Core: roles, derivation, contrast, storage, `irida check` (days 1 to 3)
2. Engine: targets, templates, writers, apply, undo, `irida apply` and `undo`
   (days 3 to 6)
3. Built-in targets: COSMIC desktop, COSMIC Terminal, btop, Neovim, fastfetch,
   lsd, zsh, cava (days 6 to 9)
4. App: layout B, role editor, preview scene, Apply sheet (days 9 to 12)
5. Compare, pairs, import of existing rices, rice export, .deb (days 12 to 13)
6. Try it live (day 14; the first thing to slip if time runs short)

## 12. Later

- Wallpaper extraction as a first draft for the role editor
- Base16 import
- Wallpaper recolour (the logic of `scripts/wallpaper.sh`)
- More built-in targets in batches: Alacritty, Kitty, Ghostty, WezTerm, Helix,
  Zed, VS Code, bat, delta, starship, lazygit, tmux
- Flatpak, once the built-in list is stable
- Export to the COSMIC Store theme format, once it exists
