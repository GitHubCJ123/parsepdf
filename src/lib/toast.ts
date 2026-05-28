
import { toast } from "sonner";

export function notifySuccess(message: string) {
  toast.success(message, { duration: 3000 });
}

export function notifyInfo(message: string) {
  toast(message, { duration: 3000 });
}

export function notifyError(message: string) {
  toast.error(message, { duration: 5000 });
}
