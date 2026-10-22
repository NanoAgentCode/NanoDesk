import React from "react";
import ReactDOM from "react-dom/client";
import WindowEntry from "./WindowEntry";

import "@mantine/core/styles.css";

// Global styles
import "./styles/main.css";

// Component styles
import "./components/Sidebar.css";
import "./components/ChatPane.css";
import "./components/OpsPanel.css";
import "./components/WorkspaceGrid.css";
import "./components/AgentRuntimePanel.css";
import "./components/settings/SettingsUsageTab.css";
import "./components/ObservabilityDetailPanel.css";
import "./components/settings/SettingsModal.css";
import "./components/settings/SettingsSkillsTab.css";
import "./components/settings/SettingsMcpTab.css";
import "./components/settings/SettingsEnvironmentTab.css";

function ReadyWindow() {
  React.useEffect(() => {
    const timer = window.setTimeout(() => {
      const startup = document.getElementById("nano-startup");
      if (!startup) return;
      startup.classList.add("nano-startup--hide");
      window.setTimeout(() => startup.remove(), 260);
    }, 1200);
    return () => window.clearTimeout(timer);
  }, []);
  return <WindowEntry search={window.location.search} />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <React.Suspense fallback={null}>
      <ReadyWindow />
    </React.Suspense>
  </React.StrictMode>
);
