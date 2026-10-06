import { Navigate, useNavigate } from "react-router-dom";
import { ChevronRight, LogOut, MapPin, Store } from "lucide-react";
import { useSession } from "@/lib/session";
import { Button } from "@/components/ui/button";
import { Loading } from "@/components/Page";
import { t } from "@/lib/i18n";

/** Users assigned to several branches choose their Current Branch after signing in. */
export default function SelectBranchPage() {
  const { profile, loading, selectBranch, signOut, branch } = useSession();
  const navigate = useNavigate();
  if (loading) return <Loading className="min-h-screen" />;
  if (!profile) return <Navigate to="/login" replace />;
  return (
    <div className="mx-auto flex min-h-screen max-w-xl flex-col justify-center px-5 py-10">
      <div className="mb-6 animate-fade-up">
        <p className="label-caps">{t("Hi")} {profile.user.name.split(" ")[0]} 👋</p>
        <h1 className="mt-1 text-2xl font-semibold">{t("Where are you working today?")}</h1>
        <p className="mt-1 text-sm text-muted-foreground">{t("Sales, stock and expenses will default to this branch. You can switch any time.")}</p>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        {profile.branches.map((b) => (
          <button
            key={b.id}
            onClick={() => {
              selectBranch(b.id);
              navigate("/", { replace: true });
            }}
            className={`surface flex items-center gap-3 p-4 text-start transition hover:border-primary/50 hover:shadow-lift animate-fade-up ${branch?.id === b.id ? "border-primary" : ""}`}
          >
            <span className="rounded-xl bg-primary/10 p-2.5 text-primary"><Store className="h-5 w-5" /></span>
            <span className="min-w-0 flex-1">
              <span className="block truncate font-semibold">{b.name}</span>
              <span className="flex items-center gap-1 truncate text-xs text-muted-foreground">
                <MapPin className="h-3 w-3" /> {b.location || b.code}
              </span>
            </span>
            <ChevronRight className="h-4 w-4 text-muted-foreground" />
          </button>
        ))}
      </div>
      <Button variant="ghost" className="mt-8 self-center text-muted-foreground" onClick={signOut}>
        <LogOut /> {t("Sign out")}
      </Button>
    </div>
  );
}
