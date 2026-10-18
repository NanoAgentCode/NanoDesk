import { Button, UnstyledButton } from "@mantine/core";
import { ArrowRight, BookOpen, Bug, CheckCircle2, Lightbulb, ListChecks, ScanSearch, Settings2, Sparkles } from "lucide-react";

const PROJECT_STARTER_ACTIONS = [
  {
    title: "快速了解项目",
    description: "梳理用途、技术栈和关键入口",
    prompt: "请先快速了解当前项目，概括它的用途、技术栈和主要目录，并告诉我最值得先关注的三个入口。",
    icon: ScanSearch
  },
  {
    title: "排查潜在问题",
    description: "按优先级识别稳定性与维护风险",
    prompt: "请检查当前项目中最可能影响稳定性或维护性的风险，按优先级给出证据和建议。",
    icon: Bug
  },
  {
    title: "规划开发任务",
    description: "澄清目标、拆解步骤和验证方式",
    prompt: "我准备开始一个开发任务。请根据当前项目上下文，帮我澄清目标、拆解步骤，并为每一步给出验证方式。",
    icon: ListChecks
  }
] as const;

const GENERAL_STARTER_ACTIONS = [
  {
    title: "梳理一个想法",
    description: "从模糊想法提炼目标和下一步",
    prompt: "我有一个还没完全想清楚的想法。请通过几个关键问题，帮我澄清目标、约束和下一步。",
    icon: Lightbulb
  },
  {
    title: "解释复杂内容",
    description: "提炼核心概念、结论与误区",
    prompt: "我会贴一段内容。请用清晰的结构解释其中的核心概念、关键结论和可能的误区。",
    icon: BookOpen
  },
  {
    title: "制定行动计划",
    description: "把目标拆成可验证的执行步骤",
    prompt: "我准备推进一件事。请先帮我澄清目标，再拆解成可执行步骤，并为每一步给出验证方式。",
    icon: ListChecks
  }
] as const;


interface ChatWelcomeProps {
  projectName: string;
  activeModel?: { name: string };
  onStarterAction: (prompt: string) => Promise<void>;
  onOpenModelSettings: () => void;
}

export default function ChatWelcome({ projectName, activeModel, onStarterAction: handleStarterAction, onOpenModelSettings }: ChatWelcomeProps) {
  const starterActions = projectName ? PROJECT_STARTER_ACTIONS : GENERAL_STARTER_ACTIONS;
  return (
          <section className="chat-welcome" aria-label="开始对话">
            <div className="chat-welcome-mark">
              <Sparkles size={16} />
              <span>{projectName ? "项目上下文已就绪" : "本地工作区"}</span>
            </div>
            <h1>{projectName ? `从 ${projectName} 开始` : "今天想推进什么？"}</h1>
            <p>
              {projectName
                ? "选择一个常用任务，我会把内容放入输入框供你确认；也可以直接描述目标。"
                : "你可以直接开始对话，或先打开一个项目，让回答带上代码和文档上下文。"}
            </p>
            <div className="chat-starter-grid">
              {starterActions.map((action) => {
                const ActionIcon = action.icon;
                return (
                  <UnstyledButton
                    key={action.title}
                    className="chat-starter-card"
                    onClick={() => void handleStarterAction(action.prompt)}
                  >
                    <ActionIcon size={18} />
                    <span>
                      <strong>{action.title}</strong>
                      <small>{action.description}</small>
                    </span>
                    <ArrowRight size={15} className="chat-starter-arrow" />
                  </UnstyledButton>
                );
              })}
            </div>
            <div className={`chat-ready-status ${activeModel ? "ready" : "warning"}`}>
              {activeModel ? <CheckCircle2 size={15} /> : <Settings2 size={15} />}
              <span>{activeModel ? `已选择 ${activeModel.name}` : "发送前需要先配置一个模型"}</span>
              {!activeModel && (
                <Button variant="subtle" size="compact-sm" onClick={onOpenModelSettings} rightSection={<ArrowRight size={13} />}>
                  配置模型
                </Button>
              )}
            </div>
          </section>
  );
}
