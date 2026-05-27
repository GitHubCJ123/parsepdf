import { useEffect, useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import { Check, Loader2, RefreshCcw, RotateCcw, SkipForward, TriangleAlert } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { aiApplyRename, aiProposeNames, libraryPendingRenames, librarySkipRename, type PendingRenameRow } from "@/lib/ipc";
import { cn } from "@/lib/utils";

type RowState = PendingRenameRow & {
  editedName: string;
  status?: "applied" | "skipped" | "working" | "error";
  error?: string;
};

export function ReviewRenamesPage() {
  const [rows, setRows] = useState<RowState[]>([]);
  const [loading, setLoading] = useState(true);

  async function refresh() {
    setLoading(true);
    try {
      const pending = await libraryPendingRenames();
      setRows(pending.map((row) => ({ ...row, editedName: row.proposed_name })));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  const duplicates = useMemo(() => {
    const counts = new Map<string, number>();
    for (const row of rows) {
      const key = row.editedName.trim().toLowerCase();
      if (key) counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return new Set([...counts].filter(([, count]) => count > 1).map(([name]) => name));
  }, [rows]);

  function updateRow(id: number, patch: Partial<RowState>) {
    setRows((current) => current.map((row) => (row.document_id === id ? { ...row, ...patch } : row)));
  }

  async function accept(row: RowState) {
    updateRow(row.document_id, { status: "working", error: undefined });
    try {
      await aiApplyRename(row.document_id, row.editedName);
      updateRow(row.document_id, { status: "applied" });
    } catch (error) {
      updateRow(row.document_id, { status: "error", error: error instanceof Error ? error.message : String(error) });
    }
  }

  async function skip(row: RowState) {
    updateRow(row.document_id, { status: "working", error: undefined });
    try {
      await librarySkipRename(row.document_id);
      updateRow(row.document_id, { status: "skipped" });
    } catch (error) {
      updateRow(row.document_id, { status: "error", error: error instanceof Error ? error.message : String(error) });
    }
  }

  async function regenerate(row: RowState) {
    updateRow(row.document_id, { status: "working", error: undefined });
    try {
      const [proposal] = await aiProposeNames([row.document_id]);
      updateRow(row.document_id, {
        proposed_name: proposal.display_name,
        editedName: proposal.display_name,
        summary: proposal.summary,
        provider: proposal.provider,
        status: undefined,
      });
    } catch (error) {
      updateRow(row.document_id, { status: "error", error: error instanceof Error ? error.message : String(error) });
    }
  }

  async function acceptAll() {
    for (const row of rows.filter((row) => !row.status && !duplicates.has(row.editedName.trim().toLowerCase()))) {
      await accept(row);
    }
  }

  async function skipAll() {
    for (const row of rows.filter((row) => !row.status)) {
      await skip(row);
    }
  }

  const pendingRows = rows.filter((row) => !row.status);

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-5">
      <header className="rounded-xl border border-border bg-card/70 p-6 shadow-2xl shadow-black/20">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.24em] text-muted-foreground">Human checkpoint</p>
            <h1 className="mt-2 text-3xl font-semibold tracking-[-0.055em]">Review AI renames</h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">
              AI proposals are staged only. Accept, edit, regenerate, or skip each filename before anything is renamed on disk.
            </p>
          </div>
          <Button asChild variant="outline"><Link to="/library">Back to Library</Link></Button>
        </div>
      </header>

      {duplicates.size > 0 && (
        <div className="flex items-center gap-2 rounded-xl border border-amber-400/30 bg-amber-400/10 px-4 py-3 text-sm text-amber-100">
          <TriangleAlert className="size-4" /> Duplicate proposed names are highlighted. Edit them before accepting all.
        </div>
      )}

      <section className="overflow-hidden rounded-xl border border-border bg-card/65">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <div>
            <h2 className="text-sm font-medium">Pending names</h2>
            <p className="text-xs text-muted-foreground">{pendingRows.length} awaiting review</p>
          </div>
          <div className="flex gap-2">
            <Button type="button" variant="outline" onClick={() => void skipAll()} disabled={pendingRows.length === 0}>Skip all</Button>
            <Button type="button" onClick={() => void acceptAll()} disabled={pendingRows.length === 0 || duplicates.size > 0}>Accept all</Button>
          </div>
        </div>

        {loading ? (
          <div className="grid place-items-center px-4 py-16 text-muted-foreground"><Loader2 className="size-5 animate-spin" /></div>
        ) : rows.length === 0 ? (
          <div className="px-4 py-16 text-center text-sm text-muted-foreground">No documents are waiting for AI naming.</div>
        ) : (
          <div className="overflow-auto">
            <table className="w-full min-w-[980px] text-left text-sm">
              <thead className="bg-background/40 font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
                <tr>
                  <th className="px-4 py-3">Original</th>
                  <th className="px-4 py-3">Proposed name</th>
                  <th className="px-4 py-3">Summary</th>
                  <th className="px-4 py-3">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {rows.map((row) => {
                  const isDuplicate = duplicates.has(row.editedName.trim().toLowerCase());
                  return (
                    <tr key={row.document_id} className={cn(row.status === "applied" && "bg-emerald-400/5", row.status === "skipped" && "opacity-60")}>
                      <td className="max-w-[16rem] px-4 py-3"><div className="truncate font-medium">{row.original_name}</div><div className="mt-1 font-mono text-[11px] text-muted-foreground">{row.provider}</div></td>
                      <td className="px-4 py-3">
                        <Input value={row.editedName} disabled={!!row.status && row.status !== "error"} onChange={(event) => updateRow(row.document_id, { editedName: event.target.value })} className={cn(isDuplicate && "border-amber-400/60 ring-2 ring-amber-400/20")} />
                        {isDuplicate && <p className="mt-1 text-xs text-amber-200">Duplicate filename proposal.</p>}
                      </td>
                      <td className="max-w-[24rem] px-4 py-3 text-muted-foreground"><p className="line-clamp-3">{row.summary ?? "No summary returned."}</p>{row.error && <p className="mt-1 text-destructive">{row.error}</p>}</td>
                      <td className="px-4 py-3">
                        <div className="flex flex-wrap gap-2">
                          {row.status === "working" ? <Loader2 className="mt-1 size-4 animate-spin text-muted-foreground" /> : null}
                          {!row.status || row.status === "error" ? (
                            <>
                              <Button type="button" size="sm" onClick={() => void accept(row)} disabled={isDuplicate}><Check className="size-4" />Accept</Button>
                              <Button type="button" size="sm" variant="outline" onClick={() => void skip(row)}><SkipForward className="size-4" />Skip</Button>
                              <Button type="button" size="sm" variant="outline" onClick={() => void regenerate(row)}><RefreshCcw className="size-4" />Regenerate</Button>
                            </>
                          ) : row.status === "applied" ? (
                            <Button type="button" size="sm" variant="outline" onClick={() => void aiApplyRename(row.document_id, row.original_name).then(refresh)}><RotateCcw className="size-4" />Undo</Button>
                          ) : (
                            <span className="rounded border border-border px-2 py-1 font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Skipped</span>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
