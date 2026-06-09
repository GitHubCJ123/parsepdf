import { Suspense, lazy, useCallback, useEffect, useState } from "react";
import { getSetting, setSetting } from "@/lib/db";

// Permanent flag: once "1", onboarding never auto-shows again — not on reopen and
// not after an app update. Users can still replay it from Settings → About.
const ONBOARDING_KEY = "onboarding.completed";
// Settings dispatches this to re-open the tour on demand (decoupled from routing).
export const START_ONBOARDING_EVENT = "pdf-parser:start-onboarding";

// The wizard (and its heavy EngineSelector dependency) is code-split so it stays
// out of the initial bundle. Returning users never download this chunk.
const OnboardingDialog = lazy(() =>
  import("@/components/onboarding-dialog").then((module) => ({ default: module.OnboardingDialog })),
);

/** Imperatively start the onboarding tour from anywhere (e.g. a Settings button). */
export function startOnboarding() {
  window.dispatchEvent(new CustomEvent(START_ONBOARDING_EVENT));
}

export function Onboarding() {
  const [open, setOpen] = useState(false);
  // Mount the lazy wizard only after the tour has been triggered at least once.
  const [mounted, setMounted] = useState(false);
  // Bumped on each open so the dialog remounts with fresh state — e.g. replaying
  // from Settings restarts at the first step instead of the last one shown.
  const [runId, setRunId] = useState(0);

  const startRun = useCallback(() => {
    setMounted(true);
    setRunId((id) => id + 1);
    setOpen(true);
  }, []);

  // First-run check: show only when the flag is absent. Reads happen in an async
  // callback (not a synchronous effect body), so this is safe under React 19.
  useEffect(() => {
    let cancelled = false;
    void getSetting(ONBOARDING_KEY)
      .then((value) => {
        if (!cancelled && value !== "1") startRun();
      })
      .catch(() => {
        // Settings unreadable: don't auto-open; the tour stays reachable from
        // Settings → About.
      });
    return () => {
      cancelled = true;
    };
  }, [startRun]);

  // Replay support: Settings (or anywhere) can re-open the tour from the start.
  useEffect(() => {
    window.addEventListener(START_ONBOARDING_EVENT, startRun);
    return () => window.removeEventListener(START_ONBOARDING_EVENT, startRun);
  }, [startRun]);

  const finish = useCallback(() => {
    setOpen(false);
    void setSetting(ONBOARDING_KEY, "1").catch(() => {
      // Persisting can fail (e.g. DB not ready); worst case the tour shows again
      // next launch, which is harmless.
    });
  }, []);

  if (!mounted) return null;

  return (
    <Suspense fallback={null}>
      <OnboardingDialog key={runId} open={open} onFinish={finish} />
    </Suspense>
  );
}
