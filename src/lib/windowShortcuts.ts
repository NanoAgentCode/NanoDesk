export function registerWindowShortcuts(
  target: EventTarget,
  minimize: () => Promise<void>,
  setNotice: (message: string) => void,
  hasOpenDialog: () => boolean = () => false
) {
  const handleKeyDown = (event: Event) => {
    const keyEvent = event as KeyboardEvent;
    if (keyEvent.key !== "Escape" || keyEvent.repeat || keyEvent.isComposing ||
      keyEvent.ctrlKey || keyEvent.altKey || keyEvent.shiftKey || keyEvent.metaKey) {
      return;
    }
    // Let the dialog handle Escape without minimizing on the same key press.
    if (hasOpenDialog()) return;
    keyEvent.preventDefault();
    keyEvent.stopImmediatePropagation();
    void minimize().catch((error) => setNotice(`最小化窗口失败：${String(error)}`));
  };

  target.addEventListener("keydown", handleKeyDown, { capture: true });
  return () => target.removeEventListener("keydown", handleKeyDown, { capture: true });
}
