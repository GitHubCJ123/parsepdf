import { AlertTriangle, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type ErrorBannerAction = {
  label: string;
  onClick: () => void;
};

type ErrorBannerProps = {
  severity: "warning" | "error";
  title: string;
  message: string;
  actions?: ErrorBannerAction[];
  dismissable?: boolean;
  details?: string;
  onDismiss?: () => void;
};

export function ErrorBanner({ severity, title, message, actions = [], dismissable, details, onDismiss }: ErrorBannerProps) {
  return (
    <section
      className={cn(
        "rounded-xl border p-4 shadow-lg shadow-black/10",
        severity === "error" ? "border-destructive/40 bg-destructive/10" : "border-amber-400/30 bg-amber-400/10",
      )}
      role="status"
    >
      <div className="flex items-start gap-3">
        <div className={cn("mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg", severity === "error" ? "bg-destructive/15 text-destructive" : "bg-amber-300/15 text-amber-200")}>
          <AlertTriangle className="size-4" />
        </div>
        <div className="min-w-0 flex-1 space-y-2">
          <div>
            <h3 className="text-sm font-medium text-foreground">{title}</h3>
            <p className="mt-1 text-sm leading-6 text-muted-foreground">{message}</p>
          </div>
          {details ? (
            <details className="rounded-lg border border-border/70 bg-background/40 px-3 py-2 text-xs text-muted-foreground">
              <summary className="cursor-pointer font-mono uppercase tracking-[0.16em]">Details</summary>
              <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-5">{details}</pre>
            </details>
          ) : null}
          {actions.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {actions.map((action) => (
                <Button key={action.label} type="button" size="sm" variant="outline" onClick={action.onClick}>
                  {action.label}
                </Button>
              ))}
            </div>
          ) : null}
        </div>
        {dismissable ? (
          <button type="button" className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-background/60 hover:text-foreground" onClick={onDismiss} aria-label="Dismiss">
            <X className="size-4" />
          </button>
        ) : null}
      </div>
    </section>
  );
}
