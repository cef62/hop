import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { calc } from "./calc";
import { live, step, type Event, type State } from "./confirm";
import { insertable, leads, rowAt, total, type Entry, type Lead, type Row } from "./leads";
import { paletteKey, type Actions } from "./keys";
import { move, prepare, search, type Frecency, type Item } from "./rank";

export type Style = "compact" | "roomy";
type Index = { generation: number; items: Item[] };
type Look = { style: Style; perf: boolean; update: string | null; calculator: boolean };

/** After the next paint: rAF runs before it, the timeout right after. */
const afterPaint = (f: () => void) => requestAnimationFrame(() => setTimeout(f, 0));

type ViewProps = {
  style: Style;
  query: string;
  rows: Item[];
  /** the rows above the items (the calculator's answer, past results), so row indexes count them */
  leads?: Lead[];
  sel: number;
  /** the command waiting for its second Enter */
  armed?: string | null;
  error: string;
  /** ⌘ is held: rows 1–9 show their ⌘N key in place of "↵ open" */
  cmd?: boolean;
  /** the version waiting to install: the footer shows its pill */
  update?: string | null;
  onQuery(q: string, timeStamp: number): void;
  onOpen(i: number): void;
  onUpdate?(): void;
  input?: React.Ref<HTMLInputElement>;
  card?: React.Ref<HTMLDivElement>;
  list?: React.Ref<HTMLUListElement>;
};

/** The command Enter (⌘Enter: `reveal`) sends for row `i`, or null: past the end, or a history row (the reducer edits
 * the query for it, `step` in confirm.ts). The result row copies (there is nothing to reveal) and records the copy
 * under its expression, but no launch. */
export function command(i: number, lead: Lead[], rows: Item[], reveal: boolean, query: string): [string, Record<string, unknown>] | null {
  const r = rowAt(i, lead, rows);
  if (!r || r.kind === "history") return null;
  if (r.kind === "result") return ["copy_text", { text: r.text, expr: query.trim() }];
  return ["run", { id: r.id, reveal }];
}

/** The footer's keys for the selected row: only an app has a Finder location to reveal. */
export const footer = (r: Row, armed: string | null | undefined, query = "") =>
  r?.kind === "result"
    ? "↵ copy · ⌘1–9 open · esc close"
    : r?.kind === "history"
      ? `↵ edit${insertable(query) ? " · ⌘↵ insert" : ""} · ⌘1–9 pick · esc close`
      : r?.confirm && r.id === armed
      ? "↵ confirm · esc close"
      : r?.kind === "command"
        ? "↵ run · ⌘1–9 open · esc close"
        : r && r.kind !== "app"
          ? "↵ open · ⌘1–9 open · esc close"
          : "↵ open · ⌘↵ reveal in Finder · ⌘1–9 open · esc close";

/** Command glyphs (24×24 strokes), drawn in place of an app icon. */
const GLYPHS: Record<string, React.ReactNode> = {
  "cmd:lock": (
    <>
      <rect x="5" y="11" width="14" height="10" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </>
  ),
  "cmd:sleep": <path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z" />,
  "cmd:restart": (
    <>
      <path d="M20 12a8 8 0 1 1-2.34-5.66" />
      <path d="M20 4v5h-5" />
    </>
  ),
  "cmd:shutdown": (
    <>
      <path d="M12 3v9" />
      <path d="M6.3 6.3a8 8 0 1 0 11.4 0" />
    </>
  ),
  "cmd:logout": (
    <>
      <path d="M10 4H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h4" />
      <path d="M14 8l4 4-4 4M18 12H9" />
    </>
  ),
  "cmd:empty-trash": (
    <>
      <path d="M4 7h16M9 7V4h6v3" />
      <path d="M6 7l1 13h10l1-13" />
    </>
  ),
};

export function PaletteView({ style, query, rows, leads: lead = [], sel, armed, error, cmd, update, onQuery, onOpen, onUpdate, input, card, list }: ViewProps) {
  const prompt = rows.find((a) => a.confirm && a.id === armed)?.confirm ?? "";
  const off = lead.length;
  const count = total(rows, lead);
  const option = (i: number) => ({
    id: `row-${i}`,
    className: "row",
    role: "option",
    "aria-selected": i === sel,
    "aria-keyshortcuts": i < 9 ? `Meta+${i + 1}` : undefined,
    onClick: () => onOpen(i),
  });
  const hint = (i: number, label: string) =>
    cmd && i < 9 ? (
      <span className="hint" aria-hidden="true">
        ⌘{i + 1}
      </span>
    ) : (
      i === sel && (
        <span className="hint" aria-hidden="true">
          ↵<span className="hint-label"> {label}</span>
        </span>
      )
    );
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
          placeholder="Search apps and settings"
          aria-label="Search apps and settings"
          aria-controls="rows"
          aria-activedescendant={count ? `row-${sel}` : undefined}
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
      {count > 0 && (
        <ul className="rows" id="rows" role="listbox" aria-label="Results" ref={list}>
          {lead.map((l, i) =>
            l.kind === "result" ? (
              <li key="=" {...option(i)} aria-label={`Result ${l.text}, press Enter to copy`}>
                <span className="icon calc" aria-hidden="true">
                  =
                </span>
                <span className="name">= {l.text}</span>
                {hint(i, "copy")}
              </li>
            ) : (
              // an expression may repeat a pane or app id never: the "history:" prefix keeps the key space apart
              <li key={`history:${l.expr}`} {...option(i)} aria-label={`${l.expr} equals ${l.result}, press Enter to edit it${insertable(query) ? ", Command Enter to insert the result" : ""}`}>
                <span className="icon glyph" aria-hidden="true">
                  <svg viewBox="0 0 24 24">
                    <circle cx="12" cy="12" r="8" />
                    <path d="M12 8v4l3 2" />
                  </svg>
                </span>
                <span className="name">
                  {l.expr} = {l.result}
                </span>
                {hint(i, "edit")}
              </li>
            ),
          )}
          {rows.map((a, j) => {
            const armedRow = a.confirm && a.id === armed;
            return (
              // every section shares the System Settings path: the id is the only unique key
              <li key={a.id} {...option(j + off)} aria-label={armedRow ? a.confirm : undefined}>
                {a.kind === "command" ? (
                  <span className="icon glyph" aria-hidden="true">
                    <svg viewBox="0 0 24 24">{GLYPHS[a.id]}</svg>
                  </span>
                ) : (
                  <span className="icon">
                    <img
                      src={convertFileSrc(a.path ?? "", "icon")}
                      alt=""
                      loading="lazy"
                      onLoad={(e) => (e.currentTarget.dataset.loaded = "")}
                      onError={(e) => (e.currentTarget.style.visibility = "hidden")}
                    />
                  </span>
                )}
                <span className="name">{armedRow ? a.confirm : a.name}</span>
                {a.kind === "pane" && <span className="tag">System Settings</span>}
                {hint(j + off, armedRow ? "confirm" : a.kind === "command" ? "run" : "open")}
              </li>
            );
          })}
        </ul>
      )}
      {/* present while empty, so VoiceOver announces the prompt when a command is armed */}
      <p className="status" role="status">
        {prompt}
      </p>
      <div className="footer">
        <span className="keys" aria-hidden="true">
          {footer(rowAt(sel, lead, rows), armed, query)}
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
  const [index, setIndex] = useState<Index>({ generation: 0, items: [] });
  const [frecency, setFrecency] = useState<Frecency>({});
  const [style, setStyle] = useState<Style>("compact");
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const [error, setError] = useState("");
  const [armed, setArmed] = useState<State["armed"]>(null);
  const [update, setUpdate] = useState<string | null>(null);
  const [cmd, setCmd] = useState(false);
  const [history, setHistory] = useState<Entry[]>([]);
  const [calculator, setCalculator] = useState(true);
  const input = useRef<HTMLInputElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const perf = useRef(false);
  /** the query was replaced by a history row: the input takes the focus back (a click blurred it) with the caret last */
  const caret = useRef(false);
  const keyAt = useRef<number | null>(null);

  const items = useMemo(() => prepare(index.items), [index]);
  const rows = useMemo(() => search(query, items, frecency), [query, items, frecency]);
  const lead = useMemo(() => leads(query, calculator ? calc(query) : null, history, calculator), [query, history, calculator]);
  const count = total(rows, lead);
  const at = Math.min(sel, Math.max(count - 1, 0)); // the list can shrink under the selection (app removed)

  const row = (i: number) => rowAt(i, lead, rows);
  // through a ref: the window listeners below are set up once and must see this render's state
  const send = useRef<(ev: Event) => void>(null!);
  send.current = (ev) => {
    const o = step({ armed, sel: at }, ev, row, query);
    setArmed(o.armed);
    setSel(o.sel);
    if (o.query !== undefined) {
      setQuery(o.query); // a history row: edit the sum in place, the palette stays open
      setError("");
      caret.current = o.query !== query; // no change, no effect to consume the flag
    }
    const c = o.run === null ? null : command(o.run, lead, rows, ev.type === "enter" && ev.meta, query); // ⌘Enter reveals
    if (c) invoke(...c).catch((e) => setError(String(e)));
  };
  const actions = useRef<Actions>(null!);
  actions.current = {
    hide: () => invoke("hide_palette"),
    move: (d) => setSel(move(at, d, count)),
    send: (ev) => send.current(ev),
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
        setCalculator(e.payload.calculator);
        perf.current = e.payload.perf;
        input.current?.focus();
        loadFrecency();
        invoke<Entry[]>("calc_history").then(setHistory, () => setHistory([]));
        if (perf.current) afterPaint(() => invoke("perf_mark", { kind: "shown" }));
      }),
      listen<Look>("palette-hidden", (e) => {
        setStyle(e.payload.style); // a changed setting applies while hidden, before the next show paints
        setCalculator(e.payload.calculator);
        setQuery("");
        setError("");
        send.current({ type: "hidden" });
        setCmd(false);
      }),
    ];
    subs[0].then(() => invoke<Index>("list_apps").then(take));
    loadFrecency();
    const onKey = (e: KeyboardEvent) => paletteKey(e, actions.current);
    const onMeta = (e: KeyboardEvent) => e.key === "Meta" && setCmd(e.type === "keydown");
    const onBlur = () => setCmd(false); // a ⌘ keyup that lands in another window never reaches us
    // the window is larger than the card and its transparent margin still takes clicks: treat them as a click outside
    const onDown = (e: PointerEvent) => {
      send.current({ type: "pointer" }); // a click, even on the armed row, only arms again: it never confirms
      // composedPath is fixed at dispatch, so a node React removes during the event still counts as inside
      if (!card.current || !e.composedPath().includes(card.current)) invoke("hide_palette");
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("keydown", onMeta);
    window.addEventListener("keyup", onMeta);
    window.addEventListener("blur", onBlur);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keydown", onMeta);
      window.removeEventListener("keyup", onMeta);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("pointerdown", onDown);
      subs.forEach((s) => s.then((off) => off()));
    };
  }, []);

  useLayoutEffect(() => {
    const el = input.current;
    if (!caret.current || !el) return;
    caret.current = false;
    el.focus();
    el.setSelectionRange(query.length, query.length);
  }, [query]);

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
      leads={lead}
      sel={at}
      armed={live({ armed, sel: at }, row, query)}
      error={error}
      cmd={cmd}
      update={update}
      onUpdate={actions.current.update}
      onQuery={(q, t) => {
        if (perf.current) keyAt.current = t;
        setQuery(q);
        setError("");
        send.current({ type: "query" });
      }}
      onOpen={(i) => send.current({ type: "click", i })}
      input={input}
      card={card}
      list={list}
    />
  );
}
