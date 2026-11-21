import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { paletteKey } from "./keys";

function Palette() {
  const [query, setQuery] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const card = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const shown = listen("palette-shown", () => input.current?.focus());
    const hidden = listen("palette-hidden", () => setQuery(""));
    const onKey = (e: KeyboardEvent) => paletteKey(e, () => invoke("hide_palette"));
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      shown.then((off) => off());
      hidden.then((off) => off());
    };
  }, []);

  // the native vibrancy view follows the card: the window is fixed and larger than the card
  useEffect(() => {
    const el = card.current!;
    const report = () => {
      const r = el.getBoundingClientRect();
      invoke("card_frame", { x: r.x, y: r.y, width: r.width, height: r.height });
    };
    const ro = new ResizeObserver(report);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  return (
    <div className="card" ref={card}>
      <input
        ref={input}
        className="search"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search apps"
        aria-label="Search apps"
        autoFocus
        spellCheck={false}
        autoComplete="off"
      />
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<Palette />);
