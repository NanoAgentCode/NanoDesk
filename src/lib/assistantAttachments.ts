export type AssistantAttachmentStatus = "pending" | "processing" | "success" | "failed";
export interface AssistantAttachmentProgress { index: number; status: AssistantAttachmentStatus; error?: string }
export const assistantAttachmentLabels = { pending: "待发送", processing: "处理中", success: "处理成功", failed: "处理失败" };

/** Continue after a file fails so mixed results remain accurate for every file. */
export async function processAssistantAttachments(count: number, process: (index: number) => Promise<void>, report: (progress: AssistantAttachmentProgress) => Promise<unknown>) {
  const errors: string[] = [];
  for (let index = 0; index < count; index++) {
    await report({ index, status: "processing" });
    try {
      await process(index);
      await report({ index, status: "success" });
    } catch (error) {
      const detail = String(error).replace(/^Error: /, "");
      errors.push(detail);
      await report({ index, status: "failed", error: detail });
    }
  }
  if (errors.length) throw new Error(errors.join("；"));
}
