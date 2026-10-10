import type {
  AgentAccessMode,
  AgentClarificationAnswer,
  AgentClarificationRequest,
  AgentToolCall,
  Memory,
  ChatImageAttachment,
  Conversation,
  Item,
  PersistedMessage,
  ProjectEntry,
  ProjectFileEntry
} from "../../types";
import type { ParsedToolCall } from "../../lib/messageHelpers";
import type { UseSpeechInputReturn } from "../useSpeechInput";
import type { UseProjectsReturn } from "../useProjects";
import type { UseModelReturn } from "../useModel";
import type { UseSkillsReturn } from "../useSkills";
import type { UseMcpReturn } from "../useMcp";
export interface UseChatReturn {
  conversations: Conversation[];
  setConversations: React.Dispatch<React.SetStateAction<Conversation[]>>;
  archivedConversations: Conversation[];
  setArchivedConversations: React.Dispatch<React.SetStateAction<Conversation[]>>;
  previewArchivedId: string;
  setPreviewArchivedId: React.Dispatch<React.SetStateAction<string>>;
  previewMessages: PersistedMessage[];
  setPreviewMessages: React.Dispatch<React.SetStateAction<PersistedMessage[]>>;
  activeConversationId: string;
  setActiveConversationId: React.Dispatch<React.SetStateAction<string>>;
  messages: PersistedMessage[];
  setMessages: React.Dispatch<React.SetStateAction<PersistedMessage[]>>;
  messageReasoning: Record<string, string>;
  setMessageReasoning: React.Dispatch<React.SetStateAction<Record<string, string>>>;
  chatInput: string;
  setChatInput: React.Dispatch<React.SetStateAction<string>>;
  ragFiles: import("../../types").RagFile[];
  setRagFiles: React.Dispatch<React.SetStateAction<import("../../types").RagFile[]>>;
  isRagDragging: boolean;
  setIsRagDragging: React.Dispatch<React.SetStateAction<boolean>>;
  indexingRagFileName: string;
  setIndexingRagFileName: React.Dispatch<React.SetStateAction<string>>;
  promptSuggestions: Item[];
  setPromptSuggestions: React.Dispatch<React.SetStateAction<Item[]>>;
  selectedPromptIndex: number;
  setSelectedPromptIndex: React.Dispatch<React.SetStateAction<number>>;
  promptTriggerIndex: number;
  setPromptTriggerIndex: React.Dispatch<React.SetStateAction<number>>;
  busy: boolean;
  setBusy: React.Dispatch<React.SetStateAction<boolean>>;
  executingToolMessageId: string | null;
  setExecutingToolMessageId: React.Dispatch<React.SetStateAction<string | null>>;
  messageToolCalls: Record<string, AgentToolCall>;
  setMessageToolCalls: React.Dispatch<React.SetStateAction<Record<string, AgentToolCall>>>;
  conversationRunIds: Record<string, string>;
  setConversationRunIds: React.Dispatch<React.SetStateAction<Record<string, string>>>;
  clarificationFallbackIds: string[];
  activeStreamRequestId: string | null;
  backgroundRunId: string | null;
  interruptingGeneration: boolean;
  uploadingImageAttachment: boolean;
  uploadingAttachment: boolean;
  speech: UseSpeechInputReturn;
  pendingImageAttachments: ChatImageAttachment[];
  removePendingImageAttachment: (relativePath: string) => void;
  attachmentProjectPath: string;
  projectFiles: ProjectFileEntry[];
  activeConversation: Conversation | undefined;
  activeConversationProject: ProjectEntry | null;
  loadMessages: (conversationId: string) => Promise<void>;
  refreshRagFiles: (conversationId: string) => Promise<void>;
  refreshConversations: (selectId?: string) => Promise<void>;
  createConversationForCurrentScope: (project: ProjectEntry | null) => Promise<Conversation>;
  ensureConversation: (project: ProjectEntry | null) => Promise<string>;
  getConversationProjectHint: () => ProjectEntry | null;
  handleNewConversation: () => Promise<void>;
  handleNewProjectConversation: (project: ProjectEntry) => Promise<void>;
  handleDeleteConversation: () => Promise<void>;
  handleArchiveConversation: () => Promise<void>;
  handleRenameConversation: (id: string, currentTitle: string) => Promise<void>;
  handleContextArchiveConversation: (conversation: Conversation) => Promise<void>;
  handleContextDeleteConversation: (conversation: Conversation) => Promise<void>;
  handleSendMessage: () => Promise<void>;
  handleInterruptGeneration: () => Promise<void>;
  handleRegenerateLastResponse: (messageId: string) => Promise<void>;
  handleExecuteTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  handleRejectTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  handleRetryTool: (messageId: string) => Promise<void>;
  handleResumeAgentRun: (runId: string) => Promise<void>;
  handleClarificationAnswer: (
    messageId: string,
    request: AgentClarificationRequest,
    answers: AgentClarificationAnswer[],
    automatic?: boolean
  ) => Promise<void>;
  handleCloseConversation: () => void;
  handleRagFiles: (files: FileList | File[]) => Promise<void>;
  handleDroppedFilePaths: (paths: string[]) => Promise<void>;
  handleDeleteRagFile: (id: string) => Promise<void>;
  handleInputChange: (value: string, cursorIndex: number) => Promise<void>;
  handleChatInputKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  handleChatInputPaste: (event: React.ClipboardEvent<HTMLTextAreaElement>) => void;
  insertPrompt: (item: Item) => void;
  loadArchivedPreview: (conversationId: string) => Promise<void>;
  resolveConversationModelId: (conversationId?: string | null) => string;
}

export interface UseChatArgs {
  setNotice: (message: string) => void;
  onMemoryCreated: (memory: Memory) => void;
  projects: UseProjectsReturn;
  model: UseModelReturn;
  skills: UseSkillsReturn;
  mcp: UseMcpReturn;
  showModelConfig: boolean;
  activeSettingsTab: string;
  accessMode: AgentAccessMode;
  chatVisible?: boolean;
}
