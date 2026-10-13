import type { ReactNode } from "react";

interface WorkspaceItemRowProps {
  selected: boolean;
  onSelect: () => void;
  badge: ReactNode;
  status: string;
  title: string;
  preview: string;
}

export default function WorkspaceItemRow({ selected, onSelect, badge, status, title, preview }: WorkspaceItemRowProps) {
  return (
    <button className={selected ? "item-row selected" : "item-row"} onClick={onSelect}>
      <div className="item-row-header">
        {badge}
        <span className="status-indicator">{status}</span>
      </div>
      <strong>{title}</strong>
      <small>{preview}</small>
    </button>
  );
}
