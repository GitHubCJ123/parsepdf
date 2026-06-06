import { useNavigate } from "@tanstack/react-router";
import { FilePlus2, FolderOpen, Keyboard, Library, MessageSquareText, ScrollText, Search, Settings, UploadCloud } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from "@/components/ui/command";
import { openAppDir } from "@/lib/ipc";
import { notifyError } from "@/lib/toast";
import { useUiStore } from "@/stores/ui-store";

type PaletteItem = {
  label: string;
  hint: string;
  icon: LucideIcon;
  keywords: string[];
  shortcut?: string;
  run: () => void;
};

type PaletteGroup = {
  heading: string;
  items: PaletteItem[];
};

export function CommandPalette() {
  const navigate = useNavigate();
  const open = useUiStore((state) => state.commandPaletteOpen);
  const setOpen = useUiStore((state) => state.setCommandPaletteOpen);
  const openShortcutHelp = useUiStore((state) => state.openShortcutHelp);

  function go(to: "/upload" | "/folders" | "/library" | "/search" | "/chat" | "/settings", after?: () => void) {
    setOpen(false);
    void navigate({ to });
    if (after) window.setTimeout(after, 50);
  }

  function focusGlobalSearch() {
    document.querySelector<HTMLInputElement>("[data-global-search='true']")?.focus();
  }

  async function openFolder(kind: "data" | "logs") {
    setOpen(false);
    try {
      await openAppDir(kind);
    } catch (error) {
      notifyError(`${kind === "data" ? "Data" : "Logs"} folder could not be opened. ${String(error)}`);
    }
  }

  const groups: PaletteGroup[] = [
    {
      heading: "Navigate",
      items: [
        { label: "Folders", hint: "Watch folders for automatic intake", icon: FolderOpen, keywords: ["folders", "watch", "automatic", "intake", "monitor", "drop"], run: () => go("/folders") },
        { label: "Upload", hint: "Manually process and queue PDFs", icon: UploadCloud, keywords: ["upload", "inbox", "queue", "process", "ocr", "manual"], run: () => go("/upload") },
        { label: "Library", hint: "Browse processed documents", icon: Library, keywords: ["library", "documents", "archive", "files"], run: () => go("/library") },
        { label: "Search", hint: "Full-text search", icon: Search, keywords: ["search", "find", "full text", "query"], shortcut: "Ctrl /", run: () => go("/search", focusGlobalSearch) },
        { label: "Chat", hint: "Ask questions about your library", icon: MessageSquareText, keywords: ["chat", "ask", "rag", "assistant", "ai"], run: () => go("/chat") },
        { label: "Settings", hint: "OCR, AI, and library", icon: Settings, keywords: ["settings", "preferences", "config", "options"], shortcut: "Ctrl ,", run: () => go("/settings") },
      ],
    },
    {
      heading: "Actions",
      items: [
        { label: "Process new file", hint: "Open Upload to add PDFs", icon: FilePlus2, keywords: ["new", "add", "process", "import", "upload", "pdf"], shortcut: "Ctrl N", run: () => go("/upload") },
        { label: "Open data folder", hint: "View app data on disk", icon: FolderOpen, keywords: ["data", "folder", "appdata", "storage", "explorer"], run: () => void openFolder("data") },
        { label: "Open logs folder", hint: "View diagnostic logs", icon: ScrollText, keywords: ["logs", "diagnostics", "debug", "troubleshoot"], run: () => void openFolder("logs") },
        { label: "Keyboard shortcuts", hint: "Show shortcut help", icon: Keyboard, keywords: ["keyboard", "shortcuts", "keys", "help", "hotkeys"], shortcut: "Ctrl ?", run: () => { setOpen(false); openShortcutHelp(); } },
      ],
    },
  ];

  return (
    <CommandDialog
      open={open}
      onOpenChange={setOpen}
      title="Command palette"
      description="Run a PDF-Parser action."
      className="max-w-xl rounded-xl border border-border bg-popover/95 shadow-2xl shadow-black/40 backdrop-blur-xl"
    >
      <Command shouldFilter>
        <CommandInput placeholder="Type a command or search…" />
        <CommandList>
          <CommandEmpty>No matching commands.</CommandEmpty>
          {groups.map((group, groupIndex) => (
            <div key={group.heading}>
              {groupIndex > 0 ? <CommandSeparator /> : null}
              <CommandGroup heading={group.heading}>
                {group.items.map((item) => {
                  const Icon = item.icon;
                  return (
                    <CommandItem
                      key={item.label}
                      value={item.label}
                      keywords={item.keywords}
                      onSelect={item.run}
                      className="gap-3 px-2 py-2"
                    >
                      <span className="grid size-8 shrink-0 place-items-center rounded-md border border-border bg-background/60 text-muted-foreground group-data-selected/command-item:text-foreground">
                        <Icon className="size-4" />
                      </span>
                      <div className="flex min-w-0 flex-col">
                        <span className="truncate font-medium">{item.label}</span>
                        <span className="truncate text-xs text-muted-foreground">{item.hint}</span>
                      </div>
                      {item.shortcut ? (
                        <CommandShortcut className="rounded border border-border bg-background/60 px-1.5 py-0.5 font-mono text-[11px] tracking-normal">
                          {item.shortcut}
                        </CommandShortcut>
                      ) : null}
                    </CommandItem>
                  );
                })}
              </CommandGroup>
            </div>
          ))}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
