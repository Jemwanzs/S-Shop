import { Languages } from "lucide-react";
import { useSession } from "@/lib/session";
import { LANGUAGES } from "@/lib/i18n";
import { Select } from "@/components/Select";

/** Language choice before sign-in; once signed in, the user's preference decides. */
export function LanguagePicker() {
  const { language, setDeviceLanguage } = useSession();
  return (
    <div className="flex items-center gap-1 text-muted-foreground">
      <Languages className="h-4 w-4" />
      <Select
        value={language}
        onChange={setDeviceLanguage}
        label="Language"
        className="h-8 w-auto gap-1 border-transparent bg-transparent px-1.5 text-sm font-medium text-foreground hover:bg-accent"
      >
        {LANGUAGES.map((l) => <option key={l.code} value={l.code}>{l.label}</option>)}
      </Select>
    </div>
  );
}
