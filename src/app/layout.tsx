import { Outlet } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { CommandPalette } from "@/components/command-palette";
import { Sidebar } from "@/components/sidebar";
import { UpdateNotifier } from "@/components/update-notifier";
import { useUiStore } from "@/stores/ui-store";

export function AppLayout() {
  const openCommandPalette = useUiStore((state) => state.openCommandPalette);

  return (
    <div className="app-grain flex h-screen overflow-hidden bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-border/80 bg-background/85 px-4 backdrop-blur-xl">
          <div className="font-mono text-sm font-medium tracking-[-0.03em] text-foreground">
            PDF-Parser
          </div>
          <div className="flex items-center gap-2">
            <Button type="button" size="sm" className="rounded-md">
              <Plus className="size-3.5" />
              New
            </Button>
            <button
              type="button"
              onClick={openCommandPalette}
              className="hidden items-center gap-1 rounded-md border border-border bg-secondary/40 px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground sm:flex"
            >
              <span>Command</span>
              <kbd className="rounded-sm border border-border bg-background px-1.5 py-0.5 font-mono text-[11px] text-foreground">
                Ctrl K
              </kbd>
            </button>
          </div>
        </header>
        <main className="min-h-0 flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </div>
      <CommandPalette />
      <UpdateNotifier />
    </div>
  );
}
