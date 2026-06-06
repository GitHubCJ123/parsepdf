import { format } from "date-fns";
import { FileText, FolderOpen, Loader2, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import type { DocumentRow } from "@/lib/ipc";

type DuplicateDialogProps = {
  open: boolean;
  fileName: string;
  existing: DocumentRow | null;
  busy: boolean;
  onOpenChange: (open: boolean) => void;
  onOpenExisting: () => void | Promise<void>;
  onReprocess: () => void | Promise<void>;
};

/**
 * Shown when an uploaded file is byte-for-byte identical to a document already
 * in the library (Duplicate Protection). Lets the user open the existing doc,
 * re-run OCR on it, or cancel — instead of silently re-processing.
 */
export function DuplicateDialog({ open, fileName, existing, busy, onOpenChange, onOpenExisting, onReprocess }: DuplicateDialogProps) {
  return (
    <Dialog open={open} onOpenChange={(next) => !busy && onOpenChange(next)}>
      <DialogContent showCloseButton={false} className="overflow-hidden sm:max-w-md">
        <DialogHeader className="flex-row items-start gap-3 space-y-0">
          <div className="grid size-9 shrink-0 place-items-center rounded-lg border border-border bg-secondary/60 text-foreground">
            <FileText className="size-4.5" />
          </div>
          <div className="min-w-0 space-y-1.5">
            <DialogTitle>Already in your library</DialogTitle>
            <DialogDescription className="break-words">
              <span className="font-medium text-foreground">{fileName}</span> is the same file as a document you already have. Nothing was processed.
            </DialogDescription>
          </div>
        </DialogHeader>

        {existing ? (
          <div className="min-w-0 overflow-hidden rounded-lg border border-border bg-background/45 p-3 text-sm">
            <div className="truncate font-medium text-foreground">{existing.display_name}</div>
            <div className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-xs text-muted-foreground">
              <span>{existing.page_count} page{existing.page_count === 1 ? "" : "s"}</span>
              <span>Imported {format(new Date(existing.ingested_at * 1000), "MMM d, yyyy")}</span>
              {existing.ocr_engine ? <span>{existing.ocr_engine}</span> : null}
            </div>
          </div>
        ) : null}

        <DialogFooter className="sm:justify-between">
          <Button type="button" variant="ghost" disabled={busy} onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <div className="flex flex-col gap-2 sm:flex-row">
            <Button type="button" variant="outline" disabled={busy} onClick={() => void onReprocess()}>
              <RotateCcw className="size-4" /> Reprocess
            </Button>
            <Button type="button" disabled={busy} onClick={() => void onOpenExisting()}>
              {busy ? <Loader2 className="size-4 animate-spin" /> : <FolderOpen className="size-4" />}
              Open existing
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
