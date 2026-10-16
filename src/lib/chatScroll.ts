/** Follow streamed content until the reader takes control of the viewport. */
export function attachChatScroll(
  viewport: HTMLElement,
  content: HTMLElement,
  onFollowingChange: (following: boolean) => void
) {
  let following = true;
  let lastTop = viewport.scrollTop;
  let touchY: number | null = null;
  let frame: number | null = null;
  let disposed = false;
  const atBottom = () => viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop <= 4;
  const setFollowing = (value: boolean) => {
    if (following === value) return;
    following = value;
    onFollowingChange(value);
  };
  const scrollToLatest = () => {
    if (frame !== null) return;
    frame = requestAnimationFrame(() => {
      frame = null;
      // Recheck after scheduling: an upward gesture may have arrived this frame.
      if (disposed || !following) return;
      viewport.scrollTop = viewport.scrollHeight;
      lastTop = viewport.scrollTop;
    });
  };
  const onScroll = () => {
    const top = viewport.scrollTop;
    if (top !== lastTop) setFollowing(atBottom());
    lastTop = top;
  };
  const onWheel = (event: WheelEvent) => {
    if (event.deltaY < 0 && viewport.scrollTop > 0) setFollowing(false);
  };
  const onTouchStart = (event: TouchEvent) => { touchY = event.touches[0]?.clientY ?? null; };
  const onTouchMove = (event: TouchEvent) => {
    const nextY = event.touches[0]?.clientY ?? null;
    if (touchY !== null && nextY !== null && nextY > touchY && viewport.scrollTop > 0) setFollowing(false);
    touchY = nextY;
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if ((event.target as HTMLElement).closest("input, textarea, select, [contenteditable=true]")) return;
    if (["ArrowUp", "PageUp", "Home"].includes(event.key) || (event.key === " " && event.shiftKey)) {
      setFollowing(false);
    }
  };
  viewport.addEventListener("scroll", onScroll, { passive: true });
  viewport.addEventListener("wheel", onWheel, { passive: true });
  viewport.addEventListener("touchstart", onTouchStart, { passive: true });
  viewport.addEventListener("touchmove", onTouchMove, { passive: true });
  viewport.addEventListener("keydown", onKeyDown);
  const observer = new ResizeObserver(scrollToLatest);
  observer.observe(content);
  observer.observe(viewport);
  onFollowingChange(true);
  scrollToLatest();
  return {
    jumpToLatest() { setFollowing(true); scrollToLatest(); },
    dispose() {
      disposed = true;
      observer.disconnect();
      if (frame !== null) cancelAnimationFrame(frame);
      viewport.removeEventListener("scroll", onScroll);
      viewport.removeEventListener("wheel", onWheel);
      viewport.removeEventListener("touchstart", onTouchStart);
      viewport.removeEventListener("touchmove", onTouchMove);
      viewport.removeEventListener("keydown", onKeyDown);
    }
  };
}
