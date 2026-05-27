import { MessageSquareText } from "lucide-react";
import { EmptyState } from "@/components/empty-state";

export function ChatPage() {
  return (
    <EmptyState
      icon={MessageSquareText}
      title="No sources to chat with."
      description="Document-aware answers and citations will unlock after indexing."
      actionLabel="Open library"
    />
  );
}
