import { Library } from "lucide-react";
import { EmptyState } from "@/components/empty-state";

export function LibraryPage() {
  return (
    <EmptyState
      icon={Library}
      title="No documents yet."
      description="Processed PDFs will appear here with names, pages, and source paths."
      actionLabel="Process file"
    />
  );
}
