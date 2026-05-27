import { Settings } from "lucide-react";
import { EmptyState } from "@/components/empty-state";

export function SettingsPage() {
  return (
    <EmptyState
      icon={Settings}
      title="Settings are ready."
      description="OCR engines, AI providers, watched folders, and output paths land in later phases."
      actionLabel="Open data folder"
    />
  );
}
