// Browser notifications for buys / sells / errors. Off by default; the user
// enables it (which requests permission). Only fires when the tab is hidden so
// it doesn't double up with the in-app toast.
const KEY = "notify.enabled";

export const Notify = {
  get enabled(): boolean {
    return localStorage.getItem(KEY) === "1" && Notification?.permission === "granted";
  },
  get supported(): boolean {
    return typeof Notification !== "undefined";
  },
  async enable(): Promise<boolean> {
    if (!Notify.supported) return false;
    let perm = Notification.permission;
    if (perm === "default") perm = await Notification.requestPermission();
    const ok = perm === "granted";
    localStorage.setItem(KEY, ok ? "1" : "0");
    return ok;
  },
  disable() {
    localStorage.setItem(KEY, "0");
  },
  show(title: string, body: string, tag?: string) {
    if (!Notify.enabled || !document.hidden) return;
    try {
      new Notification(title, { body, tag, silent: false });
    } catch {
      /* ignore */
    }
  },
};
