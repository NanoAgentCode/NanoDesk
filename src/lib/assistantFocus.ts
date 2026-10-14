export function createAssistantBlurTimer(collapse: () => void) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  function cancel() { if (timer !== undefined) clearTimeout(timer); timer = undefined; }
  return {
    setFocused(focused: boolean) {
      if (focused) cancel();
      else if (timer === undefined) timer = setTimeout(() => { timer = undefined; collapse(); }, 5000);
    },
    dispose: cancel
  };
}
