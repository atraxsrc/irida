# Irida core and engine implementation plan (plan 1 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A headless `irida` binary that checks palette contrast, applies a
palette to COSMIC Terminal and btop through the data-driven target engine, and
undoes the last apply.

**Architecture:** A Cargo workspace with `irida-core` (all logic, no GUI, every
module testable against a temporary home folder) and `irida` (a thin `clap`
front end). Targets are folders of `target.toml` plus templates, embedded at
build time; COSMIC Terminal is a built-in native target that writes cosmic
config key files. Every apply is one transaction with a record that rollback
and undo replay.

**Tech Stack:** Rust 2024, `cosmic-theme` and `cosmic-config` from libcosmic
(git, pinned), `hex_color` 3, `minijinja` 2, `serde`, `toml`, `ron` 0.12,
`include_dir`, `thiserror`, `nix` (signals), `clap`, `tempfile`, `assert_cmd`.

**Spec:** `docs/superpowers/specs/2026-10-16-irida-design.md` (amended
2026-10-16). This plan covers milestones 1 and 2 and the first two built-in
targets. **Plan 2:** the other built-in targets (COSMIC desktop and mode,
Neovim, fastfetch, lsd, zsh, cava), rice import (including rice repo folders
as `<palette>`), rice export, the .deb. **Plan 3:** the libcosmic app, named
snapshots, compare, pairs in the UI, try it live.

## Global Constraints

- Workspace `edition = "2024"`, `rust-version = "1.93"`.
- libcosmic git dependency pinned to `rev = "e1f2cd2e52226aa50d59bb2cd6d43ffe290106ed"` for every crate taken from it.
- No separate `palette` dependency: use `cosmic_theme::palette`.
- Colours Irida writes are uppercase `#RRGGBB`.
- `mix(a, b, t)` = `a + (b - a) * t` per channel in `f64`, `round_ties_even`, clamp 0..=255.
- Contrast: luminance threshold 0.03928, coefficients 0.2126 / 0.7152 / 0.0722, ratio `(L1 + 0.05) / (L2 + 0.05)`; verdicts on the raw ratio, display `{:.1}:1`.
- Contrast minimums: 4.5 for text, accent, success, warning, error, blue, magenta, cyan; 3.0 for muted; none for comment and the three surfaces; checked against `background` and `pane`.
- Slugs match `^[a-z]+$`.
- Block markers are `<comment> >>> irida >>>` and `<comment> <<< irida <<<`, each on its own line.
- Paths: `~/.config/` maps to `$XDG_CONFIG_HOME` when set; other `~/` paths map to `$HOME`.
- Storage: `$XDG_DATA_HOME/irida/palettes/<slug>/palette.toml`, `.../versions/<n>.toml`, `$XDG_CONFIG_HOME/irida/settings.toml`, `$XDG_CONFIG_HOME/irida/targets/<name>/`, `$XDG_STATE_HOME/irida/applies/<n>/`.
- Keep the last 10 apply records.
- Tests never read, write or signal anything outside a `tempfile::TempDir`: paths come from `Dirs::under(tmp)`, `Ctx.path` is empty or a temp `bin/`, and process names used in signal tests are unique test-only copies of `/bin/sleep`.
- Shell commands run through the absolute `/bin/sh`.
- No em-dashes or en-dashes in code, comments, output strings or commit messages. Commits carry no `Co-Authored-By` trailer and no email address.
- Stage files by explicit path only. The sandbox mounts `/dev/null` placeholders such as `.bashrc` and `.claude/*` in the repo root; they must never be added.
- Every task that adds a module also adds its `pub mod` line to `irida-core/src/lib.rs` and commits it.

**Environment (this machine only).** `~/.cargo` and `~/.gitconfig` are not
writable or readable from the sandbox, so every cargo command runs with:

```bash
export RUSTUP_HOME=/home/0xdev1/.rustup CARGO_HOME=<scratchpad>/cargo-home \
       HOME=<scratchpad>/home GIT_CONFIG_GLOBAL=/dev/null CARGO_NET_GIT_FETCH_WITH_CLI=true
```

with network access to `index.crates.io`, `static.crates.io`, `github.com`,
`codeload.github.com`. Git commands use `GIT_CONFIG_GLOBAL=/dev/null` and the
repo's existing author (`git log --format='%an <%ae>' -1`).

## Review Focus

1. **A user's config is a symlink into a dotfiles repo.** Apply must write through the link and keep the link; rollback and undo must restore through it too. Tests in Tasks 10, 13 and 14.
2. **The user edited the file between apply and undo.** Undo of a `block` or `key` write restores only Irida's part and keeps the user's other edits. Tests in Tasks 13 and 14.
3. **The user already has COSMIC Terminal schemes or a default profile.** Apply must keep every other scheme and id and set the profile's theme for both halves of a pair; a config with a field Irida does not know must stop that target with a clear error and write nothing to it. Tests in Tasks 11 and 14.
4. **btop 1.3.0 is running.** SIGUSR2 would kill it; reload must report "restart btop" instead. Test in Task 12.
5. **A write fails halfway through an apply.** Everything already written is restored byte for byte, files Irida created are deleted, the record survives if restoring fails, and the error names the file. Test in Task 14.

---

## File structure

```
Cargo.toml                          workspace, shared dependency versions
.gitignore                          target/
irida-core/Cargo.toml
irida-core/build.rs                 rebuild when targets/ changes
irida-core/src/lib.rs               module list
irida-core/src/color.rs             Rgb, hex, mix
irida-core/src/role.rs              Role, RoleGroup, minimums
irida-core/src/palette.rs           Mode, Palette, palette.toml (de)serialisation
irida-core/src/slot.rs              Ansi, Tone, Slot, PinValue, Pins
irida-core/src/term.rs              COSMIC Terminal ColorScheme mirror (RON)
irida-core/src/derive.rs            TermColors, Derived, derive()
irida-core/src/contrast.rs          luminance, ratio, check()
irida-core/src/ramp.rs              OKLab ramp
irida-core/src/dirs.rs              Dirs, expand
irida-core/src/store.rs             palettes, versions, settings
irida-core/src/target.rs            TargetDef, Target, loading, detection
irida-core/src/template.rs          minijinja environment and context
irida-core/src/write.rs             atomic_write, block and key edits
irida-core/src/native/mod.rs        NativeKind dispatch
irida-core/src/native/cosmic_term.rs
irida-core/src/reload.rs            signal, command, version guard
irida-core/src/record.rs            apply records on disk
irida-core/src/apply.rs             apply transaction, rollback, undo
irida-core/targets/cosmic-term/target.toml
irida-core/targets/btop/{target.toml,btop.theme.tmpl,color_theme.tmpl}
irida-core/tests/common/mod.rs      TestEnv, fixture loaders
irida-core/tests/fixtures/palettes/{ember,emberday,noir,dossier}.toml
irida-core/tests/fixtures/rices/{Ember,EmberDay,Noir,Dossier}-term.ron
irida-core/tests/fixtures/rices/{Ember,EmberDay}.theme
irida-core/tests/<module>.rs        one integration test file per module
irida/Cargo.toml
irida/src/main.rs                   clap definitions and dispatch
irida/src/output.rs                 text formatting of reports
irida/tests/cli.rs
```

---

### Task 1: Workspace and the colour type

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `irida-core/Cargo.toml`, `irida-core/src/lib.rs`, `irida-core/src/color.rs`, `irida/Cargo.toml`, `irida/src/main.rs`
- Test: `irida-core/tests/color.rs`

**Interfaces:**
- Produces: `irida_core::color::Rgb { pub r: u8, pub g: u8, pub b: u8 }` (`Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash`); `Rgb::from_hex(s: &str) -> Result<Rgb, ColorError>` (accepts `#RRGGBB` and `RRGGBB`, any case); `Rgb::to_hex(&self) -> String` (`#RRGGBB` uppercase); `impl Display` (same as `to_hex`); `impl FromStr`; serde as the hex string; `pub const WHITE: Rgb`, `pub const BLACK: Rgb`; `pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb`; `ColorError::InvalidHex(String)` via `thiserror`.

- [ ] **Step 1: Scaffold the workspace.** Root `Cargo.toml` with `members = ["irida-core", "irida"]`, `resolver = "3"`, `[workspace.package] edition = "2024"`, `rust-version = "1.93"`, `license = "MIT"`, and `[workspace.dependencies]`: `cosmic-theme` and `cosmic-config` (git `https://github.com/pop-os/libcosmic`, pinned rev, `default-features = false`), `hex_color = { version = "3", features = ["serde"] }`, `minijinja = "2"`, `toml = "0.9"`, `ron = "0.12"`, `serde = { version = "1", features = ["derive"] }`, `thiserror = "2"`, `include_dir = "0.7"`, `nix = { version = "0.30", features = ["signal"] }`, `clap = { version = "4", features = ["derive"] }`, `anyhow = "1"`, `tempfile = "3"`, `assert_cmd = "2"`, `predicates = "3"`. `irida-core` depends on `cosmic-theme`, `hex_color`, `minijinja`, `serde`, `toml`, `ron`, `include_dir`, `thiserror`, `nix`, with `cosmic-config` and `tempfile` as dev-dependencies. `irida` depends on `irida-core`, `clap`, `anyhow`, with `assert_cmd`, `predicates`, `tempfile` as dev-dependencies; its `main.rs` is `fn main() {}` for now. `.gitignore` holds `target/`.

- [ ] **Step 2: Write the failing tests** in `irida-core/tests/color.rs`:

```rust
use irida_core::color::{mix, Rgb, BLACK, WHITE};

#[test]
fn hex_round_trip() {
    let c = Rgb::from_hex("#e8713a").unwrap();
    assert_eq!(c, Rgb { r: 0xE8, g: 0x71, b: 0x3A });
    assert_eq!(c.to_hex(), "#E8713A");
    assert_eq!(Rgb::from_hex("E8713A").unwrap(), c);
}

#[test]
fn bad_hex_is_an_error() {
    for s in ["", "#12345", "#1234567", "#GGGGGG", "#E8713AFF"] {
        assert!(Rgb::from_hex(s).is_err(), "{s}");
    }
}

#[test]
fn mix_rounds_half_to_even() {
    // Ember Day fastfetch box line: mix(text, pane, 0.70); blue is exactly 162.5.
    let text = Rgb::from_hex("#2B2E36").unwrap();
    let pane = Rgb::from_hex("#DDD9D1").unwrap();
    assert_eq!(mix(text, pane, 0.70).to_hex(), "#A8A6A2");
}

#[test]
fn mix_endpoints_and_derivation_examples() {
    let red = Rgb::from_hex("#D85A5A").unwrap();
    assert_eq!(mix(red, WHITE, 0.0), red);
    assert_eq!(mix(red, WHITE, 1.0), WHITE);
    assert_eq!(mix(red, WHITE, 0.12).to_hex(), "#DD6E6E");
    assert_eq!(mix(Rgb::from_hex("#9E3A3A").unwrap(), BLACK, 0.15).to_hex(), "#863131");
}
```

- [ ] **Step 3: Run** `cargo test -p irida-core --test color`. Expected: compile error, `color` module missing.

- [ ] **Step 4: Implement `irida-core/src/color.rs`**; `lib.rs` declares `pub mod color;`. `mix` computes each channel as `(a as f64 + (b as f64 - a as f64) * t).round_ties_even().clamp(0.0, 255.0) as u8`.

- [ ] **Step 5: Run** `cargo test -p irida-core --test color` and `cargo build --workspace`. Expected: 4 passed; the workspace, including the libcosmic git crates, builds.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore irida-core/Cargo.toml irida-core/src/lib.rs irida-core/src/color.rs irida-core/tests/color.rs irida/Cargo.toml irida/src/main.rs
git commit -m "feat(core): workspace and Rgb colour type with half-to-even mix"
```

---

### Task 2: Roles and the palette file

**Files:**
- Create: `irida-core/src/role.rs`, `irida-core/src/palette.rs`, `irida-core/src/slot.rs` (stub), `irida-core/tests/common/mod.rs`, `irida-core/tests/fixtures/palettes/{ember,emberday,noir,dossier}.toml`
- Modify: `irida-core/src/lib.rs`
- Test: `irida-core/tests/palette.rs`

**Interfaces:**
- Consumes: `Rgb` (Task 1).
- Produces:
  - `enum Role { Background, Pane, Container, Text, Muted, Comment, Accent, Success, Warning, Error, Blue, Magenta, Cyan }` (`Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash`), `Role::ALL: [Role; 13]` in that order, `Role::as_str(self) -> &'static str` (lowercase), `impl Display` (via `f.pad(self.as_str())`, so width specifiers work), `FromStr`, `Role::group(self) -> RoleGroup` (`Surfaces | Text | Accent | Status | Hues`), `Role::min_contrast(self) -> Option<f64>`.
  - `enum Mode { Dark, Light }`, serde lowercase, `Display` lowercase.
  - `struct Palette { pub name: String, pub slug: String, pub mode: Mode, pub pair: Option<String>, pub notes: Option<String>, pub roles: Roles, pub names: BTreeMap<Role, String>, pub pins: Pins }`; `Palette`, `Roles`, `Mode` derive `Debug, Clone, PartialEq, Eq`.
  - `struct Roles([Rgb; 13])` with `get(Role) -> Rgb`, `set(Role, Rgb)`.
  - Stub in `slot.rs`: `#[derive(Debug, Clone, Default, PartialEq, Eq)] pub struct Pins(BTreeMap<String, String>)`, replaced in Task 3 with the same derives.
  - `Palette::from_toml(s: &str) -> Result<Palette, PaletteError>`, `Palette::to_toml(&self) -> String`, `Palette::stem(&self) -> String` (name with spaces removed), `validate_slug(s: &str) -> Result<(), PaletteError>`.
  - `PaletteError::{Toml(String), MissingRole(Role), UnknownRole(String), BadSlug(String), BadColor { key: String, value: String }}` (`Debug, PartialEq`).
  - Test helper `tests/common/mod.rs`: `pub fn fixture(rel: &str) -> PathBuf` (under `tests/fixtures`), `pub fn fixture_text(rel: &str) -> String`, `pub fn palette(slug: &str) -> Palette`.

- [ ] **Step 1: Write the four fixture palettes** from these values (role order as in `Role::ALL`):

| slug | name | mode | pair | background | pane | container | text | muted | comment | accent | success | warning | error | blue | magenta | cyan |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ember | Ember | dark | emberday | #15171C | #1B1E24 | #22262E | #D6D9E0 | #7C8290 | #5A6070 | #E8713A | #8FB58A | #D9A441 | #D85A5A | #7FA3C7 | #B08BC4 | #7FB7B3 |
| emberday | Ember Day | light | ember | #E9E6DF | #DDD9D1 | #E2DED6 | #2B2E36 | #6B6F78 | #8C9099 | #A4441A | #4C6B45 | #8A5E12 | #9E3A3A | #2F5C8A | #6E4E8C | #2E6E6A |
| noir | Noir | dark | dossier | #171717 | #171717 | #222222 | #CCD0CF | #7D837D | #686C69 | #F25623 | #88A57D | #F56E0F | #F25623 | #F56E0F | #F56E0F | #7B837B |
| dossier | Dossier | light | noir | #D0D0C8 | #C7C7C0 | #DCDCD4 | #242424 | #59574D | #6B7360 | #953B19 | #51573B | #754E3E | #A80026 | #43432B | #754E3E | #6B1F2F |

`[names]` for ember: Night, Pane, Soot, Fog, Muted, Comment, Ember, Moss, Amber, Brick, Slate blue, Heather, Verdigris. For emberday: Stone, Pane, Linen, Ink, Muted, Comment, Ember, Moss, Amber, Brick, Slate blue, Heather, Verdigris. Noir and Dossier have no `[names]`. No fixture has `[pins]` yet (Task 3 adds Ember's).

- [ ] **Step 2: Write the failing tests** in `irida-core/tests/palette.rs`:
  - `loads_all_fixtures`: each of the four loads; `palette("ember").roles.get(Role::Accent).to_hex() == "#E8713A"`; `palette("emberday").mode == Mode::Light`; `palette("emberday").stem() == "EmberDay"`; `palette("ember").names[&Role::Background] == "Night"`.
  - `toml_round_trip`: `assert_eq!(Palette::from_toml(&p.to_toml()).unwrap(), p)` for all four.
  - `missing_role_is_named`: removing the `cyan` line gives `Err(PaletteError::MissingRole(Role::Cyan))`.
  - `unknown_role_is_named`: adding `orange = "#FF8800"` under `[roles]` gives `Err(PaletteError::UnknownRole(ref s)) if s == "orange"`.
  - `slug_rules`: `validate_slug` accepts `"ember"`, rejects `"Ember"`, `"ember-day"`, `"ember2"`, `""`; `from_toml` with `slug = "ember-day"` gives `BadSlug`.
  - `minimums_and_display`: `Role::Text.min_contrast() == Some(4.5)`, `Role::Muted.min_contrast() == Some(3.0)`, `Role::Comment.min_contrast() == None`, `Role::Pane.min_contrast() == None`; `format!("{:<8}|", Role::Error) == "error   |"`.

- [ ] **Step 3: Run** `cargo test -p irida-core --test palette`. Expected: compile errors.

- [ ] **Step 4: Implement `role.rs` and `palette.rs`.** Parse through a private raw struct (`#[serde(deny_unknown_fields)]` at top level, with `roles` and `names` as `BTreeMap<String, String>` and `pins` as `BTreeMap<String, String>` defaulting to empty) and convert, so missing and unknown roles produce the named errors instead of serde messages. `to_toml` writes `name, slug, mode, pair, notes, [roles], [names], [pins]` with roles in `Role::ALL` order, omitting empty optional parts.

- [ ] **Step 5: Run** `cargo test -p irida-core --test palette`. Expected: 6 passed.

- [ ] **Step 6: Commit** `feat(core): roles and palette.toml model` with every file listed above.

---

### Task 3: Slots, pins, the terminal scheme mirror and derivation

**Files:**
- Create: `irida-core/src/term.rs`, `irida-core/src/derive.rs`, `irida-core/tests/fixtures/rices/{Ember,EmberDay,Noir,Dossier}-term.ron` (copied verbatim from the rice repos' `cosmic-term/<Stem>-term.ron`)
- Modify: `irida-core/src/slot.rs` (replace the stub), `irida-core/src/palette.rs` (parse and write `[pins]`, mapping pin errors to `PaletteError::BadColor`), `irida-core/src/lib.rs`, `irida-core/tests/fixtures/palettes/ember.toml` (add `[pins]` with `"normal.black" = "#2A2F38"` and `"cosmic.neutral_10" = "#E6E8EE"`)
- Test: `irida-core/tests/derive.rs`, `irida-core/tests/term.rs`

**Interfaces:**
- Consumes: `Rgb`, `mix`, `Palette`, `Role`, `Mode`.
- Produces:
  - `enum Ansi { Black, Red, Green, Yellow, Blue, Magenta, Cyan, White }`, `Ansi::ALL: [Ansi; 8]`, `as_str`.
  - `enum Tone { Normal, Bright, Dim }`, `as_str`.
  - `enum Slot { Ansi(Tone, Ansi), Foreground, Background, Cursor, BrightForeground, DimForeground, CosmicNeutral10, CosmicGray2 }` (`Copy, Eq, Ord, Hash, Debug`) with `Display`/`FromStr` using the spec names (`normal.black`, `bright_foreground`, `cosmic.neutral_10`), and `Slot::ALL_TERM: [Slot; 29]`.
  - `enum PinValue { Color(Rgb), None }`; `struct Pins(BTreeMap<Slot, PinValue>)` (`Debug, Clone, Default, PartialEq, Eq`) with `get(Slot) -> Option<PinValue>`, `set(Slot, PinValue)`, `remove(Slot)`, `clear()`, `is_pinned(Slot) -> bool`. `"none"` is valid only for `background`.
  - `struct AnsiSet([Rgb; 8])` with `get(Ansi) -> Rgb`.
  - `struct TermColors { pub foreground: Rgb, pub background: Option<Rgb>, pub cursor: Rgb, pub bright_foreground: Rgb, pub dim_foreground: Rgb, pub normal: AnsiSet, pub bright: AnsiSet, pub dim: AnsiSet }`.
  - `struct Derived { pub term: TermColors, pub neutral_10: Rgb, pub gray_2: Rgb }`, `Derived::slot(&self, s: Slot) -> Option<Rgb>` (`None` only for an unset background).
  - `pub fn derive(p: &Palette) -> Derived`.
  - `term::ColorScheme { pub name: String, pub foreground, background, cursor, bright_foreground, dim_foreground: Option<HexColor>, pub normal, bright, dim: AnsiScheme }` and `term::AnsiScheme { pub black .. white: Option<HexColor> }` (`hex_color::HexColor`, so schemes Irida did not write, including ones with alpha, round-trip unchanged), both `#[serde(default, deny_unknown_fields)]` with `Debug, Clone, Default, PartialEq, Eq`. Colour fields use custom `serialize_with`/`deserialize_with` that write and read a bare string, as upstream's `ser_color_opt`/`de_color_opt` do, with `skip_serializing_if = "Option::is_none"`; an `AnsiScheme` with every colour `None` is skipped. `ColorScheme::from_ron(&str) -> Result<ColorScheme, ron::error::SpannedError>`, `ColorScheme::to_ron(&self) -> String` (`ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::new())`), `ColorScheme::slot(&self, Slot) -> Option<Rgb>`, `TermColors::to_scheme(&self, name: &str) -> ColorScheme`.

- [ ] **Step 1: Write the failing tests.** `tests/term.rs`:
  - `fixtures_round_trip`: for each fixture `.ron`, `from_ron(&text).unwrap().to_ron()` equals `text` with its final newline removed, and contains no `Some(`.
  - `noir_has_no_background`: `from_ron(Noir-term.ron).background == None`.
  - `unknown_field_is_rejected`: inserting `    italic: "#FFFFFF",` after the `name` line makes `from_ron` fail.

  `tests/derive.rs` (expected scheme: `ColorScheme::from_ron(&fixture_text("rices/Ember-term.ron")).unwrap()`):
  - `ember_rule_within_two`: with `pins.clear()`, every slot in `Slot::ALL_TERM` differs from the file by at most 2 per channel, and the only slots that differ at all are `normal.black` (derived `#2B2F37`) and `dim.black` (derived `#25282F`).
  - `ember_with_pin_is_exact`: with the fixture's pins, all 29 slots equal the file.
  - `emberday_is_exact`: all 29 slots equal `EmberDay-term.ron`, with no pins.
  - `pins_propagate`: on Ember, pinning `normal.red = #FF0000` makes `dim.red == #A40C0E` and `bright.red == #FF1F1F`; pinning `bright.white = #FFFFFF` makes `bright_foreground == #FFFFFF`; pinning `normal.white = #FFFFFF` leaves `bright.white == #E2E4E9` (dark `bright.black` and `bright.white` come from `muted` and `text`, not from normal); on Ember Day pinning `bright.black = #000000` makes `bright_foreground == #000000`.
  - `background_none`: pinning `background = "none"` gives `derive(p).term.background == None` and `to_scheme("x").background == None`.
  - `cosmic_defaults`: Ember gives `neutral_10 == #E6E8EE` (pinned) and `gray_2 == #2A2F38` (effective `normal.black`); Ember Day gives `neutral_10 == #2B2E36` (text) and `gray_2 == #DDD9D1` (pane).
  - `pins_toml_round_trip`: Ember's `[pins]` survives `to_toml`/`from_toml`; `"dim.blue" = "none"` and `"bright.orange" = "#FFFFFF"` are each `Err(PaletteError::BadColor { .. })`.

- [ ] **Step 2: Run** `cargo test -p irida-core --test term --test derive`. Expected: compile errors.

- [ ] **Step 3: Implement.** Derivation order, applying pins at each stage so later stages see effective values:
  1. normal: dark `black = mix(container, text, 0.05)`, light `black = text`; red, green, yellow = error, success, warning; blue, magenta, cyan = the hue roles; dark `white = text`, light `white = muted`. Apply `normal.*` pins.
  2. bright: dark red to cyan `mix(effective normal, #FFFFFF, 0.12)`, `black = muted`, `white = mix(text, #FFFFFF, 0.29)`; light all eight `mix(effective normal, #000000, 0.15)`. Apply `bright.*` pins.
  3. dim, all eight: `mix(effective normal, pane, 0.40)`. Apply `dim.*` pins.
  4. `foreground = text`, `background = Some(pane)`, `cursor = accent`, `bright_foreground` = effective `bright.white` (dark) or `bright.black` (light), `dim_foreground = muted`. Apply pins.
  5. `neutral_10 = text`, `gray_2` = effective `normal.black` (dark) or `pane` (light). Apply `cosmic.*` pins.

- [ ] **Step 4: Run** `cargo test -p irida-core`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): derived terminal slots, pins and COSMIC Terminal scheme mirror` with every file listed above.

---

### Task 4: Contrast

**Files:**
- Create: `irida-core/src/contrast.rs`
- Modify: `irida-core/src/lib.rs`
- Test: `irida-core/tests/contrast.rs`

**Interfaces:**
- Consumes: `Rgb`, `Palette`, `Role`.
- Produces: `pub fn luminance(c: Rgb) -> f64`; `pub fn ratio(a: Rgb, b: Rgb) -> f64`; `struct RoleContrast { pub role: Role, pub color: Rgb, pub on_background: f64, pub on_pane: f64, pub min: Option<f64> }` with `fails_on_background(&self) -> bool`, `fails_on_pane(&self) -> bool`, `passes(&self) -> bool`; `struct ContrastReport { pub rows: Vec<RoleContrast> }` (the 10 non-surface roles in `Role::ALL` order) with `failures(&self) -> Vec<&RoleContrast>`; `pub fn check(p: &Palette) -> ContrastReport`.

- [ ] **Step 1: Write the failing tests** (`{:.1}` of `(on_background, on_pane)` per role, written `bg/pane`):
  - `known_pairs`: `ratio(WHITE, BLACK) == 21.0` (to 1e-9); `ratio(#777777, #FFFFFF)` is `4.478` to 3 places; `ratio(a, b) == ratio(b, a)`.
  - `ember_matches_contrast_py`: text 12.7/11.8, accent 5.9/5.4, success 7.8/7.3, warning 8.0/7.4, error 4.7/4.4, blue 6.8/6.3, magenta 6.3/5.8, cyan 8.0/7.4, muted 4.7/4.3, comment 2.9/2.7.
  - `emberday_matches_contrast_py`: text 10.9/9.6, accent 4.9/4.4, success 4.8/4.3, warning 4.6/4.0, error 5.4/4.8, blue 5.6/4.9, magenta 5.4/4.8, cyan 4.7/4.2, muted 4.0/3.6, comment 2.6/2.3.
  - `noir_dossier_fixture_table`: Noir text 11.5/11.5, muted 4.6/4.6, comment 3.4/3.4, accent 5.2/5.2, success 6.6/6.6, warning 6.1/6.1, error 5.2/5.2, blue 6.1/6.1, magenta 6.1/6.1, cyan 4.6/4.6; Dossier text 10.0/9.1, muted 4.7/4.3, comment 3.2/2.9, accent 4.6/4.2, success 4.9/4.5, warning 4.6/4.2, error 5.0/4.6, blue 6.5/6.0, magenta 4.6/4.2, cyan 7.3/6.6.
  - `verdicts_follow_section_3`: failing roles are Ember `[Error]`; Ember Day `[Accent, Success, Warning, Cyan]`; Noir `[]`; Dossier `[Accent, Success, Warning, Magenta]`; every one of these fails on the pane only.
  - `raw_ratio_decides`: Dossier success on the pane is `4.4516` to 4 places, displays as `"4.5"`, and fails.
  - `comment_never_fails`: a palette whose comment equals its background has no comment failure.

- [ ] **Step 2: Run** `cargo test -p irida-core --test contrast`. Expected: compile errors.

- [ ] **Step 3: Implement** with the constants in Global Constraints.

- [ ] **Step 4: Run** the tests. Expected: 7 passed.

- [ ] **Step 5: Commit** `feat(core): WCAG contrast check against background and pane`.

---

### Task 5: OKLab ramps

**Files:**
- Create: `irida-core/src/ramp.rs`
- Modify: `irida-core/src/lib.rs`
- Test: `irida-core/tests/ramp.rs`

**Interfaces:**
- Produces: `pub fn ramp(from: Rgb, to: Rgb, steps: usize) -> Result<Vec<Rgb>, RampError>`; `RampError::Steps(usize)` for `steps` outside 2..=16.

- [ ] **Step 1: Write the failing tests:** `ramp(a, b, 8)` has 8 entries, first `== a`, last `== b`; `ramp(#000000, #FFFFFF, 5)` has strictly increasing OKLab lightness; `ramp(a, b, 1)` and `ramp(a, b, 17)` are `Err(RampError::Steps(_))`; `ramp(#FF0000, #00FFFF, 16)` returns 16 colours without panicking.

- [ ] **Step 2: Run** `cargo test -p irida-core --test ramp`. Expected: compile errors.

- [ ] **Step 3: Implement** through `cosmic_theme::palette::{Srgb, Oklab, FromColor}`: convert both ends to `Oklab`, interpolate `l`, `a`, `b` linearly at `i / (steps - 1)`, convert back to `Srgb<f32>`, clamp each channel to 0..=1, scale with `(c * 255.0).round_ties_even()`. Return the exact endpoints for `i == 0` and `i == steps - 1`.

- [ ] **Step 4: Run** the tests. Expected: 4 passed.

- [ ] **Step 5: Commit** `feat(core): OKLab colour ramps`.

---

### Task 6: Folders, path expansion and storage

**Files:**
- Create: `irida-core/src/dirs.rs`, `irida-core/src/store.rs`
- Modify: `irida-core/src/lib.rs`, `irida-core/tests/common/mod.rs` (add `TestEnv`)
- Test: `irida-core/tests/store.rs`

**Interfaces:**
- Consumes: `Palette`.
- Produces:
  - `struct Dirs { pub home: PathBuf, pub config: PathBuf, pub data: PathBuf, pub state: PathBuf }` (`Debug, Clone`); `Dirs::from_env() -> Result<Dirs, DirsError>` (`$HOME` required; each XDG variable used only when set and absolute, else `~/.config`, `~/.local/share`, `~/.local/state`); `Dirs::under(root: &Path) -> Dirs` (`root/home`, `root/home/.config`, `root/home/.local/share`, `root/home/.local/state`); `Dirs::expand(&self, p: &str) -> PathBuf` (`~/.config/x` gives `config/x`; other `~/x` gives `home/x`; `$XDG_CONFIG_HOME/x` gives `config/x`; absolute paths unchanged).
  - `struct Store { dirs: Dirs }`; `Store::new(Dirs)`; `list(&self) -> Vec<StoreEntry>` with `StoreEntry { pub slug: String, pub result: Result<Palette, StoreError> }` sorted by slug; `load(&self, slug: &str) -> Result<Palette, StoreError>`; `save(&self, p: &Palette) -> Result<(), StoreError>`; `save_version(&self, p: &Palette) -> Result<u32, StoreError>` (writes `versions/<n>.toml` with `n` = highest existing plus 1, starting at 1, and `palette.toml`); `latest_version(&self, slug: &str) -> Result<Option<(u32, Palette)>, StoreError>`; `resolve(&self, arg: &str) -> Result<Palette, StoreError>`: an existing file is read as a palette file; an existing directory containing `palette.toml` is read from there; an existing directory without one is `NotFound("<dir>: rice repo folders are supported from plan 2")`; anything else is a slug.
  - `struct Settings { pub disabled: BTreeSet<String>, pub last_palette: Option<String> }` (serde, all fields defaulted) with `Store::settings(&self) -> Result<Settings, StoreError>` and `Store::save_settings(&self, s: &Settings) -> Result<(), StoreError>`.
  - `StoreError::{NotFound(String), Unreadable { path: PathBuf, message: String }, Io { path: PathBuf, source: std::io::Error }}` with `Display`.
  - Test helper `TestEnv { pub tmp: TempDir, pub dirs: Dirs }` with `TestEnv::new()` and `TestEnv::write(&self, rel_to_home: &str, contents: &str) -> PathBuf`.
- Writes go through a private temp-file-and-rename helper until Task 10 replaces it with `atomic_write`.

- [ ] **Step 1: Write the failing tests:**
  - `expand_paths`: with `Dirs { home: "/h".into(), config: "/x/cfg".into(), data: "/x/data".into(), state: "/x/state".into() }`, `expand("~/.config/btop/btop.conf") == "/x/cfg/btop/btop.conf"`, `expand("~/.zshrc") == "/h/.zshrc"`, `expand("$XDG_CONFIG_HOME/cava/config") == "/x/cfg/cava/config"`, `expand("/etc/x") == "/etc/x"`.
  - `save_load_list`: save Ember and Ember Day; `list()` slugs are `["ember", "emberday"]`; `load("ember")` equals the fixture.
  - `versions_count_up`: two `save_version` calls return 1 then 2; `latest_version("ember")` returns 2 with the second palette.
  - `unreadable_palette_is_reported_and_untouched`: write `not toml [` to `.../palettes/broken/palette.toml`; `list()` has `broken` with `Err(StoreError::Unreadable { .. })`; the file's bytes are unchanged afterwards.
  - `resolve_forms`: `resolve("ember")` after saving, `resolve(<path to fixture ember.toml>)` and `resolve(<dir holding palette.toml>)` all give Ember; `resolve("nope")` and `resolve(<empty dir>)` are `NotFound`.
  - `settings_round_trip`: default settings when the file is absent; saved `disabled = {"cava"}` reads back.

- [ ] **Step 2: Run** `cargo test -p irida-core --test store`. Expected: compile errors.

- [ ] **Step 3: Implement.** `list`, `load` and `resolve` never write.

- [ ] **Step 4: Run** the tests. Expected: 6 passed.

- [ ] **Step 5: Commit** `feat(core): XDG folders, palette storage, versions and settings`.

---

### Task 7: `irida check` and `irida list`

**Files:**
- Create: `irida/src/output.rs`, `irida/tests/cli.rs`
- Modify: `irida/src/main.rs`
- Test: `irida/tests/cli.rs`

**Interfaces:**
- Consumes: `Store::resolve`, `Store::list`, `contrast::check`, `Dirs::from_env`.
- Produces: `clap` derive `Cli { command: Command }` with `arg_required_else_help = true` (plan 3 replaces this with opening the app) and `Command::{Check { palette: String }, List}` (Task 16 adds `Apply` and `Undo`); `output::format_check(p: &Palette, r: &ContrastReport) -> String`. Exit codes: 0 pass, 1 contrast failure, 2 error (message on stderr, prefixed `irida: `). Rice repo folders as `<palette>` come with import in plan 2.

`format_check` prints `"{name} ({mode})"`, a header line, then one row per `RoleContrast`:

```rust
format!("{:<8} {}  {:>6}  {:>6}  {:<3}  {}",
        row.role.as_str(), row.color.to_hex(), bg, pane, min, verdict)
// bg and pane are format!("{:.1}:1", r); min is "4.5", "3.0" or "-";
// verdict is "", "FAIL on background", "FAIL on pane" or "FAIL on both"
```

with trailing spaces trimmed, then `"{n} failure(s)"` or `"all pass"`.

- [ ] **Step 1: Write the failing tests** in `irida/tests/cli.rs`. Every test runs the binary with `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_STATE_HOME` inside a `TempDir`:
  - `check_ember_fails_on_pane`: `irida check <path to ember.toml>` exits 1; stdout contains `"Ember (dark)"`, the line `"error    #D85A5A   4.7:1   4.4:1  4.5  FAIL on pane"` and `"1 failure(s)"`.
  - `check_noir_passes`: exits 0 and prints `"all pass"`.
  - `check_unknown_palette`: `irida check nope` exits 2 and stderr starts with `"irida: "`.
  - `list_shows_saved`: after copying `ember.toml` to `$XDG_DATA_HOME/irida/palettes/ember/palette.toml`, `irida list` prints `"ember  Ember (dark)"`.
  - `no_args_prints_help`: exits 2 with `Usage` on stderr.

- [ ] **Step 2: Run** `cargo test -p irida --test cli`. Expected: failures.

- [ ] **Step 3: Implement** `main.rs` and `output.rs`; `list` prints `"{slug}  {name} ({mode})"` per readable palette and `"{slug}  unreadable: {message}"` otherwise.

- [ ] **Step 4: Run** the tests. Expected: 5 passed.

- [ ] **Step 5: Commit** `feat(cli): irida check and irida list`.

---

### Task 8: Target definitions, loading and detection

**Files:**
- Create: `irida-core/src/target.rs`, `irida-core/build.rs` (`fn main() { println!("cargo:rerun-if-changed=targets"); }`, because `include_dir!` does not track new files on stable Rust), `irida-core/targets/.keep`
- Modify: `irida-core/src/lib.rs`
- Test: `irida-core/tests/target.rs`

**Interfaces:**
- Consumes: `Dirs`.
- Produces:
  - `struct TargetDef { pub name: String, pub detect: Detect, pub native: Option<NativeKind>, pub files: Vec<FileEntry>, pub reload: Option<Reload>, pub rice: Vec<RiceEntry>, pub live: bool }` with `#[serde(deny_unknown_fields)]`, `#[serde(rename = "file", default)]` on `files`, and `#[serde(default)]` on `detect`, `native`, `reload`, `rice` and `live` (false).
  - `struct Detect { pub binary: Option<String>, pub config: Option<String>, pub flatpak: Option<String> }`, `#[serde(default, deny_unknown_fields)]`, `Default`.
  - `enum WriteKind { File, Block, Key }` (serde lowercase); `struct FileEntry { pub path: String, pub write: WriteKind, pub template: String, #[serde(default)] pub comment: Option<String>, #[serde(default)] pub key: Option<String> }`, `deny_unknown_fields`.
  - `enum NativeKind { CosmicTerm }` (serde `"cosmic-term"`; plan 2 adds the desktop).
  - `struct Reload { pub signal: Option<SignalReload>, pub command: Option<String>, pub min_version: Option<MinVersion> }` (all `#[serde(default)]`), `SignalReload { pub process: String, pub signal: String }`, `MinVersion { pub command: String, pub version: String }`.
  - `struct RiceEntry { pub path: String, pub install: WriteKind, #[serde(default)] pub template: Option<String> }` (parsed and validated now, used in plan 2).
  - `enum Source { Builtin, User(PathBuf) }`; `struct Target { pub def: TargetDef, pub templates: BTreeMap<String, String>, pub source: Source }`.
  - `struct LoadedTarget { pub name: String, pub result: Result<Target, TargetError> }`; `pub fn load_targets(dirs: &Dirs) -> Vec<LoadedTarget>` sorted by name: built-ins from `include_dir!("$CARGO_MANIFEST_DIR/targets")`, then each folder of `config/irida/targets/`; a user target replaces the built-in of the same name. A folder that fails to load is named after the folder.
  - Validation: `block` needs `comment`; `key` needs `key`; every `template` must exist in the folder; `native` is allowed only in built-ins; the folder name must equal `def.name`.
  - `TargetError::{Parse(String), Invalid(String), Io(String)}` with `Display`.
  - `pub fn is_detected(def: &TargetDef, dirs: &Dirs, path: &[PathBuf]) -> bool`: the binary is an executable file in one of `path`; or `dirs.expand(config)` is a folder; or `home/.var/app/<flatpak>/config` is a folder. A target whose `detect` is empty is always detected.

- [ ] **Step 1: Write the failing tests** (user targets written into `TestEnv`):
  - `user_target_loads`: a folder `cava/` holding the spec's example `target.toml` (with `[[rice]]` instead of `[rice]`, no `live`) and a `cava.tmpl` loads with `Source::User`, one `FileEntry` of kind `Block`, comment `"#"`, and `live == false`.
  - `broken_target_is_reported_not_fatal`: a folder `x/` whose `target.toml` is `name = "x"\nbogus = 1` gives `LoadedTarget { name: "x", result: Err(TargetError::Parse(_)) }`, and `cava` still loads.
  - `validation_errors`: a block without comment, a key without key, a missing template file, `native = "cosmic-term"` in a user target, and a folder name that differs from `name` each give `Err(TargetError::Invalid(_))`.
  - `detection`: a binary found in a temp `bin/` passed as `path`; a config folder under `TestEnv`; a flatpak folder `home/.var/app/io.github.x/config`; none present gives `false`; empty `detect` gives `true`.
  - Unit test in `target.rs` (`#[cfg(test)]`), `user_replaces_builtin`: the private merge of a built-in list and a user list keeps the user entry for a shared name.

- [ ] **Step 2: Run** `cargo test -p irida-core --test target`. Expected: compile errors.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test -p irida-core --test target` and `cargo test -p irida-core --lib`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): target definitions, built-in and user loading, detection`.

---

### Task 9: Templates

**Files:**
- Create: `irida-core/src/template.rs`
- Modify: `irida-core/src/lib.rs`
- Test: `irida-core/tests/template.rs`

**Interfaces:**
- Consumes: `Palette`, `derive`, `Derived`, `Slot`, `Role`, `mix`, `ramp`.
- Produces: `struct Renderer` with `Renderer::new(p: &Palette) -> Renderer` and `render(&self, template_name: &str, source: &str) -> Result<String, TemplateError>`; `pub fn check_syntax(template_name: &str, source: &str) -> Result<(), TemplateError>`; `TemplateError { pub template: String, pub message: String }` with `Display`.

The environment uses `set_keep_trailing_newline(true)` (minijinja strips one trailing newline by default, which would break byte-exact files), `UndefinedBehavior::Strict` and no auto-escaping. Context: every role as a top-level `"#RRGGBB"` string; `name`, `slug`, `stem`, `mode` (`"dark"`/`"light"`), `notes` (empty when absent); `names` (role name to display name, falling back to the role name); `term` (`foreground`, `background` (empty string when unset), `cursor`, `bright_foreground`, `dim_foreground`, and `normal`, `bright`, `dim` maps keyed by ANSI colour name); `cosmic` (`neutral_10`, `gray_2`). Filters on `"#RRGGBB"` strings: `hex` (uppercase), `strip`, `rgb`, `rgba(alpha)`, `ansi`. Functions `mix(a, b, t)` and `ramp(from, to, steps)`: each colour argument is a role name (checked first), else a slot name, else a `#RRGGBB` string; `mix` returns a hex string and `ramp` a list of hex strings. `check_syntax` compiles the template without rendering it.

- [ ] **Step 1: Write the failing tests** (Ember fixture unless named):
  - `roles_and_filters`: `"{{ accent }}"` gives `#E8713A`; `"{{ accent|strip }}"` gives `E8713A`; `"{{ accent|rgb }}"` gives `232,113,58`; `"{{ accent|rgba(0.8) }}"` gives `rgba(232,113,58,0.8)`; `"{{ accent|ansi }}"` gives `38;2;232;113;58`; `"{{ '#e8713a'|hex }}"` gives `#E8713A`.
  - `trailing_newline_kept`: `"{{ accent }}\n"` gives `"#E8713A\n"`.
  - `term_and_cosmic_namespaces`: `"{{ background }} {{ term.background }} {{ term.bright.red }} {{ term.normal.black }} {{ cosmic.neutral_10 }}"` gives `#15171C #1B1E24 #DD6E6E #2A2F38 #E6E8EE`.
  - `names_and_meta`: `"{{ names.success|lower }} {{ name }} {{ slug }} {{ stem }} {{ mode }}"` gives `moss Ember ember Ember dark`; for Noir `"{{ names.success }}"` gives `success`.
  - `mix_function`: on Ember Day `"{{ mix('text', 'pane', 0.70) }}"` gives `#A8A6A2`; on Ember `"{{ mix('bright.red', '#000000', 0) }}"` gives `#DD6E6E` and `"{{ mix('background', '#000000', 0) }}"` gives `#15171C` (the role wins over the slot).
  - `ramp_function`: `"{% for c in ramp('success', 'error', 3) %}{{ c }} {% endfor %}"` starts with `#8FB58A` and ends with `#D85A5A `.
  - `errors_name_the_template`: rendering `"{{ nope }}"` and `"{{ mix('nope', 'text', 0.5) }}"` as `"t"` gives `Err(TemplateError { template: "t", .. })`; `check_syntax("t", "{% for %}")` is an error and `check_syntax("t", "{{ nope }}")` is not.

- [ ] **Step 2: Run** `cargo test -p irida-core --test template`. Expected: compile errors.

- [ ] **Step 3: Implement** with `minijinja::Environment`; build the context with `Value::from_serialize` of a serde struct.

- [ ] **Step 4: Run** the tests. Expected: 7 passed.

- [ ] **Step 5: Commit** `feat(core): template rendering with colour filters, mix and ramp`.

---

### Task 10: Writers

**Files:**
- Create: `irida-core/src/write.rs`
- Modify: `irida-core/src/lib.rs`, `irida-core/src/store.rs` (use `atomic_write`)
- Test: `irida-core/tests/write.rs`

**Interfaces:**
- Produces:
  - `pub fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()>`: writes to `resolve_write_path(path)`, creating its parent folder; writes `.<file name>.irida-tmp` in the same folder, copies the permissions of the file it replaces (new files `0o644`), `sync_all`, renames over the target; on any error after creating the temp file, removes it before returning.
  - `pub fn resolve_write_path(path: &Path) -> std::io::Result<PathBuf>`: `fs::canonicalize` when `path` exists, else `path`.
  - `pub fn begin_marker(comment: &str) -> String`, `pub fn end_marker(comment: &str) -> String`.
  - `struct BlockEdit { pub contents: String, pub before: Option<String>, pub appended_newline: bool }`; `pub fn set_block(existing: Option<&str>, comment: &str, body: &str) -> Result<BlockEdit, BlockError>`; `pub fn restore_block(current: &str, comment: &str, before: Option<&str>, appended_newline: bool) -> Result<String, BlockError>`; `BlockError::Broken`.
  - `struct KeyEdit { pub contents: String, pub before: Option<String>, pub appended_newline: bool }`; `pub fn set_key(existing: Option<&str>, key: &str, value: &str) -> KeyEdit`; `pub fn restore_key(current: &str, key: &str, before: Option<&str>, appended_newline: bool) -> String`.

Block rules: markers match as whole lines after trimming trailing whitespace. No markers: append; if the existing text is non-empty and does not end in `\n`, first append `\n` and set `appended_newline`. Exactly one begin followed later by exactly one end: replace the lines between them, and `before` is the old body. Anything else: `Broken`. The body always ends in `\n`. `restore_block` with `before = None` removes the marker lines and everything between them, then removes the `\n` it appended when `appended_newline`; with neither marker present it returns `current` unchanged. Key rules: the first line matching `^\s*<key>\s*=` becomes `<key> = <value>` and `before` is the old line; no match appends the line (adding `\n` first when needed and setting `appended_newline`). `restore_key` puts the `before` line back, or removes the key line (and the added `\n`) when `before` is `None`; with no key line it returns `current` unchanged.

- [ ] **Step 1: Write the failing tests:**
  - `block_append_then_replace_then_restore`: on `"a\nb"` set a block with comment `#`, body `x\n`: contents `"a\nb\n# >>> irida >>>\nx\n# <<< irida <<<\n"`, `appended_newline`; setting body `y\n` on that gives `before == Some("x\n")`; restoring the second edit with `Some("x\n")` gives the first contents; restoring that with `None, true` gives `"a\nb"`.
  - `block_keeps_user_edits`: after appending a block, prepend `"user\n"`; `restore_block(.., None, ..)` keeps `"user\n"`.
  - `block_restore_without_markers_is_a_no_op`: `restore_block("a\n", "#", None, true) == Ok("a\n")`.
  - `broken_markers`: a begin without an end, an end before a begin, and two begins each give `Err(BlockError::Broken)`.
  - `block_on_missing_file`: `set_block(None, "#", "x\n")` gives just the block.
  - `key_replace_append_create_restore`: on `"a = 1\ncolor_theme = \"Default\"\nb = 2\n"` setting `color_theme` to `"irida"` (with quotes) replaces the middle line and `before` is the old line; `restore_key` gives the original back; on `"a = 1"` the result is `"a = 1\ncolor_theme = \"irida\"\n"` and restoring with `None, true` gives `"a = 1"`; on `None` the result is `"color_theme = \"irida\"\n"`.
  - `atomic_write_through_symlink`: with `real.conf` and `link.conf -> real.conf`, `atomic_write(link.conf, b"new")` leaves `link.conf` a symlink and `real.conf` holding `new`.
  - `atomic_write_keeps_mode_and_cleans_up`: a `0o600` file is still `0o600` after a write; a new file is `0o644`; no `.irida-tmp` file remains; writing under a path whose parent is a regular file fails and leaves no temp file.

- [ ] **Step 2: Run** `cargo test -p irida-core --test write`. Expected: compile errors.

- [ ] **Step 3: Implement**, then switch `Store::save` and `save_version` to `atomic_write`.

- [ ] **Step 4: Run** `cargo test -p irida-core`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): atomic, symlink-safe writes and block and key edits`.

---

### Task 11: Native COSMIC Terminal target

**Files:**
- Create: `irida-core/src/native/mod.rs`, `irida-core/src/native/cosmic_term.rs`, `irida-core/targets/cosmic-term/target.toml`
- Remove: `irida-core/targets/.keep`
- Modify: `irida-core/src/lib.rs`, `irida-core/tests/target.rs`
- Test: `irida-core/tests/cosmic_term.rs`

**Interfaces:**
- Consumes: `Palette`, `derive`, `TermColors::to_scheme`, `term::ColorScheme`, `Dirs`, `TargetError`, `NativeKind`.
- Produces:
  - `struct KeyFileWrite { pub path: PathBuf, pub contents: String }`.
  - `struct NativePlan { pub writes: Vec<KeyFileWrite>, pub warnings: Vec<String> }`.
  - `pub fn native::plan(kind: NativeKind, palettes: &[&Palette], dirs: &Dirs) -> Result<NativePlan, TargetError>` (one palette, or both halves of a pair).
  - `cosmic_term::CONFIG_ID = "com.system76.CosmicTerm"`, `cosmic_term::VERSION: u64 = 1`, `cosmic_term::config_dir(dirs: &Dirs) -> PathBuf` = `config/cosmic/com.system76.CosmicTerm/v1`.
  - Private mirrors, `#[serde(deny_unknown_fields)]`: `Profile { name: String, #[serde(default)] command, syntax_theme_dark, syntax_theme_light, tab_title, working_directory: String, #[serde(default)] drain_on_exit: bool }` (the field order of cosmic-term's `Profile`), `enum AppTheme { Dark, Light, System }`.

`target.toml`:

```toml
name = "cosmic-term"
native = "cosmic-term"
live = true
detect = { binary = "cosmic-term", config = "~/.config/cosmic/com.system76.CosmicTerm" }
```

Planning:
1. Two palettes must have different modes, else `TargetError::Invalid("a pair needs one dark and one light palette")`.
2. Read each key file once, if present: `color_schemes_dark`, `color_schemes_light` (`BTreeMap<u64, ColorScheme>`), `default_profile` (`Option<u64>`), `profiles` (`BTreeMap<u64, Profile>`), `app_theme` (`AppTheme`). A file that exists but does not parse fails the target with `TargetError::Invalid("COSMIC Terminal config changed shape (<key>: <ron error>); nothing written")`.
3. For each palette (mode `m`): in `color_schemes_<m>`, replace the lowest-id entry whose `name` equals the palette name, keeping its id (COSMIC Terminal renames later duplicates to "Name (2)", so only the lowest answers to the name), or insert at the highest id plus 1 (0 when empty); set `syntax_theme_<m>` to the palette name; if `default_profile` is `Some(id)` and `profiles` has it, set that profile's `syntax_theme_<m>`.
4. Emit one `KeyFileWrite` per changed key, serialised with `ron::ser::to_string_pretty(&value, ron::ser::PrettyConfig::new())`, the exact bytes `cosmic_config::ConfigSet::set` writes. Irida writes these files itself with `atomic_write` instead of through `cosmic_config`, so the apply record and rollback cover them.
5. If `app_theme` is present and not `System`, warn `"COSMIC Terminal is set to <Dark|Light>; it will not follow the desktop mode"`.

- [ ] **Step 1: Write the failing tests** (config folder under `TestEnv`):
  - `fresh_config`: planning Ember on an empty folder writes `color_schemes_dark` (one entry, id 0, equal to `derive(ember).term.to_scheme("Ember")`) and `syntax_theme_dark` (`"Ember"`), and nothing else.
  - `keeps_other_schemes`: with `{0: "Gruvbox", 4: "Ember" (old colours)}` the result keeps `Gruvbox` at 0 and replaces `Ember` at 4.
  - `duplicate_names_replace_lowest_id`: `{2: "Ember", 5: "Ember"}` replaces id 2 only.
  - `appends_after_highest_id`: `{0: "A", 7: "B"}` gets Ember at 8.
  - `pair_fills_both_modes_and_profile_once`: with `default_profile = Some(1)` and profile 1 present, planning `[ember, emberday]` writes `color_schemes_dark`, `color_schemes_light`, both `syntax_theme_*` keys and exactly one `profiles` write whose profile 1 has `syntax_theme_dark: "Ember"`, `syntax_theme_light: "Ember Day"` and its other fields unchanged.
  - `same_mode_pair_is_invalid`: `[ember, noir]` is `Err(TargetError::Invalid(_))`.
  - `app_theme_warning`: `app_theme` = `Dark` gives one warning; `System` gives none.
  - `unknown_field_fails_cleanly`: a scheme in `color_schemes_dark` with an extra field gives `Err(TargetError::Invalid(msg))` with `msg` containing `color_schemes_dark`.
  - `matches_cosmic_config_format`: writing the planned `color_schemes_dark` value with `cosmic_config::Config::with_custom_path(CONFIG_ID, 1, tmp)` and `ConfigSet::set` produces a file at `tmp/cosmic/com.system76.CosmicTerm/v1/color_schemes_dark` with the same bytes as the planned `contents`.

  Add to `tests/target.rs`:
  - `builtin_cosmic_term_and_user_override`: `load_targets` returns `cosmic-term` with `Ok`, `Source::Builtin` and `native == Some(NativeKind::CosmicTerm)`; with a user `cosmic-term/` folder (no `native`, one `file` entry) it returns `Source::User` and `native == None`.

- [ ] **Step 2: Run** `cargo test -p irida-core --test cosmic_term --test target`. Expected: failures.

- [ ] **Step 3: Implement** `native/mod.rs` (dispatch on `NativeKind`) and `native/cosmic_term.rs`.

- [ ] **Step 4: Run** the same command. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): native COSMIC Terminal target`.

---

### Task 12: Reload

**Files:**
- Create: `irida-core/src/reload.rs`
- Modify: `irida-core/src/lib.rs`
- Test: `irida-core/tests/reload.rs`

**Interfaces:**
- Consumes: `Reload`, `SignalReload`, `MinVersion`.
- Produces: `enum ReloadStatus { Reloaded, NotRunning, NotNeeded, Restart(String), Failed(String) }` (`Debug, Clone, PartialEq`); `pub fn reload(r: Option<&Reload>, target: &str, path: &[PathBuf]) -> ReloadStatus`; `pub fn parse_version(s: &str) -> Option<(u64, u64, u64)>` (the first `N.N.N` in `s`); `pub fn find_processes(name: &str) -> Vec<i32>` (scans `/proc/<pid>/comm` for an exact match, keeping only processes whose `/proc/<pid>` owner uid equals that of `/proc/self`).

Rules: `None` gives `NotNeeded`. With `min_version`, run its command as `Command::new("/bin/sh").arg("-c")` with `PATH` set from `path`; if the version is below the minimum, the command fails, or no version is found, return `Restart(format!("restart {target} to see it"))` and send nothing. With `signal`, map `USR1`, `USR2`, `HUP`, `TERM` to `nix::sys::signal::Signal` by hand and `kill` every match; no match is `NotRunning`. With `command`, run it the same way; a non-zero exit is `Failed(<stderr>)`. Success is `Reloaded`.

- [ ] **Step 1: Write the failing tests.** Each signal test copies `/bin/sleep` into its own temp folder under its own process name and spawns it with `30`:
  - `version_parsing`: `parse_version("btop version: 1.3.0")` is `Some((1, 3, 0))`; `parse_version("none")` is `None`.
  - `old_btop_is_not_signalled`: a fake `btop` script in a temp `bin/` printing `btop version: 1.3.0`; `reload` with `min_version = { command = "btop --version", version = "1.3.2" }` and signal `TERM` to `iridaoldproc` returns `Restart("restart btop to see it")`, and `iridaoldproc` is still alive afterwards.
  - `new_btop_is_signalled`: the fake prints `btop version: 1.3.2`; signal `TERM` to `iridanewproc` returns `Reloaded` and the child exits by signal within 2 s.
  - `not_running`: signal to `iridanoproc` returns `NotRunning`.
  - `command_reload`: command `true` gives `Reloaded`; `echo boom >&2; exit 3` gives `Failed(s)` with `s` containing `boom`.
  - `none_is_not_needed`: `reload(None, "x", &[])` is `NotNeeded`.

- [ ] **Step 2: Run** `cargo test -p irida-core --test reload`. Expected: compile errors.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** the tests. Expected: 6 passed.

- [ ] **Step 5: Commit** `feat(core): reload by signal or command with a version guard`.

---

### Task 13: Apply records

**Files:**
- Create: `irida-core/src/record.rs`
- Modify: `irida-core/src/lib.rs`
- Test: `irida-core/tests/record.rs`

**Interfaces:**
- Consumes: `Dirs`, `atomic_write`, `resolve_write_path`, `restore_block`, `restore_key`.
- Produces:
  - `enum Entry` (serde `tag = "kind"`, lowercase), every variant with `blob: Option<String>` (a file inside the record folder holding the bytes before the apply; `None` means the file did not exist): `File { path: PathBuf, blob: Option<String> }`; `Block { path: PathBuf, blob: Option<String>, comment: String, before: Option<String>, appended_newline: bool }`; `Key { path: PathBuf, blob: Option<String>, key: String, before: Option<String>, appended_newline: bool }`.
  - `struct Record { pub id: u32, pub palette: String, pub entries: Vec<Entry> }` (`palette` is the slug).
  - `struct RecordWriter`: `RecordWriter::create(dirs: &Dirs, palette_slug: &str) -> io::Result<RecordWriter>` (folder `state/irida/applies/<n>/`, `n` = highest plus 1, starting at 1); `capture(&mut self, path: &Path) -> io::Result<Option<String>>` (copies the bytes of `resolve_write_path(path)` into `files/<i>` and returns `Some("files/<i>")`; returns `None` when reading fails with `NotFound` or `NotADirectory`); `push(&mut self, e: Entry)`; `finish(self) -> io::Result<Record>` (writes `record.toml`).
  - `enum RestoreMode { Rollback, Undo }`.
  - `pub fn latest(dirs: &Dirs) -> io::Result<Option<Record>>`; `pub fn restore(dirs: &Dirs, rec: &Record, mode: RestoreMode) -> Vec<(PathBuf, io::Error)>` (every entry in reverse order, returning failures); `pub fn delete(dirs: &Dirs, id: u32) -> io::Result<()>`; `pub fn prune(dirs: &Dirs, keep: usize) -> io::Result<()>`.

Restore rules. In both modes a `File` entry writes its blob back with `atomic_write`, or deletes the file when `blob` is `None`; a delete that fails with `NotFound` or `NotADirectory` is not a failure. `Rollback` treats `Block` and `Key` entries the same way, byte for byte. `Undo` reads the current file and writes `restore_block(..)` or `restore_key(..)` so the user's later edits survive; when `blob` is `None` and the result is empty, it deletes the file; a file that has since disappeared is skipped.

- [ ] **Step 1: Write the failing tests:**
  - `file_round_trip`: capture an existing file and an absent path, change the first and create the second, `finish`, `restore(.., Undo)`: the first has its old bytes and the second is gone.
  - `capture_under_a_file_is_absent`: `capture(<regular file>/x)` is `Ok(None)`, and restoring that `File` entry reports no failure.
  - `block_and_key_undo_keep_user_edits`: apply a block to `~/.zshrc` and a key to `btop.conf` with `set_block` and `set_key`, record matching entries, append `"# mine\n"` to both, `restore(.., Undo)`: both keep `# mine` and lose Irida's part.
  - `rollback_is_byte_exact`: the same edits without user changes, `restore(.., Rollback)`: both files equal their captured bytes.
  - `created_file_is_deleted`: a block applied to a file that did not exist is deleted by `restore(.., Undo)`.
  - `restore_through_symlink`: `~/.zshrc -> dotfiles/zshrc`; set a block, record it, `restore(.., Undo)`: `~/.zshrc` is still a symlink and `dotfiles/zshrc` holds its original bytes; the same for a `File` entry with `Rollback`.
  - `numbering_latest_prune`: twelve records give ids 1 to 12; `latest` is 12; `prune(10)` leaves 3 to 12.
  - `record_toml_round_trips`: `latest` reads back the same `Record` as `finish` returned.

- [ ] **Step 2: Run** `cargo test -p irida-core --test record`. Expected: compile errors.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** the tests. Expected: 8 passed.

- [ ] **Step 5: Commit** `feat(core): apply records with rollback, undo and pruning`.

---

### Task 14: The apply transaction and undo

**Files:**
- Create: `irida-core/src/apply.rs`
- Modify: `irida-core/src/lib.rs`
- Test: `irida-core/tests/apply.rs`

**Interfaces:**
- Consumes: `load_targets`, `is_detected`, `Renderer`, `check_syntax`, `set_block`, `set_key`, `atomic_write`, `native::plan`, `RecordWriter`, `record::{restore, prune, latest, delete, RestoreMode}`, `reload`, `ReloadStatus`, `Settings`.
- Produces:
  - `struct Ctx { pub dirs: Dirs, pub path: Vec<PathBuf> }` with `Ctx::from_env() -> Result<Ctx, DirsError>` (`path` split from `$PATH`).
  - `struct ApplyRequest<'a> { pub palette: &'a Palette, pub pair: Option<&'a Palette>, pub only: Option<Vec<String>> }`.
  - `enum SkipReason { NotInstalled, Disabled, Broken(String) }`; `enum TargetStatus { Applied(ReloadStatus), Skipped(SkipReason) }`; `struct TargetResult { pub name: String, pub status: TargetStatus, pub warnings: Vec<String> }`; `struct ApplyReport { pub record_id: u32, pub targets: Vec<TargetResult> }`; `ApplyReport::get(&self, name: &str) -> Option<&TargetResult>`.
  - `enum ApplyError { UnknownTarget(String), Render { target: String, message: String }, Write { path: PathBuf, source: std::io::Error, rolled_back: bool }, NothingToApply(Vec<TargetResult>), Record(std::io::Error) }` with `Display` naming the file or target; `Write` with `rolled_back == false` says the record was kept and `irida undo` can retry.
  - `pub fn apply(ctx: &Ctx, settings: &Settings, req: &ApplyRequest) -> Result<ApplyReport, ApplyError>`.
  - `struct UndoReport { pub record_id: u32, pub palette: String, pub failures: Vec<(PathBuf, std::io::Error)> }`; `pub fn undo(ctx: &Ctx) -> Result<UndoReport, UndoError>`; `UndoError::{NothingToUndo, Io(std::io::Error)}`.

Inside `apply`:
1. **Select.** `only` names that are unknown give `UnknownTarget`; with `only` set, other targets are left out of the report. A target that failed to load, or has a template failing `check_syntax`, is `Skipped(Broken(msg))`; undetected is `Skipped(NotInstalled)`; in `settings.disabled` and not named in `only` is `Skipped(Disabled)`.
2. **Plan, before writing anything.** Render each `[[file]]` with `Renderer::new(req.palette)`; a render error returns `Render` and nothing is written. For `key` writes the rendered value has trailing `\n` and `\r` trimmed. A `block` file whose markers are broken is dropped with the warning `"<path>: irida markers are broken; skipped"`; a target left with no writes that way is `Skipped(Broken(<that warning>))`. Native targets call `native::plan` with `[palette]` or `[palette, pair]`; an error makes that target `Skipped(Broken(msg))` and the others still apply. If no write remains in any target, return `NothingToApply(<every target's result>)`.
3. **Record.** `RecordWriter::create(dirs, &req.palette.slug)`; for each planned write `capture` its path and push the matching `Entry` (`File` for file writes and native key files, `Block`, `Key`); `finish` before the first write.
4. **Write** each file with `atomic_write`, in plan order. On the first error at write `i`, call `record::restore(.., Rollback)` on a copy of the record whose entries are the first `i` (the writes that completed). Delete the record only when that restore has no failures. Return `Write { path, source, rolled_back: failures.is_empty() }`.
5. **Reload** each applied target with `reload(def.reload.as_ref(), name, &ctx.path)`, then `prune(10)`.

`undo` restores `record::latest` with `RestoreMode::Undo`, deletes the record only when there were no failures, and returns the report; no record gives `NothingToUndo`.

- [ ] **Step 1: Write the failing tests.** Every test builds `Ctx { dirs: env.dirs.clone(), path: vec![] }` (or a temp `bin/`), never `Ctx::from_env`, and looks targets up with `report.get(name)`, since built-ins appear as `Skipped(NotInstalled)`. User targets are written into the `TestEnv` config folder, each with an empty `detect`:
  - `applies_file_block_and_key`: targets `a` (`file` to `~/.config/a/a.conf`), `b` (`block`, comment `#`, to `~/.zshrc`, which holds `"export X=1\n"`), `c` (`key` `theme` in `~/.config/c/c.conf`, created beforehand as `"a = 1\ntheme = old\n"`), applied with Ember, produce the rendered contents, and `a`, `b`, `c` are each `Applied(NotNeeded)`.
  - `undo_restores_and_keeps_user_edits`: after that apply, append `"alias x=y\n"` to `~/.zshrc` and `"mine = 1\n"` to `c.conf`; `undo` removes the block and keeps the alias; `a.conf`, which did not exist, is deleted; `c.conf` is `"a = 1\ntheme = old\nmine = 1\n"`.
  - `symlink_apply_and_undo`: `~/.zshrc -> dotfiles/zshrc`; after apply and after undo `~/.zshrc` is still a symlink and `dotfiles/zshrc` ends with its original bytes.
  - `render_error_writes_nothing`: one target's template is `{{ nope }}`; `apply` returns `Render { target, .. }`, no file was created and no record exists.
  - `syntax_error_skips_one_target`: a target whose template is `{% for %}` is `Skipped(Broken(_))` while `a` applies.
  - `write_failure_rolls_back`: target `a` writes `~/.config/a/a.conf` (existing, bytes `old`) and `~/.config/a/new.conf` (absent); target `z` writes `~/.config/blocker/b.conf`, where `blocker` is a regular file. `apply` returns `Write { rolled_back: true, .. }` whose `Display` contains `blocker/b.conf`; `a.conf` holds `old`; `new.conf` does not exist; no record remains.
  - `broken_markers_skip_the_file`: `~/.zshrc` holds two begin markers; with targets `a` and `b`, `b` is `Skipped(Broken(msg))` with `msg` containing `.zshrc` and `irida markers are broken`, `a.conf` is written, and `.zshrc` is unchanged.
  - `selection_rules`: an undetected target is `Skipped(NotInstalled)`; one in `settings.disabled` is `Skipped(Disabled)` unless named in `only`; `only = ["nope"]` gives `UnknownTarget("nope")`; a user target that fails to parse is `Skipped(Broken(_))` while the others apply.
  - `cosmic_term_apply_and_undo`: with the built-in `cosmic-term` detected through its config folder, holding a `color_schemes_dark` with `Gruvbox` at 0 and `syntax_theme_dark` = `"Gruvbox"`, applying the Ember and Ember Day pair writes both scheme maps; `undo` restores those two files byte for byte and deletes the key files it created.
  - `cosmic_term_bad_config_skips_only_that_target`: with an unknown field in `color_schemes_dark`, `cosmic-term` is `Skipped(Broken(_))`, `a` applies, and `color_schemes_dark` is unchanged.
  - `records_are_pruned`: eleven applies leave ten records.
  - `undo_with_nothing`: `undo` on a fresh `TestEnv` is `NothingToUndo`.

- [ ] **Step 2: Run** `cargo test -p irida-core --test apply`. Expected: compile errors.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test -p irida-core`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): transactional apply with rollback, and undo`.

---

### Task 15: Built-in btop target

**Files:**
- Create: `irida-core/targets/btop/target.toml`, `irida-core/targets/btop/btop.theme.tmpl`, `irida-core/targets/btop/color_theme.tmpl`, `irida-core/tests/fixtures/rices/{Ember,EmberDay}.theme` (copied verbatim from the rice repos' `btop/<Stem>.theme`)
- Test: `irida-core/tests/builtin_btop.rs`

**Interfaces:**
- Consumes: `Renderer`, `load_targets`, `apply`, `Ctx`.

`target.toml` (btop is not a live target: the spec's live list is COSMIC desktop, COSMIC Terminal and cava):

```toml
name = "btop"
detect = { binary = "btop", config = "~/.config/btop" }

[[file]]
path = "~/.config/btop/themes/irida.theme"
write = "file"
template = "btop.theme.tmpl"

[[file]]
path = "~/.config/btop/btop.conf"
write = "key"
key = "color_theme"
template = "color_theme.tmpl"

[reload]
signal = { process = "btop", signal = "USR2" }
min_version = { command = "btop --version", version = "1.3.2" }

[[rice]]
path = "btop/{{ stem }}.theme"
install = "file"
template = "btop.theme.tmpl"
```

`color_theme.tmpl` is the bytes `"irida"` (quotes included). `btop.theme.tmpl` ends with exactly one `\n` and reproduces the rice files: the first comment names `{{ term.background }}`; the gradient comment is `{{ names.success|lower }} -> {{ names.accent|lower }} -> {{ names.error|lower }}`; `main_bg` is `""`; `main_fg`, `title` and the four `*_box` keys are text; `hi_fg`, `selected_bg` and `proc_misc` are accent; `selected_fg` is `term.background`; `inactive_fg` is comment; `div_line` is muted; `temp_`, `cpu_`, `free_`, `download_` and `upload_` start, mid and end are success, accent, error; `cached_*` is warning; `available_*` is cyan; `used_*` is error.

- [ ] **Step 1: Write the failing tests:**
  - `matches_rice_files`: rendering `btop.theme.tmpl` with Ember gives exactly the bytes of `Ember.theme`, and with Ember Day exactly `EmberDay.theme`.
  - `apply_sets_theme_and_key`: with `Ctx.path` = a temp `bin/` holding a fake `btop` that prints `btop version: 1.3.0`, and `~/.config/btop/btop.conf` holding `color_theme = "Default"\nshown_boxes = "cpu"\n`, `apply` with `only = ["btop"]` writes `themes/irida.theme`, changes only the `color_theme` line to `color_theme = "irida"`, and reports `Applied(Restart("restart btop to see it"))`.

- [ ] **Step 2: Run** `cargo test -p irida-core --test builtin_btop`. Expected: failures.

- [ ] **Step 3: Write the three target files.**

- [ ] **Step 4: Run** `cargo test -p irida-core`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(targets): built-in btop target`.

---

### Task 16: `irida apply` and `irida undo`

**Files:**
- Modify: `irida/src/main.rs`, `irida/src/output.rs`, `irida/tests/cli.rs`

**Interfaces:**
- Consumes: `Ctx::from_env`, `Store`, `apply`, `undo`, `ApplyReport`, `UndoReport`, `ApplyError`.
- Produces: `Command::Apply { palette: String, #[arg(long, value_delimiter = ',')] only: Option<Vec<String>> }`, `Command::Undo`; `output::format_apply(&ApplyReport) -> String`, `output::format_undo(&UndoReport) -> String`.

`apply` resolves the palette with `Store::resolve` and, when `pair` is set, loads it with `Store::load`; a missing pair, or one of the same mode, gives a warning line and the palette is applied alone. It saves `last_palette` in settings. Report lines, in report order: `Applied(Reloaded)`, `Applied(NotNeeded)` print `"{name}: applied"`; `Applied(NotRunning)` prints `"{name}: applied, not running"`; `Applied(Restart(msg))` prints `"{name}: applied, {msg}"`; `Applied(Failed(msg))` prints `"{name}: applied, reload failed ({msg}); restart {name} to see it"`; `Skipped(Disabled)` prints `"{name}: skipped, turned off"`; `Skipped(Broken(msg))` prints `"{name}: skipped, {msg}"`. Each warning follows its target on its own line, indented by two spaces. Targets skipped as not installed fold into one line after the others: `"not installed: {names joined by ", "}"`. The last line is `"Undo with: irida undo"`. `NothingToApply` prints its target lines, then the error. Exit 0 on success, 2 on `ApplyError` or `UndoError` (message on stderr after `irida: `). `undo` prints `"Restored apply {id} ({palette})"` and each failure as `"  could not restore {path}: {error}"`, exiting 1 when there are failures.

- [ ] **Step 1: Write the failing tests.** Temp HOME and XDG variables; `PATH` = a temp `bin/` holding a fake `btop` (prints `btop version: 1.3.0`, so no signal is ever sent) and a fake `cosmic-term` (exits 0), plus `/usr/bin:/bin`:
  - `apply_then_undo`: put `ember.toml` and `emberday.toml` into the data folder and `"color_theme = \"Default\"\n"` into `~/.config/btop/btop.conf`; record every file under `$XDG_CONFIG_HOME` except `irida/` with its bytes; `irida apply ember` exits 0 and prints `"btop: applied, restart btop to see it"`, `"cosmic-term: applied"` and `"Undo with: irida undo"`; the btop theme and both scheme maps exist; `irida undo` exits 0 and prints `"Restored apply 1 (ember)"`; the recorded listing of files and bytes is the same again (folders Irida created may remain).
  - `apply_only`: `irida apply ember --only btop` prints no `cosmic-term` line.
  - `apply_unknown_target`: `--only nope` exits 2 with `irida: ` and `nope` on stderr.
  - `undo_nothing`: `irida undo` on a fresh home exits 2.

- [ ] **Step 2: Run** `cargo test -p irida --test cli`. Expected: the new tests fail.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`. Expected: all tests pass, no warnings.

- [ ] **Step 5: Commit** `feat(cli): irida apply and irida undo`.

---

## Done when

- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` pass.
- `irida check` on the four fixture palettes gives the verdicts in Task 4.
- `irida apply` then `irida undo` against a temporary home restores every file Irida changed to its previous bytes and deletes every file it created; folders it created and Irida's own settings and state may remain.
- No file in the diff contains an em-dash or en-dash (`grep -rnP '[\x{2013}\x{2014}]'` finds nothing).
