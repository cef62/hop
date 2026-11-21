# Spike: palette panel and global hotkey (M1)

Plan §4.7, §4.8, §9 M1. Code: `src-tauri/src/panel.rs`, `src-tauri/src/hotkey.rs`.

## Approach chosen: option 1 (plain Tauri window + objc2)

No `tauri-nspanel`. The plain window:

- `tauri.conf.json`: `macOSPrivateApi: true` (with tauri's `macos-private-api` feature), transparent, no decorations,
  not resizable, hidden at start, `skipTaskbar`, `alwaysOnTop`, fixed 640×460 logical (Roomy card 600 px + footer
  fits inside with margin). No window-state plugin.
- Accessory activation policy, set before `run` (tao forces Regular at launch otherwise) plus `LSUIElement` in `Info.plist`: no Dock icon, not in ⌘Tab, not even at launch.
- `objc2-app-kit` (0.3.2, the version tauri/tao already pull in) on the NSWindow at setup:
  collection behavior `CanJoinAllSpaces | FullScreenAuxiliary | Transient | IgnoresCycle`, level
  `NSStatusWindowLevel` (25). tauri's `alwaysOnTop` only gives the floating level (3), which sits below a full-screen
  app's windows.
- Vibrancy: tauri's own `windowEffects` (`hudWindow`, state `active`, radius 12). That inserts one
  NSVisualEffectView covering the whole window, but the window is larger than the card, so the frontend reports the
  card's rect (`ResizeObserver` → `card_frame` command) and Rust refits the effect view (found by window-vibrancy's
  tag) to it. Fires on card size changes only, never per keystroke in M1; in M2 it fires when the result count
  changes, which resizes a subview, not the window.
- tao's borderless window class already returns YES from `canBecomeKeyWindow`, so the plain window can take keyboard
  focus; that is the usual reason to reach for an NSPanel.

Why option 1: it needs no new dependency beyond crates already in `Cargo.lock`, and every §4.7 requirement maps to a
documented NSWindow property. Option 2 stays the fallback if the owner's check shows a Space switch or no keyboard
focus over a full-screen app (rows 1 and 2 below); `tauri-nspanel` would then replace `mac::float_over_everything`.

### Traps found

- **Monitor scales.** tao on macOS reports the cursor as points × the *primary* monitor's scale and each monitor as
  points × *its own* scale. `panel::place` converts everything back to points and returns a `LogicalPosition`, which
  tao hands to Cocoa unchanged. Unit-tested with a 1× monitor at a negative origin beside a 2× primary, and the
  reverse.
- **No active displays.** With the screen locked (display asleep), `available_monitors()` returned an empty list and
  the window kept NSWindow's default centering. `place` now falls back to the primary monitor.
- **Rapid double press / blur race.** `is_visible`/`is_focused` lag behind a queued show, so the palette keeps its own
  `Visibility` state: toggle flips it; a blur hides only after a focus arrived since the last show (a stale blur during
  activation is ignored); our own hide's blur is ignored. Unit-tested.
- **Hotkey conflicts are invisible.** Two hop processes both registered `Alt+Space` successfully: Carbon's
  `RegisterEventHotKey` does not fail when another *process* holds the chord. Registration errors therefore cannot be
  the M3 conflict signal by themselves (Settings needs another check, or must say "may be taken by another app").
  Owner confirmed with Raycast on ⌥Space: hop logs nothing and both apps react to the press. Plan §4.8 ("Settings shows
  the conflict") only holds for chords the OS refuses; Owner decision: M3 Settings shows a permanent hint
  under the recorder instead (plan §4.8 updated).
  Spotlight's ⌥Space is a system symbolic hotkey (`com.apple.symbolichotkeys` id 64); on this Mac it is disabled.
- **Margin clicks.** The fixed window is larger than the card, and its transparent margin still takes clicks (owner
  step 15: thin strips left, top and right, about 450 px below). A `pointerdown` outside the card now hides the palette,
  like a blur. The click is consumed, not passed to the app below (Spotlight and Raycast behave the same). True
  click-through would need per-region `ignoresMouseEvents` driven by a global mouse monitor; not worth it.
- **Focus return.** `panel::hide` hides the window, then `app.hide()` (velo's trick) so macOS reactivates the
  previously active app.

## Results

Verification on this Mac was limited: the session ran with the **screen locked**
(`CGSSessionScreenIsLocked = 1`), so screenshots were black and System Events keystrokes went to the lock screen.
Owner run on 2026-11-22 (checklist below): step 12 failed (known limit, see Risks), step 15 failed and is fixed (re-tested: pass), IME skipped by owner, no other failure reported.

| # | Requirement | Result | How |
|---|---|---|---|
| 4.7-1 | Over full-screen apps, any Space, no Space switch | owner: pass | config in place (collection behavior + level 25) |
| 4.7-2 | Keyboard focus on show, focus back on hide | owner: pass | |
| 4.7-3 | Not in Dock, ⌘Tab, Mission Control | owner: pass | Accessory policy + `Transient`/`IgnoresCycle` set |
| 4.7-4 | Transparent, borderless, rounded, vibrancy, fixed size | owner: pass; margin clicks fixed (below) | `CGWindowListCopyWindowInfo`: window 640×460 at layer 25 on screen; look (vibrancy, radius): owner pass |
| 4.7-5 | Centered on the screen under the cursor, top at ~1/3 | verified (one display), unit-tested (two) | window at x=580, y=390 on a 1800×1169 pt display (expected 580, 389.7); `place` tests cover a second monitor at another scale and a negative origin |
| 4.7-6 | Hide on blur and Esc, query cleared on hide | owner: pass | logic unit-tested |
| 4.8 | ⌥Space typed no U+00A0 into the frontmost app | owner: pass | |
| — | Reopen (open hop.app again) shows the palette | verified | second `open hop.app` put the window on screen (window list) |

### M1 attack list

| Attack | Result |
|---|---|
| Full-screen Space | owner: pass |
| Second monitor with a different scale | owner: pass |
| Stage Manager | owner: pass |
| Hotkey while the palette is visible | unit-tested (toggle hides); owner: pass |
| App that grabs ⌥Space first | **fails, known limit**: with Raycast on ⌥Space, hop starts with no stderr and both apps react to the press. Carbon registration never reports a cross-process conflict, so M3 cannot detect it from registration (see Risks) |
| Focus return to the previous app | owner: pass |
| Rapid double press | unit-tested (second press hides even before focus arrives); owner: pass |

### Measured (release build, Apple silicon, this Mac)

| Budget | Result |
|---|---|
| Cold start to hotkey registered | 131–159 ms from `run()` (5 runs), 172–194 ms from process spawn |
| Release bundle | `hop.app` 3.9 MB, zipped 1.7 MB |
| Idle RSS, palette hidden | hop process 93 MB; with its WebKit helpers (WebContent 41 MB, GPU 25 MB, Networking 14 MB) about 172 MB |
| Idle CPU, palette hidden | 0.0% (`ps -o pcpu`) |
| Hotkey → visible and typeable | not measured (needs the unlocked screen; formal budget in M2) |

The §8 RSS target (under 120 MB) holds for the hop process alone, not with WebKit's helper processes counted.

## Owner checklist

Quit any other hop first. Build: `pnpm tauri build --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'`,
then open `src-tauri/target/release/bundle/macos/hop.app`.

1. Open TextEdit with a new document, click in it. Press ⌥Space: the palette appears centered, a third down, input
   focused (type a few letters). No non-breaking space appeared in TextEdit (⌥Space again, then check the document).
2. Press Esc: palette gone, TextEdit is frontmost and the caret is back. ⌥Space again: input is empty.
3. Show, then click on another window: palette hides.
4. Show, press ⌥Space again: hides. Press ⌥Space twice very fast: ends hidden; three times: ends shown.
5. Put TextEdit full screen (⌃⌘F). Press ⌥Space: palette appears over it, no Space switch, input focused; Esc returns
   to the full-screen TextEdit.
6. Switch to another Space (⌃→), press ⌥Space: palette appears there.
7. Check the Dock, ⌘Tab and Mission Control (with the palette open): hop is in none.
8. Look: rounded card with blur behind, nothing around it; switch Light/Dark in System Settings and reopen.
9. Second display at a different scale (if available): move the mouse there, press ⌥Space: palette on that display,
   centered, a third down.
10. Stage Manager on: repeat 1–3.
11. Set Raycast (or Spotlight) to ⌥Space, relaunch hop from Terminal
    (`.../hop.app/Contents/MacOS/hop`): note what stderr says and which app gets the press. Open hop.app again: the
    palette shows (Reopen).
12. With the palette open, press ⌘Q: hop quits. If not, `pkill -x hop` and report it.
