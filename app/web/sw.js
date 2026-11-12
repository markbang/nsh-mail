// nsh-mail's service worker: shows Web Push notifications of new mail while
// no page has the server's event stream open, and brings the app forward
// when one is pressed. `nsh package --target web` does not ship extra files:
// it is copied next to index.html (see app/README.md); the server serves it
// as /sw.js with `Service-Worker-Allowed: /`.

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

// The server's payload: {type: "newMail", account, id, from?, subject?};
// with "hide content" on, only the ids.
self.addEventListener("push", (event) => {
  let data = {};
  try {
    data = event.data ? event.data.json() : {};
  } catch {
    // Not ours to read: still say something arrived.
  }
  const title = data.from || "nsh-mail";
  event.waitUntil(
    self.registration.showNotification(title, {
      body: data.subject || "New mail",
      tag: data.id ? `message:${data.id}` : undefined,
      data,
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const app = new URL("./", self.registration.scope).href;
  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((windows) => {
      const open = windows.find((w) => w.url.startsWith(app));
      return open ? open.focus() : self.clients.openWindow(app);
    }),
  );
});
