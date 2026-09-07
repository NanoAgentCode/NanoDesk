import { Button, TextInput } from "@mantine/core";
import { Check, CircleHelp, Hammer, SkipForward } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
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

  useEffect(() => {
    setQuestionIndex(0);
    setAnswers({});
    setCustomQuestionId(null);
    setCustomText("");
  }, [clarification?.messageId]);

  const question = clarification?.request.questions[questionIndex];
  const answeredCount = useMemo(() => Object.keys(answers).length, [answers]);
  const allAnswered = Boolean(
    clarification && answeredCount === clarification.request.questions.length
  );

  useEffect(() => {
    if (!question || disabled) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
      const numericIndex = Number(event.key) - 1;
      const option = question.options[numericIndex];
      if (!option) return;
      event.preventDefault();
      setAnswers((current) => ({
        ...current,
        [question.id]: { question_id: question.id, option_id: option.id }
      }));
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

  function selectOption(optionId: string) {
    setAnswers((current) => ({
      ...current,
      [activeQuestion.id]: { question_id: activeQuestion.id, option_id: optionId }
    }));
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
    const nextAnswers = {
      ...answers,
      [activeQuestion.id]: { question_id: activeQuestion.id, skipped: true }
    };
    setAnswers(nextAnswers);
    setCustomQuestionId(null);
    setCustomText("");
    if (questionIndex < activeClarification.request.questions.length - 1) {
      setQuestionIndex(questionIndex + 1);
    }
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
          <TextInput
            autoFocus
            value={customText}
            disabled={disabled}
            onChange={(event) => saveCustom(event.currentTarget.value)}
            placeholder="输入你的答案"
            aria-label="自定义澄清答案"
          />
        )}
      </div>
      <footer>
        <div className="chat-decision-question-nav">
          {clarification.request.questions.length > 1 && (
            <>
              <Button variant="subtle" disabled={disabled || questionIndex === 0} onClick={() => setQuestionIndex(questionIndex - 1)}>
                上一题
              </Button>
              <Button variant="subtle" disabled={disabled || questionIndex >= clarification.request.questions.length - 1} onClick={() => setQuestionIndex(questionIndex + 1)}>
                下一题
              </Button>
            </>
          )}
        </div>
        <Button variant="default" leftSection={<SkipForward size={15} />} disabled={disabled} onClick={skipQuestion}>
          跳过本题
        </Button>
        <Button
          disabled={disabled || !allAnswered}
          onClick={() => void onSubmitClarification(
            clarification.messageId,
            clarification.request,
            clarification.request.questions.map((item) => answers[item.id])
          )}
        >
          提交
        </Button>
      </footer>
    </section>
  );
}
