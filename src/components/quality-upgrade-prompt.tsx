import { useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import { Sparkles, X } from "lucide-react";
import { Button } from "@/components/ui/button";

const DISMISS_KEY = "pdf-parser.rapidocr-upgrade-dismissed";
const NEVER_ASK_KEY = "pdf-parser.rapidocr-upgrade-never";

type QualityUpgradePromptProps = {
  lowConfidenceCount: number;
  scanLikeQueueCount: number;
  rapidOcrInstalled: boolean;
  onUseOnce: () => void;
  onSetDefault: () => void;
};

export function QualityUpgradePrompt({
  lowConfidenceCount,
  scanLikeQueueCount,
  rapidOcrInstalled,
  onUseOnce,
  onSetDefault,
}: QualityUpgradePromptProps) {
  const [dismissed, setDismissed] = useState(() => localStorage.getItem(DISMISS_KEY) === "true");
  const [neverAsk, setNeverAsk] = useState(() => localStorage.getItem(NEVER_ASK_KEY) === "true");
  const shouldShow = !dismissed && !neverAsk && (lowConfidenceCount > 0 || scanLikeQueueCount > 5);

  const copy = useMemo(() => {
    if (lowConfidenceCount > 0) {
      return `Tesseract found low-confidence text in ${lowConfidenceCount} document${lowConfidenceCount === 1 ? "" : "s"}. RapidOCR (200 MB download) is more accurate on scans and tables. Try it?`;
    }
    return `You have ${scanLikeQueueCount} scan-like PDFs queued. RapidOCR (200 MB download) is more accurate on scans and tables. Try it?`;
  }, [lowConfidenceCount, scanLikeQueueCount]);

  if (!shouldShow) {
    return null;
  }

  function dismiss() {
    localStorage.setItem(DISMISS_KEY, "true");
    setDismissed(true);
  }

  function never() {
    localStorage.setItem(NEVER_ASK_KEY, "true");
    setNeverAsk(true);
  }

  return (
    <aside className="relative overflow-hidden rounded-xl border border-amber-300/25 bg-amber-300/10 p-4 text-amber-50 shadow-xl shadow-black/15" aria-live="polite">
      <div className="pointer-events-none absolute inset-0 [background:radial-gradient(circle_at_8%_0%,oklch(0.85_0.12_80/0.24),transparent_16rem)]" />
      <div className="relative flex gap-3">
        <div className="grid size-9 shrink-0 place-items-center rounded-lg border border-amber-200/25 bg-amber-200/10">
          <Sparkles className="size-4" />
        </div>
        <div className="min-w-0 flex-1 space-y-3">
          <div>
            <h2 className="text-sm font-semibold tracking-[-0.02em]">Try high-quality OCR for this batch</h2>
            <p className="mt-1 text-sm leading-6 text-amber-100/80">{copy}</p>
          </div>
          <div className="flex flex-wrap gap-2">
            {rapidOcrInstalled ? (
              <Button type="button" size="sm" onClick={onUseOnce}>
                Use once for this batch
              </Button>
            ) : (
              <Button size="sm" asChild>
                <Link to="/settings">Use once for this batch</Link>
              </Button>
            )}
            {rapidOcrInstalled ? (
              <Button type="button" size="sm" variant="outline" onClick={onSetDefault}>
                Set as default
              </Button>
            ) : (
              <Button size="sm" variant="outline" asChild>
                <Link to="/settings">Set as default</Link>
              </Button>
            )}
            <Button type="button" size="sm" variant="ghost" onClick={dismiss}>
              Not now
            </Button>
            <Button type="button" size="sm" variant="ghost" onClick={never}>
              Don&apos;t ask again
            </Button>
          </div>
        </div>
        <button type="button" onClick={dismiss} className="rounded-md p-1 text-amber-100/70 transition-colors hover:bg-amber-200/10 hover:text-amber-50" aria-label="Dismiss RapidOCR suggestion">
          <X className="size-4" />
        </button>
      </div>
    </aside>
  );
}
