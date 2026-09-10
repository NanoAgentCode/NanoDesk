import { describe, expect, it } from "vitest";

import {
  buildAutomaticClarificationAnswers,
  findPendingClarification,
  formatClarificationAnswerMessage,
  parseClarificationRequest,
  parseTaskPlan,
  resolveUserMemoryRoute
} from "./messageHelpers";

describe("resolveUserMemoryRoute", () => {
  it("routes an explicit profile preference only to profile analysis", () => {
    expect(resolveUserMemoryRoute("请记住我的回答偏好是简洁中文")).toEqual({
      kind: "profile",
      memoryDraft: null
    });
  });

  it("routes an explicit ordinary-memory request only to memory", () => {
    expect(resolveUserMemoryRoute("记住：项目发布前运行 cargo test")).toMatchObject({
      kind: "memory",
      memoryDraft: {
        content: "项目发布前运行 cargo test",
        tags: ["chat"],
        enabled: true
      }
    });
  });

  it("leaves ordinary chat on automatic profile-candidate handling", () => {
    expect(resolveUserMemoryRoute("帮我检查这段代码")).toEqual({
      kind: "auto",
      memoryDraft: null
    });
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
