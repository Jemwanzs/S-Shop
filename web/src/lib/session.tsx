import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api, session } from "./api";
import type { Branch, Profile } from "./types";

interface SessionValue {
  profile: Profile | null;
  loading: boolean;
  branch: Branch | null;
  /** True when the user operates multiple branches and has not chosen one yet. */
  needsBranch: boolean;
  can: (perm: string) => boolean;
  canAny: (...perms: string[]) => boolean;
  currency: string;
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

  const value = useMemo<SessionValue>(
    () => ({
      profile: token ? (profile ?? null) : null,
      loading: !!token && isLoading,
      branch,
      needsBranch: !!profile && !branch,
      can,
      canAny: (...perms) => perms.some(can),
      currency: profile?.tenant.currency ?? "KSh",
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
    [token, profile, isLoading, branch, can, qc],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useSession() {
  const v = useContext(Ctx);
  if (!v) throw new Error("useSession outside SessionProvider");
  return v;
}
