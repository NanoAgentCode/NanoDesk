import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import TaskSummaryPanel from "../../src/components/TaskSummaryPanel";
import "../../src/styles/_base.css";

function Fixture() {
  const [conversationId, setConversationId] = useState("a");
  const [revision, setRevision] = useState(0);
  return <div className="app-shell" style={{ background: "#f7f8fa", color: "#20252d", "--border-color": "#dde1e7", "--text-primary": "#20252d", "--text-secondary": "#697483", "--chat-pane-bg": "#ffffff" } as React.CSSProperties}>
    <nav style={{ gridArea: "sidebar", padding: 20 }}><h3>NanoDesk</h3><button onClick={() => setConversationId("a")}>会话 A</button><button onClick={() => setConversationId("b")}>会话 B</button><button onClick={() => setRevision(n => n + 1)}>完成一轮</button></nav>
    <div className="chat-workspace"><main className="chat-pane" style={{ padding: 32 }}><h1>设计任务规划</h1><p>聊天正文保持原样，规划步骤和进度展示在右侧。</p></main><TaskSummaryPanel conversationId={conversationId} revision={String(revision)} busy={false} /></div>
  </div>;
}
createRoot(document.getElementById("root")!).render(<Fixture />);
