
import { useNavigate } from "@tanstack/react-router";
import { openPath } from "@tauri-apps/plugin-opener";
import { FilePlus2, FolderOpen, Keyboard, Search, Settings } from "lucide-react";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@/components/ui/command";
import { appPaths } from "@/lib/ipc";
import { notifyError } from "@/lib/toast";
import { useUiStore } from "@/stores/ui-store";

export function CommandPalette() {
  const navigate = useNavigate();
  const open = useUiStore((state) => state.commandPaletteOpen);
  const setOpen = useUiStore((state) => state.setCommandPaletteOpen);
  const openShortcutHelp = useUiStore((state) => state.openShortcutHelp);

  async function run(action: string) {
    setOpen(false);
    if (action === "process") {
      void navigate({ to: "/inbox" });
      return;
    }
    if (action === "settings") {
      void navigate({ to: "/settings" });
      window.setTimeout(() => window.dispatchEvent(new CustomEvent("pdf-parser:settings-section", { detail: "ai" })), 50);
      return;
    }
    if (action === "search") {
      void navigate({ to: "/search" });
      window.setTimeout(() => document.querySelector<HTMLInputElement>("[data-global-search='true']")?.focus(), 50);
      return;
    }
    if (action === "data") {
      try {
        const paths = await appPaths();
        await openPath(paths.data_dir);
      } catch (error) {
        notifyError(`Data folder could not be opened. ${String(error)}`);
      }
      return;
    }
    if (action === "shortcuts") {
      openShortcutHelp();
    }
  }

  const paletteItems = [
    { label: "Process file", hint: "Open Inbox", icon: FilePlus2, action: "process", shortcut: "Ctrl N" },
    { label: "Open settings", hint: "Configure AI and folders", icon: Settings, action: "settings", shortcut: "Ctrl ," },
    { label: "Search library", hint: "Focus full-text search", icon: Search, action: "search", shortcut: "Ctrl /" },
    { label: "Open data folder", hint: "View app data", icon: FolderOpen, action: "data" },
    { label: "Keyboard shortcuts", hint: "Show shortcut help", icon: Keyboard, action: "shortcuts", shortcut: "Ctrl ?" },
  ];

  return (
    <CommandDialog
      open={open}
      onOpenChange={setOpen}
      title="Command palette"
      description="Run a PDF-Parser action."
      className="max-w-xl rounded-lg border border-border bg-popover/95 shadow-none backdrop-blur-xl"
    >
      <Command shouldFilter>
        <CommandInput placeholder="Type a command or search" />
        <CommandList>
          <CommandEmpty>No commands found.</CommandEmpty>
          <CommandGroup heading="Actions">
            {paletteItems.map((item) => {
              const Icon = item.icon;
              return (
                <CommandItem key={item.label} value={`${item.label} ${item.hint}`} onSelect={() => void run(item.action)}>
                  <Icon className="size-4 text-muted-foreground" />
                  <div className="flex flex-col gap-0.5">
                    <span>{item.label}</span>
                    <span className="text-xs text-muted-foreground">{item.hint}</span>
                  </div>
                  {item.shortcut ? <CommandShortcut>{item.shortcut}</CommandShortcut> : null}
                </CommandItem>
              );
            })}
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
