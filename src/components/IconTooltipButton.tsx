import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Tooltip } from "@mantine/core";

interface IconTooltipButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "children" | "title"> {
  label: string;
  children: ReactNode;
  tone?: "default" | "success" | "danger";
  tooltipOpened?: boolean;
}

export default function IconTooltipButton({ label, children, tone = "default", tooltipOpened, className = "", ...props }: IconTooltipButtonProps) {
  const toneClass = tone === "success" ? "success-btn" : tone === "danger" ? "danger-btn" : "";
  const buttonClassName = ["icon-text-btn", "settings-icon-action", toneClass, className].filter(Boolean).join(" ");

  return (
    <Tooltip label={label} openDelay={450} opened={tooltipOpened} withArrow>
      <span className="icon-tooltip-target">
        <button type="button" {...props} className={buttonClassName} aria-label={label}>
          {children}
        </button>
      </span>
    </Tooltip>
  );
}
