import React from "react";

type Props = {
  children: React.ReactNode;
  fallback?: (error: Error, reset: () => void) => React.ReactNode;
};

type State = {
  error: Error | null;
};

export class ErrorBoundary extends React.Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("[ErrorBoundary] React tree crashed", error, info);
  }

  reset = () => this.setState({ error: null });

  render() {
    if (this.state.error) {
      if (this.props.fallback) return this.props.fallback(this.state.error, this.reset);
      return <DefaultFallback error={this.state.error} onReset={this.reset} />;
    }
    return this.props.children;
  }
}

function DefaultFallback({ error, onReset }: { error: Error; onReset: () => void }) {
  return (
    <main className="grid min-h-screen place-items-center bg-background p-6 text-foreground">
      <section role="alertdialog" className="max-w-lg rounded-xl border border-destructive/40 bg-card p-6 shadow-2xl shadow-black/30">
        <p className="font-mono text-xs uppercase tracking-[0.2em] text-destructive">Something broke</p>
        <h1 className="mt-2 text-2xl font-semibold tracking-[-0.05em]">PDF-Parser hit an unexpected error</h1>
        <p className="mt-3 text-sm leading-6 text-muted-foreground">The interface ran into a problem. Reload the app, or click "Try again" to recover this view.</p>
        <pre className="mt-4 max-h-40 overflow-auto rounded-lg border border-border bg-background p-3 font-mono text-xs text-muted-foreground">{error.message}{error.stack ? `\n\n${error.stack}` : ""}</pre>
        <div className="mt-5 flex flex-wrap gap-2">
          <button type="button" onClick={onReset} className="rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground">Try again</button>
          <button type="button" onClick={() => window.location.reload()} className="rounded-lg border border-border px-3 py-2 text-sm font-medium text-foreground">Reload app</button>
        </div>
      </section>
    </main>
  );
}
