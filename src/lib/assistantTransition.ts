export const ASSISTANT_TRANSITION_MS = 220;
export type AssistantPhase = "icon" | "preparing" | "opening" | "open" | "closing";
type Presentation = "icon" | "input" | "picker";

// Native window commands cannot be canceled. Serialize them and ignore obsolete completions.
export function createAssistantTransition(
  resize: (presentation: Presentation) => Promise<unknown>,
  onPhase: (phase: AssistantPhase) => void,
  onError: (error: unknown, expanding: boolean) => void
) {
  let phase: AssistantPhase = "icon";
  let target: Presentation | null = "icon";
  let revision = 0;
  let disposed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending = Promise.resolve();
  const clearTimer = () => { if (timer !== undefined) clearTimeout(timer); timer = undefined; };
  const publish = (next: AssistantPhase) => { phase = next; onPhase(next); };

  function update(next: Presentation) {
    if (disposed || target === next) return;
    target = next;
    const version = ++revision;
    clearTimer();
    const current = () => !disposed && version === revision;
    const enqueue = (done: () => void) => {
      pending = pending.then(async () => {
        if (!current()) return;
        await resize(next);
        if (current()) done();
      }).catch(error => {
        if (!current()) return;
        target = null;
        publish(next === "icon" ? "open" : "icon");
        onError(error, next !== "icon");
      });
    };
    if (next === "icon") {
      if (phase === "preparing" || phase === "icon") { enqueue(() => publish("icon")); return; }
      publish("closing");
      timer = setTimeout(() => { timer = undefined; enqueue(() => publish("icon")); }, ASSISTANT_TRANSITION_MS);
    } else {
      const alreadyOpen = phase === "open";
      if (phase === "icon") publish("preparing");
      enqueue(() => {
        if (alreadyOpen) return;
        publish("opening");
        timer = setTimeout(() => { timer = undefined; if (current()) publish("open"); }, ASSISTANT_TRANSITION_MS);
      });
    }
  }
  return { update, dispose() { disposed = true; revision++; clearTimer(); } };
}
