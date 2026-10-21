import { useEffect, useRef, useState } from "react";
import { APP_STORAGE_PREFIX } from "../config/brand";
import { clampSidebarWidth, parseSidebarWidth } from "../lib/sidebarSizing";
import { beginResize } from "../lib/layoutResize";

const SIDEBAR_COLLAPSED_KEY = `${APP_STORAGE_PREFIX}-sidebar-collapsed`;
const SIDEBAR_WIDTH_KEY = `${APP_STORAGE_PREFIX}-sidebar-width`;

export function useAppLayout() {
  const workspaceRef = useRef<HTMLElement | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() =>
    localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === "true"
  );
  const [sidebarWidth, setSidebarWidth] = useState(() =>
    parseSidebarWidth(localStorage.getItem(SIDEBAR_WIDTH_KEY))
  );
  const [workspaceListRatio, setWorkspaceListRatio] = useState(38);

  useEffect(() => {
    localStorage.setItem(SIDEBAR_COLLAPSED_KEY, String(sidebarCollapsed));
  }, [sidebarCollapsed]);

  useEffect(() => {
    localStorage.setItem(SIDEBAR_WIDTH_KEY, String(sidebarWidth));
  }, [sidebarWidth]);

  function beginWorkspaceSplitResize() {
    const rect = workspaceRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }

    beginResize((event) => {
      const nextRatio = ((event.clientY - rect.top) / rect.height) * 100;
      setWorkspaceListRatio(Math.min(70, Math.max(24, nextRatio)));
    }, "row-resize");
  }

  function beginSidebarResize() {
    beginResize((event) => {
      setSidebarWidth(clampSidebarWidth(event.clientX));
    });
  }

  return { workspaceRef, sidebarCollapsed, setSidebarCollapsed, sidebarWidth, setSidebarWidth,
    workspaceListRatio, beginWorkspaceSplitResize, beginSidebarResize };
}
