import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MantineProvider } from "@mantine/core";
import { formatUserFacingError, getNoticeData } from "./notices";
import NotificationToast from "../components/NotificationToast";
import FriendlyError from "../components/FriendlyError";

const commandError = "Command failed with code Some(1)\nStdout: success\nStderr: ���\n+ CategoryInfo: ParserError\n+ FullyQualifiedErrorId: UnexpectedToken\nprivate diagnostic";

describe("user-facing error presentation", () => {
  it("hides raw command output in the real notification component", () => {
    const html = renderToStaticMarkup(<MantineProvider><NotificationToast notice={commandError} onClose={() => {}} /></MantineProvider>);
    expect(html).toContain("命令执行失败，请重试。");
    for (const raw of ["Some(1)", "Stdout", "Stderr", "CategoryInfo", "UnexpectedToken", "private diagnostic", "���"]) {
      expect(html).not.toContain(raw);
    }
    expect(getNoticeData(commandError).type).toBe("error");
  });

  it("retains diagnostics only in a closed details section", () => {
    const html = renderToStaticMarkup(<FriendlyError error={commandError} />);
    expect(html).toContain('<p role="alert">命令执行失败，请重试。</p>');
    expect(html).toContain("<details>");
    expect(html).not.toContain("<details open");
    expect(html).toContain("查看错误详情");
    expect(html).toContain("private diagnostic");
  });

  it.each([
    ["MCP tool call failed: provider internal output", "工具调用失败，请重试。"],
    ["Command timed out after 45 seconds", "命令执行超时，请稍后重试。"],
    [new Error("Failed to fetch"), "网络连接失败，请稍后重试。"],
    ["保存记忆失败：SQLITE_BUSY: locked", "保存记忆失败，请重试。"],
    ["任务预算已达到上限: raw budget json", "任务预算不足，请调整预算后继续。"],
    [{ message: "private backend payload" }, "操作失败，请重试。"],
    ["permission denied", "操作失败，请重试。"],
  ])("formats error %s into a short actionable summary", (input, expected) => {
    expect(formatUserFacingError(input)).toBe(expected);
  });

  it("keeps normal success, validation and progress notices intact", () => {
    expect(getNoticeData("配置已保存。")).toEqual({ content: "配置已保存。", type: "success" });
    expect(getNoticeData("请先保存并选择一个模型")).toEqual({ content: "请先保存并选择一个模型", type: "warning" });
    expect(getNoticeData("未检测到 Tavily CLI，请先安装。").content).toBe("未检测到 Tavily CLI，请先安装。");
    expect(getNoticeData("正在建立项目索引")).toEqual({ content: "正在建立项目索引", type: "info" });
    expect(getNoticeData("connection refused: backend payload").content).toBe("网络连接失败，请稍后重试。");
    expect(getNoticeData("SQLITE_BUSY: database is locked").content).toBe("操作失败，请重试。");
  });
});
