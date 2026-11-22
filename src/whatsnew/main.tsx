// Entry of the What's new window (whatsnew.html). Styled by the Settings window's sheet plus whatsnew.css.
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import * as api from "./api";
import { WhatsNew } from "./WhatsNew";

function App() {
  const [info, setInfo] = useState<api.WhatsNewInfo | null>(null);
  const [phase, setPhase] = useState<api.Phase>({ kind: "idle" });
  const load = () => {
    // the reply carries the install's phase now, the channel every later one. A channel message can land before the
    // reply: the snapshot is then older and is dropped
    let live = false;
    api
      .info((p) => {
        live = true;
        setPhase(p);
      })
      .then((i) => {
        if (!live) setPhase(i.phase);
        setInfo(i);
      })
      .catch(() => setInfo({ update: null, phase: { kind: "idle" } }));
  };
  useEffect(load, []);
  const install = () => {
    if (!info?.update) return;
    setPhase({ kind: "downloading", downloaded: 0, total: null });
    // success relaunches hop. A failed install already sent its phase; this covers one refused before it started (busy)
    api.install(info.update.version).catch((e) => {
      setPhase((p) => (p.kind === "failed" ? p : { kind: "failed", message: String(e) }));
      // the update this window shows is gone or replaced: retrying it would fail the same way, so show the current one
      if (api.STALE.includes(String(e))) load();
    });
  };
  return <WhatsNew info={info} phase={phase} onInstall={install} onClose={() => void api.close()} />;
}

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !e.repeat && !e.defaultPrevented) void api.close();
});

createRoot(document.getElementById("root")!).render(<App />);
