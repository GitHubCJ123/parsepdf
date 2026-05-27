import { useEffect } from "react";
import { useNavigate } from "@tanstack/react-router";
import { FilePlus2, FolderOpen, Settings } from "lucide-react";
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
import { useUiStore } from "@/stores/ui-store";

const paletteItems = [
  {
    label: "Process file",
    hint: "Queue a PDF",
    icon: FilePlus2,
  },
  {
    label: "Open settings",
    hint: "Configure app",
    icon: Settings,
    to: "/settings" as const,
  },
  {
    label: "Open data folder",
    hint: "View app data",
    icon: FolderOpen,
  },
];

export function CommandPalette() {
  const navigate = useNavigate();
  const open = useUiStore((state) => state.commandPaletteOpen);
  const setOpen = useUiStore((state) => state.setCommandPaletteOpen);
  const toggle = useUiStore((state) => state.toggleCommandPalette);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey && !event.metaKey && event.key.toLowerCase() === "k") {
        event.preventDefault();
        toggle();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [toggle]);

  return (
    <CommandDialog
      open={open}
      onOpenChange={setOpen}
      title="Command palette"
      description="Run a PDF-Parser action."
      className="max-w-xl rounded-lg border border-border bg-popover/95 shadow-none backdrop-blur-xl"
    >
      <Command shouldFilter>
        <CommandInput placeholder="Type a command or search..." />
        <CommandList>
          <CommandEmpty>No commands found.</CommandEmpty>
          <CommandGroup heading="Actions">
            {paletteItems.map((item) => {
              const Icon = item.icon;
              return (
                <CommandItem
                  key={item.label}
                  value={`${item.label} ${item.hint}`}
                  onSelect={() => {
                    if (item.to) {
                      void navigate({ to: item.to });
                    }
                    setOpen(false);
                  }}
                >
                  <Icon className="size-4 text-muted-foreground" />
                  <div className="flex flex-col gap-0.5">
                    <span>{item.label}</span>
                    <span className="text-xs text-muted-foreground">
                      {item.hint}
                    </span>
                  </div>
                  <CommandShortcut>Ctrl K</CommandShortcut>
                </CommandItem>
              );
            })}
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
