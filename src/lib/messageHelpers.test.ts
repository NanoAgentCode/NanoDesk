import { describe, expect, it } from "vitest";

import {
  buildAutomaticClarificationAnswers,
  findPendingClarification,
  formatClarificationAnswerMessage,
  getLastResponseRegenerationContext,
  parseClarificationRequest,
  parseTaskPlan,
  stripClarificationRequest
} from "./messageHelpers";

describe("clarification supersession", () => {
  it("invalidates old clarification only when a trusted mainline update supersedes it",()=>{
    const clarification={id:"old",role:"assistant",content:'<clarification>{"questions":[{"id":"q","prompt":"Old question?","options":[{"id":"a","label":"A","description":"A","recommended":true},{"id":"b","label":"B","description":"B","recommended":false}],"allow_custom":true}]}</clarification>'} as import("../types").PersistedMessage;
    expect(findPendingClarification([clarification])?.messageId).toBe("old");
    const boundary={id:"update",role:"user",content:"[临时任务结束，更新主线] changed",metadata:{task_control:true}} as import("../types").PersistedMessage;
    expect(findPendingClarification([clarification,boundary])).toBeNull();
    expect(findPendingClarification([clarification,{...boundary,metadata:undefined}])?.messageId).toBe("old");
    expect(findPendingClarification([clarification,boundary,{...clarification,id:"new"}])?.messageId).toBe("new");
  });

});

describe("task plans", () => {
  const content = `<task_plan>{"goal":"完成发布","steps":[{"id":"inspect","title":"检查改动","status":"completed"},{"id":"verify","title":"运行验证","status":"in_progress"},{"id":"publish","title":"提交推送","status":"pending"}]}</task_plan>`;

  it("parses a valid structured plan", () => {
    const plan = parseTaskPlan(content);
    expect(plan?.goal).toBe("完成发布");
    expect(plan?.steps).toHaveLength(3);
    expect(plan?.steps[0]).toMatchObject({ id: "inspect", status: "completed" });
    expect(plan?.steps[1]).toMatchObject({ id: "verify", status: "in_progress" });
  });

  it("rejects duplicate ids, unsupported statuses, and multiple active steps", () => {
    expect(parseTaskPlan(`<task_plan>{"goal":"x","steps":[{"id":"same","title":"A","status":"pending"},{"id":"same","title":"B","status":"pending"}]}</task_plan>`)).toBeNull();
    expect(parseTaskPlan(`<task_plan>{"goal":"x","steps":[{"id":"a","title":"A","status":"running"},{"id":"b","title":"B","status":"pending"}]}</task_plan>`)).toBeNull();
    expect(parseTaskPlan(`<task_plan>{"goal":"x","steps":[{"id":"a","title":"A","status":"in_progress"},{"id":"b","title":"B","status":"in_progress"}]}</task_plan>`)).toBeNull();
  });
});

describe("clarification messages", () => {
  const content = `<clarification>{"questions":[{"id":"theme","prompt":"选择主题？","options":[{"id":"gallery","label":"画廊对比","recommended":true},{"id":"keep","label":"保持现状"}],"allow_custom":true}]}</clarification>`;

  it("parses a structured clarification request", () => {
    expect(parseClarificationRequest(content)).toMatchObject({
      questions: [{
        id: "theme",
        options: [{ id: "gallery", recommended: true }, { id: "keep", recommended: false }]
      }]
    });
  });

  it("removes the structured payload while preserving surrounding assistant text", () => {
    expect(stripClarificationRequest(`先确认两个问题：\n\n${content}\n\n请选择最符合的一项。`))
      .toBe("先确认两个问题：\n\n请选择最符合的一项。");
    expect(stripClarificationRequest(content)).toBe("");
  });

  it("rejects malformed or ambiguous option payloads", () => {
    expect(parseClarificationRequest("<clarification>{bad}</clarification>")).toBeNull();
    expect(parseClarificationRequest(`<clarification>{"questions":[{"id":"q","prompt":"?","options":[{"id":"same","label":"A"},{"id":"same","label":"B"}]}]}</clarification>`)).toBeNull();
  });

  it("keeps clarification pending until its answer message exists", () => {
    const assistant = { id: "m1", conversation_id: "c1", role: "assistant" as const, content, created_at: "2026-09-07T00:00:00Z" };
    const request = parseClarificationRequest(content)!;
    expect(findPendingClarification([assistant])?.messageId).toBe("m1");
    const answer = {
      id: "m2", conversation_id: "c1", role: "user" as const,
      content: formatClarificationAnswerMessage("m1", request, [{ question_id: "theme", option_id: "gallery" }], true),
      created_at: "2026-09-07T00:00:01Z"
    };
    expect(findPendingClarification([assistant, answer])).toBeNull();
    expect(answer.content).toContain("（自动选择）");
  });

  it("uses recommended options and falls back to the first option in automatic mode", () => {
    const request = parseClarificationRequest(`<clarification>{"questions":[{"id":"with-recommendation","prompt":"A?","options":[{"id":"first","label":"First"},{"id":"best","label":"Best","recommended":true}]},{"id":"without-recommendation","prompt":"B?","options":[{"id":"fallback","label":"Fallback"},{"id":"other","label":"Other"}]}]}</clarification>`)!;
    expect(buildAutomaticClarificationAnswers(request)).toEqual([
      { question_id: "with-recommendation", option_id: "best" },
      { question_id: "without-recommendation", option_id: "fallback" }
    ]);
  });
});

describe("response regeneration", () => {
  const messages = [
    { id: "u1", conversation_id: "c1", role: "user" as const, content: "问题", created_at: "2026-09-11T00:00:00Z" },
    { id: "a1", conversation_id: "c1", role: "assistant" as const, content: "回答", created_at: "2026-09-11T00:00:01Z" }
  ];

  it("uses the history before the latest assistant answer", () => {
    expect(getLastResponseRegenerationContext(messages, "a1")).toEqual({
      previousMessages: [messages[0]],
      triggerMessage: messages[0]
    });
  });

  it("rejects non-latest or non-assistant targets", () => {
    expect(getLastResponseRegenerationContext(messages, "u1")).toBeNull();
    expect(getLastResponseRegenerationContext([...messages, { ...messages[0], id: "u2" }], "a1")).toBeNull();
  });
});

describe("overall plan completion", () => {
  const plan = { goal: "生成 PDF", steps: [{ id: "word", title: "Word 导出", status: "blocked" }, { id: "fallback", title: "备用生成验证", status: "completed" }], outcome: { status: "completed", detail: "PDF 已验证" } };
  it("retains historical block and overall completion evidence together", () => {
    const parsed = parseTaskPlan(`<task_plan>${JSON.stringify(plan)}</task_plan>`);
    expect(parsed?.steps[0].status).toBe("blocked");
    expect(parsed?.outcome).toEqual(plan.outcome);
  });
  it("rejects completion with outstanding steps or no evidence", () => {
    expect(parseTaskPlan(`<task_plan>${JSON.stringify({ ...plan, steps: [plan.steps[0], { ...plan.steps[1], status: "pending" }] })}</task_plan>`)).toBeNull();
    expect(parseTaskPlan(`<task_plan>${JSON.stringify({ ...plan, outcome: { status: "completed", detail: "" } })}</task_plan>`)).toBeNull();
  });
});
