
import { Link, Outlet, useNavigate } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { Toaster } from "sonner";
import { Button } from "@/components/ui/button";
import { CommandPalette } from "@/components/command-palette";
import { KeyboardShortcutsDialog } from "@/components/keyboard-shortcuts-dialog";
import { Sidebar } from "@/components/sidebar";
import { UpdateNotifier } from "@/components/update-notifier";
import { WindowControls } from "@/components/window-controls";
import { useGlobalShortcuts } from "@/hooks/use-global-shortcuts";
import { aiHealthCheck } from "@/lib/ipc";
import { getSetting } from "@/lib/db";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";

export function AppLayout() {
  const navigate = useNavigate();
  const openCommandPalette = useUiStore((state) => state.openCommandPalette);
  const shortcutHelpOpen = useUiStore((state) => state.shortcutHelpOpen);
  const setShortcutHelpOpen = useUiStore((state) => state.setShortcutHelpOpen);
  useGlobalShortcuts();

  return (
    <div className="app-grain flex h-screen overflow-hidden bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header
          data-tauri-drag-region
          className="flex h-14 shrink-0 items-center justify-between gap-2 border-b border-border/80 bg-background/85 pl-4 backdrop-blur-xl"
        >
          <div data-tauri-drag-region className="font-mono text-sm font-medium tracking-[-0.03em] text-foreground">PDF-Parser</div>
          <div className="flex items-center gap-2">
            <ProviderStatusIndicator />
            <Button type="button" size="sm" className="rounded-md" onClick={() => void navigate({ to: "/upload" })}>
              <Plus className="size-3.5" />
              New
            </Button>
            <button
              type="button"
              onClick={openCommandPalette}
              className="hidden items-center gap-1 rounded-md border border-border bg-secondary/40 px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground sm:flex"
            >
              <span>Command</span>
              <kbd className="rounded-sm border border-border bg-background px-1.5 py-0.5 font-mono text-[11px] text-foreground">Ctrl K</kbd>
            </button>
            <WindowControls />
          </div>
        </header>
        <main className="min-h-0 flex-1 overflow-auto">
          <div className="mx-auto w-full max-w-[1600px] p-6">
            <Outlet />
          </div>
        </main>
      </div>
      <CommandPalette />
      <KeyboardShortcutsDialog open={shortcutHelpOpen} onOpenChange={setShortcutHelpOpen} />
      <UpdateNotifier />
      <Toaster position="bottom-right" richColors closeButton duration={3000} toastOptions={{ className: "border-border bg-popover text-popover-foreground" }} />
    </div>
  );
}

type ProviderStatus = "connected" | "offline" | "not-configured";

function ProviderStatusIndicator() {
  const [provider, setProvider] = useState("none");
  const [status, setStatus] = useState<ProviderStatus>("not-configured");

  useEffect(() => {
    let cancelled = false;
    async function refresh() {
      const configuredProvider = (await getSetting("ai.default_provider").catch(() => null)) ?? "none";
      if (cancelled) return;
      setProvider(configuredProvider);
      if (configuredProvider === "none") {
        setStatus("not-configured");
        return;
      }
      try {
        const ok = await aiHealthCheck(configuredProvider);
        if (!cancelled) setStatus(ok ? "connected" : "not-configured");
      } catch {
        if (!cancelled) setStatus("offline");
      }
    }
    void refresh();
    const interval = window.setInterval(() => void refresh(), 120_000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, []);

  const label = provider === "none" ? "Not configured" : provider;
  return (
    <Link
      to="/settings"
      title="Open AI provider settings"
      onClick={() => window.setTimeout(() => window.dispatchEvent(new CustomEvent("pdf-parser:settings-section", { detail: "ai" })), 50)}
      className="hidden items-center gap-2 rounded-md border border-border bg-secondary/30 px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground sm:flex"
    >
      <span
        className={cn(
          "size-2 rounded-full",
          status === "connected" && "bg-emerald-400",
          status === "offline" && "bg-amber-400",
          status === "not-configured" && "bg-muted-foreground",
        )}
      />
      <span className={provider === "none" ? undefined : "capitalize"}>{label}</span>
    </Link>
  );
}
