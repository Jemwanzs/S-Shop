import { Component, type ErrorInfo, type ReactNode } from "react";
import { RefreshCw, TriangleAlert } from "lucide-react";
import { Button } from "@/components/ui/button";

/** Contains a crash to the page that caused it, so navigation keeps working. Re-key it to reset. */
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Page crashed", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="surface mx-auto mt-6 max-w-md space-y-4 p-6 text-center">
        <span className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-destructive/10 text-destructive">
          <TriangleAlert className="h-6 w-6" />
        </span>
        <div>
          <p className="font-semibold">This page could not be shown</p>
          <p className="mt-1 text-sm text-muted-foreground">Your data is safe. Reload to try again, or use the menu to go elsewhere.</p>
        </div>
        <Button onClick={() => window.location.reload()}><RefreshCw /> Reload</Button>
      </div>
    );
  }
}
