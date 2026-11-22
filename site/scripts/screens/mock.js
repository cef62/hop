// Fake Tauri backend for the hop pages, injected before they load (page.addInitScript).
// Data is made up: Apple system apps and example paths only.
(() => {
  const APPS = [
    ['Safari', '/Applications/Safari.app'], ['Mail', '/System/Applications/Mail.app'],
    ['Maps', '/System/Applications/Maps.app'], ['Notes', '/System/Applications/Notes.app'],
    ['Calendar', '/System/Applications/Calendar.app'], ['Music', '/System/Applications/Music.app'],
    ['Photos', '/System/Applications/Photos.app'], ['System Settings', '/System/Applications/System Settings.app'],
    ['Messages', '/System/Applications/Messages.app'], ['Terminal', '/System/Applications/Utilities/Terminal.app'],
    ['Activity Monitor', '/System/Applications/Utilities/Activity Monitor.app'], ['Preview', '/System/Applications/Preview.app'],
    ['Reminders', '/System/Applications/Reminders.app'], ['Stocks', '/System/Applications/Stocks.app'],
    ['Shortcuts', '/System/Applications/Shortcuts.app'], ['Weather', '/System/Applications/Weather.app'],
  ].map(([name, path]) => ({ name, altNames: [], path }));
  const HUES = { Safari: 205, Mail: 215, Maps: 140, Notes: 48, Calendar: 5, Music: 345, Photos: 30, 'System Settings': 220, Messages: 130, Terminal: 0 };
  // simple rounded-square placeholder icon with the app's initial
  const icon = (path) => {
    const name = (path.split('/').pop() || '').replace('.app', '');
    const h = HUES[name] ?? (name.length * 47) % 360;
    const dark = name === 'Terminal' || name === 'System Settings';
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="hsl(${h} 80% ${dark ? 38 : 62}%)"/><stop offset="1" stop-color="hsl(${h} 80% ${dark ? 18 : 42}%)"/></linearGradient></defs><rect width="64" height="64" rx="14" fill="url(#g)"/><text x="32" y="43" font-size="30" font-family="-apple-system,Helvetica" font-weight="600" text-anchor="middle" fill="#fff">${name[0]}</text></svg>`;
    return 'data:image/svg+xml,' + encodeURIComponent(svg);
  };
  const frecency = Object.fromEntries(['Safari', 'Mail', 'Notes', 'Calendar', 'Music'].map((n, i) => [APPS.find((a) => a.name === n).path, 50 - i * 8]));
  const view = {
    settings: { hotkey: 'Alt+Space', extraFolders: ['/Users/me/Tools'], ignoredApps: ['/System/Applications/Stocks.app'], theme: 'system', paletteStyle: 'compact', dailyUpdateCheck: true },
    hotkeyError: null, login: true, loginRefusal: null, version: '0.1.2', update: null,
  };
  // the real 0.1.2 entry of CHANGELOG.md, as release-notes.mjs writes it (no commit hash)
  const notes = '### Patch Changes\n\n- Made hop easier to read and to use with VoiceOver: dimmed text in the palette has more contrast, Settings starts reading at its first section and names the current hotkey, and Settings no longer leaves empty space below its content.\n\n---\n';
  let cbId = 0;
  const cbs = {};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback(cb) { const id = ++cbId; cbs[id] = cb; return id; },
    unregisterCallback(id) { delete cbs[id]; },
    convertFileSrc: (p) => icon(p),
    async invoke(cmd) {
      switch (cmd) {
        case 'plugin:event|listen': return ++cbId;
        case 'list_apps': return { generation: 1, apps: APPS };
        case 'frecency': return frecency;
        case 'get_settings': case 'set_settings': return view;
        case 'whatsnew_info': return { update: { version: '0.1.2', current_version: '0.1.1', notes }, phase: { kind: 'idle' } };
        default: return null;
      }
    },
  };
})();
