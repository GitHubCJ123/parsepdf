import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import ReactMarkdown from "react-markdown";
import { Button } from "@/components/ui/button";
import { getSetting } from "@/lib/db";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

type UpdateDetails = {
  version: string;
  date?: string;
  body?: string;
};

type UpdatePromptProps = {
  update: UpdateDetails | null;
  open: boolean;
  installing: boolean;
  errorMessage: string | null;
  onInstall: () => void;
  onLater: () => void;
};

const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

function formatReleaseDate(date?: string) {
  if (!date) {
    return null;
  }

  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) {
    return null;
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(parsed);
}

export function UpdatePrompt({
  update,
  open,
  installing,
  errorMessage,
  onInstall,
  onLater,
}: UpdatePromptProps) {
  const releaseDate = formatReleaseDate(update?.date);
  const releaseNotes = update?.body?.trim();

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onLater()}>
      <DialogContent className="max-w-lg border-border/80 bg-popover/95 shadow-2xl shadow-black/40 backdrop-blur-xl">
        <DialogHeader>
          <div className="mb-1 w-fit rounded-full border border-border bg-secondary/40 px-2 py-0.5 font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
            Signed update
          </div>
          <DialogTitle>Update available: v{update?.version}</DialogTitle>
          <DialogDescription>
            PDF-Parser can install this release now and restart when finished.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          {releaseDate && (
            <p className="font-mono text-xs text-muted-foreground">
              Released {releaseDate}
            </p>
          )}
          <div className="max-h-56 overflow-auto rounded-lg border border-border bg-background/70 p-3 text-sm text-muted-foreground">
            {releaseNotes ? (
              <div className="space-y-2 leading-relaxed [&_a]:text-foreground [&_a]:underline [&_code]:rounded [&_code]:bg-secondary [&_code]:px-1 [&_h1]:text-base [&_h2]:text-base [&_h3]:text-sm [&_li]:ml-4 [&_li]:list-disc [&_strong]:text-foreground">
                <ReactMarkdown>{releaseNotes}</ReactMarkdown>
              </div>
            ) : (
              <p>No release notes were provided for this update.</p>
            )}
          </div>
          {errorMessage && (
            <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {errorMessage}
            </p>
          )}
        </div>

        <DialogFooter className="items-center sm:justify-between">
          <Button
            type="button"
            variant="outline"
            onClick={onLater}
            disabled={installing}
          >
            Later
          </Button>
          <Button type="button" onClick={onInstall} disabled={installing}>
            {installing ? "Preparing update..." : "Install and restart"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function UpdateNotifier() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const checkForUpdates = useCallback(async () => {
    try {
      // Respect the user's "Automatic update checks" setting (Settings → Updates).
      // The manual "Check for updates" button there bypasses this.
      const autoCheck = await getSetting("updater.auto_check").catch(() => null);
      if (autoCheck === "0") {
        return;
      }
      const nextUpdate = await check();
      if (nextUpdate) {
        setUpdate(nextUpdate);
        setErrorMessage(null);
      }
    } catch (error) {
      console.info("Update check skipped", error);
    }
  }, []);

  useEffect(() => {
    void checkForUpdates();
    const intervalId = window.setInterval(checkForUpdates, CHECK_INTERVAL_MS);

    return () => window.clearInterval(intervalId);
  }, [checkForUpdates]);

  const installUpdate = useCallback(async () => {
    if (!update) {
      return;
    }

    setInstalling(true);
    setErrorMessage(null);

    try {
      await invoke("prepare_for_update");
      await update.downloadAndInstall();
      await relaunch();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setErrorMessage(message || "Unable to install the update.");
      setInstalling(false);
    }
  }, [update]);

  return (
    <UpdatePrompt
      update={update}
      open={Boolean(update)}
      installing={installing}
      errorMessage={errorMessage}
      onInstall={installUpdate}
      onLater={() => !installing && setUpdate(null)}
    />
  );
}
