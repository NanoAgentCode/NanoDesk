import type { ReactNode } from "react";

export default function SettingsSectionHeader({ title, children }: { title: string; children?: ReactNode }) {
  return <div className="model-header-row"><h3>{title}</h3>{children}</div>;
}
