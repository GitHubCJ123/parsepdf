import { type ReactNode, useCallback, useEffect, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  CheckCircle2,
  Command,
  DownloadCloud,
  ExternalLink,
  FolderOpen,
  Library,
  MessageSquareText,
  ScanText,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  UploadCloud,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { EngineSelector } from "@/components/engine-selector";
import { aiListModels } from "@/lib/ipc";
import { cn } from "@/lib/utils";

const OLLAMA_DOWNLOAD_URL = "https://ollama.com/download";

// "unknown" until the first probe finishes; thereafter reflects whether a local
// Ollama server answered and whether it already has any models pulled.
type OllamaDetection = "unknown" | "not-running" | "running-no-models" | "ready";

type OnboardingDialogProps = {
  open: boolean;
  /** Called when the tour is dismissed or completed — persists the "seen" flag. */
  onFinish: () => void;
};

/**
 * The multi-step welcome wizard. Loaded lazily (see {@link Onboarding}) so its
 * weight — including {@link EngineSelector} — never lands in the initial bundle
 * for returning users who have already completed onboarding.
 */
export function OnboardingDialog({ open, onFinish }: OnboardingDialogProps) {
  const navigate = useNavigate();
  const [step, setStep] = useState(0);
  const [ollama, setOllama] = useState<OllamaDetection>("unknown");

  // Probe for an existing Ollama install when the tour opens so the AI step can
  // greet users who already have it instead of telling them to install it.
  // Listing succeeds with models -> ready; succeeds empty -> running-no-models;
  // throws -> server unreachable (not installed or not running).
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void aiListModels("ollama")
      .then((models) => {
        if (!cancelled) setOllama(models.length > 0 ? "ready" : "running-no-models");
      })
      .catch(() => {
        if (!cancelled) setOllama("not-running");
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  const openSettingsAi = useCallback(() => {
    onFinish();
    void navigate({ to: "/settings" });
    // Let the settings page mount before asking it to switch sections.
    window.setTimeout(() => window.dispatchEvent(new CustomEvent("pdf-parser:settings-section", { detail: "ai" })), 80);
  }, [onFinish, navigate]);

  const steps: OnboardingStep[] = [
    {
      eyebrow: "Welcome",
      title: "Your PDFs, private and searchable",
      icon: Sparkles,
      body: (
        <div className="space-y-4">
          <p className="text-sm leading-6 text-muted-foreground">
            PDF-Parser turns piles of PDFs into a tidy, fully searchable library — and everything runs locally on your computer.
          </p>
          <div className="grid gap-2.5">
            <FeatureRow icon={ScanText} title="OCR built in" text="Scanned and image-only PDFs become real, selectable text." />
            <FeatureRow icon={Search} title="Instant search" text="Find any phrase across your whole library in milliseconds." />
            <FeatureRow icon={MessageSquareText} title="Optional AI chat" text="Ask questions about your documents and get answers with citations." />
          </div>
          <p className="flex items-center gap-2 rounded-lg border border-emerald-400/25 bg-emerald-400/[0.05] p-3 text-xs leading-5 text-muted-foreground">
            <ShieldCheck className="size-4 shrink-0 text-emerald-300" />
            No cloud and no account. Your files never leave your machine.
          </p>
        </div>
      ),
    },
    {
      eyebrow: "The basics",
      title: "Six areas, one simple workflow",
      icon: FolderOpen,
      body: (
        <div className="space-y-4">
          <p className="text-sm leading-6 text-muted-foreground">Here's where everything lives — you'll find these in the sidebar on the left.</p>
          <div className="grid gap-2.5 sm:grid-cols-2">
            <PlaceCard icon={FolderOpen} title="Folders" text="Watch folders so any PDF you drop in is imported and OCR'd automatically." />
            <PlaceCard icon={UploadCloud} title="Upload" text="Add PDFs by hand — drag files in or pick them to process on demand." />
            <PlaceCard icon={Library} title="Library" text="Browse, preview, rename, and manage every processed document." />
            <PlaceCard icon={Search} title="Search" text="Full-text search across all your OCR'd text, with matches highlighted." />
            <PlaceCard icon={MessageSquareText} title="Chat" text="Ask questions about your documents and get cited answers (needs AI)." />
            <PlaceCard icon={Settings} title="Settings" text="Configure OCR engines, AI, library behavior, and updates." />
          </div>
        </div>
      ),
    },
    {
      eyebrow: "Text recognition",
      title: "Choose your OCR engine",
      icon: ScanText,
      body: (
        <div className="space-y-4">
          <p className="text-sm leading-6 text-muted-foreground">
            OCR is what reads text out of scanned pages. <span className="font-medium text-foreground">Tesseract</span> is bundled and ready to go.
            Want higher accuracy on messy scans or non-Latin scripts? Install <span className="font-medium text-foreground">RapidOCR</span> below — it's optional and fully local.
          </p>
          <EngineSelector />
        </div>
      ),
    },
    {
      eyebrow: "Optional",
      title: "Turn on AI chat",
      icon: MessageSquareText,
      body: (
        <div className="space-y-4">
          {ollama === "ready" || ollama === "running-no-models" ? (
            <>
              <div className="flex items-start gap-3 rounded-lg border border-emerald-400/25 bg-emerald-400/[0.05] p-3">
                <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-300" />
                <div className="space-y-0.5">
                  <p className="text-sm font-medium text-foreground">Ollama is already installed</p>
                  <p className="text-xs leading-5 text-muted-foreground">
                    Nice — we detected Ollama running on your computer{ollama === "ready" ? " with at least one model ready to go" : ""}. You're all set for AI chat.
                  </p>
                </div>
              </div>
              <p className="text-sm leading-6 text-muted-foreground">
                {ollama === "ready"
                  ? "No setup needed right now — head to Chat whenever you want to ask questions about your library. You can fine-tune the model and connection in Settings → AI anytime."
                  : "Ollama is running but has no models yet. You can set it up later — Settings → AI walks you through downloading one whenever you're ready."}
              </p>
              <Button type="button" size="sm" variant="outline" onClick={openSettingsAi}>
                <Settings className="size-3.5" />
                {ollama === "ready" ? "Open AI settings" : "Add a model later"}
              </Button>
            </>
          ) : (
            <>
              <p className="text-sm leading-6 text-muted-foreground">
                Chat is powered by <span className="font-medium text-foreground">Ollama</span>, a free app that runs AI models privately on your own computer. It's completely optional — skip it now and set it up anytime.
              </p>
              <ol className="space-y-2.5">
                <NumberedStep n={1} title="Install Ollama">Download it once and it runs quietly in the background.</NumberedStep>
                <NumberedStep n={2} title="Add a model">Pick a model in Settings → AI — the app guides you through it.</NumberedStep>
                <NumberedStep n={3} title="Start chatting">PDF-Parser connects automatically once Ollama is running.</NumberedStep>
              </ol>
              <div className="flex flex-wrap gap-2">
                <Button type="button" size="sm" onClick={() => void openUrl(OLLAMA_DOWNLOAD_URL)}>
                  <DownloadCloud className="size-3.5" />
                  Download Ollama
                  <ExternalLink className="size-3" />
                </Button>
                <Button type="button" size="sm" variant="outline" onClick={openSettingsAi}>
                  <Settings className="size-3.5" />
                  Set up in Settings
                </Button>
              </div>
            </>
          )}
        </div>
      ),
    },
    {
      eyebrow: "You're all set",
      title: "A few handy tips",
      icon: Sparkles,
      body: (
        <div className="space-y-4">
          <div className="grid gap-2.5">
            <FeatureRow
              icon={Command}
              title="Command palette"
              text="Press Ctrl + K anywhere to jump between pages and run actions fast."
            />
            <FeatureRow
              icon={Settings}
              title="Everything is configurable"
              text="Settings has tabs for OCR, AI, Library, Updates, About, and Diagnostics — tweak anything later."
            />
            <FeatureRow
              icon={ShieldCheck}
              title="Stays up to date"
              text="The app checks for signed updates automatically and verifies them before installing."
            />
          </div>
          <p className="text-sm leading-6 text-muted-foreground">
            That's it — start by adding a watched folder or uploading a PDF. You can replay this tour anytime from <span className="font-medium text-foreground">Settings → About</span>.
          </p>
        </div>
      ),
    },
  ];

  const total = steps.length;
  const current = steps[Math.min(step, total - 1)];
  const isFirst = step === 0;
  const isLast = step === total - 1;
  const Icon = current.icon;

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) onFinish(); }}>
      <DialogContent
        showCloseButton
        onPointerDownOutside={(event) => event.preventDefault()}
        className="flex max-h-[88vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl"
      >
        <header className="relative shrink-0 overflow-hidden border-b border-border px-6 pt-6 pb-5">
          <div className="pointer-events-none absolute inset-0 opacity-90 [background:radial-gradient(circle_at_15%_0%,oklch(0.88_0.03_120/0.14),transparent_20rem)]" />
          <div className="relative flex items-start gap-4">
            <span className="grid size-11 shrink-0 place-items-center rounded-xl border border-border bg-background/70 text-foreground shadow-sm shadow-black/20">
              <Icon className="size-5" />
            </span>
            <div className="min-w-0">
              <p className="font-mono text-[11px] uppercase tracking-[0.24em] text-muted-foreground">{current.eyebrow}</p>
              <DialogTitle className="mt-1.5 text-xl font-semibold tracking-[-0.04em]">{current.title}</DialogTitle>
              <DialogDescription className="sr-only">
                Onboarding step {step + 1} of {total}: {current.title}
              </DialogDescription>
            </div>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">{current.body}</div>

        <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border bg-muted/30 px-6 py-4">
          <div className="flex items-center gap-1.5" role="presentation">
            {steps.map((_, index) => (
              <span
                key={index}
                aria-hidden
                className={cn(
                  "h-1.5 rounded-full transition-all",
                  index === step ? "w-5 bg-foreground" : "w-1.5 bg-muted-foreground/30",
                )}
              />
            ))}
            <span className="ml-2 font-mono text-[11px] text-muted-foreground">{step + 1} / {total}</span>
          </div>
          <div className="flex items-center gap-2">
            {isFirst ? (
              <Button type="button" variant="ghost" size="sm" onClick={onFinish}>Skip</Button>
            ) : (
              <Button type="button" variant="outline" size="sm" onClick={() => setStep((value) => Math.max(0, value - 1))}>
                <ArrowLeft className="size-3.5" />
                Back
              </Button>
            )}
            {isLast ? (
              <Button type="button" size="sm" onClick={onFinish}>
                <Check className="size-3.5" />
                Get started
              </Button>
            ) : (
              <Button type="button" size="sm" onClick={() => setStep((value) => Math.min(total - 1, value + 1))}>
                Next
                <ArrowRight className="size-3.5" />
              </Button>
            )}
          </div>
        </footer>
      </DialogContent>
    </Dialog>
  );
}

type OnboardingStep = {
  eyebrow: string;
  title: string;
  icon: LucideIcon;
  body: ReactNode;
};

function FeatureRow({ icon: Icon, title, text }: { icon: LucideIcon; title: string; text: string }) {
  return (
    <div className="flex items-start gap-3 rounded-lg border border-border bg-background/40 p-3">
      <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg border border-border bg-secondary/40 text-foreground">
        <Icon className="size-4" />
      </span>
      <div className="space-y-0.5">
        <p className="text-sm font-medium text-foreground">{title}</p>
        <p className="text-xs leading-5 text-muted-foreground">{text}</p>
      </div>
    </div>
  );
}

function PlaceCard({ icon: Icon, title, text }: { icon: LucideIcon; title: string; text: string }) {
  return (
    <div className="flex items-start gap-3 rounded-lg border border-border bg-background/40 p-3">
      <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg border border-border bg-secondary/40 text-foreground">
        <Icon className="size-4" />
      </span>
      <div className="space-y-0.5">
        <p className="text-sm font-medium text-foreground">{title}</p>
        <p className="text-xs leading-5 text-muted-foreground">{text}</p>
      </div>
    </div>
  );
}

function NumberedStep({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="mt-0.5 grid size-6 shrink-0 place-items-center rounded-full border border-border bg-background text-xs font-semibold text-foreground">{n}</span>
      <div className="space-y-0.5 text-xs leading-5 text-muted-foreground">
        <p className="text-sm font-medium text-foreground">{title}</p>
        <p>{children}</p>
      </div>
    </li>
  );
}
