
import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type EmptyStateProps = {
  icon: LucideIcon;
  title: string;
  description: string;
  actionLabel?: string;
  onAction?: () => void;
  children?: ReactNode;
  className?: string;
};

export function EmptyState({
  icon: Icon,
  title,
  description,
  actionLabel,
  onAction,
  children,
  className,
}: EmptyStateProps) {
  return (
    <section className={cn("grid min-h-72 place-items-center px-4 py-12 text-center", className)}>
      <div className="w-full max-w-md">
        <Icon className="mx-auto size-8 text-muted-foreground" aria-hidden="true" strokeWidth={1.6} />
        <h2 className="mt-5 text-lg font-semibold tracking-[-0.04em] text-card-foreground">{title}</h2>
        <p className="mx-auto mt-2 max-w-sm text-sm leading-6 text-muted-foreground">{description}</p>
        {actionLabel ? (
          <Button type="button" className="mt-6 rounded-md" onClick={onAction}>
            {actionLabel}
          </Button>
        ) : null}
        {children ? <div className="mt-5">{children}</div> : null}
      </div>
    </section>
  );
}
