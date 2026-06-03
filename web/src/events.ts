// Live WebSocket feed proxied by the gateway at /events. Reconnects on drop.
import type { LiveEvent } from "./types";

export class EventStream {
  private ws: WebSocket | null = null;
  private shouldRun = false;
  private seq = 0;
  private attempt = 0;
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
    ws.onopen = () => {
      this.attempt = 0; // connection proven; reset backoff
      this.onState(true);
    };
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
      if (!this.shouldRun) return;
      // Exponential backoff (1s → 30s) with ±25% jitter so a downed server
      // isn't hammered at a fixed cadence.
      const base = Math.min(30, 2 ** this.attempt);
      const jitter = 0.75 + Math.random() * 0.5;
      const delay = Math.max(0.5, base * jitter) * 1000;
      this.attempt += 1;
      setTimeout(() => this.connect(), delay);
    };
    ws.onerror = () => ws.close();
  }
}
