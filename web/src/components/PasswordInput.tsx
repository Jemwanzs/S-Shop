import { forwardRef, useState, type ComponentProps } from "react";
import { Eye, EyeOff, Lock } from "lucide-react";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";

/** PIN/password field with a show/hide eye. Use for every secret the user types. */
export const PasswordInput = forwardRef<HTMLInputElement, Omit<ComponentProps<"input">, "type"> & { withIcon?: boolean }>(
  ({ className, withIcon, ...props }, ref) => {
    const [show, setShow] = useState(false);
    return (
      <div className="relative">
        {withIcon && <Lock className="pointer-events-none absolute start-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />}
        <Input ref={ref} type={show ? "text" : "password"} className={cn("pe-10", withIcon && "ps-9", className)} {...props} />
        <button
          type="button"
          tabIndex={-1}
          onClick={() => setShow((s) => !s)}
          className="absolute end-1.5 top-1/2 -translate-y-1/2 rounded-md p-1.5 text-muted-foreground hover:text-foreground"
          aria-label={show ? "Hide" : "Show"}
        >
          {show ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
        </button>
      </div>
    );
  },
);
PasswordInput.displayName = "PasswordInput";
