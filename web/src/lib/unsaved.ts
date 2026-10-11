/** Unsaved work on screen (roadmap 87): forms register while they have changes, so the inactivity warning can say
 * that they will be lost. Nothing is ever submitted automatically, and unsaved work never extends a session. */
import { useEffect } from "react";

const dirty = new Set<string>();

export function hasUnsaved() {
  return dirty.size > 0;
}

/** Marks `key` as having unsaved changes while `isDirty` is true. */
export function useUnsaved(key: string, isDirty: boolean | undefined) {
  useEffect(() => {
    if (isDirty) dirty.add(key);
    else dirty.delete(key);
    return () => {
      dirty.delete(key);
    };
  }, [key, isDirty]);
}
