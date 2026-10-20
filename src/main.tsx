import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import AssistantMode from "./components/AssistantMode";
import DocumentChat from "./components/DocumentChat";

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

const documentPath = new URLSearchParams(window.location.search).get("document-chat");
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {documentPath !== null ? <DocumentChat path={documentPath} /> : new URLSearchParams(window.location.search).has("assistant") ? <AssistantMode /> : <App />}
  </React.StrictMode>
);

const STARTUP_MIN_VISIBLE_MS = 1200;
const STARTUP_FADE_MS = 260;

window.requestAnimationFrame(() => {
  window.setTimeout(() => {
    const startup = document.getElementById("nano-startup");
    if (!startup) return;
    startup.classList.add("nano-startup--hide");
    window.setTimeout(() => startup.remove(), STARTUP_FADE_MS);
  }, STARTUP_MIN_VISIBLE_MS);
});
