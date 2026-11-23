import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { calc } from "./calc";
import { paletteKey, type Actions } from "./keys";
import { move, prepare, search, type App, type Frecency } from "./rank";

export type Style = "compact" | "roomy";
type Index = { generation: number; items: App[] };
type Look = { style: Style; perf: boolean; update: string | null };

/** After the next paint: rAF runs before it, the timeout right after. */
const afterPaint = (f: () => void) => requestAnimationFrame(() => setTimeout(f, 0));

type ViewProps = {
  style: Style;
  query: string;
  rows: App[];
  /** the calculator's answer to the query: a row above the apps, so row indexes count it */
  result?: string | null;
  sel: number;
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

/** The card. Both styles render the same markup; `data-style` only switches CSS variables and the footer rule. */
/** What row `i` of the list is: the calculator result (row 0 when there is one) or an app. */
export const rowAt = (i: number, result: string | null | undefined, rows: App[]) => (result == null ? rows[i] : i === 0 ? result : rows[i - 1]);
export const total = (rows: App[], result: string | null) => rows.length + (result === null ? 0 : 1);

/** The command Enter (⌘Enter: `reveal`) sends for row `i`. The result row copies (there is nothing to reveal) and so
 * records no launch. */
export function command(i: number, result: string | null, rows: App[], reveal: boolean): [string, Record<string, unknown>] | null {
  const r = rowAt(i, result, rows);
  if (typeof r === "string") return ["copy_text", { text: r }];
  return r ? ["launch", { path: r.path, reveal }] : null;
}

export function PaletteView({ style, query, rows, result, sel, error, cmd, update, onQuery, onOpen, onUpdate, input, card, list }: ViewProps) {
  const off = result == null ? 0 : 1;
  const count = rows.length + off;
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
          placeholder="Search apps"
          aria-label="Search apps"
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
          {result != null && (
            <li key="=" {...option(0)} aria-label={`Result ${result}, press Enter to copy`}>
              <span className="icon calc" aria-hidden="true">
                =
              </span>
              <span className="name">= {result}</span>
              {hint(0, "copy")}
            </li>
          )}
          {rows.map((a, j) => (
            <li key={a.path} {...option(j + off)}>
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
              {hint(j + off, "open")}
            </li>
          ))}
        </ul>
      )}
      <div className="footer">
        <span className="keys" aria-hidden="true">
          {typeof rowAt(sel, result, rows) === "string" ? "↵ copy · ⌘1–9 open · esc close" : "↵ open · ⌘↵ reveal in Finder · ⌘1–9 open · esc close"}
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
  const [update, setUpdate] = useState<string | null>(null);
  const [cmd, setCmd] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const perf = useRef(false);
  const keyAt = useRef<number | null>(null);

  const items = useMemo(() => prepare(index.items), [index]);
  const rows = useMemo(() => search(query, items, frecency), [query, items, frecency]);
  const result = useMemo(() => calc(query), [query]);
  const count = total(rows, result);
  const at = Math.min(sel, Math.max(count - 1, 0)); // the list can shrink under the selection (app removed)

  const open = (i: number, reveal = false) => {
    const c = command(i, result, rows, reveal);
    if (c) invoke(...c).catch((e) => setError(String(e)));
  };
  const actions = useRef<Actions>(null!);
  actions.current = {
    hide: () => invoke("hide_palette"),
    move: (d) => setSel(move(at, d, count)),
    open: (reveal) => open(at, reveal),
    openAt: (i) => open(i),
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
      result={result}
      sel={at}
      error={error}
      cmd={cmd}
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
