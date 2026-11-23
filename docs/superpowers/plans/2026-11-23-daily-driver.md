# Daily driver (System Settings sections, system commands, calculator history) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the palette find System Settings sections and system commands, ranked with apps, and give the calculator a reusable history.

**Architecture:** The Rust app index becomes an item index (`app`, `pane`, `command`), each with an `id`; the frontend ranks one list and sends Enter to one Rust `run` command that dispatches on the kind. Frecency is keyed by id (app ids are their paths, so `history.json` carries over). Calculator history is a small Rust JSON store plus frontend rows ahead of the items.

**Tech Stack:** Tauri 2 (Rust, objc2 / objc2-app-kit / objc2-foundation), React 19 + Vite, Vitest (static render, no DOM), Astro docs site in `site/`.

**Spec:** `docs/superpowers/specs/2026-11-23-daily-driver-design.md`

## Global Constraints

- No new npm or cargo dependency. Extra features on existing crates (`objc2-foundation`, `objc2-app-kit`) are allowed.
- Run the full gate before every commit: `pnpm build`, `pnpm test`, `cd src-tauri && cargo test`, `pnpm docs:build`. Never push a red commit.
- Tests run without a DOM: logic goes in pure functions; what a test must see goes through `PaletteView` props.
- Every PR updates tests, `README.md`, `site/src/pages/usage.astro` (and `index.astro` cards when relevant) and adds a `minor` changeset for user-visible features.
- New Tauri commands go in `MAIN_COMMANDS` only (`src-tauri/src/lib.rs`), except settings reads/writes already in `SETTINGS_COMMANDS`.
- Command names are English only. Section names: system language as `name`, English in `altNames` when different.
- Confirm copy, verbatim: "Press Enter again to restart", "Press Enter again to shut down", "Press Enter again to log out", "Press Enter again to empty the Trash".
- History: max 20 stored, 5 shown under a live result, 20 shown for the query `=`.
- Settings keys `systemCommands` and `calculator`, both default `true`; a settings file without them reads as `true`.
- `NSAppleEventsUsageDescription` in `src-tauri/Info.plist`; hop is ad-hoc signed, so macOS may re-prompt after updates (owner accepted). README and Usage say so.
- Workflow per PR: branch + worktree, PR with `## Summary` / `## Test plan`, adversarial review with mutation checks, merge with `gh pr merge <n> --squash --admin` when CI is green and the review passed.

## Review Focus

1. A section and an app with the same visible icon path (every section uses the System Settings app icon): React row keys must be `id`, not `path`, or rows collide. Test in Task 3.
2. Typing after arming Restart ("re" Enter, then "a"): the query change must disarm, so a later Enter launches the new top row instead of restarting. Test in Task 6.
3. A 0.2.0 `settings.json` (no `systemCommands`/`calculator` keys) must keep both features on. Test in Task 5 and Task 9.
4. ⌘Enter on a history row whose result is in exponent form (`1.84467440737e+19`): the appended query must still parse. Test in Task 8.
5. The user ignores "System Settings.app": section rows still open (icon hidden, not an error). Test in Task 3 (`known` stays app-only; `run` checks pane ids against the pane list).

---

## PR 1: `feat: System Settings sections in search` (branch `feat/settings-sections`)

### Task 1: Items in the Rust index

**Files:**
- Modify: `src-tauri/src/apps.rs` (struct `App` → `Item`, `Index.apps` → `Index.items`, `known`)
- Modify: `src-tauri/src/icons.rs:88` (uses `known`)

**Interfaces:**
- Produces: `pub enum Kind { App, Pane, Command }` (serde `lowercase`); `pub struct Item { kind: Kind, id: String, name: String, alt_names: Vec<String>, path: Option<String>, confirm: Option<String> }` (serde camelCase, `path`/`confirm` skipped when `None`); `pub fn known_app(app, path) -> bool` (apps only); `pub fn item(app, id) -> Option<Item>`.
- App items: `id == path`, `path: Some(path)`.

- [ ] **Step 1: Write failing tests** in `apps.rs` tests: `app_items_use_the_path_as_id` (`app_entry(p, "X").id == p` and `kind == Kind::App`); `known_app_ignores_other_kinds` (an index with a pane item whose `path` is the System Settings app is not `known_app` unless an app item has that path).
- [ ] **Step 2: Run** `cd src-tauri && cargo test apps::` — expect compile FAIL (no `Kind`).
- [ ] **Step 3: Implement** the types; keep `list_apps`/`apps-changed` names, payload `{ generation, items }`. Rename `known` to `known_app` and update `icons.rs`.
- [ ] **Step 4: Run** `cargo test` — PASS.
- [ ] **Step 5: Commit** `refactor: the app index holds items with ids`.

### Task 2: Scan System Settings sections

**Files:**
- Create: `src-tauri/src/panes.rs`
- Modify: `src-tauri/Cargo.toml` (`objc2-foundation` features if needed: `NSLocale`, `NSArray`)
- Modify: `src-tauri/src/apps.rs` (`refresh` appends `panes::index()`)

**Interfaces:**
- Produces: `pub const ROOT: &str = "/System/Library/ExtensionKit/Extensions"`; `pub const SETTINGS_APP: &str = "/System/Applications/System Settings.app"`; `pub fn scan(root: &Path, langs: &[String]) -> Vec<Item>` (pure over the filesystem: reads each `*.appex/Contents/Info.plist` and `Contents/Resources/InfoPlist.loctable` with `NSDictionary::dictionaryWithContentsOfFile`, XML or binary); `pub fn index() -> Vec<Item>` (real root, `NSLocale::preferredLanguages`); `pub fn bundle_id(id: &str) -> Option<&str>` (strips `pane:`).
- Pane item: `kind: Pane`, `id: "pane:<CFBundleIdentifier>"`, `path: Some(SETTINGS_APP)`, `name` from the first of `langs` found in the loctable (try `it_IT` then `it` for `it-IT`), English `altNames` entry when different; skipped without a name; kept only when `EXAppExtensionAttributes.EXExtensionPointIdentifier == "com.apple.Settings.extension.ui"` and `SettingsExtensionAttributes.allowsXAppleSystemPreferencesURLScheme == true`.

- [ ] **Step 1: Write failing tests** with XML plist fixtures under a temp dir: `keeps_settings_sections_with_url_scheme` (one valid bundle → one item `id == "pane:com.apple.settings.PrivacySecurity.extension"`, `name == "Privacy e sicurezza"`, `alt_names == ["Privacy & Security"]` for `langs = ["it-IT", "en"]`); `skips_other_extension_points`; `skips_sections_without_url_scheme`; `skips_sections_without_a_name`; `english_system_has_no_alt_name`. Plus `#[cfg(target_os = "macos")] real_scan_finds_privacy` (skipped when `ROOT` is missing) asserting an item with id `pane:com.apple.settings.PrivacySecurity.extension` exists.
- [ ] **Step 2: Run** `cargo test panes::` — FAIL.
- [ ] **Step 3: Implement** `scan`/`index`; scan runs where `refresh` already runs (off the main thread).
- [ ] **Step 4: Run** `cargo test` — PASS; print the real scan's count once in a test with `--nocapture` and note it in the PR (spec expects 52 on macOS 26).
- [ ] **Step 5: Commit** `feat: index System Settings sections`.

### Task 3: `run` and the palette rows

**Files:**
- Modify: `src-tauri/src/launch.rs` (add `run`; `launch` logic becomes its app branch)
- Modify: `src-tauri/src/lib.rs` (`MAIN_COMMANDS`: replace `"launch"` with `"run"`; handler list)
- Modify: `src/rank.ts` (`App` → `Item` type mirroring Rust; frecency and `search` keyed by `id`)
- Modify: `src/Palette.tsx` (`command()` → `["run", { id, reveal }]`; row key `id`; pane rows show a "System Settings" tag; icon `convertFileSrc(item.path ?? "", "icon")`)
- Modify: `src/Palette.test.tsx`, `src/rank.test.ts`, `site/scripts/screens/mock.js`

**Interfaces:**
- Consumes: `apps::item`, `panes::bundle_id`, `history::launched(app, id)`.
- Produces: Tauri command `run(id: String, reveal: bool) -> Result<(), String>`; TS `type Item = { kind: "app" | "pane" | "command"; id: string; name: string; altNames: string[]; path?: string; confirm?: string }`; `command(i, result, rows, reveal)` returns `["run", { id, reveal }]` for items.

- [ ] **Step 1: Write failing tests**
  - Rust `launch.rs`: `pane_url` — `pane_url("pane:com.apple.settings.Storage") == Some("x-apple.systempreferences:com.apple.settings.Storage")`, `pane_url("/Applications/X.app") == None`.
  - TS `Palette.test.tsx`: `items render keyed by id` (two items with the same `path`, ids `pane:a`/`pane:b`, both rows render with distinct `id`s and the tag "System Settings" appears twice); `command sends run with the id` (`command(0, null, [pane], true)` → `["run", { id: "pane:a", reveal: true }]`).
  - TS `rank.test.ts`: `frecency is keyed by id` (a pane with frecency ranks above an unused app with the same match tier).
  - Rust `lib.rs`: `allowed("main", "run") && !allowed("main", "launch") && !allowed("settings", "run")`.
- [ ] **Step 2: Run** `pnpm test` and `cargo test` — FAIL.
- [ ] **Step 3: Implement** `run`: look the id up with `apps::item` (unknown → `Err("Not something hop knows")`). App: existing launch path. Pane: ignore `reveal`, `open <pane_url>`, hide, record frecency under the id. Command: `Err` until Task 5.
- [ ] **Step 4: Run** the gate — PASS. Verify by hand in `pnpm tauri dev` only if available to you; otherwise list in the PR's manual checks: "type `privacy`, Enter → System Settings opens on Privacy & Security"; "type `storage` → opens General › Storage".
- [ ] **Step 5: Commit** `feat: open System Settings sections from the palette`.

### Task 4: PR 1 docs and release note

**Files:** `README.md`, `site/src/pages/usage.astro` (new "System Settings" section), `site/src/pages/index.astro` (card text if a card lists what hop finds), `.changeset/settings-sections.md` (minor: "Added System Settings sections to search: type a section's name, such as Privacy, to open it.").

- [ ] **Step 1:** Update docs; run the gate.
- [ ] **Step 2:** Commit, push, open the PR, watch CI, run the adversarial review (attack list: Review Focus 1 and 5, loctable language fallback, id collisions, URL scheme for sub-sections, `run` refusing unknown ids, mutation checks), fix, merge.

---

## PR 2: `feat: system commands` (branch `feat/system-commands`)

### Task 5: Command items, settings toggle, execution

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/settings.rs` (`system_commands: bool`, default `true`)
- Modify: `src-tauri/src/apps.rs` (`refresh` appends `commands::items()` when the toggle is on; `settings::set` with a changed toggle triggers `apps::reload`)
- Modify: `src-tauri/src/launch.rs` (`run` command branch)
- Modify: `src-tauri/Info.plist` (`NSAppleEventsUsageDescription`: "hop empties the Trash and asks macOS to restart, shut down or log out when you choose those commands.")
- Modify: `src-tauri/Cargo.toml` (`objc2-foundation` `NSAppleEventDescriptor` feature)

**Interfaces:**
- Produces: `pub fn items() -> Vec<Item>` with exactly:

  | id | name | altNames | confirm |
  |---|---|---|---|
  | `cmd:lock` | Lock Screen | lock | — |
  | `cmd:sleep` | Sleep | | — |
  | `cmd:restart` | Restart | reboot | Press Enter again to restart |
  | `cmd:shutdown` | Shut Down | shutdown, power off | Press Enter again to shut down |
  | `cmd:logout` | Log Out | logout, sign out | Press Enter again to log out |
  | `cmd:empty-trash` | Empty Trash | trash | Press Enter again to empty the Trash |

  `pub fn exec(app: &AppHandle, id: &str) -> Result<(), String>`. Lock: hide, then `SACLockScreenImmediate` via `dlopen("/System/Library/PrivateFrameworks/login.framework/login")` (`Err("Could not lock the screen")` if missing). Sleep: hide, then `/usr/bin/pmset sleepnow`. Restart/Shut Down/Log Out: Apple Event class `aevt` ids `rest` / `shut` / `rlgo` to bundle `com.apple.loginwindow`; Empty Trash: class `fndr` id `empt` to `com.apple.finder`; hide only after the send succeeds, else `Err` with the send error. Frecency recorded on success.

- [ ] **Step 1: Write failing tests:** `commands_table_is_exact` (ids, names, alt names and confirm strings above); `settings_default_turns_commands_on` and `old_settings_file_keeps_commands_on` (deserialize the 0.2.0 JSON without the key → `true`); `index_without_commands_when_off` (a pure `fn assemble(apps, panes, commands_on) -> Vec<Item>` used by `refresh`).
- [ ] **Step 2: Run** `cargo test` — FAIL.
- [ ] **Step 3: Implement**; keep the unsafe Objective-C calls in one `mac` module with `SAFETY` comments, like `panel.rs`.
- [ ] **Step 4: Run** the gate — PASS. Manual checks for the PR: Lock Screen locks; Sleep sleeps; Empty Trash prompts once for Finder, then empties; Restart with an unsaved TextEdit document shows TextEdit's save dialog and can be cancelled (do Log Out / Shut Down the same way). Record in the PR whether `loginwindow` events prompted for Automation.
- [ ] **Step 5: Commit** `feat: system commands in the index`.

### Task 6: Two-Enter confirmation in the palette

**Files:**
- Create: `src/confirm.ts`, `src/confirm.test.ts`
- Modify: `src/keys.ts` (`Actions.disarm()`: called on every keydown except Enter and bare modifier keys)
- Modify: `src/Palette.tsx` (`armed` state; reset on query change, selection change, `palette-hidden`, pointer down outside; `PaletteView` prop `armed?: string | null`)
- Modify: `src/keys.test.ts`, `src/Palette.test.tsx`

**Interfaces:**
- Produces: `export function press(armed: string | null, item: Item, plain: boolean): { armed: string | null; run: boolean }` — item without `confirm`: `{ armed: null, run: true }`; with `confirm`, `plain` and `armed === item.id`: `{ armed: null, run: true }`; with `confirm` otherwise: `{ armed: plain ? item.id : null, run: false }` (⌘Enter / ⌘N never confirm: they disarm).

- [ ] **Step 1: Write failing tests:** `confirm.test.ts` covering each branch above; `keys.test.ts` `any other key disarms` (`"a"`, `ArrowDown`, `Escape` call `disarm`; `Enter`, `Meta`, `Shift` do not); `Palette.test.tsx` `armed row shows the confirm copy` (row text "Press Enter again to restart", hint "↵ confirm", `aria-label` equal to the confirm copy, a `role="status"` element containing it) and `query change disarms` (pure reducer used by `onQuery` returns `armed: null`).
- [ ] **Step 2: Run** `pnpm test` — FAIL.
- [ ] **Step 3: Implement.** Command rows: inline SVG glyph in the icon box (`aria-hidden`), no image request.
- [ ] **Step 4: Run** the gate — PASS.
- [ ] **Step 5: Commit** `feat: commands that end the session ask for a second Enter`.

### Task 7: Settings switch and PR 2 docs

**Files:** `src/settings/Settings.tsx` (new "Search" group, switch "System commands" bound to `systemCommands`), `src/settings/Settings.test.tsx` (switch renders checked from `initial`), `README.md`, `site/src/pages/usage.astro` ("System commands" section: list, two-Enter rule, Automation prompt and re-prompt after updates), shortcut table, `.changeset/system-commands.md` (minor: "Added system commands to search: Lock Screen, Sleep, Restart, Shut Down, Log Out and Empty Trash; the last four ask for a second Enter.").

- [ ] **Step 1:** Failing Settings test; implement; gate.
- [ ] **Step 2:** Commit, PR, CI, adversarial review (Opus: native code, private API, Apple Events; attack list: Review Focus 2, every disarm path, ⌘Enter/⌘N on armed rows, toggle off removes commands and `run` refuses them, Apple Event failure keeps the palette open, Info.plist merge in the built bundle, mutation checks), fix, merge.

---

## PR 3: `feat: calculator history` (branch `feat/calc-history`)

### Task 8: Exponent literals

**Files:** `src/calc.ts`, `src/calc.test.ts`

- [ ] **Step 1: Failing rows:** `["1.84467440737e+19*1", "1.84467440737e+19"]`, `["100-1e+2", "0"]`, `["2e3+1", "2001"]`, `["1e-7*10", "0.000001"]`, `["1e5", null]` (bare literal), `["1e", null]`, `["1e+", null]`, `["e+5+1", null]`.
- [ ] **Step 2:** Run — FAIL. **Step 3:** extend the number token to `\d*\.?\d+(?:e[+-]?\d+)?` (case-insensitive `e`), keep the 200-char cap. **Step 4:** Run — PASS. **Step 5:** Commit `feat: the calculator reads exponent form`.

### Task 9: History store and the calculator switch

**Files:**
- Create: `src-tauri/src/calc_history.rs`
- Modify: `src-tauri/src/clipboard.rs` (`copy_text(text: String, expr: Option<String>)` records `{expr, result: text}` after a successful write when `expr` is `Some`)
- Modify: `src-tauri/src/settings.rs` (`calculator: bool`, default `true`), `src-tauri/src/panel.rs` (`Look.calculator`)
- Modify: `src-tauri/src/lib.rs` (`MAIN_COMMANDS += "calc_history"`)

**Interfaces:**
- Produces: `pub struct Entry { expr: String, result: String }` (serde); `pub const KEEP: usize = 20`; `pub fn record(list: &mut Vec<Entry>, e: Entry)` (removes an entry with the same `expr`, inserts at 0, truncates to `KEEP`); command `calc_history() -> Vec<Entry>`; file `calc_history.json` in the app data dir, missing or corrupt → empty; `expr` over 200 chars refused like `copy_text`'s cap.

- [ ] **Step 1: Failing tests:** `record_moves_a_repeat_to_the_top`, `record_keeps_twenty`, `corrupt_file_reads_empty`, `old_settings_file_keeps_calculator_on`, `look_carries_calculator`, guard `allowed("main", "calc_history") && !allowed("settings", "calc_history")`.
- [ ] **Step 2–4:** Run FAIL, implement, run PASS.
- [ ] **Step 5: Commit** `feat: keep the last twenty calculator results`.

### Task 10: History rows in the palette

**Files:** `src/Palette.tsx`, `src/Palette.test.tsx`, `src/keys.ts` (none if `open(reveal)`/`openAt` suffice)

**Interfaces:**
- Produces: `export type Lead = { kind: "result"; text: string } | { kind: "history"; expr: string; result: string }`; `export function leads(query: string, result: string | null, history: Entry[], on: boolean): Lead[]` — off → `[]`; query trimmed `=` → up to 20 history leads; `result` non-null → result lead then up to 5 history leads whose `expr` differs from the query; else `[]`. `rowAt(i, leads, rows)`, `total(rows, leads)`; `export function action(row, reveal: boolean, query: string): { run?: [string, Record<string, unknown>]; query?: string }` — result: `run: ["copy_text", { text, expr: query }]`; history: `reveal ? { query: query + result } : { query: expr }`; item: `run: ["run", { id, reveal }]`.
- Palette: on a `query` action set the query, keep the palette open, keep focus in the input (caret at end), selection 0. History is fetched with `calc_history` on `palette-shown` and after a copy.
- Hints: result "↵ copy"; history "↵ edit", and "⌘↵ insert" while ⌘ is held; ⌘N shows over the combined rows.

- [ ] **Step 1: Failing tests:** `leads` for each branch (incl. `=` with 25 entries → 20; live `2+2` with history containing `2+2` → not repeated); `action` for result / history plain / history ⌘ / item; Review Focus 4: `calc(action(historyRow("2^64","1.84467440737e+19"), true, "1+").query)` is not null; render: history row text `12*7+3 = 87` and hint "↵ edit"; `total` and `rowAt` with 1 result + 5 history + 3 items; footer hint matches the selected row kind.
- [ ] **Step 2–4:** Run FAIL, implement, run the gate PASS.
- [ ] **Step 5: Commit** `feat: calculator history in the palette`.

### Task 11: Calculator switch and PR 3 docs

**Files:** `src/settings/Settings.tsx` (switch "Calculator" in the "Search" group), Settings test, `README.md`, `site/src/pages/usage.astro` (Calculator section: history, `=`, Enter edits, ⌘Enter inserts), `.changeset/calc-history.md` (minor: "Added calculator history: your last results show under a sum, type = to see them all, Enter edits one and ⌘Enter inserts its result.").

- [ ] **Step 1:** Failing test, implement, gate.
- [ ] **Step 2:** Commit, PR, CI, adversarial review (Sonnet unless the diff grew large; attack list: lead ordering and ⌘N numbering, `=` edge cases, history write failure, recording only on copy, toggle off, Review Focus 3 and 4, mutation checks), fix, merge.

---

## PR 4: `docs: screenshots for sections, commands and history` (branch `docs/v3-screens`)

### Task 12: Screenshots and release

**Files:** `site/scripts/screens/mock.js` (pane, command and history fixtures), `site/scripts/screens/capture.mjs` (shots: `palette-settings` query `priv`, `palette-armed` query `restart` with one Enter, `palette-history` query `=`), `site/src/pages/usage.astro`, `site/src/pages/index.astro` (one card each for sections and commands, one sentence).

- [ ] **Step 1:** Capture with `PLAYWRIGHT_FROM=/Volumes/Dev/matteo/velo/node_modules/.pnpm/playwright-core@1.65.0/node_modules/playwright-core/package.json node site/scripts/screens/capture.mjs` against `pnpm dev`; view every image; total under 1.5 MB.
- [ ] **Step 2:** Gate, PR, CI, Sonnet review, merge.
- [ ] **Step 3:** Merge the Version Packages PR after its `workflow_dispatch` CI is green (owner approved releasing), watch `build-release.yml`, verify `latest.json` serves the new version and the site's download button follows.
- [ ] **Step 4:** Append a retrospective to `docs/v2-plan.md`'s Log (or a new `## Log` in this plan) and send the owner the end-of-step report.

## Log

Retrospectives, newest last. Lessons carried in from v2 (`docs/v2-plan.md`): gate before every commit; mutation checks in every review; list boundary classes in briefs; check new shortcuts against macOS ones; fix small Majors inline; narrow late review rounds.
