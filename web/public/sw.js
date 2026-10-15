// S'Shop service worker: lets the app open without a connection (offline POS).
// Caches only the app shell and its hashed assets — never API responses, so one user's data can never be served to
// another on a shared device. Business data for offline selling lives in IndexedDB (src/lib/offline.ts).
const SHELL = "sshop-shell-v2";
const ASSETS = "sshop-assets-v2";
// Roadmap 118: the release a page belongs to (its main script); a new release clears the previous build's files.
const release = (html) => (html.match(/assets\/main-[\w-]+\.js/) || [""])[0];
const SHELL_FILES = ["/", "/theme-init.js", "/manifest.webmanifest", "/icon-192.png", "/favicon-32.png"];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(SHELL).then((c) => c.addAll(SHELL_FILES)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== SHELL && k !== ASSETS).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin || url.pathname.startsWith("/api/")) return;

  // Hashed build files never change: cache first.
  if (url.pathname.startsWith("/assets/")) {
    event.respondWith(
      caches.open(ASSETS).then(async (c) => {
        const hit = await c.match(req);
        if (hit) return hit;
        const res = await fetch(req);
        if (res.ok) c.put(req, res.clone());
        return res;
      }),
    );
    return;
  }
  // Pages: network first (new releases), the cached shell when offline.
  if (req.mode === "navigate") {
    event.respondWith(
      fetch(req)
        .then((res) => {
          // Only the app page becomes the offline shell (never a PDF or other file opened directly).
          if (res.ok && (res.headers.get("content-type") || "").includes("text/html")) {
            const copy = res.clone();
            event.waitUntil(
              (async () => {
                const shell = await caches.open(SHELL);
                const before = await shell.match("/");
                const [was, now] = await Promise.all([before ? before.text() : "", copy.clone().text()]);
                if (release(now) && release(was) && release(now) !== release(was)) await caches.delete(ASSETS);
                await shell.put("/", copy);
              })().catch(() => undefined),
            );
          }
          return res;
        })
        .catch(() => caches.match("/")),
    );
    return;
  }
  if (SHELL_FILES.includes(url.pathname)) {
    event.respondWith(caches.match(req).then((hit) => hit || fetch(req)));
  }
});
