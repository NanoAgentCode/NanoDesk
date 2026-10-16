"""清空 NanoAgent 的执行记录（agent runs / steps / tool calls / 预算记录），
并删除聊天记录里残留的"待审批工具调用"消息，让会话可以干净重试。

用法：先完全退出 NanoAgent，然后运行：
    py scripts/clear_agent_runs.py
"""

import os
import sqlite3
import sys

BASE = os.path.join(os.environ.get("APPDATA", ""), "com.nanodesk.desktop")
RUNTIME_DB = os.path.join(BASE, "nanodesk-runtime.sqlite3")
CONVERSATIONS_DB = os.path.join(BASE, "nanodesk-conversations.sqlite3")


def clear_runtime() -> None:
    conn = sqlite3.connect(RUNTIME_DB)
    try:
        conn.execute("PRAGMA foreign_keys = ON")
        runs = conn.execute("SELECT COUNT(*) FROM agent_runs").fetchone()[0]
        # agent_steps / agent_tool_calls / agent_execution_requests 靠外键级联删除
        conn.execute("DELETE FROM agent_runs")
        budgets = conn.execute("SELECT COUNT(*) FROM task_budget_records").fetchone()[0]
        conn.execute("DELETE FROM task_budget_records")
        conn.commit()
        print(f"runtime: 删除 {runs} 条 run 记录、{budgets} 条预算记录")
    finally:
        conn.close()


def clear_stale_tool_messages() -> None:
    """删除没有执行结果的工具调用助手消息（它们会渲染成点不动的审批卡）。"""
    conn = sqlite3.connect(CONVERSATIONS_DB)
    try:
        rows = conn.execute(
            "SELECT id, conversation_id, role, content FROM messages "
            "ORDER BY conversation_id, created_at"
        ).fetchall()
        stale = []
        for index, (mid, cid, role, content) in enumerate(rows):
            if role != "assistant" or "<tool_call" not in content:
                continue
            has_result = any(
                later[1] == cid
                and later[2] == "user"
                and later[3].startswith("[工具执行结果:")
                for later in rows[index + 1 :]
            )
            if not has_result:
                stale.append(mid)
        conn.executemany(
            "DELETE FROM messages WHERE id = ?", [(mid,) for mid in stale]
        )
        conn.commit()
        print(f"conversations: 删除 {len(stale)} 条残留工具调用消息")
    finally:
        conn.close()


def main() -> int:
    if not os.path.exists(RUNTIME_DB):
        print(f"找不到运行时数据库：{RUNTIME_DB}", file=sys.stderr)
        return 1
    try:
        clear_runtime()
        if os.path.exists(CONVERSATIONS_DB):
            clear_stale_tool_messages()
    except sqlite3.OperationalError as err:
        print(f"数据库被占用（{err}）：请先完全退出 NanoAgent 再运行。", file=sys.stderr)
        return 1
    print("完成。可以重新打开 NanoAgent 重试任务。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
