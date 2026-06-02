// Live WebSocket feed proxied by the gateway at /events. Reconnects on drop.
import type { LiveEvent } from "./types";

export class EventStream {
  private ws: WebSocket | null = null;
  private shouldRun = false;
  private seq = 0;
  onEvent: (e: LiveEvent) => void = () => {};
  onState: (connected: boolean) => void = () => {};

  start() {
    this.shouldRun = true;
    this.connect();
  }

  stop() {
    this.shouldRun = false;
    this.ws?.close();
    this.ws = null;
    this.onState(false);
  }

  private connect() {
    if (!this.shouldRun) return;
    const proto = location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(`${proto}://${location.host}/events`);
    this.ws = ws;
    ws.onopen = () => this.onState(true);
    ws.onmessage = (ev) => {
      try {
        const raw = JSON.parse(ev.data as string);
        this.onEvent({
          id: this.seq++,
          type: raw.type || "unknown",
          ts: Number(raw.ts || 0),
          raw,
        });
      } catch {
        /* ignore malformed frames */
      }
    };
    ws.onclose = () => {
      this.onState(false);
      if (this.shouldRun) setTimeout(() => this.connect(), 1500);
    };
    ws.onerror = () => ws.close();
  }
}
