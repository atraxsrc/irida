# Irida design

Date: 2026-10-16
Status: reviewed, amended from source 2026-10-16 (checked against the four
rice repos, libcosmic `e1f2cd2` and cosmic-term `3bfca1e`)

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
`t`, per channel: `a + (b - a) * t` in `f64`, rounded half to even
(`f64::round_ties_even`), then clamped to 0..=255. Half to even is what the
rices use: Ember Day's fastfetch line colour has a blue channel of exactly
162.5 and the file says `A2`.

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

Ember's `normal.black` is the hand-picked `#2A2F38`; the rule gives
`#2B2F37`, 1 off. Import keeps the real value as a pin.

**Pins:** any derived slot can be set by hand. A pinned slot keeps its value
when roles change and shows a marker in the editor. Brights and dims derive
from the effective `normal.<colour>`, pinned or not. `bright_foreground`
follows the effective `bright.white` (dark) or `bright.black` (light). Pins are
stored in the palette file as a `[pins]` table keyed by slot name:

```toml
[pins]
"bright.red" = "#DD6E6E"
"normal.black" = "#2A2F38"
```

Slot names: `normal.<colour>`, `bright.<colour>` and `dim.<colour>` for the
eight ANSI colours (`black`, `red`, `green`, `yellow`, `blue`, `magenta`,
`cyan`, `white`), plus `foreground`, `background`, `cursor`,
`bright_foreground` and `dim_foreground`. `background` can be pinned to
`"none"`, which leaves the terminal on the desktop background (Noir does this).
Desktop-only colours that no role derives use the `cosmic.` prefix:
`cosmic.neutral_10` and `cosmic.gray_2`.

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

The luminance formula is the one in the rices' `contrast.py` (sRGB threshold
0.03928; for 8-bit input it gives the same result as 0.04045). Verdicts use the
raw ratio; ratios display with one decimal. `contrast.py` gates on the window
only, so the pane check and the muted 3:1 rule are new: under them Ember,
Ember Day and Dossier have pane failures that `contrast.py` never reported.
That is intended.

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

`palette.toml`:

```toml
name = "Ember Day"        # display name
slug = "emberday"         # [a-z]+, the rices' switch.sh only matches that
mode = "light"
pair = "ember"            # optional, the other half's slug, stored on both
notes = """..."""         # optional

[roles]                   # all 13, "#RRGGBB"
background = "#E9E6DF"

[names]                   # optional display names per role, for the README
background = "Stone"

[pins]                    # optional
```

Templates and export also get a `stem`: the name with spaces removed
(`EmberDay`), used in rice file names such as `btop/EmberDay.theme`.

Import reads an existing rice repo. Roles come from `scripts/contrast.py` when
it exists (Ember and Ember Day only). Otherwise they come from
`cosmic/<Stem>.ron` (background, container, accent, success, warning, error),
the terminal scheme (pane = `background` or, when absent, the window
background; text = `foreground`; muted = `dim_foreground`; blue, magenta, cyan
from `normal`) and `nvim/colors/<slug>.lua` (comment = `cmt`). The terminal
scheme is the one named in
`cosmic/config/com.system76.CosmicTerm/v1/syntax_theme_<mode>`, never a glob
of `cosmic-term/*.ron` (Noir ships six). Every slot that differs from its
derived value is imported as a pin, so Noir and Dossier open unchanged. Base16
import comes later.

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
comments. btop rewrites `btop.conf` on exit. A `key` file entry names the key
(`key = "color_theme"`) and its template renders the value. It handles flat
`key = value` lines only: the line is replaced in place, or appended when
absent, and the file is created when missing.

A `block` file with no markers gets the block appended at the end (the file is
created when missing), which is what a first apply needs. A file whose markers
are broken (one without the other, or more than one pair) is skipped and named.

Paths expand `~` and honour `$XDG_CONFIG_HOME`. A symlinked config is written
through to its target, so dotfile managers keep working, and the file keeps its
permissions. A file that did not exist before is recorded as absent and
deleted on undo.

Block markers:

```
# >>> irida >>>
...
# <<< irida <<<
```

### Templates

Jinja syntax through `minijinja`. Every role, every derived slot and the
palette's name, slug, stem and mode are available. Roles are top-level
(`{{ accent }}`); derived slots sit under `term`
(`{{ term.background }}`, `{{ term.bright.red }}`), since the role
`background` and the slot `background` are different colours. Filters: `hex` (default,
`#E8713A`), `strip` (`E8713A`), `rgb` (`232,113,58`), `rgba(alpha)` with alpha
from 0 to 1 (`rgba(232,113,58,0.8)`), `ansi` (`38;2;232;113;58`). Functions:
`mix(a, b, t)` (the derivation mix; fastfetch lines, zsh autosuggest and the
Neovim `bg2` need it) and `ramp(from, to, steps)`. Both take roles, slots or
hex strings; a role wins over a slot of the same name (`background`). `ramp` steps run from 2 to 16 and each step is clamped into sRGB
before rounding.

Other `target.toml` fields: `detect.flatpak` (the flatpak id), `reload.command`
(a command line, the alternative to `reload.signal`) and
`reload.min_version = { command = "btop --version", version = "1.3.2" }`.
Below that version Irida reports "restart" instead of signalling: SIGUSR2
kills btop 1.3.0. `[rice]` is an array (`[[rice]]`), one entry per file, since
btop needs a `file` for the theme and a `key` for `color_theme`.

### Detection

A target is detected when its binary is on `PATH` or its config folder exists,
including the flatpak config location under `~/.var/app/<id>/config/` when the
target lists a flatpak id. Undetected targets are hidden under "Not installed".

### Native targets

- **COSMIC desktop.** Build a `cosmic_theme::ThemeBuilder` from the palette,
  write it with `write_entry` (generated by the `CosmicConfigEntry` derive) to
  `ThemeBuilder::dark_config()` or `light_config()`, then write the built
  `Theme` to `Theme::dark_config()` or `light_config()`. This is what Settings'
  Import does. A pair fills both slots. Apply reads the user's current builder
  and overlays only the colour fields, so their spacing and radii survive.
  Export writes the rices' shared house style instead (radii 2.0, `frosted:
  Low`, `gaps: (0, 7)`, `active_hint: 1`, the alpha map). The `CosmicPalette`
  comes from the roles as in `Ember.ron` and `EmberDay.ron`: `neutral_0..10`
  is a linear ramp from background (dark) or container (light) to text, with
  dark `neutral_10` pinnable as `cosmic.neutral_10`; `neutral_tint` is
  `neutral_5`; `gray_1` is container; `gray_2` is `normal.black` (dark) or
  pane (light). Irida builds configs with `Config::with_custom_path` so tests
  never touch the real config folder.
- **COSMIC mode.** Applying a single palette switches COSMIC to that palette's
  mode (`com.system76.CosmicTheme.Mode`, `is_dark`), as `switch.sh` does today.
  Applying a pair leaves the mode alone, since both slots are filled. Only
  `is_dark` is written; when `auto_switch` is on the result warns that COSMIC
  may switch back.
- **COSMIC Terminal.** COSMIC Terminal stores schemes as maps in its own config
  (`com.system76.CosmicTerm`, keys `color_schemes_dark` and
  `color_schemes_light`) and selects one by name (`syntax_theme_dark` and
  `syntax_theme_light`). Its types are not a published crate, so Irida mirrors
  the `ColorScheme` layout in its own serde types. **Risk:** an upstream change
  to that layout breaks this target. The test suite pins the layout, and the
  target fails with a clear error rather than writing a malformed entry. The
  mirror uses `deny_unknown_fields`, as upstream does, so an unknown field is
  that clear error. Irida writes these key files itself, in exactly the
  bytes COSMIC's config library writes, so the apply record covers them.
  Schemes are keyed by a `u64` id and selected by display name (later
  duplicates become "Name (2)"): Irida replaces the lowest-id scheme with the
  palette's name (keeping its id) or
  adds one at the last id plus 1, and never drops the user's other schemes. It
  sets `syntax_theme_<mode>`, and the same key on the default profile when one
  is set, since a profile's choice wins. When `app_theme` is not `System` the
  result warns that the terminal will not follow the mode.

### Apply

Apply is one transaction:

1. Render every enabled target. If any template fails, nothing is written and
   the failing target is named. Templates are syntax-checked when targets
   load, and a target with a broken template is skipped like a broken
   `target.toml`; only a template that fails while rendering (an undefined
   value) stops the apply.
2. Create an apply record: copy every file about to change, and for `block` and
   `key` writes record the previous block or value. For `native` targets, copy
   the raw key files of every config id about to change
   (`cosmic/<id>/v<n>/<key>`), and note which keys did not exist. Reading back
   through COSMIC's config library is not enough: it returns defaults for keys
   that were never set, and the light builder's default carries the dark
   palette.
3. Write each file atomically (write a temporary file, then rename).
4. Reload each target by command or signal. Targets that need a restart are
   named in the result.

If a write fails, Irida restores everything already written from the record
and reports the file and the reason.

### Undo

Undo restores the newest apply record. `block` and `key` writes restore only
their own part, so edits the user made elsewhere in the file survive. `native`
targets write the stored key files back and delete the keys that did not
exist. The last
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

Export writes into `cosmic-<slug>-theme/`, matching the current repos:

- `cosmic/<Stem>.ron`, `cosmic/install.sh`, `cosmic/config/` (the terminal and
  toolkit settings `switch.sh` copies, with `syntax_theme_<mode>` filled in)
  and `cosmic-term/<Stem>-term.ron`, importable as today
- `btop/<Stem>.theme` (no `install.sh`, as today: `switch.sh` installs it),
  `nvim/`, `fastfetch/`, `lsd/`, `zsh/<slug>.zsh` and `cava/` (new), each
  with its `install.sh`, made executable
- `scripts/switch.sh`, the shared script with its settings block filled in
  and the slug added to its `RICES` list
- `README.md` with the palette table, and `docs/showcase.md` as a skeleton
  with the contrast table (window and pane ratios)
- `.gitignore` and `.pre-commit-config.yaml` when absent

Exporting into an existing rice repo replaces only generated files. In
`README.md` and `docs/showcase.md`, Irida fills only its marked blocks:

```
<!-- >>> irida palette >>> -->
...
<!-- <<< irida palette <<< -->
```

A file with no markers is left alone and named in the summary. The summary
lists what is still done by hand (wallpaper, icons, cursor, Firefox,
Claude Desktop, `update_system.sh`, LICENSE, screenshot) and any contrast
failures.

## 7. Command line

The same binary. With no arguments it opens the app.

```
irida check <palette>                    contrast table; exit 1 on a failure
irida apply <palette> [--only btop,nvim]
irida undo
irida export <palette> <folder>
irida list
```

`check` replaces the rice repos' `contrast.py`. `<palette>` is a slug, a path
to a `palette.toml`, or a rice repo folder (imported on the fly). `list` lists
saved palettes.

## 8. Architecture

A Cargo workspace with two crates:

- **`irida-core`**, no GUI: colour types and maths (OKLab through the `palette`
  crate, taken from the `cosmic_theme::palette` re-export so the types match
  libcosmic's), contrast, derivation, palette and version storage, target loading,
  templating (`minijinja`), detection, writers, apply records, undo, reload,
  rice export. Everything here is testable without a display.
- **`irida`**, the binary: the libcosmic app and the `clap` command line, both
  thin layers over `irida-core`.

Built-in targets live in `irida-core/targets/<name>/` and are embedded at build
time. libcosmic is pinned to one git revision for both crates; the workspace
uses edition 2024 and Rust 1.93 or newer, as libcosmic does.

## 9. Errors

| Situation | Behaviour |
|---|---|
| Template or target file broken | Target shows the error in the Apply sheet and stays off; others still apply |
| Write fails partway | Roll back from the apply record; toast names the file and reason |
| Markers missing in a `block` file | Append the block at the end of the file |
| Markers broken in a `block` file | Skip that file and say so; never guess where to write |
| Palette file unreadable | Open read-only with the error shown; never reset it |
| Reload fails | Files stay written; toast says to restart the app |
| COSMIC Terminal layout mismatch | Target fails with a clear error; nothing written |

## 10. Testing

- **Contrast:** known WCAG pairs, and `irida check` prints the same window and
  pane ratios as `contrast.py` (to one decimal) on Ember and Ember Day. The
  verdicts follow section 3: Ember fails error on the pane, Ember Day fails
  accent, success, warning and cyan on the pane, Dossier fails accent,
  success, warning and magenta on the pane, and Noir passes. Noir and Dossier
  have no `contrast.py`, so their ratios are checked against a fixture table.
- **Derivation:** the derived terminal colours reproduce `Ember-term.ron`
  within 2 per channel (only `normal.black` and `dim.black` are off, by 1) and
  `EmberDay-term.ron` exactly. With `normal.black` pinned, both are exact.
  Importing Noir and Dossier round-trips every slot.
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
