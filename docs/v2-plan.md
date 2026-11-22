# hop v2 plan: small gaps, ⌘1–9, calculator

Approved by the owner on 2026-11-22. Extends `docs/launcher-plan.md`; everything else in its non-goals list still needs the owner's OK.

## Rules for every PR

- Feature branch and worktree, failing test first for logic, quality gate table, changeset for user-visible changes.
- Tests and documentation stay current with every change: README, the site's home and Usage pages, this plan.
- Adversarial review with an attack list written for the change (Sonnet for small PRs, Opus for the calculator); each round posted on the PR.
- Merge (`gh pr merge <n> --squash --admin`) once CI is green and the review passed. No release without the owner's OK.
- After each merge: retrospective in the Log below, then adjust the remaining PRs.
- Stop and ask the owner on: a strike-rule failure, red CI that does not clear, a design choice, anything that needs the owner's Mac.

## PR 1: `fix: small gaps`

1. Settings render test (Vitest, mocked `invoke`): folder and app lists have `role="list"`, the broken-hotkey message `role="alert"`, the hotkey button's accessible name, the "General" heading is focusable. Not covered (needs a DOM, no new dependency): the recorder and error alerts as rendered, focus moving to "General" after load (manual check).
2. Docs site: `site/src/pages/404.astro` in the site's style with a link home.
3. Settings comes to the front after a system activation: find the cause in `panel.rs` (`hide_for_window`, `give_back`). Fix with a test of the decision logic if the cause is clear; otherwise record the findings and change nothing. Timeboxed.
4. This plan; `docs/launcher-plan.md` notes the approved v2 items.

### Findings: Settings comes forward (item 3, no code change)

The cause is known; a safe fix needs a signal only a test on the owner's Mac can confirm. Already logged as Minor 3 ("accepted as a known limit") in the [PR #6 review](https://github.com/cef62/hop/pull/6#issuecomment-7078741673).

Mechanism: macOS reactivates the app that was active before a system UI took over, and activating hop orders its main window, Settings, forward. The palette is borderless, so it never becomes the main window.

Paths checked:

- **Blur hide while the system takes activation** (Spotlight, lock screen, a system alert, Notification Center). The palette loses the focus, `focus_changed` hides it with `blur = true` (`panel.rs:197`), and `give_back(true, Some(pid), true)` returns `Stay` (`panel.rs:128`): nothing is activated. hop was the active app, so macOS gives activation back to hop when the system UI closes, and Settings comes forward over the app the palette came up over. Most likely match for the report.
- **`RunEvent::Reopen`** (opening hop.app again from Finder or Spotlight while it runs, `lib.rs:148`). Hypothesis: LaunchServices activates hop before the event, so Settings comes forward first. `panel::show` then records `PREVIOUS = frontmost_other()` (`panel.rs:148`), which is `None` because hop is frontmost. Esc gives `give_back(true, None, false) = Stay`, so Settings stays in front.
- **Hide before the show's activation lands.** `set_focus` asks for activation asynchronously. A hide that runs first (a double hotkey press) calls `mac::activate(pid)` while hop is not active yet, which cooperative activation (macOS 14+) may ignore. hop's pending activation then completes with the palette hidden, and Settings comes forward. Hypothesis only.
- Not a cause: a minimized Settings (an activation does not unminimize it). Settings on another Space is untested: macOS may switch to that Space on activation.
- Not a cause: the tray (`lib.rs:69-70`; opening the status-item menu does not activate hop), `settings::open` and `whatsnew::open` (`settings.rs:295`, `whatsnew.rs:26`; the user asked for the window), a launch (`launch.rs`; `hide` gives the focus to the previous app, then `open` activates the launched one).

Why no fix now: on a blur, `give_back` cannot tell a click on Settings (keep `Stay`) from a system UI taking the key window (should be `Activate(previous)`). The candidate signals are "frontmost app is still hop at blur time" and "Settings is key at blur time". Both may lag behind the blur event, and the lock screen may report `loginwindow` as frontmost. Guessing wrong reintroduces PR #6 round 2 Major 1: a click on Settings would hand the focus to another app.

What would confirm it (owner's Mac, Settings open behind another app X, palette shown over X):

1. Press ⌘Space for Spotlight, then Esc. If Settings is now in front of X, the blur path is confirmed.
2. Lock the screen with ⌃⌘Q, then unlock. Same check.
3. Log, at the palette's `Focused(false)`, the frontmost app's pid and whether Settings is key, for a click on Settings, a click on X, and ⌘Space. If the three cases differ reliably, `give_back` gets a fourth input and a unit test.
4. Reopen: run `open -a hop` from Terminal, then press Esc in the palette. If Settings stays in front, the Reopen path is confirmed. A fix would record the previous app when hop resigns active (`NSWorkspace` notification), not at show time.

## PR 2: `feat: ⌘1–9 open a result directly`

- ⌘1…⌘9 open the Nth result (list position, like Raycast, even if scrolled out of view). No ⌘⇧N reveal: ⌘⇧3/4/5 are macOS screenshot shortcuts, so it would work for some digits only; ⌘Enter reveals the selection.
- Match `e.code` (`Digit1`…) so layouts where digits need ⇧ (AZERTY) work.
- Rows show "⌘1"… hints only while ⌘ is held; `aria-keyshortcuts` on each row.
- Tests: key mapping (`keys.test.ts`), hints (`Palette.test.tsx`). Docs: README, site Usage shortcut table, palette footer if needed.

## PR 3: `feat: calculator in the palette`

- A query that parses as a whole math expression with at least one operator shows a result row ("= 14") first; app results follow. A bare number stays a plain search.
- Enter (or ⌘1) on the result row copies the result and hides the palette.
- Hand-written parser, no `eval`: `+ - * / % ^`, parentheses, decimals, unary minus. Division by zero and non-finite results show no row. Results rounded to 12 significant digits. `.` is the only decimal separator.
- Clipboard: Rust command `copy_text` through `NSPasteboard` (existing `objc2-app-kit`), in `MAIN_COMMANDS`, length-capped.
- Accessible name "Result 14, press Enter to copy".
- Out of scope: units, currency, variables.
- Tests: parser table with edge cases; palette row; Rust command input cap. Docs: README, site Usage section, home card.
- As built: `copy_text` hides the palette itself after a successful copy, like `launch` does, so a copy and its hide are one call and a failure keeps the palette open with the error. It also refuses empty text (an empty copy would only wipe the clipboard). The listbox is now named "Results", since it holds more than apps. The home page's cards shrink to fit five in a row. Integers up to 2^53 stay exact (review round 1: 12-digit rounding changed `1234567890123+1`); only other results round to 12 significant digits.

## After PR 3

Recapture palette screenshots (`site/scripts/screens/`), final retrospective, end-of-step report, ask the owner about the 0.2.0 release.

## Log

Retrospectives, newest last.

### After PR 1 (#15, merged 2026-11-22)

- Cost: one extra round. The agent first mocked `react`'s `useState` and tried to add happy-dom; both were wrong for this repo (no new dependencies, a `PaletteView`-style props seam already exists).
- The test environment has no DOM: only static renders. Anything a test must see has to be reachable through props of a view component, not through effects or live key state.
- The reviewer's mutation check (remove a role, rerun the tests) found real coverage gaps cheaply. Keep it in every attack list.
- Agents report "CI not checked". The orchestrator always watches CI before merge.
- Adjustments: briefs now say "no new dependency; extend existing seams (`PaletteView` props, pure functions in `keys.ts`/`rank.ts`)". PR 2 puts the ⌘-held state in a `PaletteView` prop so hints are testable statically. Attack lists include mutation checks.

### After PR 2 (#16, merged 2026-11-22)

- The plan had a design flaw the implementer's own attack list caught: ⌘⇧3/4/5 are macOS screenshot shortcuts, so ⌘⇧N reveal was dropped. Lesson: check every new shortcut against macOS system shortcuts before planning it.
- The implementer pushed a commit that did not compile. Briefs now require the full gate before every commit, not only before the PR.
- Mutation checks in review paid off again (all visible mutations caught). DOM-only wiring (window listeners) stays a manual check; keep that wiring tiny.
- Deferred: key repeat on ⌘N (same as Enter), late hints when the palette opens with ⌘ held.
- Adjustments for PR 3: the brief lists the system-shortcut and clipboard constraints up front; the gate runs before each commit; the parser lives in its own pure module with a table test so mutation checks reach it.
