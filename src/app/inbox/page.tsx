import { open } from "@tauri-apps/plugin-dialog";
import { Inbox } from "lucide-react";
import { EmptyState } from "@/components/empty-state";

export function InboxPage() {
  async function chooseFolder() {
    try {
      await open({ directory: true, multiple: false });
    } catch (error) {
      console.error("Unable to open folder picker", error);
    }
  }

  return (
    <EmptyState
      icon={Inbox}
      title="No PDFs in the queue."
      description="Drag a file here or pick a folder to watch."
      actionLabel="Choose folder"
      onAction={() => {
        void chooseFolder();
      }}
    />
  );
}
