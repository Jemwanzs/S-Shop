import { useState } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft, CheckCircle2, Loader2, Phone } from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { NativeSelect } from "@/components/Form";
import { AuthLabel, AuthLayout } from "./AuthLayout";

const BUSINESS_TYPES = ["Retail shop", "Supermarket / mini-mart", "Fashion & boutique", "Jewellery & accessories", "Electronics", "Pharmacy & beauty", "Hardware", "Restaurant / café", "Wholesale", "Other"];
const SUPPORT = ["0798 993 404", "0732 968 898"];

/** Public form: businesses ask for access; a platform admin reviews and activates. No open signup. */
export default function RequestAccessPage() {
  const [f, setF] = useState({ business_name: "", contact_name: "", phone: "", email: "", location: "", business_type: "", branches: "", message: "", website: "" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [done, setDone] = useState(false);
  const set = (k: keyof typeof f) => (v: string) => setF((x) => ({ ...x, [k]: v }));

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      await api("/access-requests", { body: { ...f, branches: f.branches ? Number(f.branches) : null } });
      setDone(true);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      <AuthLayout title="Request received">
        <div className="space-y-4 text-center">
          <CheckCircle2 className="mx-auto h-10 w-10 text-success" />
          <p className="text-sm font-medium">Access request submitted successfully. We will review your request and get back to you.</p>
          <div className="rounded-xl bg-muted/60 p-3">
            <p className="text-xs text-muted-foreground">In case of any delays, please call:</p>
            <div className="mt-1.5 flex flex-col items-center gap-1">
              {SUPPORT.map((p) => (
                <a key={p} href={`tel:${p.replace(/\s/g, "")}`} className="num inline-flex items-center gap-1.5 text-sm font-semibold text-primary"><Phone className="h-3.5 w-3.5" />{p}</a>
              ))}
            </div>
          </div>
          <Button variant="outline" className="w-full" asChild><Link to="/login">Back to sign in</Link></Button>
        </div>
      </AuthLayout>
    );
  }

  return (
    <AuthLayout title="Request Access" subtitle="Tell us about your business — we'll set you up">
      <form onSubmit={submit} className="space-y-3.5">
        <label className="block"><AuthLabel>Business name</AuthLabel><Input value={f.business_name} onChange={(e) => set("business_name")(e.target.value)} required maxLength={120} /></label>
        <label className="block"><AuthLabel>Your name</AuthLabel><Input autoComplete="name" value={f.contact_name} onChange={(e) => set("contact_name")(e.target.value)} required maxLength={120} /></label>
        <div className="grid grid-cols-2 gap-2.5">
          <label className="block"><AuthLabel>Phone</AuthLabel><Input type="tel" autoComplete="tel" className="num" value={f.phone} onChange={(e) => set("phone")(e.target.value)} placeholder="07…" required /></label>
          <label className="block"><AuthLabel>Branches</AuthLabel><Input inputMode="numeric" className="num" value={f.branches} onChange={(e) => set("branches")(e.target.value.replace(/\D/g, "").slice(0, 3))} placeholder="1" /></label>
        </div>
        <label className="block"><AuthLabel>Email</AuthLabel><Input type="email" autoComplete="email" value={f.email} onChange={(e) => set("email")(e.target.value)} placeholder="your@email.com" required /></label>
        <label className="block">
          <AuthLabel>Type of business</AuthLabel>
          <NativeSelect value={f.business_type} onChange={set("business_type")}>
            <option value="">Choose…</option>
            {BUSINESS_TYPES.map((t) => <option key={t}>{t}</option>)}
          </NativeSelect>
        </label>
        <label className="block"><AuthLabel>Town / location</AuthLabel><Input value={f.location} onChange={(e) => set("location")(e.target.value)} maxLength={120} /></label>
        <label className="block"><AuthLabel>Anything else? (optional)</AuthLabel><Textarea rows={3} value={f.message} onChange={(e) => set("message")(e.target.value)} maxLength={1000} /></label>
        {/* Honeypot for bots: hidden from people and assistive tech. */}
        <input type="text" name="website" tabIndex={-1} autoComplete="off" aria-hidden value={f.website} onChange={(e) => set("website")(e.target.value)} className="absolute -left-[9999px] h-0 w-0 opacity-0" />
        {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>}
        <Button type="submit" className="w-full" disabled={busy}>{busy ? <Loader2 className="animate-spin" /> : "Submit request"}</Button>
        <Link to="/login" className="flex items-center justify-center gap-1.5 text-xs text-muted-foreground hover:text-foreground"><ArrowLeft className="h-3.5 w-3.5" /> Back to sign in</Link>
      </form>
    </AuthLayout>
  );
}
