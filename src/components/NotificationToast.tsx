import { Notification } from "@mantine/core";
import { AlertCircle, AlertTriangle, CheckCircle, Info } from "lucide-react";
import { getNoticeData } from "../lib/notices";

interface NotificationToastProps {
  notice: string;
  onClose: () => void;
}

const icons = {
  info: Info,
  success: CheckCircle,
  warning: AlertTriangle,
  error: AlertCircle
};

export default function NotificationToast({ notice, onClose }: NotificationToastProps) {
  if (!notice) return null;

  const { content, type } = getNoticeData(notice);
  const Icon = icons[type];
  const color = type === "error" ? "red" : type === "warning" ? "yellow" : type === "success" ? "teal" : "nanoBlue";

  return (
    <div className="notification-toast-container">
      <Notification
        className="notification-toast"
        color={color}
        icon={<Icon size={16} />}
        onClose={onClose}
        role="status"
        aria-live="polite"
        withBorder
      >
        {content}
      </Notification>
    </div>
  );
}
