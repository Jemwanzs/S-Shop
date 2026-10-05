import { createContext, Fragment, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api, session } from "./api";
import type { Branch, Profile } from "./types";
import { setDisplayCurrency } from "./format";
import { applyFont, DEFAULT_PREFERENCES, type Fx, type Preferences } from "./prefs";

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
  signIn: (token: string, profile: Profile) => void;
  signOut: () => void;
  selectBranch: (id: string) => void;
}

const Ctx = createContext<SessionValue | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const [token, setToken] = useState(session.token);
  const [branchId, setBranchId] = useState(session.branchId);

  const { data: profile, isLoading } = useQuery({
    queryKey: ["me", token],
    queryFn: () => api<Profile>("/auth/me"),
    enabled: !!token,
    staleTime: 5 * 60_000,
  });

  useEffect(() => {
    const onUnauthorized = () => {
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

  const can = useCallback((perm: string) => !!profile?.permissions.some((p) => p === "*" || p === perm), [profile]);

  // Preferences: font now; figures convert once exchange rates arrive (refreshed when the app opens).
  const signedIn = !!token && !!profile;
  const preferences = (signedIn && profile.user.preferences) || DEFAULT_PREFERENCES;
  useEffect(() => applyFont(preferences.font), [preferences.font]);
  const wantsFx = signedIn && preferences.currency !== "KES";
  const fx = useQuery({ queryKey: ["fx"], queryFn: () => api<Fx>("/fx"), enabled: wantsFx, staleTime: 60 * 60_000, refetchOnWindowFocus: false });
  const rate = wantsFx ? fx.data?.rates[preferences.currency] : undefined;
  const displayCode = rate ? preferences.currency : "KES";
  setDisplayCurrency(displayCode, rate ?? 1);

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
      signIn: (t, p) => {
        session.setToken(t);
        qc.setQueryData(["me", t], p);
        setToken(t);
        if (p.branches.length === 1) {
          session.setBranch(p.branches[0].id);
          setBranchId(p.branches[0].id);
        }
      },
      signOut: () => {
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
    [token, profile, isLoading, branch, can, qc, preferences, displayCode, fx.data],
  );

  // Re-render every screen when the display currency changes (figures are formatted outside React state).
  return <Ctx.Provider value={value}><Fragment key={displayCode}>{children}</Fragment></Ctx.Provider>;
}

export function useSession() {
  const v = useContext(Ctx);
  if (!v) throw new Error("useSession outside SessionProvider");
  return v;
}
