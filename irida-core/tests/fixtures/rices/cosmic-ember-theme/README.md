<div align="center">

# COSMIC · Ember

Pop!_OS 24.04 on COSMIC in Ember blue-black, fog and one ember accent: desktop,
terminal, shell, editor, Firefox, cursor, wallpapers. The dark half of a pair;
the light half is [cosmic-emberday-theme](https://github.com/atraxsrc/cosmic-emberday-theme).

![Pop!_OS](https://img.shields.io/badge/Pop!_OS-24.04_LTS-e8713a?style=for-the-badge&logo=popos&logoColor=white)
![COSMIC](https://img.shields.io/badge/COSMIC-1.0.0-8fb58a?style=for-the-badge)
![Ember](https://img.shields.io/badge/Theme-Ember-15171c?style=for-the-badge&labelColor=e8713a)
![Firefox](https://img.shields.io/badge/Firefox-Cosmic_Ember-e8713a?style=for-the-badge&logo=firefox-browser&logoColor=white)
![License](https://img.shields.io/badge/License-MIT-7c8290?style=for-the-badge)

</div>

---
![Pop!_OS COSMIC desktop with the Ember theme](assets/screenshot.png)

## Palette

Ember is one design in two halves: cold blue-black night, warm stone day, one
ember accent shared by both. Big shapes (window, containers, text) are neutral; the
ember is small and loud (accent, window hint, commands) and everything else in the
ANSI set stays calm, pairing by lightness only. Every role you read as text clears
4.5:1 on the window `#15171C`; the terminal is an opaque pane `#1B1E24`, one step lighter
than the desktop.

| Role | Color | Hex |
|------|-------|-----|
| Background | ![#15171C](https://placehold.co/12x12/15171c/15171c.png) Night | `#15171C` |
| Containers | ![#22262E](https://placehold.co/12x12/22262e/22262e.png) Soot / Pane | `#22262E` / `#1B1E24` |
| Terminal | ![#1B1E24](https://placehold.co/12x12/1b1e24/1b1e24.png) Pane, opaque | `#1B1E24` |
| Text | ![#D6D9E0](https://placehold.co/12x12/d6d9e0/d6d9e0.png) Fog | `#D6D9E0` |
| Accent | ![#E8713A](https://placehold.co/12x12/e8713a/e8713a.png) Ember | `#E8713A` |
| Success | ![#8FB58A](https://placehold.co/12x12/8fb58a/8fb58a.png) Moss | `#8FB58A` |
| Warning | ![#D9A441](https://placehold.co/12x12/d9a441/d9a441.png) Amber | `#D9A441` |
| Error | ![#D85A5A](https://placehold.co/12x12/d85a5a/d85a5a.png) Brick | `#D85A5A` |
| Blue | ![#7FA3C7](https://placehold.co/12x12/7fa3c7/7fa3c7.png) Slate blue | `#7FA3C7` |
| Magenta | ![#B08BC4](https://placehold.co/12x12/b08bc4/b08bc4.png) Heather | `#B08BC4` |
| Cyan | ![#7FB7B3](https://placehold.co/12x12/7fb7b3/7fb7b3.png) Verdigris | `#7FB7B3` |
| Muted | ![#7C8290](https://placehold.co/12x12/7c8290/7c8290.png) Muted | `#7C8290` |
| Comment | ![#5A6070](https://placehold.co/12x12/5a6070/5a6070.png) Comment | `#5A6070` |

Window hint `#E8713A`. Ratios per role are in `docs/showcase.md`, or run
`python3 scripts/contrast.py`.

| Role | Ember | Ember Day |
|------|-------|-----------|
| Background | `#15171C` | `#E9E6DF` |
| Text | `#D6D9E0` | `#2B2E36` |
| Accent | `#E8713A` | `#A4441A` |
| Success | `#8FB58A` | `#4C6B45` |

---

## Repo Structure

```
.
├── btop
│   └── Ember.theme              # btop colour theme
├── claude-desktop
│   ├── cosmic-ember.jsonc       # Claude Desktop theme (claude-desktop-extra), dark + light
│   └── install.sh
├── cosmic
│   ├── Ember.ron                # COSMIC Appearance import (Dark)
│   ├── config/                  # baseline settings: fonts, icons, terminal look
│   └── install.sh
├── cosmic-term
│   └── Ember-term.ron           # COSMIC Terminal colour scheme (Dark)
├── cursors
│   ├── Oxygen-Ember-*/          # cursor themes, one per icon colour, patched for COSMIC apps
│   ├── install.sh
│   ├── uninstall.sh
│   └── README.md
├── icons
│   └── install.sh               # Cosmic-Ember icon pack from cosmic-icon-themes
├── fastfetch
│   ├── config.jsonc             # fog / ember fastfetch config
│   ├── cosmic.txt               # COSMIC logo
│   └── install.sh
├── firefox
│   ├── theme                    # Cosmic Ember theme add-on (manifest + icons)
│   ├── chrome
│   │   ├── userChrome.css       # rounded chrome, ember menu hover
│   │   └── userContent.css      # new tab accents, blank tiles
│   ├── install.sh
│   └── README.md
├── lsd
│   ├── config.yaml              # theme: custom
│   ├── colors.yaml              # permission / size / date / git columns
│   └── install.sh
├── nvim                         # Neovim colorscheme, LazyVim spec, RON syntax, install.sh
├── scripts
│   ├── update_system.sh         # nala + flatpak, fog ANSI for a dark terminal
│   ├── switch.sh                # make this the active rice (see Switching rice)
│   ├── contrast.py              # palette contrast table, feeds the README
│   └── wallpaper.sh             # renders a wallpaper from any source image
├── zsh
│   ├── ember.zsh                # LS_COLORS, highlighting colours, aliases, starship
│   └── install.sh
├── assets
│   ├── cursors/                 # cursor preview
│   └── screenshot.png           # README screenshot
├── backgrounds/                 # ember-anime.png
├── docs
│   └── showcase.md              # the rice describing itself, rendered in Postilla
├── LICENSE
└── README.md
```

---

## COSMIC Desktop

This is a **Dark** theme, so it goes in COSMIC's Dark slot:

Settings → Desktop → Appearance → **Dark** → **Import** → `cosmic/Ember.ron`

Accent and window hint are the ember `#E8713A`, matching the other half.
Use the `+` control if you want a custom swatch.

Frosted glass is on in the `.ron` at `frosted: Low` with an opacity table of
0.66 to 0.96, tinted with the palette's middle neutral `#7E8085`, for windows, panel,
applets and system UI (maximized apps stay solid), and corners are squared
(`radius_*: 2.0`), the same as the DarkGold baseline. Adjust frosted glass on
Appearance → Style → Frosted glass after import; those sliders survive a theme switch.

**Known COSMIC bug:** moving the frosted glass opacity / thickness sliders can
reset `text_tint` to `#FFFFFF` and drop the frosted block, which loses the fog
tint the labels carry. If the labels lose their tint, re-import `cosmic/Ember.ron`:
it carries the fog tint and the whole frosted block, so one import restores both.
Upstream fix proposed in [libcosmic#1450](https://github.com/pop-os/libcosmic/pull/1450),
not merged.

Export from Appearance if you tweak backgrounds / tints so you do not lose them.

Pairing tip: import `EmberDay.ron` from
[cosmic-emberday-theme](https://github.com/atraxsrc/cosmic-emberday-theme) into the
Light slot and COSMIC's Dark / Light toggle flips between the two. Wallpaper,
Firefox and the shell layer do not follow the toggle; those are per-rice files.

### Baseline settings

`cosmic/config/` holds the settings that make up the look and rarely change
(panel, dock, applets and shortcuts are left out on purpose):

| Where | Setting |
|-------|---------|
| Fonts | Interface `Maple Normal UI`, monospace `Maple Mono Normal NFM` |
| Icons | Cosmic-Ember-Ember |
| Windows | Minimize / maximize buttons hidden, theme applied to GNOME apps |
| Terminal | Maple Mono 15 (weights 500 / bold 800 / dim 300), opaque, no header bar, Ember (dark scheme) |

Install the [Maple fonts](https://github.com/subframe7536/maple-font) and the
Cosmic-Ember icons (see Icons below) and import the terminal scheme first, then:

```bash
./cosmic/install.sh
```

It backs up each file it replaces to `*.bak` and COSMIC applies it live.

---

## COSMIC Terminal

The desktop `.ron` does not colour ANSI text.

1. COSMIC Terminal → **View → Color schemes…** (not Settings → Appearance)
2. **Dark** tab → **Import** → `cosmic-term/Ember-term.ron`
3. View → Settings → Appearance → Color scheme (dark) → **Ember**

If a profile is set as default, set the scheme on that profile too or the
dropdown will look like it did nothing.

Text on an opaque pane `#1B1E24`, one step lighter than the desktop; ember for
commands, moss for strings and success, amber for warnings, brick for errors,
slate blue for directories, verdigris for symlinks, heather in the magenta slot.

---

## Firefox

Two layers:

1. **Theme (colours):** install
   **[cosmic-ember-theme on addons.mozilla.org](https://addons.mozilla.org/en-US/firefox/addon/cosmic-ember-theme/)**.
   This is the main look.
2. **Stylesheets (optional):** `firefox/chrome/` adds rounded corners, ember
   menu hover, ember accents and blank new-tab tiles on top of the add-on
   (they also work with Firefox's built-in Dark theme):

```bash
./firefox/install.sh
```

Fully quit Firefox afterward. Details and CSS rules: [`firefox/README.md`](firefox/README.md).

---

## Claude Desktop

For the unofficial [claude-desktop-extra](https://github.com/patrickjaja/claude-desktop-extra)
Linux build. `claude-desktop/cosmic-ember.jsonc` is one theme with both halves:
Ember in dark mode and Ember Day in light mode, so Claude's own
Settings → Appearance toggle switches between the two rices.
Colours come from `cosmic/*.ron`, contrast-checked (text and button text
4.5:1, accents 3:1).

```bash
./claude-desktop/install.sh
```

It copies the file into `~/.config/Claude/themes.d/`. Then press
<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> in Claude Desktop and pick **cosmic-ember**.

The same file ships in [`cosmic-emberday-theme`](https://github.com/atraxsrc/cosmic-emberday-theme),
so installing from either repo gives the same theme.

---

## Cursor

**Oxygen** cursors recoloured to the icon colours, one per folder colour of the
Cosmic-Ember icon pack: **Ember** (default), **Moss**, **Slate**, **Dark**. Patched with the cursor
names COSMIC apps ask for. COSMIC has no cursor picker yet, so the script sets it:

```bash
./cursors/install.sh                      # Ember
./cursors/install.sh --variant Moss      # or another, see --list
reboot
```

Previews, what the script touches, how to undo it and how the colours are made:
[`cursors/README.md`](cursors/README.md).

---

## Scripts

### `update_system.sh`

nala + flatpak updater. Headers in fog, info in ember, success in moss,
errors in brick; the banner ramps from fog to ember.

```bash
chmod +x scripts/update_system.sh
./scripts/update_system.sh
```

### `switch.sh`

Makes this the active rice, coming from any other `cosmic-*-theme` rice. See
[Switching rice](#switching-rice).

```bash
./scripts/switch.sh
```

---

## Terminal extras

Need a [Nerd Font](https://www.nerdfonts.com/) in the terminal for the icons
(Maple Mono NFM in the baseline above).

### zsh

`zsh/ember.zsh` is the rice part of the shell only: `LS_COLORS` (also used
by lsd for file names and by the completion menu), ember / moss / brick colours
for zsh-autosuggestions and zsh-syntax-highlighting, lsd / bat / nvim aliases
and the starship prompt. Your own `~/.zshrc` stays private and sources it.

Needs zsh + [oh-my-zsh](https://ohmyz.sh/). `install.sh` clones the two
plugins if missing and appends one `source` line to `~/.zshrc` (backup in
`~/.zshrc.bak`):

```bash
./zsh/install.sh
exec zsh
```

In `~/.zshrc` keep `ZSH_THEME=""` and
`plugins=(git sudo zsh-autosuggestions zsh-syntax-highlighting)`.

### lsd

File names are coloured by `LS_COLORS` from `zsh/ember.zsh`;
`lsd/colors.yaml` colours the other columns with the 256-colour codes nearest
the palette (173 ember, 108 moss, 179 amber, 167 brick, 110 slate blue, 140 heather, 109 verdigris, 102 muted, 241 comment, 253 text). lsd 1.0.0 does not accept `#hex` there.

```bash
./lsd/install.sh
lsd -l
```

### fastfetch

COSMIC logo in fog, Hardware / Software / Age boxes with ember keys. `install.sh` backs up any existing `~/.config/fastfetch/config.jsonc`
to `config.jsonc.bak`.

```bash
./fastfetch/install.sh
fastfetch
```

### Neovim

`nvim/` carries the rice into Neovim: a dependency-free colorscheme built from
the palette (`nvim/colors/ember.lua`), a LazyVim spec that selects it and makes
sure the treesitter parsers for the files in this repo are installed
(`nvim/lua/plugins/ember.lua`), and a small RON syntax file so `cosmic/*.ron`
is coloured even before the parser exists. `:terminal` inside Neovim uses the
Ember scheme.

```bash
./nvim/install.sh
# restart nvim; LazyVim switches to the ember colorscheme
```

Without LazyVim, copy `nvim/colors/ember.lua` to `~/.config/nvim/colors/` and
run `:colorscheme ember`.

### btop

`btop/Ember.theme`: terminal background, fog boxes, ember highlights,
meters ramp from moss through ember to brick.

```bash
mkdir -p ~/.config/btop/themes
cp btop/Ember.theme ~/.config/btop/themes/
# btop → Esc → Options → Color theme → Ember
```

---

## Icons and wallpaper

Icons: **Cosmic-Ember**, the [Kanagawa icons](https://github.com/Fausto-Korpsvart/Kanagawa-GKT-Theme/tree/main/icons/Kanagawa)
by Fausto Korpsvart recolored to this palette in
[cosmic-icon-themes](https://github.com/atraxsrc/cosmic-icon-themes). Folders come in
four colors: **Ember** `#E8713A` (default), **Moss** `#8FB58A`, **Slate** `#7FA3C7` and **Dark** `#2A2F38` with ember symbols.

![Cosmic-Ember folder colors](https://raw.githubusercontent.com/atraxsrc/cosmic-icon-themes/main/previews/Cosmic-Ember.png)

```bash
./icons/install.sh                  # Ember folders
./icons/install.sh --variant Moss   # or Slate, Dark
```

It downloads the pack from the latest cosmic-icon-themes release, checks its SHA256,
installs it to `~/.local/share/icons/` and sets it as the COSMIC icon theme.
Restart open apps afterwards (Files caches icons per window).
Add `--system` to install to `/usr/share/icons` instead, for every user and the
login screen (asks for sudo); `--user` is the default.

One wallpaper in [`backgrounds/`](backgrounds/): `ember-anime.png`, the one in
the screenshot. Any other source renders in the Ember palette with one command:

```bash
./scripts/wallpaper.sh night path/to/source.png backgrounds/ember-<subject>.png
```

Settings → Wallpaper → Add image → a file from `backgrounds/`.

---

## Setup

```bash
git clone https://github.com/atraxsrc/cosmic-ember-theme.git
cd cosmic-ember-theme

# desktop
# Settings → Appearance → Dark → Import cosmic/Ember.ron

# terminal
# View → Color schemes → Dark tab → Import cosmic-term/Ember-term.ron

# firefox add-on
# https://addons.mozilla.org/en-US/firefox/addon/cosmic-ember-theme/

# optional extra chrome
./firefox/install.sh

# claude desktop (claude-desktop-extra), then Ctrl+Shift+T
./claude-desktop/install.sh

# cursor (then reboot), see cursors/README.md
./cursors/install.sh                      # Ember
./cursors/install.sh --variant Moss      # or another, see --list
reboot

# shell + terminal tools (see Terminal extras above)
./zsh/install.sh
./lsd/install.sh
./fastfetch/install.sh
./nvim/install.sh
mkdir -p ~/.config/btop/themes && cp btop/Ember.theme ~/.config/btop/themes/

# wallpaper
# Settings → Wallpaper → Add image → a file from backgrounds/ (see Icons and wallpaper above)

./icons/install.sh                  # icons (see Icons above)
# Maple fonts (https://github.com/subframe7536/maple-font), then
# fonts, icons, terminal look from cosmic/config/
./cosmic/install.sh
```

---

## Switching rice

The installers replace what they own (with a `*.bak` copy), but some things
pile up instead: shell layers in `~/.zshrc`, LazyVim specs, the updater link and
btop's theme setting. `scripts/switch.sh` makes Ember the active rice, coming
from any other `cosmic-*-theme` rice, and tidies those up. Every rice repo ships
the same script.

```bash
./scripts/switch.sh               # switch, icons included
./scripts/switch.sh --keep-icons  # leave the current icon theme alone
```

It backs up every file it replaces, then:

- copies `cosmic/config/` (fonts, terminal look, icons) and turns dark mode on.
  The terminal scheme and icon theme are only replaced when the current one is
  not already a Ember variant, so a dimmer text variant or another icon colour
  survives. Icons stay as they are until `./icons/install.sh` has run.
- runs the fastfetch, lsd, Neovim and Claude Desktop installers and sets
  btop's `color_theme`
- moves every other rice's LazyVim spec from `~/.config/nvim/lua/plugins/` to
  `~/.config/nvim/rice-off/` (LazyVim loads every spec there and the last
  colorscheme wins); your own specs stay
- comments out every other rice's `source` line in `~/.zshrc`, and
  cosmic-ricing's `ricing/zsh/colors.zsh`, then uncomments Ember's or adds it
- points `~/update_system.sh` at this repo's updater
- installs the Firefox userChrome / userContent

The GUI steps it cannot do are printed at the end:

1. Settings → Desktop → Appearance → Dark → Import `cosmic/Ember.ron`
2. COSMIC Terminal → View → Color schemes → Dark tab → Import `cosmic-term/Ember-term.ron`
3. Firefox: disable the old rice's add-on, enable [this one](https://addons.mozilla.org/en-US/firefox/addon/cosmic-ember-theme/), restart Firefox
4. Settings → Wallpaper → a file from `backgrounds/`
5. Claude Desktop: Ctrl+Shift+T → `cosmic-ember`, Appearance → Dark
6. `exec zsh`, restart Neovim (`:Lazy clean` drops plugins only the old specs used)

Skip 1 and 2 when that slot already holds Ember. The first time on this rice,
run [Setup](#setup) once for the fonts, icons and cursor.

**Within the pair** (Ember ↔ [Ember Day](https://github.com/atraxsrc/cosmic-emberday-theme)): the two halves
sit side by side in COSMIC's Dark and Light slots and share one Claude Desktop
theme, so after the first time only the Firefox add-on and the wallpaper are
left by hand. Add `--keep-icons` to keep one icon colour across the pair.

Leaving Ember is the next rice's `./scripts/switch.sh`. `./cursors/uninstall.sh`
puts the Pop cursor back.

---

## Stack

| Tool | What it does |
|------|-------------|
| [Pop!_OS 24.04](https://pop.system76.com/) | Base OS by System76 |
| [COSMIC DE](https://system76.com/cosmic) | Desktop environment |
| [oh-my-zsh](https://ohmyz.sh/) | zsh framework + plugins |
| [lsd](https://github.com/lsd-rs/lsd) | `ls` with icons and colours |
| [fastfetch](https://github.com/fastfetch-cli/fastfetch) | System info |
| [btop](https://github.com/aristocratos/btop) | Resource monitor |
| [cosmic-icon-themes](https://github.com/atraxsrc/cosmic-icon-themes) | Icon pack (Cosmic-Ember), recolored from [Kanagawa icons](https://github.com/Fausto-Korpsvart/Kanagawa-GKT-Theme/tree/main/icons/Kanagawa) |
| [Oxygen-Cursors](https://github.com/wo2ni/Oxygen-Cursors) | Cursor, recoloured to the icon colours |

## Credits

- Palette: Ember, designed for this pair (see `docs/superpowers/specs/2026-10-14-ember-design.md`).
- Subjects: [cool-wallpapers](https://github.com/atraxsrc/cool-wallpapers); the eagle from
  [omarchy-thegreek-theme](https://github.com/HANCORE-linux/omarchy-thegreek-theme) (MIT).
- Templates: [cosmic-tokyonight-theme](https://github.com/atraxsrc/cosmic-tokyonight-theme) and
  [cosmic-tokyoday-theme](https://github.com/atraxsrc/cosmic-tokyoday-theme).
  Shared baseline (frosted glass, squared corners, fonts) from
  [cosmic-darkgold-theme](https://github.com/atraxsrc/cosmic-darkgold-theme).
- Other half: [cosmic-emberday-theme](https://github.com/atraxsrc/cosmic-emberday-theme).

## License

MIT. The cursor artwork keeps the terms of KDE's Oxygen project, see
[`cursors/README.md`](cursors/README.md).
