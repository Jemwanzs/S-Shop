/** Entry for businesses' public websites (site.html). The server embeds the published website in #site-data. */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./site.css";
import type { SiteData } from "./types";
import { basePath, call, cartStore, configureAnalytics, runtime } from "./lib";
import SiteApp, { Unavailable } from "./App";

function embedded(): SiteData | null {
  try {
    return JSON.parse(document.getElementById("site-data")?.textContent ?? "null");
  } catch {
    return null;
  }
}

/** /s/{slug}/… on the S'Shop host, or the business's own domain (the server already chose the business). */
function slugFromPath(): string {
  const m = /^\/s\/([^/]+)/.exec(location.pathname);
  return m ? decodeURIComponent(m[1]) : "";
}

async function boot() {
  const root = createRoot(document.getElementById("site-root")!);
  let data = embedded();
  runtime.slug = data?.slug || slugFromPath();

  // Preview from the Website Management Centre: the draft, read with the staff member's own session (same origin).
  const params = new URLSearchParams(location.search);
  if (params.get("preview") === "1" || params.get("preview") === "true") {
    let token: string | null = null;
    try {
      token = localStorage.getItem("sshop.token");
    } catch { /* no session */ }
    if (token) {
      runtime.preview = true;
      runtime.previewToken = token;
      try {
        data = await call<SiteData>("/site");
      } catch {
        runtime.preview = false;
        runtime.previewToken = null;
      }
    }
  }

  if (!data || !data.available) {
    root.render(<Unavailable data={{ business: data?.business ?? { name: "Website", currency: "", logo_url: null } }} />);
    return;
  }
  configureAnalytics(data);
  cartStore.load();
  root.render(
    <StrictMode>
      <SiteApp data={data} base={basePath(runtime.slug)} />
    </StrictMode>,
  );
}

boot();
