
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

const shortcuts = [
  ["Ctrl", "K", "Open command palette"],
  ["Ctrl", ",", "Open settings"],
  ["Ctrl", "/", "Focus search"],
  ["Ctrl", "N", "New chat thread"],
  ["Ctrl", "?", "Show keyboard shortcuts"],
  ["Esc", "", "Close dialog or drawer"],
] as const;

type KeyboardShortcutsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function KeyboardShortcutsDialog({ open, onOpenChange }: KeyboardShortcutsDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg border-border/80 bg-popover/95 shadow-2xl shadow-black/40 backdrop-blur-xl">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>Use these shortcuts to move through PDF-Parser faster.</DialogDescription>
        </DialogHeader>
        <div className="divide-y divide-border rounded-lg border border-border bg-background/45">
          {shortcuts.map(([first, second, label]) => (
            <div key={label} className="flex items-center justify-between gap-4 px-3 py-2.5 text-sm">
              <span className="text-foreground/90">{label}</span>
              <span className="flex items-center gap-1 font-mono text-[11px] text-muted-foreground">
                <Key>{first}</Key>
                {second ? <><span>+</span><Key>{second}</Key></> : null}
              </span>
            </div>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Key({ children, className }: { children: string; className?: string }) {
  return <kbd className={cn("rounded border border-border bg-secondary/70 px-1.5 py-0.5 text-foreground", className)}>{children}</kbd>;
}
