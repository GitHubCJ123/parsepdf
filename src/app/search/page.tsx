import { Search } from "lucide-react";
import { EmptyState } from "@/components/empty-state";

export function SearchPage() {
  return (
    <EmptyState
      icon={Search}
      title="Nothing indexed yet."
      description="Full-text results and snippets will appear after the first PDF is processed."
      actionLabel="Focus search"
    />
  );
}
