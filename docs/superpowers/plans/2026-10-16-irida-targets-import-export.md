# Irida targets, import and export implementation plan (plan 2 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Irida applies a palette to the whole v1 app list (COSMIC desktop and
mode, COSMIC Terminal, btop, Neovim, fastfetch, lsd, zsh, cava), opens the
existing rice repos as palettes, and exports a palette as a complete rice repo
with `irida export`.

**Architecture:** Builds on plan 1's engine (`irida-core` targets, templates,
writers, records, apply). New built-in targets are data folders under
`irida-core/targets/`; the COSMIC desktop is a second native target that stages
COSMIC's own config writer in a temporary folder and hands back key files.
`import.rs` reads a rice folder into a `Palette`; `export.rs` renders every
target's `[[rice]]` entries, the native exports and Irida's embedded rice
assets into `cosmic-<slug>-theme/`.

**Tech Stack:** as plan 1 (Rust 2024, libcosmic `e1f2cd2`, minijinja 2, ron
0.12, include_dir, tempfile), plus `regex` (already in `Cargo.lock`).

**Spec:** `docs/superpowers/specs/2026-10-16-irida-design.md` (amended again
2026-10-16 for this plan). Plan 1, already implemented, is
`docs/superpowers/plans/2026-10-16-irida-core-engine.md`. **Plan 3:** the
libcosmic app (including a core call that lists target statuses for the Apply
sheet without applying), named snapshots, compare, pairs in the UI, the .deb,
try it live.

## Global Constraints

- Everything in plan 1's Global Constraints still holds (edition 2024, pinned libcosmic rev, `round_ties_even` mix, uppercase `#RRGGBB`, no em-dashes or en-dashes, explicit `git add` paths, the sandbox environment).
- Every rice file that exists in the Ember and Ember Day repos and that this plan generates must come out byte for byte equal to that repo's file. The exceptions, which are Irida's own reviewed output: `lsd/colors.yaml` (truecolor), everything under `cava/`, `scripts/switch.sh` (three changes), new-repo `README.md` and `docs/showcase.md` skeletons, the README hint line, and apply-only files named `irida`.
- Rice fixtures are verbatim copies (`cp`) from `github.com/atraxsrc/cosmic-<slug>-theme` (cloned for this work at `<scratchpad>/refs/`), placed under `irida-core/tests/fixtures/rices/cosmic-<slug>-theme/` at the same relative path. Plan 1's flat fixtures (`rices/Ember.theme`, `rices/*-term.ron`) stay where they are.
- Palette names match `^[A-Za-z0-9]+( [A-Za-z0-9]+)*$`. `[names]` entries that are empty or whitespace are dropped when a palette is read.
- The COSMIC palette name inside desktop themes is `name.replace(' ', "-")`; the terminal scheme name and `syntax_theme_<mode>` use the plain name.
- Tests never touch a process they did not start. A reload signal is sent only when the target's `detect.binary` is found on `Ctx.path` (Task 3), so core tests with an empty path never signal. CLI tests put only a temporary `bin/` on `PATH`, holding fake `btop` (prints `btop version: 1.3.0`), `cosmic-term`, `cosmic-settings`, `nvim`, `fastfetch` and `lsd` scripts (`#!/bin/sh`), and never a fake `cava`.
- Planning a native target never creates anything under `dirs.config`: COSMIC's `Config::with_custom_path` is only ever pointed at a `tempfile::TempDir`.
- Adding a field to a struct updates every literal site: `ApplyRequest` (`tests/apply.rs`, `tests/builtin_btop.rs`, `irida/src/main.rs`), `TargetDef` (`src/target.rs` unit test, `tests/target.rs` `def()`), `Reload` (`tests/reload.rs` `term_reload`), and the exhaustive matches in `native::plan` and `output::format_targets`.
- Every new built-in target also adds its name to plan 1's `tests/apply.rs` `nothing_to_apply_lists_every_target`, which Task 5 changes to compare against `["b"]` plus every other name from `load_targets(&env.dirs)` in sorted order.
- No step depends on a file under `/tmp`. Verified template drafts are at `<scratchpad>/plan2-drafts/templates/` (templates only: the drafted `target.toml` files predate this plan's schema and must not be copied); each task states what its templates must reproduce.

**Environment (this machine only).** As plan 1: source the SDD workspace's
`env.sh` before every cargo or git command and use `CARGO_NET_OFFLINE=true`
throughout (`regex` and its dependencies are already in the scratch registry
and the lock file).

## Review Focus

1. **The user has customised COSMIC** (gaps, radii, frosting). Apply must write only the 11 colour keys of the builder, so those survive. Test in Task 5.
2. **A bash user who has zsh installed but no `~/.zshrc`.** The zsh target must not be detected, so no `~/.zshrc` is created. Test in Task 9.
3. **Exporting into an existing, hand-made repo.** Wallpapers, an unmarked README, a hand-tuned `cosmic/config` value and extra files must be left alone; files that look generated but were not written are named, never deleted; rewritten files are counted. Tests in Tasks 12 and 13.
4. **A repo with several candidate files** (two `cosmic/*.ron`, or Noir's six terminal schemes). Import must pick the scheme named in `syntax_theme_<mode>` or fail with a message naming the files. Test in Task 11.
5. **A `[[rice]]` path that escapes the repo** (`../x`, an absolute path, a template that renders to one, or a symlinked folder inside the repo). Nothing may be written outside the repo. Tests in Task 12.

---

## File structure

```
irida-core/Cargo.toml                    cosmic-config, tempfile runtime deps (Task 5); regex (Task 11)
irida-core/build.rs                      also reruns on rice/ (Task 12)
irida-core/src/template.rs               export flag, for_export, render_with (Tasks 1, 13)
irida-core/src/palette.rs                validate_name, Roles::new, empty names (Task 1)
irida-core/src/target.rs                 RiceEntry, create_only, file detect, reload.hint (Task 2)
irida-core/src/apply.rs                  same-path guard, create_only, replaced-file note, signal gate (Task 3)
irida-core/src/reload.rs                 hint (Task 3)
irida-core/src/native/mod.rs             check_palettes, ExportFile, export (Task 4)
irida-core/src/native/cosmic_desktop.rs  desktop + mode (Task 5)
irida-core/src/import.rs                 rice folder to Palette (Task 11)
irida-core/src/store.rs                  resolve rice folders, StoreError::Import (Task 11)
irida-core/src/export.rs                 export engine (Task 12)
irida-core/src/export_docs.rs            README, showcase, switch.sh (Task 13)
irida-core/src/write.rs                  replace_marked (Task 13)
irida-core/targets/{cosmic-desktop,nvim,fastfetch,lsd,zsh,cava}/   built-in targets
irida-core/rice/                         embedded rice assets (Tasks 12, 13)
irida-core/tests/fixtures/rices/cosmic-<slug>-theme/...            rice fixtures
irida-core/tests/fixtures/expected/{lsd,cava}/                     Irida-own expected output
irida/src/main.rs, irida/src/output.rs   irida export (Task 14)
```

---

### Task 1: Template and palette rules

**Files:**
- Modify: `irida-core/src/template.rs`, `irida-core/src/palette.rs`
- Test: `irida-core/tests/template.rs`, `irida-core/tests/palette.rs`

**Interfaces:**
- Produces: `Renderer::new(p: &Palette) -> Renderer` (context `export = false`, otherwise unchanged); `Renderer::for_export(p: &Palette) -> Renderer` (`export = true`); both share a private constructor. `names.<role>` falls back to the role name when a name is missing. `palette::validate_name(s: &str) -> Result<(), PaletteError>` (rule in Global Constraints); `PaletteError::BadName(String)`; `Palette::from_toml` calls it and drops `[names]` values that are empty or whitespace. `Roles::new(colors: [Rgb; 13]) -> Roles` (`Role::ALL` order).

- [ ] **Step 1: Write the failing tests:**
  - `export_flag`: on Ember, `Renderer::new` renders `"{{ export }}"` as `false` and `"{{ slug if export else 'irida' }}"` as `irida`; `Renderer::for_export` renders `true` and `ember`.
  - `empty_names_dropped`: Ember's TOML with `blue = ""` and `cyan = "  "` under `[names]` gives a palette without those two names, and `"{{ names.blue }} {{ names.cyan }} {{ names.accent }}"` renders `blue cyan Ember`.
  - `name_rules`: `validate_name` accepts `"Ember"`, `"Ember Day"`, `"Noir 2"`; rejects `""`, `" Ember"`, `"Ember "`, `"Ember  Day"`, `"Ember-Day"`, `"Ember\"s"`, `"Émber"`, `"a\nb"`; `from_toml` with `name = "Ember-Day"` is `Err(PaletteError::BadName(_))`.
  - `roles_new`: `Roles::new` from the Ember fixture's 13 colours in `Role::ALL` order equals the fixture's `roles`.

- [ ] **Step 2: Run** `cargo test -p irida-core --test template --test palette`. Expected: compile errors.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test -p irida-core`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): export template flag, name rules and Roles::new`.

---

### Task 2: Target schema

**Files:**
- Modify: `irida-core/src/target.rs`, `irida-core/targets/btop/target.toml` (drop `install`), `irida-core/tests/target.rs` (`def()` and the cava example), `irida-core/tests/reload.rs` (`term_reload` gains `hint: None`)
- Test: `irida-core/tests/target.rs`

**Interfaces:**
- Produces:
  - `RiceEntry { pub path: String, pub template: String, #[serde(default)] pub executable: bool }`, `deny_unknown_fields`. `template` is required; `install` is gone. Loading no longer checks that rice templates exist (export reports a missing one), so a broken rice entry never blocks apply.
  - `FileEntry` gains `#[serde(default)] pub create_only: bool`. `create_only` with a `write` other than `file` is `Invalid("{path}: create_only needs write = \"file\"")`.
  - `Reload` gains `#[serde(default)] pub hint: Option<String>`.
  - `is_detected`: `dirs.expand(config)` counts when it exists as a folder or a file.

- [ ] **Step 1: Write the failing tests:**
  - `rice_entries`: `[[rice]] path = "x/{{ stem }}.y"` `template = "t.tmpl"` `executable = true` loads with those values; without `template` it is `Err(TargetError::Parse(_))`; with `install = "file"` it is `Err(TargetError::Parse(_))`; a rice template missing from the folder still loads `Ok`.
  - `create_only_needs_file`: `write = "block"` with `create_only = true` is `Err(TargetError::Invalid(_))`; with `write = "file"` it loads.
  - `reload_hint_parses`: `[reload] hint = "open a new shell"` loads with `hint == Some(..)`.
  - `config_file_detects`: `detect = { config = "~/.zshrc" }` is detected when `~/.zshrc` is a file and not when it is absent.

- [ ] **Step 2: Run** `cargo test -p irida-core --test target`. Expected: failures.

- [ ] **Step 3: Implement**; update the literal sites and the btop `target.toml`.

- [ ] **Step 4: Run** `cargo test -p irida-core`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): rice entries with templates, create_only, file detection, reload hints`.

---

### Task 3: Apply and reload rules

**Files:**
- Modify: `irida-core/src/apply.rs`, `irida-core/src/reload.rs`
- Test: `irida-core/tests/apply.rs`, `irida-core/tests/reload.rs`

**Interfaces:**
- Consumes: `FileEntry.create_only`, `Reload.hint`, `record::latest`.
- Produces:
  - `reload`: with no `signal` and no `command` but a `hint`, it returns `Restart(hint)`; otherwise unchanged.
  - Signal gate in `apply`: when a target's reload has a `signal` and its `detect.binary` is set but not found as an executable on `ctx.path`, the status is `NotRunning` and no signal is sent.
  - `plan_file` returns `Result<Option<PlannedWrite>, String>`: `Ok(None)` is a write dropped with nothing to report; `plan_target` keeps only `Some`.
  - `create_only`: when the file exists (via `resolve_write_path`), the write is dropped. If some non-empty rendered line (trimmed) is missing from the existing file's trimmed lines, the warning is ``"{path}: already exists, so Irida left it alone; add `{line}` under `{parent}`"`` where `parent` is the nearest earlier rendered line with less indentation (trimmed), and the `under` part is left out when there is none; otherwise `Ok(None)`.
  - Same-path guard: after each target is planned, a write whose `resolve_write_path` (or the raw path if that fails) is already planned by an earlier target or earlier in the same target is dropped with ``"{path}: already written by {first target}; skipped"``. A target left with no writes is `Skipped(Broken(first warning or "nothing to write"))`, as in plan 1.
  - Replaced-file note: a `file` write over an existing file with different bytes, whose path is not an entry of `record::latest` (read once per apply), adds ``"{path}: replaced your file; irida undo restores it"``.

- [ ] **Step 1: Write the failing tests:**
  - `reload.rs` `hint_only_is_restart`: `Reload { hint: Some("open a new shell".into()), .. }` gives `Restart("open a new shell")`; with `command = Some("true")` too it gives `Reloaded`.
  - `signal_needs_binary_on_path`: a user target with `detect = { binary = "iridafake", config = "~/.config/f" }`, the folder created, and `[reload] signal = { process = "iridafake", signal = "TERM" }`: with an empty `ctx.path` it is `Applied(NotRunning)`.
  - `same_path_second_target_skipped`: user targets `a` and `b` each write a `block` (comments `#` and `##`) to `~/.zshrc`; `.zshrc` then holds only `a`'s block; `b` is `Skipped(Broken(msg))` with `msg` containing `already written by a`; undo restores the original bytes.
  - `same_path_within_one_target`: one target with a `key` then a `block` on `~/.config/x/app.conf`: the key line is written, the block is dropped with the warning, the target is `Applied`.
  - `create_only_cases` (each run with plan 1's user target `a` present so the apply has a write): a create-only target `co` writing `~/.config/c/conf.yaml` = `color:\n  theme: custom\n` creates it when absent (undo deletes it); over an existing `color:\n  when: auto\n` the file is unchanged and `co` is `Skipped(Broken(msg))` with `msg` ending ``add `theme: custom` under `color:` ``; over an existing `color:\n  theme: custom\n`, `co` is `Skipped(Broken("nothing to write"))`; a target `co2` with the create-only file plus another `file` write is `Applied` with the note in its `warnings`.
  - `replaced_file_note`: a `file` target over an existing `~/.config/r/r.conf` (`mine`) warns ``replaced your file``; a second apply of the same target does not.
  - `hint_reaches_report`: a user target with `[reload] hint = "x"` is `Applied(Restart("x"))`.
  - Plan 1's tests still pass.

- [ ] **Step 2: Run** `cargo test -p irida-core --test apply --test reload`. Expected: failures.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): same-path guard, create-only writes, replaced-file notes, reload hints`.

---

### Task 4: Native refactor and COSMIC Terminal export

**Files:**
- Modify: `irida-core/src/native/mod.rs`, `irida-core/src/native/cosmic_term.rs`
- Test: `irida-core/tests/cosmic_term.rs`

**Interfaces:**
- Produces:
  - `native::check_palettes(palettes: &[&Palette]) -> Result<(), TargetError>`: one palette, or two of different modes, with plan 1's messages; `cosmic_term::plan` calls it.
  - `native::ExportFile { pub path: String, pub contents: String }` (`path` relative to the repo).
  - `native::export(kind: NativeKind, p: &Palette) -> Vec<ExportFile>`; the `CosmicTerm` arm returns `cosmic-term/<Stem>-term.ron` = `derive(p).term.to_scheme(&p.name).to_ron() + "\n"` and `cosmic/config/com.system76.CosmicTerm/v1/syntax_theme_<mode>` = the RON string of the plain name (`"Ember Day"`, no newline).

- [ ] **Step 1: Write the failing test** `export_term`: for Ember and Ember Day the `.ron` equals `fixture_text("rices/Ember-term.ron")` and `rices/EmberDay-term.ron`, and the syntax theme contents are `"Ember"` and `"Ember Day"`.

- [ ] **Step 2: Run** `cargo test -p irida-core --test cosmic_term`. Expected: failure.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test -p irida-core`. Expected: all pass.

- [ ] **Step 5: Commit** `refactor(core): shared palette checks and native export`.

---

### Task 5: COSMIC desktop and mode

**Files:**
- Create: `irida-core/src/native/cosmic_desktop.rs`, `irida-core/targets/cosmic-desktop/target.toml`, `irida-core/tests/cosmic_desktop.rs`, fixtures `rices/cosmic-ember-theme/cosmic/Ember.ron`, `rices/cosmic-emberday-theme/cosmic/EmberDay.ron`
- Modify: `irida-core/Cargo.toml` (`cosmic-config` and `tempfile` move to `[dependencies]`), `irida-core/src/target.rs` (`NativeKind::CosmicDesktop`, serde `"cosmic-desktop"`), `irida-core/src/native/mod.rs`, `irida-core/tests/apply.rs` (`nothing_to_apply_lists_every_target` compares against the loaded names, see Global Constraints)

**Interfaces:**
- Consumes: `derive`, `mix`, `check_palettes`, `KeyFileWrite`, `NativePlan`, `ExportFile`.
- Produces (`cosmic_desktop`): `pub const COLOR_KEYS: [&str; 11]` = `palette, neutral_tint, bg_color, primary_container_bg, secondary_container_bg, text_tint, accent, success, warning, destructive, window_hint`; `pub fn cosmic_palette(p: &Palette) -> cosmic_theme::CosmicPalette`; `pub fn overlay(b: &mut ThemeBuilder, p: &Palette)`; `pub fn house_style(b: &mut ThemeBuilder)`; `pub fn export_ron(p: &Palette) -> String`; `pub(crate) fn plan(palettes: &[&Palette], dirs: &Dirs) -> Result<NativePlan, TargetError>`. `native::export(CosmicDesktop, p)` returns `cosmic/<Stem>.ron` = `export_ron(p)`.

`target.toml`:

```toml
name = "cosmic-desktop"
native = "cosmic-desktop"
live = true
detect = { binary = "cosmic-settings", config = "~/.config/cosmic" }
```

Mapping, with `R(role)`, `D = derive(p)`, `n0` = background (dark) or container (light), `N(k) = mix(n0, D.neutral_10, k / 10)`, colours as `Srgb`/`Srgba` (alpha 255) via `Srgb::<u8>::new(..).into_format()`:
- `CosmicPaletteInner`: `name` = `p.name.replace(' ', "-")`; `bright_red`, `accent_red`, `accent_pink`, `ext_pink` = error; `bright_green`, `accent_green` = success; `bright_orange`, `accent_orange`, `ext_orange` = accent; `gray_1` = container; `gray_2` = `D.gray_2`; `neutral_0..10` = `N(0..10)`; `accent_blue` = blue; `accent_indigo`, `accent_purple`, `ext_purple`, `ext_indigo` = magenta; `accent_yellow`, `ext_yellow` = warning; `accent_warm_grey`, `ext_warm_grey` = muted; `ext_blue` = cyan. Variant `Dark` or `Light` from the mode.
- `overlay` sets `palette`, `neutral_tint = N(5)`, `bg_color = background`, `primary_container_bg = container`, `secondary_container_bg = pane`, `text_tint = text`, `accent`, `success`, `warning`, `destructive = error`, `window_hint = accent`.
- `house_style`: `Spacing::default()`; `radius_0 = [0.0; 4]`, the other radii `[2.0; 4]`; `frosted = BlurStrength::Low`; `gaps = (0, 7)`; `active_hint = 1`; `frosted_windows`, `frosted_system_interface`, `frosted_panel`, `frosted_applets` true, `frosted_maximized_apps` false; `alpha_map = AlphaMap::default().offset(0.06)`.
- `export_ron` = `ron::ser::to_string_pretty(&b, PrettyConfig::new()) + "\n"` of `ThemeBuilder::default()` after `house_style` then `overlay`.

`plan`, per palette, inside one `TempDir`:
1. Copy the regular files of `dirs.config/cosmic/<builder id>/v2/` and `v1/` (when present) to `<stage_in>/cosmic/<builder id>/v2/` and `.../v1/`, then read the base with `ThemeBuilder::get_entry(&Config::with_custom_path(builder_id, 2, stage_in))`. On `Err((errors, partial))` use `partial` and warn `"COSMIC <dark|light> theme: {n} unreadable keys, defaults used"`.
2. `overlay`, then `write_entry` the builder and `build()`'s `Theme` into a separate stage output folder.
3. Emit the 11 `COLOR_KEYS` builder files and all `Theme` files (sorted by key) as `KeyFileWrite { path: dirs.config/cosmic/<id>/v2/<key>, .. }`.

After both palettes: with exactly one, also write `dirs.config/cosmic/com.system76.CosmicTheme.Mode/v1/is_dark` (`true` or `false`, no newline) and, when `.../Mode/v1/auto_switch` parses to `true`, warn `"COSMIC switches between dark and light automatically; it may switch away from {name}"`. Errors from cosmic-config become `TargetError::Io(format!("COSMIC theme: {e}"))`.

- [ ] **Step 1: Copy the two fixtures and write the failing tests:**
  - `export_matches_rice`: `export_ron(ember)` equals `Ember.ron` and `export_ron(emberday)` equals `EmberDay.ron`, byte for byte.
  - `pins_are_needed`: with Ember's pins cleared, the output's `gray_2` is `#2B2F37FF` and `neutral_tint` is `Some("#76787EFF")`; Ember Day's `neutral_5` is `#868686FF` (the tie 134.5 rounds to even).
  - `plan_single_dark`: Ember on an empty `TestEnv` gives 51 writes: 11 under `cosmic/com.system76.CosmicTheme.Dark.Builder/v2/`, 39 under `cosmic/com.system76.CosmicTheme.Dark/v2/`, and `cosmic/com.system76.CosmicTheme.Mode/v1/is_dark` = `true`. Builder: `accent` = `Some("#E8713AFF")`, `neutral_tint` = `Some("#7E8085FF")`, `bg_color` = `Some("#15171CFF")`, `primary_container_bg` = `Some("#22262EFF")`, `secondary_container_bg` = `Some("#1B1E24FF")`, `text_tint` = `Some("#D6D9E0FF")`, `success` = `Some("#8FB58AFF")`, `warning` = `Some("#D9A441FF")`, `destructive` = `Some("#D85A5AFF")`, `window_hint` = `Some("#E8713AFF")`. Theme `name` = `"Ember"`, `is_dark` = `true`, `gaps` = `(0, 8)`. `env.dirs.config.join("cosmic")` does not exist afterwards.
  - `plan_single_light`: Ember Day gives 51 writes under the Light ids; the builder `palette` starts `Light((\n    name: "Ember-Day"`; Theme and mode `is_dark` are `false`.
  - `plan_pair`: Ember + Ember Day gives 100 writes and no mode write; two dark palettes are `Err(TargetError::Invalid(_))`.
  - `user_settings_survive`: with `.../Dark.Builder/v2/gaps` = `(3, 9)` there is no builder `gaps` write and the Theme `gaps` is `(3, 9)`.
  - `bad_key_warns`: `.../Dark.Builder/v2/frosted` = `NotAVariant` plans with exactly one warning.
  - `auto_switch_warns`: `.../Mode/v1/auto_switch` = `true` warns; absent does not.
  - `deterministic`: two plans are equal.
  - `apply_and_undo`: `apply` with `only = ["cosmic-desktop"]` after creating `~/.config/cosmic`, then `undo`: every file under `~/.config` has its original bytes and the created key files are gone.

- [ ] **Step 2: Run** `cargo test -p irida-core --test cosmic_desktop`. Expected: failures.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test --workspace` and clippy. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): native COSMIC desktop target with mode`.

---

### Tasks 6 to 10: the app targets

Each task creates `irida-core/targets/<app>/` (a `target.toml` and its templates), copies its rice fixtures, and adds `irida-core/tests/builtin_<app>.rs`. A template is the rice file with `Ember Day` becoming `{{ name }}`, `EmberDay` `{{ stem }}`, `emberday` `{{ slug }}` (or Ember's equivalents), and each colour becoming the role or formula given. `matches_rice_files` renders with `Renderer::for_export` for `ember` and `emberday` and compares with the fixture byte for byte. Apply tests make the target detectable first (create its config folder, or `~/.zshrc`) and pass `only = ["<app>"]`. Each task adds its name to the expected list in `nothing_to_apply_lists_every_target` if Task 5's form needs it, ends with `cargo test --workspace` and clippy passing, and commits `feat(targets): built-in <app> target`.

### Task 6: Neovim

`target.toml`: `detect = { binary = "nvim", config = "~/.config/nvim" }`; `[[file]]` `~/.config/nvim/colors/irida.lua` from `colors.lua.tmpl`; `[reload] hint = "run :colorscheme irida in Neovim (add vim.cmd.colorscheme('irida') to your config to keep it)"`; `[[rice]]` `nvim/colors/{{ slug }}.lua` (`colors.lua.tmpl`), `nvim/lua/plugins/{{ slug }}.lua` (`plugin.lua.tmpl`), `nvim/syntax/ron.vim` (`ron.vim.tmpl`), `nvim/install.sh` (`install.sh.tmpl`, executable).

- `colors.lua.tmpl` starts `{% set scheme = slug if export else "irida" -%}`. Header line 1 names `{{ name }}` and `cosmic-{{ slug }}-theme`; line 3 holds `({{ pane }}, floats on the window colour)`; line 5 ends `:colorscheme {{ scheme }}.`; `vim.g.colors_name = "{{ scheme }}"`. The `p` table: `bg = pane`, `bg1 = background`, `bg2 = mix(pane, "#FFFFFF" if mode == "dark" else "#000000", 0.08)`, `fg = text`, `muted`, `dim = comment`, `accent`, `success`, `warning`, `error`, `kw = magenta`, `fn = blue`, `str = success`, `num = warning`, `typ = cyan`, `op = muted`, `cmt = comment`, `spec = accent`, `link = cyan`. `vim.o.background = "{{ mode }}"`. `terminal_color_0..7` = `term.normal.<black..white>`, `8..15` = `term.bright.<black..white>`. Everything else is verbatim.
- `plugin.lua.tmpl`: line 1 uses `{{ stem }}`, line 4 `colorscheme = "{{ slug }}"`. `ron.vim.tmpl`: line 2 `cosmic-{{ slug }}-theme`. `install.sh.tmpl`: `{{ stem }}` on line 2, `{{ slug }}` on lines 7, 8, 27, 28, 33, 34 and 37.
- Fixtures: `nvim/colors/<s>.lua`, `nvim/lua/plugins/<s>.lua`, `nvim/syntax/ron.vim`, `nvim/install.sh` for both rices.
- Tests: `matches_rice_files` (4 files x 2); `apply_names_scheme_irida`: the `Renderer::new` render differs from the rice `colors/ember.lua` in exactly lines 5 and 19, which read `irida` where the rice reads `ember`; `bg2_formula`: Ember `#2D3036`, Ember Day `#CBC8C0`; `apply_writes_colors_only`: with `~/.config/nvim` created, apply writes only `colors/irida.lua` and reports `Applied(Restart(<the hint>))`.

### Task 7: fastfetch

`target.toml`: `detect = { binary = "fastfetch", config = "~/.config/fastfetch" }`; `[[file]]` `~/.config/fastfetch/config.jsonc` (`config.jsonc.tmpl`) and `~/.config/fastfetch/cosmic.txt` (`cosmic.txt`, no tags); `[[rice]]` `fastfetch/config.jsonc` (`config.jsonc.tmpl`), `fastfetch/cosmic.txt` (`cosmic.txt`), `fastfetch/install.sh` (`install.sh.tmpl`, executable). No reload.

- `config.jsonc.tmpl` is Ember's file, starting `{% set lines = mix(text, pane, 0.70)|ansi -%}`, with `"38;2;214;217;224"` (2 places) as `"{{ text|ansi }}"`, `\u001b[38;2;83;86;92m` (9) as `\u001b[{{ lines }}m`, `\u001b[38;2;176;139;196m` (3) as `\u001b[{{ magenta|ansi }}m`, `"38;2;232;113;58"` (20) as `"{{ accent|ansi }}"`, and the palette-dots command (line 118) as `echo -n '{{ name }} '; for c in {{ text|rgb }} {{ accent|rgb }} {{ success|rgb }} {{ warning|rgb }} {{ error|rgb }} {{ magenta|rgb }} {{ cyan|rgb }};` followed by the rest of the original command. A future `{#` must be written `{{ '{#' }}`. `install.sh.tmpl`: `{{ name }}` on line 2.
- Fixtures: `fastfetch/config.jsonc`, `cosmic.txt`, `install.sh` for both rices.
- Tests: `matches_rice_files` (3 x 2); `lines_round_half_even`: Ember Day's render contains `38;2;168;166;162m`; `replaces_existing_with_note`: with an existing `config.jsonc`, apply warns `replaced your file`.

### Task 8: lsd

`target.toml`: `detect = { binary = "lsd", config = "~/.config/lsd" }`; `[[file]]` `~/.config/lsd/colors.yaml` (`colors.yaml.tmpl`); `[[file]]` `~/.config/lsd/config.yaml` (`theme.yaml`, `create_only = true`, content `color:\n  theme: custom\n`); `[[rice]]` `lsd/colors.yaml` (`colors.yaml.tmpl`), `lsd/config.yaml` (`config.yaml`, static: `color:\n  when: auto\n  theme: custom\n`), `lsd/install.sh` (`install.sh.tmpl`, executable; `{{ name }}` on line 2, `zsh/{{ slug }}.zsh` where it appears).

- `colors.yaml.tmpl` keeps the rices' keys and writes each value as `[{{ role|rgb }}]`: `user` accent, `group` muted, `permission.read` muted, `write` warning, `exec` and `exec-sticky` error, `no-access` comment, `octal`/`acl`/`context` text, `date.hour-old` accent, `day-old` success, `older` comment, `size.none` comment, `small` success, `medium` warning, `large` error, `inode.valid` success, `invalid` error, `links.valid` text, `invalid` error, `tree-edge` comment, `git-status.default`/`unmodified`/`ignored` comment, `new-in-index`/`new-in-workdir` success, `typechange` warning, `deleted`/`conflicted` error, `renamed` text, `modified` accent. Its 3-line `#` header names `{{ name }}` and says the arrays are truecolor.
- Fixtures: `lsd/config.yaml`, `lsd/install.sh` for both rices; `expected/lsd/ember-colors.yaml` and `expected/lsd/emberday-colors.yaml`, rendered once from the template, checked against the mapping above, and committed.
- Tests: `static_files_match_rice` (config.yaml, install.sh x 2); `colors_snapshot`; `colors_are_rgb_arrays`: every line that is not a `#` comment and does not end with `:` matches `^\s*[a-z-]+: \[\d{1,3},\d{1,3},\d{1,3}\]$`, and Ember's `user` line is `user: [232,113,58]`; `apply_creates_theme_once`: with `~/.config/lsd` created, apply creates `config.yaml` as `color:\n  theme: custom\n`; with an existing `color:\n  when: auto\n` the file is unchanged and lsd's warnings contain ``add `theme: custom` under `color:` ``.

### Task 9: zsh

`target.toml`: `detect = { config = "~/.zshrc" }`; `[[file]]` `~/.zshrc`, `write = "block"`, `comment = "#"`, `zsh.tmpl`; `[reload] hint = "open a new shell or run exec zsh"`; `[[rice]]` `zsh/{{ slug }}.zsh` (`zsh.tmpl`), `zsh/install.sh` (`install.sh.tmpl`, executable; `{{ slug }}` on lines 2, 13 and 28, line 31 `printf '\n# {{ name }} rice\n%s\n'`).

- `zsh.tmpl`: `{% if export -%}` around the rice header (lines 1 to 14), then the colour part (rice lines 15 to 46) always, then `{% if export %}` around the alias and prompt part (lines 48 to 75, verbatim); the template ends with `{% endif %}` and no newline after it. In the header, line 9 (`pane:`, `fg:`) and line 13 (`muted:`, `comment:`) keep their labels and template only the hexes; the seven labels on lines 10 to 12 (accent, success, warning, error, blue, magenta, cyan) are `{{ "%-8s"|format((names.<role>|lower|split|first|default("<role>")) ~ ":") }}` followed by one space and the hex. Colours: `di=1;{{ blue|ansi }}`, `ln=3;` cyan, `ex=1;` success, archives `4;` warning, images magenta, video `3;` magenta, source files success, text files text, `or=9;` error; `ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE='fg={{ mix(comment, pane, 0.30) }}'`; the 20 `ZSH_HIGHLIGHT_STYLES` as in the rice.
- Fixtures: `zsh/<s>.zsh`, `zsh/install.sh` for both rices.
- Tests: `export_matches_rice` (2 files x 2); `apply_block_is_colour_part`: the `Renderer::new` render equals rice lines 15 to 46 (`split_inclusive('\n').skip(14).take(32)`); `autosuggest`: `#474C59` (Ember), `#A4A6AA` (Ember Day); `apply_appends_marked_block`: a `.zshrc` of `export X=1\n` ends with `# >>> irida >>>\n<body># <<< irida <<<\n` after apply, reports `Applied(Restart("open a new shell or run exec zsh"))`, and is `export X=1\n` again after undo; `not_detected_without_zshrc`: with a fake `zsh` binary on `ctx.path` and no `~/.zshrc`, apply with `only = ["zsh"]` is `Err(ApplyError::NothingToApply(results))` whose `zsh` entry is `Skipped(NotInstalled)`, and `~/.zshrc` does not exist.

### Task 10: cava

`target.toml`: `detect = { binary = "cava", config = "~/.config/cava" }`; `live = true`; `[[file]]` `~/.config/cava/config`, `write = "block"`, `comment = "#"`, `cava.tmpl`; `[reload] signal = { process = "cava", signal = "USR2" }`; `[[rice]]` `cava/config.colors` (`cava.tmpl`), `cava/install.sh` (`install.sh.tmpl`, executable).

- `cava.tmpl`:

```
{% set g = ramp("success", "accent", 4) + ramp("accent", "error", 5)[1:] -%}
[color]
gradient = 1
gradient_count = {{ g|length }}
{% for c in g -%}
gradient_color_{{ loop.index }} = '{{ c }}'
{% endfor -%}
```

- `install.sh.tmpl` (Irida's own installer; `#!/usr/bin/env bash`, `set -euo pipefail`; `{{ name }}` in the header comment only). When `cava` is not on `PATH` and `${XDG_CONFIG_HOME:-$HOME/.config}/cava` does not exist, it prints `  cava not installed, skipped` and exits 0. Otherwise it creates the folder, then writes `$dir/config.colors` between `# >>> irida >>>` and `# <<< irida <<<` in `.../cava/config`: appends when there are no markers (adding a newline first when the file lacks one), replaces between them when there is exactly one ordered pair, and otherwise prints `<file> has broken irida markers; fix them by hand, nothing written` and exits 1. It backs up to `config.bak` only when the file existed and changed, prints `already up to date` when nothing changed, and ends with `Done. A running cava picks up the colours with: pkill -USR2 cava`.
- Fixtures: `expected/cava/ember.block`, `expected/cava/emberday.block`, `expected/cava/ember-install.sh`, `expected/cava/emberday-install.sh`.
- Tests: `block_snapshot`; `install_snapshot`; `gradients`: Ember's colours 1 to 8 are `#8FB58A #B4A272 #D18C58 #E8713A #E46B44 #E0664C #DC6054 #D85A5A`, Ember Day's `#4C6B45 #706239 #8C552C #A4441A #A24225 #A13F2D #9F3D34 #9E3A3A`, and `gradient_count = 8`; `apply_appends_then_replaces`: with `~/.config/cava/config` = `[general]\n[eq]\n` (empty `ctx.path`, so no signal is sent and the status is `NotRunning`), the block is appended after `[eq]`, and a second apply with Ember Day leaves one marker pair with the new colours; `install_script_is_valid`: the render passes `/bin/bash -n`.

---

### Task 11: Rice import

**Files:**
- Create: `irida-core/src/import.rs`, `irida-core/tests/import.rs`, fixtures under `rices/cosmic-<slug>-theme/` (below)
- Modify: `irida-core/Cargo.toml` (`regex = "1"`), `irida-core/src/store.rs`, `irida-core/src/lib.rs`, `irida-core/tests/fixtures/palettes/{noir,dossier}.toml` (add `[pins]`)

**Interfaces:**
- Consumes: `term::ColorScheme`, `derive`, `Roles::new`, `Pins`, `validate_slug`, `validate_name`, `cosmic_theme::ThemeBuilder` (parsed with `ron::from_str`).
- Produces: `pub fn import_rice(dir: &Path) -> Result<Palette, ImportError>`; `ImportError { pub path: PathBuf, pub message: String }`, `Display` `"{path}: {message}"`; `pub fn is_rice_folder(dir: &Path) -> bool`; `StoreError::Import { path: PathBuf, message: String }`. Both functions read the folder name from `dir.canonicalize()` (a folder that cannot be canonicalised is not a rice folder), so `.` works. `Store::resolve`: a folder without `palette.toml` that passes `is_rice_folder` is imported; any other folder is `NotFound(format!("{arg}: no palette.toml, and not a rice repo (the folder must be named cosmic-<slug>-theme)"))`.

Rules, in order:
1. Slug from the folder name `cosmic-<slug>-theme`. Exactly one `cosmic/*.ron` (else an error naming the count and the files); stem = its file name without `.ron`. Mode from the parsed builder's `palette.is_dark()`.
2. The scheme name V = the unquoted contents of `cosmic/config/com.system76.CosmicTerm/v1/syntax_theme_<mode>`. Use `cosmic-term/<stem>-term.ron` when its `name == V`, else the one `cosmic-term/*.ron` whose `name == V` (else an error naming every file read). If `V.replace(' ', "") != stem`, fail with `syntax_theme_<mode> selects "V" but cosmic/<stem>.ron names <stem>; select the <stem> scheme or rename the files`. The palette name is V.
3. Roles. Path 1 when `scripts/contrast.py` exists and `background`, `pane` and `container` each match exactly once, `roles` matches exactly 10 times with each role label once, and `surfaces` matches exactly 3 times with each of Background, Containers and Terminal once:

```
background: (?m)^WINDOW = "(#[0-9A-Fa-f]{6})"$
pane:       (?m)^PANE = "(#[0-9A-Fa-f]{6})"$
container:  (?m)^\s*\("Containers", "[^"]*", \["(#[0-9A-Fa-f]{6})"
roles:      (?m)^\s*\("(Text|Accent|Success|Warning|Error|Blue|Magenta|Cyan|Muted|Comment)", "([^"]*)", "(#[0-9A-Fa-f]{6})", (True|False)\),$
surfaces:   (?m)^\s*\("(Background|Containers|Terminal)", "([^"]*)", \[([^\]]*)\]\),$
```

   Names: each role's group 2; background's from the `Background` surface; `Containers`' name split on `" / "` gives container then pane.
   Path 2 otherwise: builder `bg_color` background, `primary_container_bg` container, `accent`, `success`, `warning`, `destructive` error; scheme `background` pane (or background when absent), `foreground` text, `dim_foreground` muted, `normal.blue`, `normal.magenta`, `normal.cyan`; comment from `nvim/colors/<slug>.lua`, or the only `nvim/colors/*.lua`, by `\bcmt\s*=\s*"(#[0-9A-Fa-f]{6})"` (exactly one match; otherwise an error mentioning `cmt`). No names.
4. Pins, staged: pin each `normal.*` that differs from `derive`; re-derive and pin each `bright.*`; re-derive and pin `dim.*`, `foreground`, `cursor`, `bright_foreground`, `dim_foreground`, and `background` (`"none"` when the scheme has none, else its colour when it differs). Then pin `cosmic.neutral_10` and `cosmic.gray_2` when they differ from the builder's `neutral_10` and `gray_2`.
5. `pair` and `notes` are `None`.

Fixtures (verbatim copies): ember and emberday `scripts/contrast.py`, `cosmic-term/<Stem>-term.ron`, `cosmic/config/com.system76.CosmicTerm/v1/syntax_theme_<mode>`; noir and dossier `cosmic/<Stem>.ron`, every `cosmic-term/*.ron` (Noir has six), `syntax_theme_<mode>`, `nvim/colors/<slug>.lua`. Add these `[pins]` to `noir.toml` and `dossier.toml` (keep their `pair`), each key and value quoted as in `ember.toml` (`"bright.black" = "#525252"`):

Noir (22): `"background" = "none"`, `"bright.black" = "#525252"`, `"bright.blue" = "#F56E0F"`, `"bright.cyan" = "#959C95"`, `"bright.green" = "#A3BC98"`, `"bright.magenta" = "#F0913A"`, `"bright.red" = "#F56E0F"`, `"bright.white" = "#DADEDD"`, `"bright.yellow" = "#C9C6B2"`, `"cosmic.gray_2" = "#2A2A2A"`, `"cosmic.neutral_10" = "#DADEDD"`, `"dim.black" = "#171717"`, `"dim.blue" = "#B85A1A"`, `"dim.cyan" = "#5E635E"`, `"dim.green" = "#6E8565"`, `"dim.magenta" = "#B85A1A"`, `"dim.red" = "#B8431F"`, `"dim.white" = "#959C95"`, `"dim.yellow" = "#8E8B78"`, `"normal.black" = "#101010"`, `"normal.white" = "#BCBFBC"`, `"normal.yellow" = "#AEAB94"`.

Dossier (21): `"bright.black" = "#383835"`, `"bright.blue" = "#3E3D31"`, `"bright.cyan" = "#4F1622"`, `"bright.green" = "#2E3125"`, `"bright.magenta" = "#5A3A2E"`, `"bright.red" = "#DB0030"`, `"bright.white" = "#6B7360"`, `"bright.yellow" = "#DE6A41"`, `"bright_foreground" = "#151515"`, `"cosmic.gray_2" = "#E4E4DC"`, `"cursor" = "#DE6A41"`, `"dim.black" = "#45443D"`, `"dim.blue" = "#6E6E58"`, `"dim.cyan" = "#8E5A64"`, `"dim.green" = "#7A7F66"`, `"dim.magenta" = "#96786A"`, `"dim.red" = "#B84A5E"`, `"dim.white" = "#7C7A70"`, `"dim.yellow" = "#B0714F"`, `"normal.black" = "#2E3125"`, `"normal.yellow" = "#953B19"`.

- [ ] **Step 1: Copy the fixtures, add the pins, and write the failing tests:**
  - `imports_all_four`: for each rice, `import_rice` gives the fixture's `name`, `slug`, `mode`, `roles`, `names` and `pins`, and `pair == None`.
  - `terminal_round_trips`: for each, `derive(&imported).term.to_scheme(&imported.name).to_ron() + "\n"` equals its `cosmic-term/<Stem>-term.ron`.
  - `desktop_round_trips_for_ember`: `export_ron` of the imported Ember and Ember Day equals their `.ron` files.
  - `noir_picks_named_scheme`: Noir's name is `Noir`, read from `Noir-term.ron` rather than the five variants; in a copy whose `syntax_theme_dark` is `"Noir Smoke"`, import fails with a message containing `rename the files`; in a copy whose value is `"Nope"`, the message names `Noir-term.ron` and `Noir-term-smoke.ron`.
  - `errors_name_the_problem`: a copy with a second `cosmic/Other.ron` errors naming both files; a folder named `my-theme` is not a rice folder; a Noir copy without `nvim/` errors mentioning `cmt`.
  - `store_resolves_rice_folders`: `Store::resolve(<path to cosmic-ember-theme>)` imports Ember; a path ending in `/.` inside the fixture repo also does; an empty folder is `NotFound` with the new message.

- [ ] **Step 2: Run** `cargo test -p irida-core --test import --test store`. Expected: failures.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: all pass, including plan 1's tests on the Noir and Dossier fixtures.

- [ ] **Step 5: Commit** `feat(core): import rice repos as palettes`.

---

### Task 12: Export engine

**Files:**
- Create: `irida-core/src/export.rs`, `irida-core/rice/cosmic/install.sh.tmpl`, `irida-core/rice/cosmic/config/...` (13 baseline files), `irida-core/rice/gitignore` (published as `.gitignore`), `irida-core/rice/pre-commit-config.yaml` (published as `.pre-commit-config.yaml`), `irida-core/tests/export.rs`, fixtures for both rices: `btop/<Stem>.theme`, `cosmic/install.sh`, the 13 baseline files, `.gitignore`, `.pre-commit-config.yaml`
- Modify: `irida-core/src/lib.rs`, `irida-core/build.rs` (`cargo:rerun-if-changed=rice`)

**Interfaces:**
- Consumes: `load_targets`, `Renderer::for_export`, `check_syntax`, `native::export`, `contrast::check`, `atomic_write`, `resolve_write_path`.
- Produces:
  - `pub struct ExportRequest<'a> { pub palette: &'a Palette, pub pair: Option<&'a Palette>, pub folder: &'a Path }`.
  - `pub enum ExportStatus { Created, Updated, Unchanged, Kept, Skipped(String), Stale }`; `pub struct ExportLine { pub path: String, pub status: ExportStatus }`; `pub struct ExportReport { pub repo: PathBuf, pub lines: Vec<ExportLine>, pub notes: Vec<String>, pub contrast: ContrastReport }`.
  - `pub enum ExportError { WrongFolder { folder: PathBuf, slug: String, name: String }, Render { target: String, message: String }, Io { path: PathBuf, source: std::io::Error } }`; `WrongFolder` displays `{folder} belongs to another rice; export {name} into a folder named cosmic-{slug}-theme or into its parent`.
  - `pub fn export(dirs: &Dirs, req: &ExportRequest) -> Result<ExportReport, ExportError>`.
  - `pub const BY_HAND: &str = "wallpaper (backgrounds/), icons (icons/, icon_theme), cursor (cursors/), Firefox (firefox/), Claude Desktop (claude-desktop/), scripts/update_system.sh, LICENSE, screenshot (assets/screenshot.png)"`.

Steps inside `export`:
1. **Folder.** Take the name from `req.folder` as given (canonicalise first only when it ends in `.` or `..`). Named `cosmic-<slug>-theme` for this slug: it is the repo. Matching `^cosmic-[a-z]+-theme$` for another slug: `WrongFolder`. Otherwise it is the parent and the repo is `<folder>/cosmic-<slug>-theme`. Folders are created only in step 3.
2. **Plan, before writing**, in this order so the first source of a path wins: native exports (`native::export` for built-in native targets), built-in targets' `[[rice]]` entries, Irida's assets (`cosmic/install.sh` from the template, executable; the `cosmic/config/` baseline, `.gitignore` and `.pre-commit-config.yaml`, create-only), the docs (Task 13), then user targets' `[[rice]]` entries. For each rice entry: render `path` with the export renderer; reject it with `Skipped("path leaves the repo")` when it is empty, absolute, or has a `..` or empty component; a missing template gives `Skipped("missing template {t}")`; `check_syntax` failure gives `Skipped`; a render error is `ExportError::Render` and nothing is written. A path already planned gives `Skipped("already written by {first source}")`. A target that fails to load gives `Skipped` on the line `target <name>`. `icon_theme` is never written; `syntax_theme_<mode>` comes from the terminal export and is always written.
3. **Write.** Create the repo folder. For each planned path, resolve it with `resolve_write_path(&repo.join(rel))`, canonicalise its nearest existing ancestor, and require the result to lie inside the canonical repo; otherwise `Skipped("path leaves the repo")`. Then: `Unchanged` when the bytes are equal, else `atomic_write` and `Created` or `Updated`; set mode `0o755` on every executable file, `Unchanged` included. Create-only assets that exist are `Kept`. A write error is `ExportError::Io`.
4. **Stale.** Each existing `cosmic/*.ron`, `cosmic-term/*-term.ron`, `btop/*.theme`, `nvim/colors/*.lua`, `nvim/lua/plugins/*.lua` and `zsh/*.zsh` that is not among this run's planned paths gets a `Stale` line. Nothing is deleted.
5. **Notes.** When `cosmic/<Stem>.ron` is `Updated`: `cosmic/<Stem>.ron: rewritten from the palette; hand-tuned radii, gaps, frosting and colours in the old file are replaced (git diff shows them)`. `contrast = contrast::check(palette)`. Lines are sorted by path.

`cosmic/install.sh.tmpl` is Ember Day's `cosmic/install.sh` with `Ember Day` as `{{ name }}` and `EmberDay` as `{{ stem }}`. The 13 baseline files are Ember's `cosmic/config/` files other than `syntax_theme_dark` and `icon_theme`, verbatim (no trailing newlines). The embedded `rice/` folder is walked recursively with `Dir::entries()` (`DirEntry::Dir` and `DirEntry::File`).

- [ ] **Step 1: Copy the fixtures and write the failing tests** (each export targets a folder inside a `TestEnv`):
  - `ember_repo_matches`: exporting Ember into a parent creates `cosmic-ember-theme/` in which each of these equals the fixture repo's file: `cosmic/Ember.ron`, `cosmic/install.sh`, the 14 `cosmic/config/` files (13 baseline plus `syntax_theme_dark`), `cosmic-term/Ember-term.ron`, `btop/Ember.theme`, the 4 nvim files, the 3 fastfetch files, `lsd/config.yaml`, `lsd/install.sh`, `zsh/ember.zsh`, `zsh/install.sh`, `.gitignore`, `.pre-commit-config.yaml`. The same for Ember Day. Every `install.sh` has mode `0o755`.
  - `second_export_unchanged`: a second export reports only `Unchanged` and `Kept` lines.
  - `existing_repo_untouched`: a repo folder holding `backgrounds/x.png`, `cosmic/config/com.system76.CosmicTerm/v1/opacity` = `77` and `cosmic/Old.ron`: after export the png and `opacity` are unchanged (`opacity` is `Kept`) and `cosmic/Old.ron` still exists with a `Stale` line.
  - `escaping_rice_path_rejected`: user targets whose `[[rice]]` paths are `../evil`, the absolute path `<env.tmp>/evil` (written into the `target.toml` at test time) and `{{ '..' }}/evil` each give `Skipped("path leaves the repo")`, and nothing exists outside the repo.
  - `symlink_escape_rejected`: a repo whose `nvim` is a symlink to a folder outside it gets `Skipped("path leaves the repo")` lines for the nvim files, and the outside folder stays empty.
  - `user_target_cannot_overwrite`: a user target whose `[[rice]]` writes `nvim/install.sh` gets `Skipped("already written by nvim")`, and the file equals the built-in render.
  - `wrong_folder`: exporting Ember into a folder named `cosmic-noir-theme` is `WrongFolder`; into a folder named `cosmic-ember-theme` it writes there; into a parent that does not exist yet it creates it.
  - `render_error_writes_nothing`: a user target whose rice template is `{{ nope }}` gives `ExportError::Render`, and the repo folder does not exist.
  - `noir_desktop_note`: exporting the imported Noir into a copy of its fixture repo gives an `Updated` line for `cosmic/Noir.ron`, the rewrite note, and no `Stale` line for the five `Noir-term-*.ron` variants.

- [ ] **Step 2: Run** `cargo test -p irida-core --test export`. Expected: failures.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): export a palette as a rice repo`.

---

### Task 13: README, showcase and switch.sh

**Files:**
- Create: `irida-core/src/export_docs.rs`, `irida-core/rice/scripts/switch.sh.tmpl`, `irida-core/tests/export_docs.rs`, fixtures for both rices: `README.md`, `docs/showcase.md`, `scripts/switch.sh`
- Modify: `irida-core/src/write.rs`, `irida-core/src/template.rs`, `irida-core/src/export.rs`, `irida-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `write::replace_marked(existing: &str, begin: &str, end: &str, body: &str) -> Result<Option<String>, BlockError>`: `Ok(None)` when neither marker is present, `Err(Broken)` for broken markers (the `set_block` rules), else the text with the lines between the markers replaced by `body`.
  - `Renderer::render_with(&self, template_name: &str, source: &str, extra: &[(&str, String)]) -> Result<String, TemplateError>`: the palette context plus `extra`.
  - `export_docs::readme_block(p: &Palette, pair: Option<&Palette>) -> String`, `showcase_block(p: &Palette) -> String`, `readme_skeleton(p: &Palette, block: &str) -> String`, `showcase_skeleton(p: &Palette, pair: Option<&Palette>, block: &str) -> String`, `switch_settings(p: &Palette) -> String`, `rices_list(slug: &str) -> String`.
  - `export` plans `README.md`, `docs/showcase.md` and `scripts/switch.sh` (executable, rendered with `render_with(.., &[("settings", switch_settings(p)), ("rices", rices_list(&p.slug))])`) in its docs slot.

Markers, each on its own line with a blank line before and after: `<!-- >>> irida palette >>> -->` / `<!-- <<< irida palette <<< -->` in `README.md`, `<!-- >>> irida contrast >>> -->` / `<!-- <<< irida contrast <<< -->` in `docs/showcase.md`. An absent file gets its skeleton; a file without markers is `Skipped("no irida markers")`; broken markers are `Skipped("irida markers are broken")`.

- `readme_block`: `notes` and a blank line when set; the role table: `| Role | Color | Hex |`, `|------|-------|-----|`, rows Background, Containers (`<container name> / <pane name>`, hexes joined by `` ` / ` ``), Terminal (`<pane name>, opaque`, or `, transparent` when `background` is pinned `none`), then Text, Accent, Success, Warning, Error, Blue, Magenta, Cyan, Muted, Comment; swatch `![#HEX](https://placehold.co/12x12/hex/hex.png) Name` with the URL lowercase; an unnamed role uses its capitalised role. Then a blank line and the two lines ``Window hint `#ACCENT`. Ratios per role are in `docs/showcase.md`, or run`` and ``` `irida check .`. ```. When `pair` is given: a blank line, `| Role | <name> | <pair name> |`, `|------|` followed by `len(name) + 2` dashes and `|` per name column (`|------|-------|-----------|` for Ember / Ember Day), rows Background, Text, Accent, Success.
- `showcase_block`: ``Every text colour is checked against the window background `#BG` and the`` / ``terminal pane `#PANE`.``, a blank line, `| Role | Hex | On background | On pane | Minimum |`, `|------|-----|---------------|---------|---------|`, then one row per `RoleContrast`. Label: `<Name>, <role>`, or the capitalised role when the role is unnamed or its name equals the capitalised role (`Muted`, `Comment`). Ratios `{:.1}:1`; a failing ratio adds ` (fail)` and, when its one-decimal form is not below the minimum, shows two decimals (`4.45:1 (fail)`). Minimum `4.5:1`, `3:1` or `-`.
- `readme_skeleton`: `# COSMIC · {name}`, `## Palette`, the marked block, `## Setup` with the switch.sh line and the per-file steps, `## License`. `showcase_skeleton`: `# {name}`, `> The {mode} half of {name}` (plus ` / {pair name}` when paired) `.`, `## Palette`, the marked block.
- `switch.sh.tmpl` is Ember's `scripts/switch.sh` with line 2 `# Make {{ name }} the active rice, coming from any other cosmic-*-theme rice:`, lines 8 to 13 replaced by `{{ settings }}`, the `RICES` line `RICES="{{ rices }}"`, and three changes: the tool loop's comment and list become `# Terminal tools; bat, starship and cava only exist in some repos` and `for tool in fastfetch lsd bat starship cava; do`; the `# Updater banner` block (the `update_system.sh` link into `$HOME`, its echo and the blank line after) is removed, because an exported rice never links anything into the home folder; the claude-desktop line ends `|| echo "  claude-desktop: skipped (see above)"` and the `ls claude-desktop/*.jsonc` gets `2>/dev/null`. `switch_settings` builds the six lines `NAME`, `SLUG`, `MODE`, `RON` (`<Stem>.ron`), `TERM_RON` (`<Stem>-term.ron`), `BTOP` (`<Stem>`) as `format!("{a:<35} # {comment}")` with the rice comments; `rices_list` is `darkgold whitegold tokyonight tokyoday ember emberday monochrome noir dossier harbordark` plus the slug when missing.

- [ ] **Step 1: Copy the fixtures and write the failing tests:**
  - `readme_block_matches`: the fixture README's lines from `| Role | Color | Hex |` through the last pair-table row equal `readme_block(ember, Some(emberday))`'s, except the hint's second line, which reads `` `irida check .`. `` instead of `` `python3 scripts/contrast.py`. ``; the same for Ember Day with Ember as its pair.
  - `showcase_rows`: Ember's block contains ``| Brick, error | `#D85A5A` | 4.7:1 | 4.4:1 (fail) | 4.5:1 |`` and ``| Comment | `#5A6070` | 2.9:1 | 2.7:1 | - |``; Dossier's contains ``| Success | `#51573B` | 4.9:1 | 4.45:1 (fail) | 4.5:1 |``.
  - `switch_sh`: Ember's render equals the fixture `scripts/switch.sh` after applying exactly the three changes to it; it passes `/bin/bash -n`; a palette named `Copper Tide` renders `NAME="Copper Tide"                  # shown in messages` and a `RICES` line ending ` coppertide"`.
  - `replace_marked_rules`: no markers, one pair, broken.
  - `docs_in_export`: exporting Ember into an empty parent writes skeletons whose blocks sit between markers; into a repo whose README has no markers, the README is unchanged with a `Skipped("no irida markers")` line; with markers, only the block changes.

- [ ] **Step 2: Run** `cargo test -p irida-core --test export_docs`. Expected: failures.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(core): README, showcase and switch.sh in exported rices`.

---

### Task 14: `irida export` and rice folders on the command line

**Files:**
- Modify: `irida/src/main.rs`, `irida/src/output.rs`, `irida/tests/cli.rs`

**Interfaces:**
- Consumes: `export`, `ExportRequest`, `ExportReport`, `BY_HAND`, `Store::resolve` (now importing rice folders), `output::format_check`.
- Produces: `Command::Export { palette: String, folder: PathBuf }`; `output::format_export(r: &ExportReport, p: &Palette) -> String`:

```
Exported {name} to {repo}
  {status:<10}{path}         status words: created, updated, unchanged, kept, skipped, stale?
  ...                        skipped lines add ": {reason}"; stale? lines add " (not written by this export)"
{each note}
{n} existing files were rewritten; review with git diff     (only when n > 0)
By hand: {BY_HAND}
Contrast: {n} failure(s)    (or "Contrast: all pass"), then format_check's header and failing rows
```

  Exit 0 on success (contrast failures included), 2 on `ExportError` (message after `irida: `). The pair is loaded as `apply` does, with the warning saying `exporting alone` instead of `applying alone`. `apply` saves `last_palette` only when `<palette>` resolved to a saved palette (by slug or from the store), not for a file or a rice folder.

- [ ] **Step 1: Update `apply_env`** in `irida/tests/cli.rs`: `PATH` holds only the temp `bin/` with fake `btop`, `cosmic-term`, `cosmic-settings`, `nvim`, `fastfetch` and `lsd`. **Write the failing tests:**
  - `export_ember`: after creating `<tmp>/out`, `irida export ember <tmp>/out` exits 0, prints `Exported Ember to` and `created   cosmic/Ember.ron`, and the repo's `cosmic/Ember.ron` equals the fixture.
  - `check_rice_folder`: `irida check <fixture cosmic-ember-theme>` exits 1 and prints `error    #D85A5A   4.7:1   4.4:1  4.5  FAIL on pane`.
  - `round_trip`: export then `irida check <tmp>/out/cosmic-ember-theme` prints the same rows as `irida check ember`.
  - `apply_rice_folder`: `irida apply <fixture cosmic-ember-theme> --only btop` exits 0, writes the btop theme, and leaves `last_palette` unset.
  - `export_wrong_folder`: into an existing `<tmp>/cosmic-noir-theme` exits 2 with stderr containing `irida: ` and `cosmic-noir-theme`.
  - `apply_all_then_undo`: with `~/.zshrc` = `export X=1\n` and `~/.config/{nvim,fastfetch,lsd}` created, `irida apply ember` reports `applied` for cosmic-desktop, cosmic-term, btop, nvim, fastfetch, lsd and zsh; after `irida undo`, `config_listing` and `~/.zshrc` equal their bytes before the apply.
  - Plan 1's CLI tests still pass with the narrower `PATH`.

- [ ] **Step 2: Run** `cargo test -p irida --test cli`. Expected: failures.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check`. Expected: all clean.

- [ ] **Step 5: Commit** `feat(cli): irida export, and rice folders as palettes`.

---

## Done when

- `cargo test --workspace`, clippy with `-D warnings` and `cargo fmt --all --check` pass.
- `irida export` of Ember and Ember Day reproduces every listed rice file byte for byte, and `irida check` on each of the four fixture repos gives plan 1's verdicts.
- Applying a palette to a temporary home writes the COSMIC desktop, COSMIC Terminal, btop, Neovim, fastfetch, lsd and zsh files (cava in its own core test), and `irida undo` restores every file's bytes.
- No file in the diff contains an em-dash or en-dash.

**Manual checks for the user** (spec section 10, "each checked once by hand against the real app"; the sandbox cannot run a desktop session): apply Ember on the real machine and look at COSMIC Settings, COSMIC Terminal, btop (restart it), Neovim (`:colorscheme irida`), fastfetch, `lsd -l`, a new zsh, and cava; then `irida undo`.
