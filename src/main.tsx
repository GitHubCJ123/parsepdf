import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles/globals.css";
import { ErrorBoundary } from "./components/error-boundary";
import { getDatabase } from "./lib/db";
import { openAppDir } from "./lib/ipc";
import { router } from "./router";

document.documentElement.classList.add("dark");

// Surface promise rejections that would otherwise vanish into the void.
window.addEventListener("unhandledrejection", (event) => {
  console.error("[unhandledrejection]", event.reason);
});
window.addEventListener("error", (event) => {
  console.error("[window.error]", event.error ?? event.message);
});

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 30_000,
    },
  },
});

function Bootstrap() {
  const [fatalError, setFatalError] = useState<string | null>(null);

  useEffect(() => {
    // The app shell has committed — fade out the static splash from index.html.
    // Runs independently of the DB load so the UI is revealed immediately.
    dismissSplash();
  }, []);

  useEffect(() => {
    void getDatabase().catch((error: unknown) => {
      setFatalError(error instanceof Error ? error.message : String(error));
    });
  }, []);

  if (fatalError) return <FatalStartupError message={fatalError} />;

  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}

/** Fade out and remove the no-JS splash injected by index.html. */
function dismissSplash() {
  const splash = document.getElementById("splash");
  if (!splash) return;
  splash.dataset.hide = "true";
  // Remove after the CSS fade so it stops trapping pointer events.
  window.setTimeout(() => splash.remove(), 300);
}

function FatalStartupError({ message }: { message: string }) {
  async function openLogFolder() {
    await openAppDir("logs");
  }

  return (
    <main className="grid min-h-screen place-items-center bg-background p-6 text-foreground">
      <section role="alertdialog" aria-labelledby="fatal-title" className="max-w-lg rounded-xl border border-destructive/40 bg-card p-6 shadow-2xl shadow-black/30">
        <p className="font-mono text-xs uppercase tracking-[0.2em] text-destructive">Startup blocked</p>
        <h1 id="fatal-title" className="mt-2 text-2xl font-semibold tracking-[-0.05em]">PDF-Parser could not start</h1>
        <p className="mt-3 text-sm leading-6 text-muted-foreground">The local database or secure storage could not be opened. Reload the app, then open the log if it happens again.</p>
        <pre className="mt-4 max-h-32 overflow-auto rounded-lg border border-border bg-background p-3 text-xs text-muted-foreground">{message}</pre>
        <div className="mt-5 flex flex-wrap gap-2">
          <button type="button" onClick={() => window.location.reload()} className="rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground">Reload app</button>
          <button type="button" onClick={() => void openLogFolder()} className="rounded-lg border border-border px-3 py-2 text-sm font-medium text-foreground">Open log</button>
        </div>
      </section>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <Bootstrap />
    </ErrorBoundary>
  </React.StrictMode>,
);
