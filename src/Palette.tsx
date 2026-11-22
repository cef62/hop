import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { paletteKey, type Actions } from "./keys";
import { move, prepare, search, type App, type Frecency } from "./rank";

export type Style = "compact" | "roomy";
type Index = { generation: number; apps: App[] };
type Look = { style: Style; perf: boolean; update: string | null };

/** After the next paint: rAF runs before it, the timeout right after. */
const afterPaint = (f: () => void) => requestAnimationFrame(() => setTimeout(f, 0));

type ViewProps = {
  style: Style;
  query: string;
  rows: App[];
  sel: number;
  error: string;
  /** the version waiting to install: the footer shows its pill */
  update?: string | null;
  onQuery(q: string, timeStamp: number): void;
  onOpen(i: number): void;
  onUpdate?(): void;
  input?: React.Ref<HTMLInputElement>;
  card?: React.Ref<HTMLDivElement>;
  list?: React.Ref<HTMLUListElement>;
};

/** The card. Both styles render the same markup; `data-style` only switches CSS variables and the footer rule. */
export function PaletteView({ style, query, rows, sel, error, update, onQuery, onOpen, onUpdate, input, card, list }: ViewProps) {
  return (
    <div className="card" data-style={style} data-update={update ? "" : undefined} ref={card}>
      <label className="search-row">
        <svg className="search-icon" viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="10.5" cy="10.5" r="6.5" />
          <path d="M15.5 15.5 21 21" />
        </svg>
        <input
          ref={input}
          className="search"
          value={query}
          onChange={(e) => onQuery(e.target.value, e.timeStamp)}
          placeholder="Search apps"
          aria-label="Search apps"
          aria-controls="rows"
          aria-activedescendant={rows.length ? `row-${sel}` : undefined}
          autoFocus
          spellCheck={false}
          autoComplete="off"
        />
      </label>
      {error && (
        <p className="message" role="alert">
          {error}
        </p>
      )}
      {rows.length > 0 && (
        <ul className="rows" id="rows" role="listbox" aria-label="Apps" ref={list}>
          {rows.map((a, i) => (
            <li key={a.path} id={`row-${i}`} className="row" role="option" aria-selected={i === sel} onClick={() => onOpen(i)}>
              <span className="icon">
                <img
                  src={convertFileSrc(a.path, "icon")}
                  alt=""
                  loading="lazy"
                  onLoad={(e) => (e.currentTarget.dataset.loaded = "")}
                  onError={(e) => (e.currentTarget.style.visibility = "hidden")}
                />
              </span>
              <span className="name">{a.name}</span>
              {i === sel && (
                <span className="hint" aria-hidden="true">
                  ↵<span className="hint-label"> open</span>
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
      <div className="footer">
        <span className="keys" aria-hidden="true">
          ↵ open · ⌘↵ reveal in Finder · esc close
        </span>
        {update && (
          // not a Tab stop (the input keeps the focus): ⌘U is its key
          <button type="button" className="pill" tabIndex={-1} onClick={onUpdate} aria-keyshortcuts="Meta+U">
            Update to {update} <span aria-hidden="true">⌘U</span>
          </button>
        )}
      </div>
    </div>
  );
}

export function Palette() {
  const [index, setIndex] = useState<Index>({ generation: 0, apps: [] });
  const [frecency, setFrecency] = useState<Frecency>({});
  const [style, setStyle] = useState<Style>("compact");
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const [error, setError] = useState("");
  const [update, setUpdate] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const perf = useRef(false);
  const keyAt = useRef<number | null>(null);

  const items = useMemo(() => prepare(index.apps), [index]);
  const rows = useMemo(() => search(query, items, frecency), [query, items, frecency]);
  const at = Math.min(sel, Math.max(rows.length - 1, 0)); // the list can shrink under the selection (app removed)

  const open = (i: number, reveal = false) => {
    const a = rows[i];
    if (a) invoke("launch", { path: a.path, reveal }).catch((e) => setError(String(e)));
  };
  const actions = useRef<Actions>(null!);
  actions.current = {
    hide: () => invoke("hide_palette"),
    move: (d) => setSel(move(at, d, rows.length)),
    open: (reveal) => open(at, reveal),
    settings: () => invoke("open_settings"),
    update: () => update && invoke("open_whatsnew"),
  };

  useEffect(() => {
    // a list_apps reply can arrive after a newer apps-changed event: keep the higher generation
    const take = (i: Index) => setIndex((cur) => (i.generation > cur.generation ? i : cur));
    const loadFrecency = () => invoke<Frecency>("frecency").then(setFrecency);
    const subs = [
      listen<Index>("apps-changed", (e) => take(e.payload)),
      listen<Look>("palette-shown", (e) => {
        setStyle(e.payload.style);
        setUpdate(e.payload.update);
        perf.current = e.payload.perf;
        input.current?.focus();
        loadFrecency();
        if (perf.current) afterPaint(() => invoke("perf_mark", { kind: "shown" }));
      }),
      listen<Look>("palette-hidden", (e) => {
        setStyle(e.payload.style); // a changed setting applies while hidden, before the next show paints
        setQuery("");
        setSel(0);
        setError("");
      }),
    ];
    subs[0].then(() => invoke<Index>("list_apps").then(take));
    loadFrecency();
    const onKey = (e: KeyboardEvent) => paletteKey(e, actions.current);
    // the window is larger than the card and its transparent margin still takes clicks: treat them as a click outside
    const onDown = (e: PointerEvent) => {
      // composedPath is fixed at dispatch, so a node React removes during the event still counts as inside
      if (!card.current || !e.composedPath().includes(card.current)) invoke("hide_palette");
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown);
      subs.forEach((s) => s.then((off) => off()));
    };
  }, []);

  useEffect(() => {
    list.current?.children[at]?.scrollIntoView({ block: "nearest" });
  }, [at, rows]);

  // keystroke → painted, reported only with HOP_PERF=1
  useLayoutEffect(() => {
    const t = keyAt.current;
    if (t === null) return;
    keyAt.current = null;
    afterPaint(() => invoke("perf_mark", { kind: "key", ms: performance.now() - t }));
  }, [rows]);

  // the native vibrancy view follows the card: the window is fixed and larger than the card
  useEffect(() => {
    const el = card.current!;
    const report = () => {
      const r = el.getBoundingClientRect();
      const radius = parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0;
      invoke("card_frame", { x: r.x, y: r.y, width: r.width, height: r.height, radius });
    };
    const ro = new ResizeObserver(report);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  return (
    <PaletteView
      style={style}
      query={query}
      rows={rows}
      sel={at}
      error={error}
      update={update}
      onUpdate={actions.current.update}
      onQuery={(q, t) => {
        if (perf.current) keyAt.current = t;
        setQuery(q);
        setSel(0);
        setError("");
      }}
      onOpen={(i) => open(i)}
      input={input}
      card={card}
      list={list}
    />
  );
}
