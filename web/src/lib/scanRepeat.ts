/** Roadmap 163: the camera counts the same barcode again only this long after it last accepted it. */
export const CAMERA_REPEAT_MS = 3000;

/**
 * Roadmap 163: each barcode has its own clock, started when the camera accepts it (never by a read it ignored), on a
 * monotonic clock so a change of the device's time cannot shorten or stretch it. `claim` decides and records in one
 * synchronous step, so two camera callbacks arriving together can never both count.
 */
export class RepeatGuard {
  private accepted = new Map<string, number>();
  private readonly ms: number;
  private readonly now: () => number;
  constructor(ms = CAMERA_REPEAT_MS, now: () => number = () => performance.now()) {
    this.ms = ms;
    this.now = now;
  }

  /** Accepts `code` if its last acceptance was at least `ms` ago; otherwise says how long is left. */
  claim(code: string): { ok: true } | { ok: false; remaining: number; until: number } {
    const now = this.now();
    for (const [c, at] of this.accepted) if (now - at >= this.ms) this.accepted.delete(c);
    const at = this.accepted.get(code);
    if (at !== undefined) return { ok: false, remaining: at + this.ms - now, until: at + this.ms };
    this.accepted.set(code, now);
    return { ok: true };
  }

  /** The scan was refused (an error): the same barcode may be tried again at once. */
  release(code: string) {
    this.accepted.delete(code);
  }

  clear() {
    this.accepted.clear();
  }
}
