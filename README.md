# hop

A minimal macOS app launcher. Work in progress.

- **⌥Space** shows the palette over any app, on the screen under the mouse; press it again to hide.
- Type to find an app by name, by its English file name, or by initials (`vsc` finds Visual Studio Code). Apps you
  open often rank higher; with nothing typed, the palette lists them. **↑↓** select, **Enter** opens,
  **⌘Enter** reveals the app in Finder.
- hop finds apps in `/Applications`, `/System/Applications`, their `Utilities` folders and `~/Applications`, one
  folder deep (vendor folders work), and picks up installed or removed apps by itself.
- **Esc**, or clicking away, hides it and returns focus to the app you were in.
- The menu-bar icon opens the palette or **Settings…**, and quits hop. It is always there, so hop stays reachable
  when another app holds the hotkey.
- **⌘,** in the palette opens Settings; **Esc** closes Settings.
- Settings: the hotkey (click it, press a new chord; ⌫ removes it, Esc cancels), start at login (on by default),
  extra folders to index, apps to hide, theme (System, Light, Dark) and palette style (Compact or Roomy). Changes
  apply at once. If another app (Raycast, Alfred) uses the same hotkey, both respond: change one of them.
- Opening the hop app again while it runs shows the palette.
- Updates: while Settings › Check every day is on, hop checks for a new version 30 seconds after it starts, then
  once a day. **Check now** in Settings and **Check for Updates…** in the menu-bar icon check at once. hop never
  interrupts you: a found update shows as a small pill in the palette footer (click it or press **⌘U**) and as
  **Update to hop X…** in the menu-bar icon. Both open the What's new window with the release notes and **Install
  and restart**. Updates are verified against hop's signing key before they install; after the restart hop stays
  hidden until you call it.
