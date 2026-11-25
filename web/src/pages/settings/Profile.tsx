/** Roadmap 185: My profile — each person's own name and profile photo (upload, crop and reposition, replace, remove),
 * with their email, role and branches shown read-only, and the way to their PIN, Face ID / passkeys, Quick PIN and
 * preferences (the existing screens — nothing duplicated). */
import { useEffect, useRef, useState, type PointerEvent as RPointerEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Camera, ChevronRight, Fingerprint, KeyRound, Pencil, SlidersHorizontal, Trash2 } from "lucide-react";
import { Link, useSearchParams } from "@/lib/router";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { toast } from "@/lib/toast";
import { t } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/Form";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { UserAvatar } from "@/components/UserAvatar";
import { ChangePin } from "@/components/layout/AppShell";
import { Card, SettingsPage } from "./shared";

interface MyProfile { name: string; email: string; role: string; all_branches: boolean; branches: string[]; avatar_url: string | null }

const VIEW = 240;
const OUT = 512;
const tidy = (s: string) => s.trim().replace(/\s+/g, " ");
const nameProblem = (s: string) => {
  const n = tidy(s);
  if (n.length < 2 || n.length > 80) return "Enter your full name (2–80 characters)";
  if (/[<>]/.test(n)) return "Your name contains characters that are not allowed";
  return null;
};

export function ProfileSettings() {
  const { profile } = useSession();
  const [params, setParams] = useSearchParams();
  const [pinOpen, setPinOpen] = useState(false);
  const q = useQuery({ queryKey: ["my-profile"], queryFn: () => api<MyProfile>("/auth/profile") });
  const editing = params.get("edit") === "1";
  const own = !profile?.acting;
  const p = q.data;
  return (
    <SettingsPage title="My profile" description="Your name and photo as your colleagues see them. Your email, role and branches are set by your administrator." loading={q.isLoading}>
      {p && (
        <>
          <Card>
            <div className="flex flex-col items-center gap-3 py-2 text-center sm:flex-row sm:text-start">
              <UserAvatar name={p.name} url={p.avatar_url} size="xl" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-lg font-semibold">{p.name}</p>
                <p className="truncate text-sm text-muted-foreground">{p.email}</p>
                <p className="text-sm text-muted-foreground">{p.role} · {p.all_branches ? t("All branches") : p.branches.join(", ") || "—"}</p>
              </div>
              {own && <Button onClick={() => setParams({ edit: "1" })}><Pencil /> {t("Edit profile")}</Button>}
            </div>
          </Card>
          <Card title="Sign-in & security">
            <div className="divide-y">
              <button className="flex w-full items-center gap-3 py-3 text-start text-sm" onClick={() => setPinOpen(true)} disabled={!own}>
                <KeyRound className="h-4 w-4 text-primary" /><span className="flex-1">{t("Change PIN")}</span><ChevronRight className="h-4 w-4 text-muted-foreground" />
              </button>
              <Link to="/settings/preferences#sign-in" className="flex items-center gap-3 py-3 text-sm">
                <Fingerprint className="h-4 w-4 text-primary" /><span className="flex-1">{t("Face ID, fingerprint, passkeys & Quick PIN")}</span><ChevronRight className="h-4 w-4 text-muted-foreground" />
              </Link>
              <Link to="/settings/preferences" className="flex items-center gap-3 py-3 text-sm">
                <SlidersHorizontal className="h-4 w-4 text-primary" /><span className="flex-1">{t("User preferences")}</span><ChevronRight className="h-4 w-4 text-muted-foreground" />
              </Link>
            </div>
          </Card>
          <EditProfile open={editing && own} onClose={() => setParams({})} me={p} />
          <ChangePin open={pinOpen} onOpenChange={setPinOpen} />
        </>
      )}
    </SettingsPage>
  );
}

/** Name and photo. Save is offered only for a valid change; the photo is cropped to a square in the browser. */
function EditProfile({ open, onClose, me }: { open: boolean; onClose: () => void; me: MyProfile }) {
  const qc = useQueryClient();
  const [name, setName] = useState(me.name);
  const [photo, setPhoto] = useState<{ blob: Blob; url: string } | null>(null);
  const [remove, setRemove] = useState(false);
  const [source, setSource] = useState<string | null>(null);
  const file = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open) return;
    setName(me.name);
    setPhoto(null);
    setRemove(false);
    setSource(null);
  }, [open, me.name]);
  useEffect(() => () => { if (photo) URL.revokeObjectURL(photo.url); }, [photo]);
  const problem = nameProblem(name);
  const nameChanged = tidy(name) !== me.name;
  const dirty = nameChanged || !!photo || (remove && !!me.avatar_url);
  const shown = photo?.url ?? (remove ? null : me.avatar_url);

  const pick = (f: File | undefined) => {
    if (!f) return;
    if (!/^image\//.test(f.type)) return toast.error("Choose a photo (PNG, JPEG or WebP)");
    if (f.size > 15 * 1024 * 1024) return toast.error("This photo is too large — choose one under 15 MB");
    setSource(URL.createObjectURL(f));
  };
  const save = async () => {
    if (nameChanged) await api("/auth/profile", { method: "PUT", body: { name: tidy(name) } });
    if (photo) {
      const fd = new FormData();
      fd.append("file", photo.blob, photo.blob.type === "image/webp" ? "avatar.webp" : "avatar.jpg");
      await api("/auth/profile/avatar", { body: fd });
    } else if (remove && me.avatar_url) {
      await api("/auth/profile/avatar", { method: "DELETE" });
    }
    toast.success("Profile saved");
    await Promise.all([qc.invalidateQueries({ queryKey: ["me"] }), qc.invalidateQueries({ queryKey: ["my-profile"] }), qc.invalidateQueries({ queryKey: ["users"] }), qc.invalidateQueries({ queryKey: ["leaderboard"] })]);
    onClose();
  };

  return (
    <ResponsiveDialog open={open} onOpenChange={(o) => !o && onClose()} title="Edit profile"
      footer={source ? undefined : <ActionButton className="w-full md:w-auto" online blockedBy={[!dirty && "No changes to save", problem]} onAction={save}>{t("Save changes")}</ActionButton>}>
      {source ? (
        <Cropper src={source} onCancel={() => { URL.revokeObjectURL(source); setSource(null); }}
          onDone={(blob) => { URL.revokeObjectURL(source); setSource(null); setRemove(false); setPhoto({ blob, url: URL.createObjectURL(blob) }); }} />
      ) : (
        <div className="space-y-4">
          <div className="flex flex-col items-center gap-3">
            <UserAvatar name={tidy(name) || me.name} url={shown} size="xl" />
            <div className="flex flex-wrap justify-center gap-2">
              <Button size="sm" variant="outline" onClick={() => file.current?.click()}><Camera /> {t(shown ? "Replace photo" : "Upload photo")}</Button>
              {shown && <Button size="sm" variant="ghost" className="text-destructive" onClick={() => { setPhoto(null); setRemove(true); }}><Trash2 /> {t("Remove photo")}</Button>}
            </div>
            <input ref={file} type="file" accept="image/png,image/jpeg,image/webp,image/*" hidden onChange={(e) => { pick(e.target.files?.[0]); e.target.value = ""; }} aria-label={t("Profile photo")} />
          </div>
          <Field label="Full name" hint={name && problem ? t(problem) : undefined}><Input value={name} onChange={(e) => setName(e.target.value)} maxLength={80} autoComplete="name" /></Field>
          <Field label="Email" hint="Your sign-in — only an administrator can change it"><Input value={me.email} readOnly disabled /></Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label="Role"><Input value={me.role} readOnly disabled /></Field>
            <Field label="Branches"><Input value={me.all_branches ? t("All branches") : me.branches.join(", ")} readOnly disabled /></Field>
          </div>
        </div>
      )}
    </ResponsiveDialog>
  );
}

/** Square crop: drag to reposition, zoom with the slider; exported at 512 × 512. */
function Cropper({ src, onCancel, onDone }: { src: string; onCancel: () => void; onDone: (b: Blob) => void }) {
  const [img, setImg] = useState<HTMLImageElement | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pos, setPos] = useState({ x: 0, y: 0 });
  const drag = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  useEffect(() => {
    const i = new Image();
    i.onload = () => {
      setImg(i);
      const base = Math.max(VIEW / i.naturalWidth, VIEW / i.naturalHeight);
      setPos({ x: (VIEW - i.naturalWidth * base) / 2, y: (VIEW - i.naturalHeight * base) / 2 });
    };
    i.onerror = () => { toast.error("This file is not an image that can be read"); onCancel(); };
    i.src = src;
  }, [src, onCancel]);
  if (!img) return <div className="h-60" />;
  const base = Math.max(VIEW / img.naturalWidth, VIEW / img.naturalHeight);
  const w = img.naturalWidth * base * zoom, h = img.naturalHeight * base * zoom;
  const clamp = (x: number, y: number, ww = w, hh = h) => ({ x: Math.min(0, Math.max(VIEW - ww, x)), y: Math.min(0, Math.max(VIEW - hh, y)) });
  const setZoomKeepingCentre = (z: number) => {
    const nw = img.naturalWidth * base * z, nh = img.naturalHeight * base * z;
    const cx = (VIEW / 2 - pos.x) / w, cy = (VIEW / 2 - pos.y) / h;
    setZoom(z);
    setPos(clamp(VIEW / 2 - cx * nw, VIEW / 2 - cy * nh, nw, nh));
  };
  const down = (e: RPointerEvent) => { (e.target as Element).setPointerCapture(e.pointerId); drag.current = { x: pos.x, y: pos.y, px: e.clientX, py: e.clientY }; };
  const move = (e: RPointerEvent) => { const d = drag.current; if (d) setPos(clamp(d.x + e.clientX - d.px, d.y + e.clientY - d.py)); };
  const done = () => {
    const c = document.createElement("canvas");
    c.width = OUT; c.height = OUT;
    const f = OUT / VIEW;
    const g = c.getContext("2d")!;
    g.imageSmoothingQuality = "high";
    g.drawImage(img, pos.x * f, pos.y * f, w * f, h * f);
    c.toBlob((b) => {
      if (b && b.type === "image/webp") return onDone(b);
      c.toBlob((j) => (j ? onDone(j) : toast.error("The photo could not be prepared")), "image/jpeg", 0.9);
    }, "image/webp", 0.9);
  };
  return (
    <div className="flex flex-col items-center gap-4">
      <p className="text-sm text-muted-foreground">{t("Drag to reposition, zoom to fit.")}</p>
      <div className="relative touch-none overflow-hidden rounded-xl bg-muted" style={{ width: VIEW, height: VIEW }}
        onPointerDown={down} onPointerMove={move} onPointerUp={() => (drag.current = null)} onPointerCancel={() => (drag.current = null)} data-testid="avatar-cropper">
        <img src={src} alt="" draggable={false} className="pointer-events-none absolute max-w-none select-none" style={{ left: pos.x, top: pos.y, width: w, height: h }} />
        <div className="pointer-events-none absolute inset-0 rounded-full shadow-[0_0_0_999px_rgba(0,0,0,0.45)]" />
      </div>
      <input type="range" min={1} max={3} step={0.01} value={zoom} onChange={(e) => setZoomKeepingCentre(Number(e.target.value))} className="w-60 accent-primary" aria-label={t("Zoom")} />
      <div className="flex w-full gap-2">
        <Button variant="outline" className="flex-1" onClick={onCancel}>{t("Cancel")}</Button>
        <Button className="flex-1" onClick={done}>{t("Use photo")}</Button>
      </div>
    </div>
  );
}
