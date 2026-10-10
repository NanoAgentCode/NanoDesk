import { invoke } from "@tauri-apps/api/core";

export function checkEnv(nodePath?: string, pythonPath?: string) {
  return invoke<Record<string, boolean>>("check_env", {
    nodePath: nodePath || null,
    pythonPath: pythonPath || null
  });
}

export function installEnv(tech: string) {
  return invoke<boolean>("install_env", { tech });
}
