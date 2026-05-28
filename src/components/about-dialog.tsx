
import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { check } from "@tauri-apps/plugin-updater";
import { ExternalLink, FolderOpen, Info, Loader2, RefreshCw, ScrollText } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { LogViewer } from "@/components/log-viewer";
import { appPaths } from "@/lib/ipc";
import { notifyError, notifyInfo, notifySuccess } from "@/lib/toast";

const credits = [
  ["Tauri", "https://github.com/tauri-apps/tauri"],
  ["React", "https://github.com/facebook/react"],
  ["Tailwind", "https://github.com/tailwindlabs/tailwindcss"],
  ["shadcn/ui", "https://github.com/shadcn-ui/ui"],
  ["Tesseract", "https://github.com/tesseract-ocr/tesseract"],
  ["PaddlePaddle", "https://github.com/PaddlePaddle/PaddleOCR"],
  ["fastembed", "https://github.com/Anush008/fastembed-rs"],
  ["sqlite-vec", "https://github.com/asg017/sqlite-vec"],
  ["BGE", "https://github.com/FlagOpen/FlagEmbedding"],
] as const;

type AboutDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function AboutDialog({ open, onOpenChange }: AboutDialogProps) {
  const [version, setVersion] = useState("0.1.0");
  const [checking, setChecking] = useState(false);
  const [logOpen, setLogOpen] = useState(false);
  const sha = import.meta.env.VITE_GIT_SHA || "unknown";

  useEffect(() => {
    if (open) void getVersion().then(setVersion).catch(() => undefined);
  }, [open]);

  async function openFolder(kind: "data" | "logs") {
    try {
      const paths = await appPaths();
      await openPath(kind === "data" ? paths.data_dir : paths.log_dir);
    } catch (error) {
      notifyError(`${kind === "data" ? "Data" : "Log"} folder could not be opened. ${String(error)}`);
    }
  }

  async function checkForUpdates() {
    setChecking(true);
    try {
      const update = await check();
      if (update) notifyInfo(`Update ${update.version} is available.`);
      else notifySuccess("PDF-Parser is up to date.");
    } catch (error) {
      notifyError(`Update check failed. ${String(error)}`);
    } finally {
      setChecking(false);
    }
  }

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-2xl border-border/80 bg-popover/95 shadow-2xl shadow-black/40 backdrop-blur-xl">
          <DialogHeader>
            <div className="mb-2 flex items-center gap-3">
              <div className="grid size-12 place-items-center rounded-xl border border-border bg-secondary/60"><Info className="size-6" /></div>
              <div>
                <DialogTitle>PDF-Parser</DialogTitle>
                <DialogDescription>Local-first OCR, search, and document chat.</DialogDescription>
              </div>
            </div>
          </DialogHeader>

          <div className="grid gap-3 rounded-lg border border-border bg-background/45 p-4 text-sm sm:grid-cols-2">
            <Meta label="Version" value={version} />
            <Meta label="Commit" value={sha.slice(0, 12)} />
          </div>

          <div className="grid gap-2 sm:grid-cols-2">
            <Button type="button" variant="outline" onClick={() => void openFolder("data")}><FolderOpen className="size-4" />Open data folder</Button>
            <Button type="button" variant="outline" onClick={() => void openFolder("logs")}><FolderOpen className="size-4" />Open log folder</Button>
            <Button type="button" variant="outline" onClick={() => setLogOpen(true)}><ScrollText className="size-4" />View logs</Button>
            <Button type="button" onClick={() => void checkForUpdates()} disabled={checking}>{checking ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}Check for updates</Button>
          </div>

          <section className="rounded-lg border border-border bg-background/45 p-4">
            <h3 className="text-sm font-medium">Credits</h3>
            <div className="mt-3 flex flex-wrap gap-2">
              {credits.map(([label, url]) => (
                <button key={label} type="button" onClick={() => void openUrl(url)} className="inline-flex items-center gap-1 rounded-full border border-border bg-secondary/45 px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground">
                  {label}<ExternalLink className="size-3" />
                </button>
              ))}
            </div>
          </section>

          <p className="text-xs leading-5 text-muted-foreground">PDF-Parser is released under the MIT license. Optional cloud AI only runs after you configure a provider.</p>
        </DialogContent>
      </Dialog>
      <LogViewer open={logOpen} onOpenChange={setLogOpen} />
    </>
  );
}

function Meta({ label, value }: { label: string; value: string }) {
  return <div><div className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">{label}</div><div className="mt-1 truncate text-foreground">{value}</div></div>;
}
