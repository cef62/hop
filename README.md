# hop

A minimal macOS app launcher. Work in progress.

- **⌥Space** shows the palette over any app, on the screen under the mouse; press it again to hide.
- Type to find an app by name, by its English file name, or by initials (`vsc` finds Visual Studio Code). Apps you
  open often rank higher; with nothing typed, the palette lists them. **↑↓** select, **Enter** opens,
  **⌘Enter** reveals the app in Finder.
- hop finds apps in `/Applications`, `/System/Applications`, their `Utilities` folders and `~/Applications`, one
  folder deep (vendor folders work), and picks up installed or removed apps by itself.
- Palette style: until Settings exist, put `{"paletteStyle": "roomy"}` in
  `~/Library/Application Support/dev.hop.app/settings.json` for the larger style (default `compact`); it applies the
  next time the palette opens.
- **Esc**, or clicking away, hides it and returns focus to the app you were in.
- hop has no Dock icon and no menu bar icon yet. Open the hop app again to show the palette (useful if another app
  holds ⌥Space). Quit with **⌘Q** while the palette is open, or `pkill -x hop`.
