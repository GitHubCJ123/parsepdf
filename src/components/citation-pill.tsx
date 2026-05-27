import { FileText } from "lucide-react";
import { cn } from "@/lib/utils";

export type CitationSource = {
  index: number;
  chunk_id: number;
  page_id: number;
  document_id: number;
  page_number: number;
  document_name: string;
  excerpt: string;
};

type CitationPillProps = {
  citation: CitationSource;
  onOpen: (citation: CitationSource) => void;
  className?: string;
};

export function CitationPill({ citation, onOpen, className }: CitationPillProps) {
  return (
    <span className="group relative inline-flex align-super">
      <button
        type="button"
        onClick={() => onOpen(citation)}
        className={cn(
          "mx-0.5 inline-flex h-4 min-w-4 items-center justify-center rounded border border-emerald-300/35 bg-emerald-300/10 px-1 font-mono text-[10px] leading-none text-emerald-100 transition-colors hover:border-emerald-200/70 hover:bg-emerald-300/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300/40",
          className,
        )}
        aria-label={`Open citation ${citation.index}: ${citation.document_name}, page ${citation.page_number}`}
      >
        {citation.index}
      </button>
      <span className="pointer-events-none absolute bottom-full left-1/2 z-50 mb-2 hidden w-80 -translate-x-1/2 rounded-lg border border-border bg-popover p-3 text-left text-xs leading-5 text-popover-foreground shadow-2xl shadow-black/40 group-hover:block group-focus-within:block">
        <span className="mb-2 flex items-center gap-2 font-medium text-foreground">
          <FileText className="size-3.5 text-emerald-200" />
          <span className="truncate">{citation.document_name}</span>
          <span className="ml-auto font-mono text-[10px] text-muted-foreground">p.{citation.page_number}</span>
        </span>
        <span className="line-clamp-5 text-muted-foreground">{citation.excerpt}</span>
        <span className="mt-2 block font-mono text-[10px] uppercase tracking-[0.16em] text-emerald-200/80">Click to open preview</span>
      </span>
    </span>
  );
}
