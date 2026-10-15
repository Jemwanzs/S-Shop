import { useCallback, useEffect, useState, type SetStateAction } from "react";

export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => typeof window !== "undefined" && window.matchMedia(query).matches);
  useEffect(() => {
    const mq = window.matchMedia(query);
    const on = () => setMatches(mq.matches);
    on();
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, [query]);
  return matches;
}

/** md breakpoint and up = tablet/desktop layouts. */
export const useIsDesktop = () => useMediaQuery("(min-width: 768px)");

export function useDebounced<T>(value: T, ms = 250): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

export function usePersistentState<T>(key: string, initial: T, legacyKey?: string) {
  // Roadmap 118: a new key (another person, business or branch) reads its own value — never carries the previous one
  // over; `legacyKey` moves a value saved under an older key once.
  const read = (k: string): T => {
    try {
      let raw = localStorage.getItem(k);
      if (raw === null && legacyKey) {
        raw = localStorage.getItem(legacyKey);
        if (raw !== null) {
          localStorage.setItem(k, raw);
          localStorage.removeItem(legacyKey);
        }
      }
      return raw ? (JSON.parse(raw) as T) : initial;
    } catch {
      return initial;
    }
  };
  const [state, setState] = useState(() => ({ key, v: read(key) }));
  let current = state;
  if (state.key !== key) {
    current = { key, v: read(key) };
    setState(current);
  }
  useEffect(() => {
    try {
      localStorage.setItem(current.key, JSON.stringify(current.v));
    } catch {
      /* storage unavailable */
    }
  }, [current.key, current.v]);
  const set = useCallback((u: SetStateAction<T>) => setState((s) => ({ key: s.key, v: typeof u === "function" ? (u as (prev: T) => T)(s.v) : u })), []);
  return [current.v, set] as const;
}
