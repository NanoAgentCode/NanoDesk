import React from "react";
import { createRoot } from "react-dom/client";
import { MantineProvider } from "@mantine/core";
import { useModel } from "../../src/hooks/useModel";
import SettingsModelTab from "../../src/components/settings/SettingsModelTab";
import "@mantine/core/styles.css";
import "../../src/styles/main.css";
import "../../src/components/settings/SettingsModal.css";
function Harness() {
  const model = useModel((message) => { (window as any).notice = message; }, "", () => {});
  (window as any).model = model;
  return <MantineProvider><SettingsModelTab model={model} setShowModelConfig={() => {}} /></MantineProvider>;
}
createRoot(document.getElementById("root")!).render(<Harness />);
