import { Button, TextInput } from "@mantine/core";
import { Check, CircleHelp, Hammer, SkipForward } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type {
  AgentClarificationAnswer,
  AgentClarificationRequest
} from "../types";
import type { ParsedToolCall } from "../lib/messageHelpers";

interface ToolDecision {
  messageId: string;
  toolCall: ParsedToolCall;
}

interface ClarificationDecision {
  messageId: string;
  request: AgentClarificationRequest;
}

interface ChatDecisionPanelProps {
  tool: ToolDecision | null;
  clarification: ClarificationDecision | null;
  disabled: boolean;
  onRunTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  onRejectTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  onSubmitClarification: (
    messageId: string,
    request: AgentClarificationRequest,
    answers: AgentClarificationAnswer[]
  ) => Promise<void>;
}

export function collectCompletedClarificationAnswers(
  request: AgentClarificationRequest,
  answers: Record<string, AgentClarificationAnswer>,
  answer: AgentClarificationAnswer
) {
  const nextAnswers = { ...answers, [answer.question_id]: answer };
  const completedAnswers = request.questions.every((question) => nextAnswers[question.id])
    ? request.questions.map((question) => nextAnswers[question.id])
    : null;
  return { nextAnswers, completedAnswers };
}

export default function ChatDecisionPanel({
  tool,
  clarification,
  disabled,
  onRunTool,
  onRejectTool,
  onSubmitClarification
}: ChatDecisionPanelProps) {
  const [questionIndex, setQuestionIndex] = useState(0);
  const [answers, setAnswers] = useState<Record<string, AgentClarificationAnswer>>({});
  const [customQuestionId, setCustomQuestionId] = useState<string | null>(null);
  const [customText, setCustomText] = useState("");
  const panelRef = useRef<HTMLElement | null>(null);
  const submittingRef = useRef(false);

  useEffect(() => {
    setQuestionIndex(0);
    setAnswers({});
    setCustomQuestionId(null);
    setCustomText("");
    submittingRef.current = false;
  }, [clarification?.messageId]);

  const question = clarification?.request.questions[questionIndex];

  useEffect(() => {
    if (!question || disabled) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
      const numericIndex = Number(event.key) - 1;
      const option = question.options[numericIndex];
      if (!option) return;
      event.preventDefault();
      completeCurrentQuestion({ question_id: question.id, option_id: option.id });
      setCustomQuestionId(null);
      setCustomText("");
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [disabled, question]);

  if (!tool && !clarification) return null;

  if (tool) {
    return (
      <section className="chat-decision-panel tool" aria-label="工具调用确认">
        <header>
          <span className="chat-decision-icon"><Hammer size={17} /></span>
          <div>
            <strong>是否允许运行工具？</strong>
            <small>请检查工具及参数后选择</small>
          </div>
        </header>
        <div className="chat-decision-tool">
          <code>{tool.toolCall.name}</code>
          {Object.entries(tool.toolCall.args).map(([key, value]) => (
            <div key={key}>
              <span>{key}</span>
              <pre>{value}</pre>
            </div>
          ))}
        </div>
        <footer>
          <Button variant="default" disabled={disabled} onClick={() => void onRejectTool(tool.messageId, tool.toolCall)}>
            拒绝
          </Button>
          <Button disabled={disabled} onClick={() => void onRunTool(tool.messageId, tool.toolCall)}>
            运行工具
          </Button>
        </footer>
      </section>
    );
  }

  if (!clarification || !question) return null;
  const activeClarification = clarification;
  const activeQuestion = question;
  const selectedAnswer = answers[activeQuestion.id];
  const isCustom = customQuestionId === activeQuestion.id;

  function answerQuestion(answer: AgentClarificationAnswer) {
    const { nextAnswers, completedAnswers } = collectCompletedClarificationAnswers(
      activeClarification.request,
      answers,
      answer
    );
    setAnswers(nextAnswers);
    if (!completedAnswers || submittingRef.current) return;
    submittingRef.current = true;
    void onSubmitClarification(
      activeClarification.messageId,
      activeClarification.request,
      completedAnswers
    ).finally(() => {
      submittingRef.current = false;
    });
  }

  function completeCurrentQuestion(answer: AgentClarificationAnswer) {
    answerQuestion(answer);
    if (questionIndex < activeClarification.request.questions.length - 1) {
      setQuestionIndex(questionIndex + 1);
    }
  }

  function selectOption(optionId: string) {
    completeCurrentQuestion({ question_id: activeQuestion.id, option_id: optionId });
    setCustomQuestionId(null);
    setCustomText("");
  }

  function selectCustom() {
    setCustomQuestionId(activeQuestion.id);
    setAnswers((current) => {
      const next = { ...current };
      delete next[activeQuestion.id];
      return next;
    });
  }

  function saveCustom(value: string) {
    setCustomText(value);
    const normalized = value.trim();
    setAnswers((current) => {
      const next = { ...current };
      if (normalized) next[activeQuestion.id] = { question_id: activeQuestion.id, custom_text: normalized };
      else delete next[activeQuestion.id];
      return next;
    });
  }

  function skipQuestion() {
    completeCurrentQuestion({ question_id: activeQuestion.id, skipped: true });
    setCustomQuestionId(null);
    setCustomText("");
  }

  return (
    <section ref={panelRef} className="chat-decision-panel clarification" aria-label="需要澄清">
      <header>
        <span className="chat-decision-icon"><CircleHelp size={18} /></span>
        <strong>{question.prompt}</strong>
        <span className="chat-decision-progress">
          {questionIndex + 1}/{clarification.request.questions.length}
        </span>
      </header>
      <div className="chat-decision-options" role="radiogroup" aria-label={question.prompt}>
        {question.options.map((option, index) => {
          const selected = selectedAnswer?.option_id === option.id;
          return (
            <button
              key={option.id}
              type="button"
              role="radio"
              aria-checked={selected}
              className={selected ? "selected" : ""}
              disabled={disabled}
              onClick={() => selectOption(option.id)}
            >
              <span className="chat-decision-number">{index + 1}</span>
              <span className="chat-decision-option-copy">
                <strong>{option.label}{option.recommended ? "（推荐）" : ""}</strong>
                {option.description && <small>{option.description}</small>}
              </span>
              {selected && <Check size={17} />}
            </button>
          );
        })}
        {question.allow_custom && (
          <button
            type="button"
            role="radio"
            aria-checked={isCustom}
            className={`chat-decision-custom${isCustom ? " selected" : ""}`}
            disabled={disabled}
            onClick={selectCustom}
          >
            <span className="chat-decision-number">{question.options.length + 1}</span>
            <span>都不是，输入自己的答案</span>
          </button>
        )}
        {isCustom && (
          <form
            className="chat-decision-custom-form"
            aria-label="提交自定义澄清答案"
            onSubmit={(event) => {
              event.preventDefault();
              const normalized = customText.trim();
              if (!normalized || disabled) return;
              completeCurrentQuestion({ question_id: activeQuestion.id, custom_text: normalized });
            }}
          >
            <TextInput
              autoFocus
              value={customText}
              disabled={disabled}
              onChange={(event) => saveCustom(event.currentTarget.value)}
              placeholder="输入答案，按 Enter 确认"
              aria-label="自定义澄清答案"
            />
          </form>
        )}
      </div>
      <footer>
        <Button variant="default" leftSection={<SkipForward size={15} />} disabled={disabled} onClick={skipQuestion}>
          跳过本题
        </Button>
      </footer>
    </section>
  );
}
