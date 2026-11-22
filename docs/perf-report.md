# Performance report

hop 0.1.1 plus the M7 changes, release build, on the owner's Mac (Apple M4 Pro, macOS 27.2.1), 2026-11-22. 129 apps
indexed. Budgets from `docs/launcher-plan.md` §8.

| Measure | Result | Budget |
|---|---|---|
| Hotkey → palette visible and typeable | p95 51.1 ms, median 29.9 ms (n = 59) | p95 < 100 ms |
| Keystroke → filtered list painted | p95 2.0 ms, median 1.0 ms, max 4.0 ms (n = 46) | p95 < 16 ms |
| Cold start → hotkey registered | 157–178 ms (3 launches) | < 1 s |
| Idle after launch, palette hidden | CPU 0.00 %, footprint 64.6 MiB (hop 25.0 MiB) | ~0 %, < 120 MB |
| Idle after 50 palette show/hides | CPU 0.23 %, footprint 80.4 MiB (hop 29.1 MiB) | ~0 %, < 120 MB |
| Settings open, idle | CPU 0.17 %, footprint 115.0 MiB (hop 34.1 MiB) | report |
| What's new open, idle | CPU 0.20 %, footprint 103.7 MiB | report |
| Release bundle (`hop.app`) | 6.26 MiB | report |

## Method

- Build: `pnpm tauri build --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'`, run from a copy
  under a `target/release/` folder so it never touches the installed app's login item. The installed hop was quit
  for the run and restarted afterwards.
- Latency: the app ran with `HOP_PERF=1`, which prints `hop perf: shown <ms>` (hotkey handler → input focused and
  painted) and `hop perf: key <ms>` (input event → list painted). Hotkeys, keystrokes and Esc were **synthetic**,
  sent through System Events (`osascript`): 50 show/hide cycles plus the shows before typing, then 8 queries of 6
  letters typed one key at a time. p95 is the nearest-rank 95th percentile. Cold start is hop's own
  `hotkey "Alt+Space" registered … after start` log line.
- CPU: CPU time (`ps -o time`) of hop and the WebKit processes it started (WebContent, GPU, Networking: the ones
  that appeared when hop launched or opened a window), summed and averaged over 30 s of idling. The first sample is
  taken 45 s after launch, past the update check 30 s in.
- Memory: `footprint` (`phys_footprint`) of the same processes, summed. "hop" is the app process alone.
- What's new needs a waiting update, so it was measured on a second build whose updater reads a local feed
  (`plugins.updater.endpoints` pointed at `http://127.0.0.1:8765/latest.json` through `--config`); nothing was
  installed.
- One run per row; a second, identical run gave the same latencies within 0.3 ms at p95. Footprint grows by a few
  MiB after the first shows (WebKit caches) and with each open window, which has its own WebContent process.
