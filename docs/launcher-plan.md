# Launcher: build plan for Claude Code

Name: `hop` (replace everywhere; bundle id `dev.hop.app`). A minimal macOS app launcher: global hotkey, type a few letters, Enter, the app opens. Nothing else.

Reference project: `/Volumes/Dev/matteo/velo` (github.com/cef62/velo, same owner). **Read it before writing code.** Where this plan says "port from velo", copy the pattern and adapt it; do not reinvent it. Where velo's own comments explain a platform trap, keep the comment.

---

## 1. Goal and non-goals

**Goal.** Press `⌥Space`, a small centered palette appears, type to fuzzy-filter installed apps, Enter launches the selected one, Esc or click-away hides it. Starts hidden at login. Updates itself with a "What's new" dialog before install. Clean, neat, minimal.

**Non-goals for v1 (do not build, do not "prepare for"):** clipboard history, calculator, web search, file search, snippets, window management, plugins or extensions, AI, aliases/quicklinks, ⌘1–9 shortcuts, themes beyond system light/dark, Windows and Linux. A feature outside this plan needs the owner's OK first.

**Platform.** macOS, Apple silicon only. Put OS-specific code (app discovery, icons, panel window) behind one small module boundary so another OS can be added later, but do not write a trait hierarchy now.

## 2. Stack (decided)

| Layer | Choice | Why |
|---|---|---|
| Shell | Tauri 2, Rust backend | Owner's stack; velo's updater, autostart, release pipeline carry over |
| UI | React 19 + Vite, plain CSS using CSS variables | ~3 small screens; no component library |
| Controls | [Sakuga VFX](https://github.com/finktheartist/sakuga-vfx) | Owner's requirement, **Settings and What's new windows only** (see §6) |
| Search | Fuzzy matching in the frontend (port `src/fuzzy.ts` from velo) | A few hundred apps: no IPC per keystroke, no Rust matcher crate |
| Storage | One JSON file for settings, one JSON file for launch history (frecency) | No SQLite |
| Package manager, Node, TypeScript, Vitest, Changesets | Same versions as velo's `package.json` | Mirror velo |

Not used from velo: Panda CSS, Park UI, Ark UI, CodeMirror, markdown, age, keyring, rusqlite, rmcp, trash, the MCP agent server.

## 3. Mirror velo's repo, agent and git configuration

The new repo must feel identical to velo to work in. Do this in the first commit, before any feature code. Copy from `/Volumes/Dev/matteo/velo`, then adapt only what is marked.

### 3.1 Agent configuration

| velo file | Action |
|---|---|
| `AGENTS.md` | Copy. Keep the `caveman` and `ponytail` blocks and the `## Versioning` and `## Workflow` sections verbatim. **Rewrite `## Stack`**: React 19, Vite, plain CSS with `--sk-*` variables, Sakuga VFX controls loaded as classic scripts and never edited, no Panda/Park UI |
| `CLAUDE.md` | **New (velo has none).** Create it with the single line `@AGENTS.md` so Claude Code reads the same rules without a second copy. Verify Claude Code loads it; if the imported rules do not show up, inline them |
| `.claude/settings.json` | Copy verbatim: plugins `caveman@caveman` and `ponytail@ponytail`, with their `extraKnownMarketplaces` |
| `.claude/settings.local.json`, `.claude/worktrees/` | Do not copy. Add both to `.gitignore` |
| `.clinerules/caveman.md`, `.clinerules/ponytail.md` | Copy verbatim |
| `.cursor/rules/caveman.mdc`, `.cursor/rules/ponytail.mdc` | Copy verbatim |
| `.windsurf/rules/caveman.md`, `.windsurf/rules/ponytail.md` | Copy verbatim |
| `.opencode/AGENTS.md` | Copy verbatim |
| `.github/copilot-instructions.md` | Copy verbatim |

These are generated copies of the caveman and ponytail blocks. Whenever the blocks in `AGENTS.md` change, update every copy in the same commit.

### 3.2 Git and repo configuration

| velo file | Action |
|---|---|
| `CONTRIBUTING.md` | Copy and adapt. Keep: branch rules (never commit to `main`), failing test first, the **quality gate table**, PR format (`## Summary` / `## Test plan`), the **adversarial review** rules with the strike rule, the Releases section. Adapt the gate rows to this repo's real commands; drop the `docs:check` and `pnpm e2e` rows unless M7 adds them. Keep the Capabilities row |
| `.changeset/config.json`, `.changeset/README.md` | Copy verbatim |
| `scripts/sync-version.mjs` | Copy (mirrors `package.json` version into `src-tauri/tauri.conf.json`) |
| `scripts/release-notes.mjs` + `scripts/release-notes.test.ts` | Copy (builds the release notes from the CHANGELOG section; the What's new window reads them) |
| `package.json` scripts | Copy the `changeset`, `version-packages`, `release:tag`, `tauri`, `build:app` entries. `build:app` reads the signing key from `~/.tauri/hop.key` and `.password` |
| `.github/workflows/ci.yml` | Copy; matrix reduced to `macos-latest`. Keep the pinned action SHAs and their version comments |
| `.github/workflows/release.yml` | Copy verbatim (changesets action opens the "Version Packages" PR, then tags, then dispatches the build) |
| `.github/workflows/build-release.yml` | Copy; matrix reduced to macOS arm64 (`aarch64-apple-darwin`). In the `publish` job the `jq` check becomes `.platforms \| has("darwin-aarch64")`. Keep the draft-release flow |
| `.gitignore`, `.gitattributes` | Copy, drop entries for things this repo lacks (Panda `styled-system`, `site/`, `dist-demo/`, `spikes/`, `fixtures/`). Add `.claude/settings.local.json` and `.claude/worktrees/` |
| `pnpm-workspace.yaml`, `tsconfig.json`, `vitest.config.ts`, `vite.config.ts` | Copy and adapt: remove Panda aliases; Vite `input` lists three pages: `index.html` (launcher), `settings.html`, `whatsnew.html` |
| `LICENSE` | MIT, same holder |
| `src-tauri/Cargo.toml` | Copy the edition, `[profile.release]` block and the plugin versions you keep; do not copy dependencies for removed features |
| `src-tauri/tauri.conf.json` | Copy the structure: `createUpdaterArtifacts`, `macOS.signingIdentity: "-"`, `requireSignedVersion`, the CSP. Change identifier, product name and updater endpoint |

**Repo and GitHub setup (owner does this, agent lists it in the first PR):**
- New GitHub repo, `main` protected by a ruleset requiring an approving review (the solo-maintainer `--admin` squash-merge rule in `AGENTS.md` applies unchanged).
- Release immutability on, as in velo.
- **New** minisign keypair for this app (`pnpm tauri signer generate`). Public key into `tauri.conf.json`; private key and password into the repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Never reuse velo's key, never commit the private key, never lose it: installed apps accept only updates signed with it.

## 4. Architecture

### 4.1 Windows

| Label | Page | Purpose | Notes |
|---|---|---|---|
| `main` | `index.html` | The palette | Created at startup, **hidden, never destroyed**: show/hide only, so it appears instantly. Plain DOM, no Sakuga |
| `settings` | `settings.html` | Settings | Normal titled window, opened on request, closed (destroyed) when dismissed. Loads Sakuga |
| `whatsnew` | `whatsnew.html` | Release notes and install | Port velo's `whatsnew.rs` + `src/whatsnew/` + `capabilities/whatsnew.json`, including the `allowed()` command guard. Loads Sakuga |

Each window gets its own capability file listing exactly what it uses. Custom commands are guarded per window label in Rust (`allowed`) the way velo does it, because capabilities do not restrict custom commands.

### 4.2 Rust modules

`apps.rs` (discovery + watcher), `icons.rs` (icon extraction + cache), `launch.rs`, `panel.rs` (window behavior), `hotkey.rs`, `settings.rs`, `history.rs` (frecency), `update.rs` and `whatsnew.rs` (ported), `log.rs` (ported). Keep each small.

### 4.3 App discovery (`apps.rs`)

- Roots: `/Applications`, `/System/Applications`, `/System/Applications/Utilities`, `/Applications/Utilities`, `~/Applications`, plus user-added folders from Settings. Walk at most 2 levels deep so vendor folders (`/Applications/Adobe X/`) work. Never descend into a `.app`.
- Display name: use the **localized name the Finder shows** (`NSFileManager displayNameAtPath:`). The owner's system is English with an Italian keyboard; on an Italian system "Impostazioni di Sistema" must match, and so must the English file name ("System Settings") as a secondary search key. Strip the `.app` suffix.
- Watch the roots with `notify` (debounced) and push a refreshed list to the frontend when apps are installed or removed. No periodic rescan.
- Hold the index in memory. A full scan of a few hundred apps is fast; if a cold scan on the owner's machine exceeds 150 ms, cache the list to disk and refresh in the background.
- Ignore list in Settings (paths) for apps the owner never wants to see (helpers, uninstallers).

### 4.4 Icons (`icons.rs`)

Extract through `NSWorkspace iconForFile:` (via `objc2` crates), render to a small PNG (64 px, so it is sharp at 2x), cache on disk keyed by bundle path + modification time. Serve to the webview through the asset protocol or a custom URI scheme, **not** through per-keystroke IPC of image bytes. Extract lazily and in the background; the list renders with a neutral placeholder and swaps the icon in when ready.

### 4.5 Search and ranking (frontend)

- Rust sends `[{ id, name, altNames[], path, iconUrl }]` once at startup and again when the index changes.
- Port velo's `src/fuzzy.ts` and its tests. Matching handles prefixes, word-initial abbreviations ("vsc" → Visual Studio Code) and is diacritic-insensitive.
- Frecency: a launch count with exponential decay, stored per app path in `history.json` (app data dir, written on launch, debounced). The final score is match score plus a small frecency boost, so a perfect prefix match still beats a frequent partial match. Local only, no setting, no telemetry.
- Empty query: show up to 6 most recent/frequent apps. Nothing if there is no history yet.

### 4.6 Launching (`launch.rs`)

Run `open <path>` (running app: it is brought to front; not running: it starts). Do not hand-roll `NSWorkspace` unless `open` proves inadequate. After the command is spawned, hide the palette. `⌘Enter` reveals the app in Finder (`opener:allow-reveal-item-in-dir`, already used in velo).

### 4.7 Palette window behavior (`panel.rs`), the riskiest part

Requirements, all to be proven in the M1 spike on the owner's Mac:
1. Appears over **full-screen apps and on any Space**, without switching Spaces.
2. Takes keyboard focus on show and returns focus to the previously active app on hide (Esc, hotkey again, blur, or launch).
3. Does not appear in the Dock, the app switcher or Mission Control. Use the Accessory activation policy and no Dock icon.
4. Transparent, borderless, rounded, with native vibrancy; fixed size with the content area positioned inside (never resize the window per keystroke, since resizing flickers). Needs `macOSPrivateApi: true` in `tauri.conf.json`.
5. Opens centered horizontally, at about one third of the height, **on the screen under the mouse cursor**. Do not use the window-state plugin for this window; always reposition on show.
6. Hide on blur and on Esc. Clear the query on hide.

Implementation options to evaluate in the spike, in order: Tauri's own window API + Accessory policy + collection behavior set through `objc2`; a community NSPanel plugin such as `tauri-nspanel` if the plain window cannot meet 1 and 2. Report which one worked and why; keep the code behind `panel.rs`.

### 4.8 Hotkey (`hotkey.rs`)

- Default `⌥Space`, registered with `tauri-plugin-global-shortcut`. Pressing it while the palette is visible hides it.
- Port velo's registration/fallback logic: if registration fails, the menu-bar icon is still there and Settings shows the conflict instead of silently rejecting the binding.
- **Verify in M1** that `⌥Space` is consumed and does not type a non-breaking space into the frontmost app. Spotlight, Raycast and Alfred default to the same chord, so Settings must show a clear message when it is taken. M1 finding: macOS does not report a chord held by another app (Raycast on `⌥Space`: hop registers fine, both apps react), so registration errors only cover chords the OS refuses. Owner decision (2026-11-22): Settings always shows a hint under the recorder: "If another app (Raycast, Alfred) uses this shortcut, both will respond; change one of them." No detection of other apps' settings.
- The shortcut recorder in Settings is a port of velo's (Backspace clears, conflicts displayed).

### 4.9 Menu-bar icon, login, single instance

- Menu-bar icon (tray), **always shown, not a setting** (the owner decided; it is the way to reach Settings and Quit when the hotkey is taken). Menu: Open, Settings…, Check for updates…, Quit. Do not port velo's "hotkey only" mode or its tray-fallback logic.
- Autostart: `tauri-plugin-autostart` (macOS LaunchAgent) with the `--hidden` argument, exactly as velo. **On by default** (owner asked for start at login); applied on first launch, visible and reversible in Settings. Port `refresh_login_item` and the cases where velo refuses (App Translocation, dev builds).
- Single instance on macOS: no `single-instance` plugin (velo explains why). Port `RunEvent::Reopen` handling and `hold_instance_lock`, so a login-item launch beside a running copy exits quietly, and relaunching the app shows the palette.
- Dev build and installed build share an identifier and lock on macOS; note in `CONTRIBUTING.md` that the installed app must be quit before `pnpm tauri dev`.

### 4.10 Palette styles

Two styles ship in v1, switchable in Settings → Appearance (Sakuga Tabs or Radio), default **Compact**. They share all logic (search, selection, keys, footer content) and differ only in CSS variables and one footer rule, so keep them as one component with a `data-style="compact|roomy"` attribute on the root, not two components.

| | Compact (default, "A") | Roomy ("B") |
|---|---|---|
| Palette width | 560 px | 600 px |
| Corner radius | 12 px | 16 px |
| Search row | 46 px high, 16 px text, 18 px search icon | 64 px high, 22 px text, 22 px search icon |
| Result rows | 36 px high, up to 5 rows | 54 px high, up to 4 rows |
| App icon | 28 px, 7 px radius | 40 px, 10 px radius |
| Row text | 14 px | 16 px |
| Selected row | neutral fill, 8 px radius, right-aligned `↵ open` hint | neutral fill, 12 px radius, right-aligned `↵` hint |
| Footer | none, unless an update is ready: then a single thin line with the update pill | always: key hints (`↵ open · ⌘↵ reveal in Finder · esc close`) left, update pill right, 38 px high |

The table is the spec. Flat fill plus native vibrancy; no shadows beyond the system window shadow. Placeholder look in the owner-approved mockups: query `fi`, first row selected, each icon a colored rounded square.

Implementation constraints:
- The palette window is a **fixed-size transparent window** sized for the tallest style (Roomy, with footer); the visible card is positioned inside it and sized by CSS. Switching style never resizes the window per keystroke. Changing the setting while the palette is hidden may adjust the card only; no window resize is needed.
- Row and icon sizes live in CSS variables so a future third style costs one block of variables.
- The setting applies the next time the palette opens (it is hidden whenever Settings is in front); no restart.
- Visible-row count differs per style; the list scrolls inside the card beyond that. Selection scrolls into view.
- Tests: ranking and selection are style-independent (one Vitest suite); add a small render test that both styles produce the same rows and the same key behavior.

## 5. Settings (keep to these)

General: global hotkey (recorder) · start at login · extra folders to index · ignored apps.
Appearance: theme (System / Light / Dark) · palette style (Compact / Roomy, default Compact; see §4.10).
Updates: check for updates now · daily check on/off · current version.

Nothing else. Settings apply live where possible; the file lives in the app config dir. Reuse velo's `settings.rs` shape (versioned, tolerant of unknown keys, atomic write).

## 6. Sakuga VFX rules

Facts from the library's docs (re-read `https://sakuga-vfx.vercel.app/llms.txt` before use):
- Each control is one plain script that registers a custom element. Load with a normal `<script>` tag, **not as a module**. Never edit, split or minify the files; they are self-contained. Vendor them into `public/sakuga-vfx/` (the shadcn CLI installs there: `npx shadcn add finktheartist/sakuga-vfx/NAME`) and pin the commit in a `VENDOR.md`.
- All controls draw on **one shared full-page canvas** with one animation loop. The toggle file is about 500 KB because it embeds animation frames.
- Value controls (toggle, slider, checkbox, radio, input, rating) work in forms and fire `change`/`input`. Button: `click`, then `el.done()`. Progress: `el.set(0..1)` and the `sakuga:done` event. Toast: `el.show(text)`. Colors come from `--sk-*` CSS variables, overridable in `:root`. Effects respect reduced motion.
- Dark/light behavior, React/TypeScript typing and performance are **not documented**. Treat them as unknown.

Rules for this app:
1. **The palette window uses no Sakuga.** Plain `<input>` and a plain list. Reason: first-show latency, idle CPU from the animation loop, typing/IME/focus behavior of a custom-element input.
2. Sakuga loads only in `settings.html` and `whatsnew.html`.
3. Map: Toggle → start at login, daily check. Tabs → Settings sections. Button → check now, install, close. Slider → none needed. Input → folder paths. Progress → update download. Toast → "Saved", "Hotkey taken". Tooltip → hotkey conflict explanation.
4. Add a thin typed React wrapper per control used (props to attributes, `change` event to callback, JSX typings in a `sakuga.d.ts`). Wrappers never touch Sakuga's files.
5. Define the `--sk-*` variables from this app's own light and dark palette in `:root`, switching with `prefers-color-scheme` (and the Settings theme override). Test both.
6. **M4 acceptance includes measurements**: idle CPU of the Settings window with a toggle on screen, and its first paint time. If the shared canvas keeps burning CPU while idle, the wrapper must pause it when nothing animates, or the window must fall back to plain controls and the owner is told.

## 7. Updates and release

Port, do not redesign:
- `update.rs`: first check 30 s after launch, then daily (toggle in Settings), manual check from Settings and the menu-bar icon. Updates verified against the minisign public key; `requireSignedVersion`.
- **What's new window** (`whatsnew.rs`, `src/whatsnew/`): this is the "new features install dialog". It shows the release notes of the pending version (rendered without HTML, as velo does), then "Install and restart" with a progress bar and failure/retry states. Restyle it with Sakuga Button and Progress.
- Surfacing: **never interrupt the launcher.** When an update is ready, show a small pill in the palette footer (and a menu-bar item). Clicking it opens the What's new window. Nothing pops up by itself.
- After an update restart, the app starts hidden (port `take_restart_mark`).
- Releases: Changesets → "Version Packages" PR → tag `vX.Y.Z` → `build-release.yml` builds macOS arm64 into a draft release → `publish` checks `latest.json` lists `darwin-aarch64` → publishes. Builds are ad-hoc signed (`signingIdentity: "-"`, as velo); the README documents the Gatekeeper workaround for the first install. No paid Apple Developer ID and no notarization (owner's decision); do not add either.
- Every user-visible change ships with a changeset (see `AGENTS.md`).

## 8. Performance and quality budgets

Measure each, record results in the PR, and fail the milestone if missed (on the owner's Apple silicon Mac, release build):

| Budget | Target |
|---|---|
| Hotkey press → palette visible and typeable | p95 under 100 ms |
| Keystroke → filtered list painted | p95 under 16 ms with the owner's real app list |
| Resident memory, palette hidden, idle | report it; target under 120 MB |
| CPU, palette hidden, idle | about 0% (no timers, no animation loop in the palette webview) |
| Cold start to hotkey ready | under 1 s |
| Release bundle size | report it |

Add a release-only test for the search budget, as velo does for its index (`src-tauri/tests/perf.rs`, ignored by default). Frontend search gets Vitest tests (`fuzzy.test.ts` ported, plus ranking/frecency tests). Rust gets inline `#[cfg(test)]` tests for display-name resolution, root walking (use `tempfile` fixture trees with nested `.app` dirs and symlinks), settings read/write, history decay.

## 9. Milestones

Every milestone is its own feature branch and PR, runs the quality gate, has a changeset when user-visible, and gets an adversarial review (attack lists below).

**M0, Scaffold and mirror.** New Tauri 2 + React + Vite app; all of §3 in place; CI green on an empty app; `pnpm tauri dev` shows a blank hidden window. *Done when* the CI, changeset, release and agent-config files match §3 and the first release PR flow works end to end on a `0.1.0` tag (can be done with a throwaway release).

**M1, Spike: panel + hotkey.** *(Highest risk; do it before anything else.)* Empty transparent palette, `⌥Space` toggles it, Esc/blur hide it. *Done when* §4.7 items 1–6 and the §4.8 NBSP check all pass on the owner's Mac, including over a full-screen app and on a second display, and a short `docs/spikes/panel.md` records what worked. *Attack list:* full-screen Space, second monitor with different scale, Stage Manager, hotkey while the palette is already visible, app that grabs `⌥Space` first, focus return to the previous app, rapid double press.

**M2, Index, search, launch, both palette styles.** §4.3–4.6 and §4.10 (the Settings switch itself lands in M3; until then the style is read from the settings file). *Done when* typing finds apps by name, abbreviation and localized name, Enter launches, ⌘Enter reveals, new/removed apps appear without restart, empty query shows recents, and the §8 budgets for show latency and keystroke latency are met. *Attack list:* app with no icon, symlinked app, app inside a vendor folder, app removed while selected, thousands of folders in a watched root, Italian vs English names, launching an app that fails to open.

**M3, Settings plumbing, tray, login.** `settings.rs`, menu-bar icon, autostart, single-instance behavior, hotkey recorder and conflict reporting, extra folders and ignore list (plain controls for now). *Attack list:* hotkey taken at launch, login item beside a running copy, App Translocation, dev build collision, corrupt settings file.

**M4, Sakuga Settings window.** §6. Replace plain controls with Sakuga wrappers; both color schemes; §6 rule 6 measurements.

**M5, Updates and What's new.** §7 port, restyled; footer pill; menu-bar item. Test against a local mock feed and a real signed test release. *Attack list:* offline, half-open socket, update found while the palette is open, install failure and retry, a second check while one is running, tampered `latest.json`.

**M6, Release pipeline for real.** Secrets set, first public `v0.1.0`, install on the owner's Mac from the release, then ship a `v0.1.1` and confirm in-app update through the What's new dialog.

**M7, Polish.** Visual pass against the minimal brief, accessibility (VoiceOver labels, focus order, reduced motion), README with install and Gatekeeper steps, memory/CPU report. Decide with the owner whether to add `docs:check` and e2e (Playwright, as velo has) or leave them out.

## 10. Open items for the owner

None blocking. Decided, not open: app name `hop`; macOS only; no Apple Developer ID; menu-bar icon always on; **both palette styles ship (Compact default, Roomy), switchable in Settings** (§4.10).

## 11. How the agent works

Follow `AGENTS.md` and `CONTRIBUTING.md` (copied in M0): feature branch, failing test first, quality gate reported as a table, conventional-commit PR title, `## Summary` and `## Test plan`, changeset for user-visible changes, adversarial review by a fresh agent with an attack list, strike rule, squash-merge only on the owner's word. Ponytail ladder applies: if something above can be skipped or done with less code, say so rather than building it.

**Start with:** read `AGENTS.md`, `CONTRIBUTING.md`, `src-tauri/src/{lib,update,whatsnew,settings,commands}.rs` and `src-tauri/tauri.conf.json` in the velo clone, then do M0 and M1 and report before starting M2.
