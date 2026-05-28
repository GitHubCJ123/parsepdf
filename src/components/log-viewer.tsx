
import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Copy, Download, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { logSaveSelection, logTail } from "@/lib/ipc";
import { notifyError, notifySuccess } from "@/lib/toast";

type LogViewerProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function LogViewer({ open, onOpenChange }: LogViewerProps) {
  const [level, setLevel] = useState("all");
  const [contents, setContents] = useState("");
  const [loading, setLoading] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const preRef = useRef<HTMLPreElement | null>(null);

  async function refresh() {
    setLoading(true);
    try {
      setContents(await logTail(level, 800));
    } catch (error) {
      notifyError(`Log file could not be read. ${String(error)}`);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (!open) return;
    void refresh();
    const id = window.setInterval(() => void refresh(), 2000);
    return () => window.clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, level]);

  useEffect(() => {
    if (autoScroll && preRef.current) preRef.current.scrollTop = preRef.current.scrollHeight;
  }, [autoScroll, contents]);

  async function copyAll() {
    await navigator.clipboard.writeText(contents);
    notifySuccess("Log copied.");
  }

  async function saveSelection() {
    const selected = window.getSelection()?.toString().trim();
    const text = selected || contents;
    if (!text.trim()) return;
    const path = await save({ defaultPath: "pdf-parser-log-selection.txt", filters: [{ name: "Text", extensions: ["txt"] }] });
    if (!path) return;
    try {
      await logSaveSelection(path, text);
      notifySuccess("Log saved.");
    } catch (error) {
      notifyError(`Log selection could not be saved. ${String(error)}`);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl border-border/80 bg-popover/95 shadow-2xl shadow-black/40 backdrop-blur-xl">
        <DialogHeader>
          <DialogTitle>Log viewer</DialogTitle>
          <DialogDescription>Tail the current redacted app log.</DialogDescription>
        </DialogHeader>
        <div className="flex flex-wrap items-center gap-2">
          <label className="flex items-center gap-2 text-sm text-muted-foreground">
            Level
            <select value={level} onChange={(event) => setLevel(event.target.value)} className="h-8 rounded-md border border-border bg-background px-2 text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring">
              <option value="all">All</option>
              <option value="info">Info</option>
              <option value="warn">Warn</option>
              <option value="error">Error</option>
            </select>
          </label>
          <label className="flex items-center gap-2 text-sm text-muted-foreground">
            <input type="checkbox" checked={autoScroll} onChange={(event) => setAutoScroll(event.target.checked)} />
            Auto-scroll
          </label>
          <div className="ml-auto flex gap-2">
            <Button type="button" variant="outline" onClick={() => void copyAll()} disabled={!contents}><Copy className="size-4" />Copy all</Button>
            <Button type="button" variant="outline" onClick={() => void saveSelection()} disabled={!contents}><Download className="size-4" />Save selection</Button>
          </div>
        </div>
        <pre ref={preRef} className="max-h-[55vh] min-h-80 overflow-auto rounded-lg border border-border bg-background/80 p-3 font-mono text-xs leading-5 text-muted-foreground">
          {loading && !contents ? "Loading log…" : contents || "No log entries yet."}
        </pre>
        {loading ? <div className="flex items-center gap-2 text-xs text-muted-foreground"><Loader2 className="size-3 animate-spin" />Refreshing</div> : null}
      </DialogContent>
    </Dialog>
  );
}
