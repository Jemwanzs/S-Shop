/**
 * Device location for geofencing. Watched only while the business restricts actions to the branch; the latest
 * reading travels with staff requests as `X-Location: lat,lng,accuracy` (the server decides and audits it).
 */
export type LocationStatus = "off" | "locating" | "ok" | "denied" | "unavailable";

let latest: { lat: number; lng: number; accuracy: number; at: number } | null = null;
let watchId: number | null = null;
let status: LocationStatus = "off";
const listeners = new Set<(s: LocationStatus) => void>();

/** Readings older than this are not sent (the user may have moved). */
const MAX_AGE_MS = 2 * 60_000;

function set(s: LocationStatus) {
  status = s;
  listeners.forEach((l) => l(s));
}

export function locationStatus() {
  return status;
}

export function onLocationStatus(l: (s: LocationStatus) => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function startLocation() {
  if (watchId !== null) return;
  if (!("geolocation" in navigator)) return set("unavailable");
  set("locating");
  watchId = navigator.geolocation.watchPosition(
    (p) => {
      latest = { lat: p.coords.latitude, lng: p.coords.longitude, accuracy: p.coords.accuracy, at: Date.now() };
      set("ok");
    },
    (e) => set(e.code === e.PERMISSION_DENIED ? "denied" : "unavailable"),
    { enableHighAccuracy: true, maximumAge: 30_000, timeout: 20_000 },
  );
}

export function stopLocation() {
  if (watchId !== null) navigator.geolocation.clearWatch(watchId);
  watchId = null;
  latest = null;
  set("off");
}

export function locationHeader(): string | null {
  if (!latest || Date.now() - latest.at > MAX_AGE_MS) return null;
  return `${latest.lat.toFixed(6)},${latest.lng.toFixed(6)},${Math.round(latest.accuracy)}`;
}

/** One fresh reading (e.g. "Use my current location" when setting up a branch). */
export function currentPosition(): Promise<GeolocationPosition> {
  return new Promise((resolve, reject) => {
    if (!("geolocation" in navigator)) return reject(new Error("Location is not available on this device"));
    navigator.geolocation.getCurrentPosition(resolve, (e) => reject(new Error(e.code === e.PERMISSION_DENIED ? "Location access was denied" : "Could not get your location")), {
      enableHighAccuracy: true,
      timeout: 20_000,
      maximumAge: 0,
    });
  });
}
