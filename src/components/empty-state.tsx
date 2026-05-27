import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type EmptyStateProps = {
  icon: LucideIcon;
  title: string;
  description: string;
  actionLabel: string;
  onAction?: () => void;
  className?: string;
};

export function EmptyState({
  icon: Icon,
  title,
  description,
  actionLabel,
  onAction,
  className,
}: EmptyStateProps) {
  return (
    <section
      className={cn(
        "flex min-h-[calc(100vh-7rem)] items-center justify-center",
        className,
      )}
    >
      <div className="w-full max-w-md rounded-lg border border-border bg-card/70 p-8 text-center backdrop-blur-xl">
        <div className="mx-auto mb-5 flex size-10 items-center justify-center rounded-md border border-border bg-secondary/50 text-muted-foreground">
          <Icon className="size-4" />
        </div>
        <h1 className="text-base font-medium tracking-[-0.02em] text-card-foreground">
          {title}
        </h1>
        <p className="mx-auto mt-2 max-w-sm text-sm leading-6 text-muted-foreground">
          {description}
        </p>
        <Button type="button" className="mt-6 rounded-md" onClick={onAction}>
          {actionLabel}
        </Button>
      </div>
    </section>
  );
}
