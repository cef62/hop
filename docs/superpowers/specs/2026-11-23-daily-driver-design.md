# hop as a daily driver: System Settings sections, system commands, calculator history

Status: draft for the owner's approval (2026-11-23). Builder check done; the owner answered the signing question.

## Intent

The owner wants hop to be their daily driver, replacing Spotlight and Raycast for three more jobs:

1. Open a System Settings section by typing its name ("privacy" opens Privacy & Security).
2. Run basic system commands: lock screen, sleep, restart, shut down, log out, empty Trash.
3. See and reuse recent calculator results.

Everything is found by plain typing in the same palette, ranked with the apps. It must stay as fast and keyboard-only as launching an app is today, and it must never do something destructive on one stray keystroke.

## Goals

- **System Settings sections.** Every section System Settings lets an app link to (50 on macOS 27.2, not only the sidebar ones) is a palette item, matched by its name in each of the user's preferred languages and in English. Enter opens System Settings on that section.
- **System commands.** Lock Screen, Sleep, Restart, Shut Down, Log Out, Empty Trash, matched by their English names and aliases ("reboot", "shutdown", "power off", "logout", "sign out"). Lock Screen and Sleep run on Enter. Restart, Shut Down, Log Out and Empty Trash ask for a second Enter in the palette.
- **Calculator history.** The last 20 copied results are kept across restarts. While the query is a sum, the live result row is followed by the 5 most recent history rows. The query `=` alone lists all 20.
  - Enter (or ⌘1–9) on a history row replaces the query with that row's expression.
  - ⌘Enter on a history row appends that row's result to the end of the query.
  - Enter on the live result copies it (as today) and records it in the history.
- **Settings toggles.** Settings gets two switches, both on by default: "System commands" and "Calculator". Off removes the commands from search, or turns off the result row and the history rows.
- One ranking for everything: apps, sections and commands share the fuzzy match and the frecency boost, so a section or command used often rises like an app.

## Non-goals

- Do Not Disturb or Focus toggles (no public API; dropped by the owner for now).
- Other commands (eject disks, screen saver, restart into recovery, …).
- Localized command names (English only for now, by the owner's decision).
- Prefixes or modes (`>restart`): everything is plain typing.
- A "clear calculator history" control; showing history anywhere but the palette.
- Sub-pages inside a section (e.g. "Privacy > Camera").

## Anti-goals (a failure even if it works)

- A single Enter that restarts, shuts down, logs out or empties the Trash.
- A slower palette: the show and keystroke budgets in `docs/perf-report.md` still hold.
- Permission prompts at install or first launch. A prompt may appear only on the first use of a command that needs it. hop is ad-hoc signed, so macOS may ask again after each update; the owner accepted that (2026-11-23) over a Developer ID. README and the Usage page say so.
- Undocumented behavior that breaks silently: where a private API is used, failure is reported in the palette, not swallowed.

## Approaches considered

1. **One combined item list from Rust (picked by the owner).** The app index becomes an item index: apps, sections, commands, each with a kind, an id and names. The frontend ranks one list; Enter goes to one Rust command that dispatches on the kind. Frecency is kept per id. Reuses search, ranking, ⌘1–9 and the window guard; generalizes the index and `launch`, so it touches the most existing code.
2. **Separate lists merged in the palette.** Smallest change, but ranking and frecency would be duplicated per kind, and each new kind adds a branch. Not picked.
3. **Prefixes.** Rejected: the owner wants plain typing.

The calculator history is frontend state plus a small Rust store, independent of this choice.

## Design

### Items

- `apps::App` becomes an item: `{ kind: "app" | "pane" | "command", id, name, altNames, path? }`.
  - `app`: `id` is the bundle path, so existing `history.json` entries keep working.
  - `pane`: `id` is `pane:<CFBundleIdentifier>` of the settings extension.
  - `command`: `id` is `cmd:<name>` (`cmd:lock`, `cmd:sleep`, `cmd:restart`, `cmd:shutdown`, `cmd:logout`, `cmd:empty-trash`).
- The index keeps today's generation and `apps-changed` event (renaming them is the builder's call). Ignored apps still apply to apps only.
- With "System commands" off, command items are left out of the index, so `run` refuses them too.

### System Settings sections

- Scan `/System/Library/ExtensionKit/Extensions/*.appex`. Keep a bundle when its `Info.plist` has `EXAppExtensionAttributes.EXExtensionPointIdentifier == "com.apple.Settings.extension.ui"` and `EXAppExtensionAttributes.SettingsExtensionAttributes.allowsXAppleSystemPreferencesURLScheme == true` (built: the attributes are nested, not top-level).
- Names come from `Contents/Resources/InfoPlist.loctable` (a binary plist keyed by language), key `CFBundleDisplayName`. Built: per preferred language, then English, the lookup tries the exact region (`it-IT` → `it_IT`), the region the loctables use for a script or Latin America (`zh-Hant` → `zh_TW`/`zh_HK`, `es-MX` → `es_419`; `zh-Hans` reaches `zh_CN` as the first Chinese region), the bare language, then any region of it (`en_AU` when there is no `en`). The first name found is `name`; the other distinct names go in `altNames`. A name equal to a `CFBundleExecutable` that ends in `Extension` (Headphones) is no name.
- A section without a bundle name uses its sidebar rows' names: `SettingsExtensionAttributes.representations[].sidebar-name`, resolved in `Localizable.loctable` (Battery: "Battery", "Energy"; the `powersource.battery == YES` row is shown, its predicate not evaluated). A section with no name at all is skipped (Headphones on macOS 27.2; Home lacks the URL-scheme flag).
- Open with `open "x-apple.systempreferences:<CFBundleIdentifier>"`. The builder verifies the URL form on macOS 26 for a few sections, including one under General (Storage).
- Scanned at startup, off the main thread. macOS updates restart the Mac, so no watcher is needed.
- Icon: the System Settings app icon through the existing icon protocol, with a "System Settings" tag on the row.

### System commands

| Command | Aliases | Confirm | How |
|---|---|---|---|
| Lock Screen | lock | no | `SACLockScreenImmediate` from `/System/Library/PrivateFrameworks/login.framework` (dlopen at use; private, stable since macOS 10.13). On failure, the palette shows an error. |
| Sleep | | no | `pmset sleepnow`, or `IOPMSleepSystem` if `pmset` needs more than a user (builder's call). |
| Restart | reboot | yes | Apple Event to `loginwindow`. Apps can still cancel for unsaved work. |
| Shut Down | shutdown, power off | yes | Apple Event to `loginwindow`. |
| Log Out | logout, sign out | yes | Apple Event to `loginwindow`, the form without the macOS dialog (the palette already confirmed). |
| Empty Trash | trash | yes | Apple Event to Finder (`empty trash`). The first use triggers the macOS Automation prompt for Finder: add `NSAppleEventsUsageDescription` to the bundle. |

- The builder checks whether the `loginwindow` events also trigger an Automation prompt. If they do, the usage description covers them, and the PR notes the one-time prompt.
- Built: tauri signs the bundle with the hardened runtime (`flags=0x10002(adhoc,runtime)` on 0.2.0), which refuses every Apple Event without the `com.apple.security.automation.apple-events` entitlement, so the bundle gets `src-tauri/Entitlements.plist`. Events go out without waiting for a reply (Empty Trash on a large Trash replies only when done); a refused send (-1743) names Privacy & Security › Automation in the error, a missing target (-600) says the app is not running. `run` is an async command, so a first-use Automation prompt never blocks the main thread; the palette hides on the main thread.
- Icons: small inline SVG glyphs in the palette's style, `aria-hidden`.
- The commands do not record frecency for the confirm step, only when they run.

### Two-Enter confirmation

- Frontend state `armed: id | null`. Enter (or ⌘N) on a row whose command needs confirmation arms it. The row then reads "Press Enter again to restart" (per command), and the hint reads "↵ confirm".
- Enter on the armed row runs it. Any other key, a query change, a selection change, a click elsewhere, or hiding the palette disarms it.
- The transitions are a pure function (like `paletteKey`) with a unit test. Disarming on hide goes through the existing `palette-hidden` reset.
- Built: ⌘Enter, ⌘N and a click arm a command (and select its row) but never confirm; a held Enter's auto-repeat never confirms either (holding Enter would otherwise restart). A command is armed only while it is the selected row: a list that changes under it (an app installed, the switch turned off) disarms it. All of it is one pure reducer, `step` in `src/confirm.ts`. Command rows' hint and footer read "↵ run".
- Built: commands never join the empty-query list of most used items, however often used: an empty palette carries no intent, and one habitual Enter there must not lock or sleep the Mac.
- Accessibility: the armed row's accessible name changes to the confirm prompt, and a polite live region announces it.

### Calculator history

- Rust store `calc_history.json` in the app data dir: up to 20 `{ expr, result }`, newest first. Recording an expression already present moves it to the top. A missing or corrupt file starts empty (as `history.json` does).
- `copy_text` gains the expression (or a separate `calc_record` command; builder's call) so a copy records it. Only copies record.
- Rows:
  - Query is a sum (`calc` not null): the live result row, then up to 5 history rows.
  - Query is exactly `=`: up to 20 history rows, no live row.
  - History rows show `expr = result`, with the hint "↵ edit" (and "⌘↵ insert" while ⌘ is held).
- Enter / ⌘N on a history row sets the query to its expression, keeps the palette open, and puts the caret at the end. ⌘Enter appends its result to the query.
- `calc` accepts exponent literals (`1.2e+21`), so any shown result can be fed back in. A bare literal still gives no row (no operator).
- With "Calculator" off: no live row, no history rows, `=` is a plain search.
- Built: one reducer decides everything (`step` in `src/confirm.ts`, rows from `src/leads.ts`). Enter, ⌘N and a click on a history row replace the query with its expression; ⌘Enter appends its result; each puts the selection on the first row, disarms any armed command and keeps the palette open with the focus and caret in the input. A held Enter's repeats do nothing on a result or history row (the first press may have changed the query, and the repeat would copy its live row). ⌘Enter on the live result copies it, as Enter does.
- Built: `=` is matched after trimming (` = ` lists the history). A history row whose expression equals the query (trimmed) is not shown under the live row, and the next entry takes its place, so five show.
- Built, the ⌘Enter rule: the result is appended as it is, to the query as typed. After an operator (`1+`, `12*`) it completes the sum; after a digit (`12`) the digits run together (`125`) and that is all: no guessing what the user meant. Exponent results parse when typed back (`1+1.84467440737e+19`).
- Built: `calc` accepts `1.2e+21`, `1E3`, `.5e2`; not `5.e2` (`5.` is no number, as before), `1e`, `1e+`, `e+5`. `1e400` is Infinity and gives no row; `1e-400*1` is `0`. A result fed back through `calc(result + "*1")` gives itself, with one exception: a non-integer from 1e11 up rounds to an integer shown in exponent form (`1.23456789013e+11`), and typed back it is a safe integer shown in full (`123456789013`): same number, other digits.
- Built: the copy is recorded by `copy_text(text, expr)` after a successful write only; a failed copy records nothing, and a failed file write is logged, not returned. An `expr` that is empty or over 200 characters is not recorded (the copy still works). The palette reads the history on `palette-shown`; no refetch after a copy, because a copy hides the palette.
- Built: the "Calculator" switch rides `palette-shown` / `palette-hidden` in `Look.calculator`; it needs no index reload.

### Settings

- `Settings` gains `system_commands: bool` and `calculator: bool`, both default `true` (older settings files without the keys read as `true`).
- The Settings window gets a "Search" group with the two switches, in the existing native style. Changes apply at once: commands toggle triggers an index reload; calculator toggle reaches the palette with the next `palette-shown` look.

### Window guard

New or changed commands go in `MAIN_COMMANDS` only (`run`, the calculator history read and record). Settings keeps `get_settings`/`set_settings` for the toggles.

## Delivery (one PR each, same workflow as v2)

1. Items + System Settings sections (generalize index and `launch` into `run`, frecency per id).
2. System commands + two-Enter confirm + "System commands" toggle.
3. Calculator history + exponent literals + "Calculator" toggle.
4. Docs site screenshots and a home card refresh.

Each PR keeps README, the site's Usage and home pages and the tests current. Release after PR 3 or 4 (the owner has approved releasing when PRs are green).

## Calls made after the builder check

- ⌘Enter on a section or command acts like Enter; `reveal` stays an app-only path. On an armed row only a plain Enter confirms; ⌘Enter arms, like ⌘N, and never confirms (built in round 1: on an armed row it keeps it armed).
- Row order: live result, history rows, then items. ⌘1–9 number rows in that order (generalize `rowAt`/`total` from one leading result row to a leading block). A history row equal to the live row is not shown. On `=` the first history row is selected.
- macOS 13 or later for sections (ExtensionKit). On older systems the scan finds nothing and sections are absent; no `.prefPane` fallback.
- Command execution: Lock Screen and Sleep hide the palette, then run. Restart, Shut Down, Log Out and Empty Trash send their Apple Event, hide on success, and keep the palette open with the error if sending fails. Apps that later cancel a restart for unsaved work are macOS's business, not an error.
- Sections are always searchable; the "System commands" switch covers commands only.
- Commands rank like any item, so "re" may put Restart first; the two-Enter confirm is the safety net. No rank tuning.

## Left to the builder

- Exact Apple Event codes and sending API (`NSAppleEventDescriptor` with objc2 is preferred over `osascript`).
- `pmset` versus IOKit for Sleep.
- Index and event names after the generalization.
- Where the "System Settings" tag and the command glyphs sit in the row layout, within the current row design.
