# Irida

Irida is a native COSMIC app for designing a colour palette and applying it to
the desktop and the apps around it. It is named after Iris, the Greek goddess
of the rainbow.

- Pick a palette, draft one from a wallpaper, or edit its 13 roles by hand
  while the preview and the contrast ratios follow every change.
- Apply it to the COSMIC desktop, COSMIC Terminal, btop, Neovim, fastfetch,
  lsd, zsh and cava in one step, and undo it in one step.
- Link a dark and a light palette as a pair, compare with saved versions and
  snapshots.
- Export a palette as a complete rice repository.

## Install

Download the .deb from the GitHub releases page and install it:

```sh
sudo apt install ./irida_0.1.0-1_amd64.deb
```

Irida then shows in the launcher. It needs a graphical session to open its
window; the commands below work anywhere.

## Use

`irida` with no arguments opens the app. The same binary has these commands:

```
irida check <palette>                    contrast table; exit 1 on a failure
irida apply <palette> [--only btop,nvim]
irida undo
irida export <palette> <folder>
irida draft <image> [--mode dark|light] [--name NAME]
irida list
```

- `check` prints the palette's contrast against its background and pane.
- `apply` writes the palette to every installed app, or only the ones named.
- `undo` puts back the files from the most recent apply.
- `export` writes a `cosmic-<slug>-theme` rice repository.
- `draft` makes a palette from a JPEG, PNG or WebP image and saves it.
- `list` lists the saved palettes.

`<palette>` is a saved palette's slug, a path to a `palette.toml`, or a rice
repository folder. Saved palettes live in `~/.local/share/irida/palettes`.

## Build

Irida needs Rust 1.93 or newer and the libraries libcosmic builds against
(on Ubuntu: `pkg-config libxkbcommon-dev libwayland-dev libexpat1-dev
libfontconfig-dev libfreetype-dev cmake`).

```sh
cargo build --release
```

To build the .deb, install cargo-deb (`cargo install cargo-deb --locked`) and
set the maintainer for this build only, as `Your Name <you AT example.com>`
with a real at-sign in place of `AT`:

```sh
DEB_MAINTAINER="..." scripts/deb.sh
scripts/check-deb.sh target/debian/irida_0.1.0-1_amd64.deb
```

`scripts/check-deb.sh` checks the files and their modes, the desktop entry,
the AppStream data and, when it is installed, lintian.

## License

MIT, see [LICENSE](LICENSE).
