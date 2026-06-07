import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * Custom window controls for the frameless window (decorations: false).
 * Renders flush to the top-right corner and is excluded from the drag region.
 */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlisten: (() => void) | undefined;

    void appWindow.isMaximized().then(setMaximized);
    void appWindow
      .onResized(() => {
        void appWindow.isMaximized().then(setMaximized);
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => unlisten?.();
  }, []);

  return (
    <div className="flex h-14 items-stretch">
      <ControlButton label="Minimize" onClick={() => void getCurrentWindow().minimize()}>
        <Minus className="size-4" />
      </ControlButton>
      <ControlButton
        label={maximized ? "Restore" : "Maximize"}
        onClick={() => void getCurrentWindow().toggleMaximize()}
      >
        {maximized ? <Copy className="size-3.5" /> : <Square className="size-3.5" />}
      </ControlButton>
      <ControlButton label="Close" danger onClick={() => void getCurrentWindow().close()}>
        <X className="size-4" />
      </ControlButton>
    </div>
  );
}

function ControlButton({
  label,
  onClick,
  danger,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        "grid w-12 place-items-center text-muted-foreground transition-colors",
        danger
          ? "hover:bg-destructive hover:text-white"
          : "hover:bg-secondary hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}
