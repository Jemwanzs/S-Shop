import { createContext, Fragment, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiError, session } from "./api";
import type { Branch, Profile } from "./types";
import { setDisplayCurrency } from "./format";
import { applyFont, DEFAULT_PREFERENCES, type Fx, type Preferences } from "./prefs";
import { deviceLanguage, rememberDeviceLanguage, setLanguage } from "./i18n";

interface SessionValue {
  profile: Profile | null;
  loading: boolean;
  branch: Branch | null;
  /** True when the user operates multiple branches and has not chosen one yet. */
  needsBranch: boolean;
  can: (perm: string) => boolean;
  canAny: (...perms: string[]) => boolean;
  currency: string;
  preferences: Preferences;
  /** Currency figures are shown in (KES until rates load for another choice). */
  displayCurrency: string;
  fx: Fx | null;
  language: string;
  /** Language choice on the public screens before sign-in. */
  setDeviceLanguage: (code: string) => void;
  signIn: (token: string, profile: Profile) => void;
  /** After a PIN change the server ends older sessions and hands back a fresh token for this device. */
  renewToken: (token: string) => Promise<void>;
  /** Platform admins: continue in another business (all cached data and the branch choice are dropped). */
  switchBusiness: (token: string, profile: Profile) => void;
  signOut: () => void;
  selectBranch: (id: string) => void;
}

const Ctx = createContext<SessionValue | null>(null);

const PROFILE_KEY = "sshop.profile";
function rememberProfile(token: string | null, profile: Profile) {
  try {
    if (token) localStorage.setItem(PROFILE_KEY, JSON.stringify({ token, profile }));
  } catch {
    /* storage unavailable: offline start simply needs a connection */
  }
}
function cachedProfile(token: string | null): Profile | null {
  try {
    const saved = JSON.parse(localStorage.getItem(PROFILE_KEY) ?? "null") as { token: string; profile: Profile } | null;
    return saved && token && saved.token === token ? saved.profile : null;
  } catch {
    return null;
  }
}
function forgetProfile() {
  try {
    localStorage.removeItem(PROFILE_KEY);
  } catch {
    /* nothing stored */
  }
}

export function SessionProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const [token, setToken] = useState(session.token);
  const [branchId, setBranchId] = useState(session.branchId);

  // The last profile for this sign-in is kept on the device, so the app (offline POS) still opens without a
  // connection. It is used only when the network is down — a real sign-out or expired session clears it.
  const { data: profile, isLoading } = useQuery({
    queryKey: ["me", token],
    queryFn: async () => {
      try {
        const p = await api<Profile>("/auth/me");
        rememberProfile(token, p);
        return p;
      } catch (e) {
        const cached = e instanceof ApiError && e.code === "network" ? cachedProfile(token) : null;
        if (cached) return cached;
        throw e;
      }
    },
    enabled: !!token,
    staleTime: 5 * 60_000,
  });

  useEffect(() => {
    const onUnauthorized = () => {
      forgetProfile();
      setToken(null);
      qc.clear();
    };
    window.addEventListener("sshop:unauthorized", onUnauthorized);
    return () => window.removeEventListener("sshop:unauthorized", onUnauthorized);
  }, [qc]);

  const branches = useMemo(() => profile?.branches ?? [], [profile]);
  const branch = branches.find((b) => b.id === branchId) ?? (branches.length === 1 ? branches[0] : null);

  // A single-branch user is always on that branch; a stale stored branch is cleared.
  useEffect(() => {
    if (!profile) return;
    if (branches.length === 1 && branchId !== branches[0].id) {
      session.setBranch(branches[0].id);
      setBranchId(branches[0].id);
    } else if (branchId && !branches.some((b) => b.id === branchId)) {
      session.setBranch(null);
      setBranchId(null);
    }
  }, [profile, branches, branchId]);

  // A permission that belongs to a module outside the business's package is treated as not held, so every screen,
  // menu and button guarded by `can` follows the package (the server refuses those modules as well).
  const can = useCallback(
    (perm: string) => {
      if (!profile) return false;
      const b = profile.billing;
      if (b?.modules) {
        const owner = b.catalogue.find((m) => m.perms.some((p) => perm === p || (p.endsWith(".") && perm.startsWith(p))));
        if (owner && !b.modules.includes(owner.key)) return false;
      }
      return profile.permissions.some((p) => p === "*" || p === perm || (p === "settings.manage" && perm.startsWith("settings.")));
    },
    [profile],
  );

  // Preferences: font now; figures convert once exchange rates arrive (refreshed when the app opens).
  const signedIn = !!token && !!profile;
  const preferences = (signedIn && profile.user.preferences) || DEFAULT_PREFERENCES;
  useEffect(() => applyFont(preferences.font), [preferences.font]);
  const wantsFx = signedIn && preferences.currency !== "KES";
  const fx = useQuery({ queryKey: ["fx"], queryFn: () => api<Fx>("/fx"), enabled: wantsFx, staleTime: 60 * 60_000, refetchOnWindowFocus: false });
  const rate = wantsFx ? fx.data?.rates[preferences.currency] : undefined;
  const displayCode = rate ? preferences.currency : "KES";
  setDisplayCurrency(displayCode, rate ?? 1);
  const [deviceLang, setDeviceLang] = useState(deviceLanguage);
  const language = signedIn ? preferences.language : deviceLang;
  setLanguage(language);
  useEffect(() => {
    if (signedIn) rememberDeviceLanguage(preferences.language);
  }, [signedIn, preferences.language]);

  const value = useMemo<SessionValue>(
    () => ({
      profile: token ? (profile ?? null) : null,
      loading: !!token && isLoading,
      branch,
      needsBranch: !!profile && !branch,
      can,
      canAny: (...perms) => perms.some(can),
      currency: profile?.tenant.currency ?? "KSh",
      preferences,
      displayCurrency: displayCode,
      fx: fx.data ?? null,
      language,
      setDeviceLanguage: (code: string) => {
        rememberDeviceLanguage(code);
        setDeviceLang(code);
      },
      signIn: (t, p) => {
        session.setToken(t);
        qc.setQueryData(["me", t], p);
        setToken(t);
        // One branch, or the user's default branch (roadmap 64), becomes the Current Branch straight away.
        const preset = p.branches.length === 1 ? p.branches[0].id : p.branches.find((b) => b.id === p.user.default_branch_id)?.id;
        if (preset) {
          session.setBranch(preset);
          setBranchId(preset);
        }
      },
      renewToken: async (t) => {
        const p = await api<Profile>("/auth/me", { token: t });
        session.setToken(t);
        qc.setQueryData(["me", t], p);
        setToken(t);
      },
      switchBusiness: (t, p) => {
        qc.clear();
        session.setBranch(null);
        setBranchId(null);
        session.setToken(t);
        qc.setQueryData(["me", t], p);
        setToken(t);
        if (p.branches.length === 1) {
          session.setBranch(p.branches[0].id);
          setBranchId(p.branches[0].id);
        }
      },
      signOut: () => {
        forgetProfile();
        session.setToken(null);
        setToken(null);
        qc.clear();
      },
      selectBranch: (id) => {
        session.setBranch(id);
        setBranchId(id);
        // Every list is branch-scoped: refetch everything for the new Current Branch.
        qc.invalidateQueries({ predicate: (q) => q.queryKey[0] !== "me" });
      },
    }),
    [token, profile, isLoading, branch, can, qc, preferences, displayCode, fx.data, language],
  );

  // Re-render every screen when the display currency or language changes (both are applied outside React state).
  return <Ctx.Provider value={value}><Fragment key={`${displayCode}-${language}`}>{children}</Fragment></Ctx.Provider>;
}

export function useSession() {
  const v = useContext(Ctx);
  if (!v) throw new Error("useSession outside SessionProvider");
  return v;
}
