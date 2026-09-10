import type { ProjectEntry } from "../types";

export function projectNameFromPath(path: string) {
  const normalized = path.replace(/[\\/]+$/, "");
  return normalized.split(/[\\/]/).pop() || normalized || "未命名项目";
}

export function mergeRecoveredProjects(
  current: ProjectEntry[],
  recoveredPaths: string[],
  openedAt: string
) {
  const knownPaths = new Set(current.map((project) => project.path.toLowerCase()));
  const recovered = recoveredPaths
    .filter((path) => {
      const key = path.toLowerCase();
      if (knownPaths.has(key)) return false;
      knownPaths.add(key);
      return true;
    })
    .map((path) => ({
      id: path,
      name: projectNameFromPath(path),
      path,
      opened_at: openedAt
    }));
  return recovered.length === 0 ? current : [...current, ...recovered];
}
