import { Languages } from "lucide-react";
import { useSession } from "@/lib/session";
import { LANGUAGES } from "@/lib/i18n";

/** Language choice before sign-in; once signed in, the user's preference decides. */
export function LanguagePicker() {
  const { language, setDeviceLanguage } = useSession();
  return (
    <label className="relative flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-sm text-muted-foreground hover:bg-accent">
      <Languages className="h-4 w-4" />
      <select
        value={language}
        onChange={(e) => setDeviceLanguage(e.target.value)}
        className="cursor-pointer appearance-none bg-transparent pe-1 text-sm font-medium text-foreground focus:outline-none"
        aria-label="Language"
      >
        {LANGUAGES.map((l) => <option key={l.code} value={l.code}>{l.label}</option>)}
      </select>
    </label>
  );
}
