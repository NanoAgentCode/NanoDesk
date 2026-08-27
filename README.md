# NanoAgent

NanoAgent 是一个本地优先的桌面 AI 工作台，使用 Tauri v2、Rust、React 和 TypeScript 构建。它把持久化对话、项目文件、轻量 RAG、长期记忆、Skills、MCP 工具、OCR 图片附件、Ops SSH 工作台和运行时观测集中在一个桌面客户端里，业务数据默认保存在本机 SQLite。

## 核心能力

- 本地笔记、提示词和长期记忆管理；普通手工记忆使用 SQLite 关系表、FTS5、sqlite-vec 和轻量知识图谱混合召回。独立用户画像只收集持久化会话中的用户输入，在本地过滤后按字符、数量或时间异步批量提取，不阻塞聊天回复。
- 持久化 AI 对话，支持归档、恢复、删除、项目作用域隔离和按实际模型标识进行会话级选择；LLM 设置可从 OpenAI/Anthropic 兼容服务获取模型列表，也保留手动输入。
- OpenAI-compatible Chat/Embeddings、Anthropic Messages API，以及 Ollama/OpenRouter 等兼容服务。
- 流式回复、reasoning/thinking 片段展示、长对话上下文压缩，以及 GFM/KaTeX 数学公式渲染。
- 轻量 RAG：拖拽文件、抽取文本、分块、生成 embedding，并在对话时召回相关片段。
- 项目索引中心：为项目构建可插拔索引，当前包含代码实体/关系索引和文档片段索引，代码、配置、说明、数据文件问答会优先召回项目级上下文。
- 图片附件和 OCR：图片保存到 `.nano-agent/uploads/images/`，消息中渲染缩略图，点击可预览，并可通过 `ocr_image` 调用本机 PaddleOCR。
- 归档预览：设置页的 Archive 预览复用普通聊天的消息渲染链路，项目会话使用 `project_path`，普通会话回退到 app data 下的 `temp/`。
- 项目工作区：添加或打开已有项目目录、构建轻量文件索引、浏览文件树、读写/重命名/删除项目文件、执行项目命令。
- 智能文件链接：聊天 Markdown 中的项目相对路径、裸文件名和已有文件链接会自动解析为项目内真实相对路径；外部 URL 会弹出到系统浏览器，避免应用内跳转。
- Agent 运行时：记录 run、step、tool call，支持用户审批后执行文件读写、命令、OCR 和 MCP 工具。
- `nano` 终端客户端：复用桌面端模型配置、会话存储和 Rust LLM 后端，支持项目问答、退出后恢复项目会话，以及不保存历史的普通临时对话。
- MCP 管理：支持 stdio、SSE、streamable HTTP，连接后把工具注入模型上下文。
- Skills 管理：同步 Anthropic Skills、维护本地 Skills 目录，并在系统提示中注入启用技能。
- Ops 工作台：管理 SSH 服务器、测试连接、上传文件、打开交互式 SSH 终端。
- 独立诊断链路：LLM、MCP、Ops、部分工具和数据库操作写入 `nano-agent-observability.sqlite3`；系统操作日志按天写入 `logs/` 并保留 7 天。
- 深色、浅色、跟随系统主题，以及可配置的关闭行为、系统托盘和 Windows 开机自启动。

## 文档

- [系统设计文档](docs/系统设计文档.md)：整体定位、模块边界、关键业务链路和系统约束。
- [架构与模块设计](docs/架构与模块设计.md)：前端、Tauri command、Rust 后端模块分层。
- [数据与存储设计](docs/数据与存储设计.md)：SQLite 数据库、核心表、索引、文件边界和附件存储。
- [用户画像异步批处理设计](docs/用户画像异步批处理设计.md)：画像与手工记忆边界、低 Token 候选过滤、批调度、租约、预算和删除屏障。
- [Agent、RAG、MCP 与 Skills](docs/智能体检索增强与扩展工具设计.md)：模型上下文、工具审批、RAG、OCR、MCP 和 Skills。
- [PaddleOCR OCR 工具](docs/图片文字识别工具.md)：本地 OCR 依赖、图片附件、运行时兼容和资源限制。
- [构建、配置与运维](docs/构建配置与运维.md)：开发、打包、数据位置、配置、安全和排查。
- [技术栈学习路线](docs/技术栈学习路线.md)：按当前项目技术栈设计的分阶段学习路径。

## 技术栈

- 桌面壳：Tauri v2
- 前端：React 18、TypeScript、Vite、Mantine 8、lucide-react、react-markdown、remark-gfm、remark-math、rehype-katex
- 后端：Rust、Tokio、rusqlite、reqwest、serde、thiserror
- 数据库：SQLite + WAL + FTS5 + sqlite-vec
- 模型：OpenAI-compatible Chat/Embeddings、Anthropic Messages API
- 扩展：MCP、Skills、本地 Agent 工具、PaddleOCR
- 运维：SSH/SFTP、Windows NSIS/MSI 打包

## 开发环境

Windows 推荐准备：

- Node.js
- Rust 工具链
- Microsoft C++ Build Tools
- WebView2 Runtime

安装依赖：

```bash
npm.cmd install
```

开发运行：

```bash
npm.cmd run tauri dev
```

前端单独调试：

```bash
npm.cmd run dev
```

安装 `nano` 命令行客户端（Windows）：运行 `npm.cmd run package:win` 后，双击下面生成的独立安装器：

```text
src-tauri\target\release\bundle\cli\NanoAgent-CLI_0.1.0_x64-setup.exe
```

安装器无需管理员权限，会将 CLI 释放到 `%USERPROFILE%\.nano` 并把该目录加入当前用户的 `PATH`。安装完成后打开新终端，即可在任意目录直接运行：

```powershell
nano
```

`nano` 默认以当前目录作为项目并在启动时更新代码/文档索引，项目会话自动保存，并与桌面端共享异步用户画像；`nano --temp` 不保存历史，因此不收集画像。首次运行且没有聊天模型时，命令行会引导配置模型并隐藏 API Key 输入；启动信息、交互命令、状态和错误使用统一终端配色，并自动兼容 `NO_COLOR`。使用 `nano --sessions` 获取会话列表、`nano --show <会话ID>` 查看历史、`nano --continue` 恢复最近会话、`nano --files` 获取项目文件列表，或用 `nano --temp` 启动不绑定项目且不保存历史的普通临时对话。详见[构建、配置与运维](docs/构建配置与运维.md#2-nano-终端客户端)。

类型检查和前端构建：

```bash
npm.cmd run build
```

Rust 检查：

```bash
cd src-tauri
cargo check
```

Windows 打包：

```bash
npm.cmd run package:win
```

`package:win` 会调用 `scripts/build-installer.ps1`，加载 Visual Studio x64 构建环境，修正 Windows 下 Git `link.exe` 抢占 MSVC `link.exe` 的 PATH 问题，然后构建 CLI、Tauri 桌面端及安装包。常见产物包括 `nano.exe`、独立 CLI 安装器、`nano-agent.exe`、桌面端标准 NSIS 安装包、内置 WebView2 Offline Installer 的离线 NSIS 安装包和 MSI 安装包。离线 NSIS 包以 `-offline-setup.exe` 结尾，最终用户安装时不需要联网下载 WebView2 Runtime，但文件通常会增加百余 MB，实际大小随微软提供的离线安装器版本变化。

## 数据位置

运行时数据保存在 Tauri app data 目录下：

```text
nano-agent.sqlite3                 主业务数据
nano-agent-runtime.sqlite3         Agent 运行时数据
nano-agent-observability.sqlite3   观测数据
settings.json                      Tavily API key
logs/                              按天滚动的系统操作日志（保留 7 天）
skills/                            本地 Skills 目录
temp/                              无项目上下文时的临时工作目录
```

项目内图片附件保存在对应根目录下的 `.nano-agent/uploads/images/`。普通对话没有真实项目路径时，会使用 app data 下的 `temp/` 作为附件和工具工作目录。

## 项目结构

```text
src/                           React + TypeScript 前端
src/api.ts                     Tauri command 调用封装
src/theme.ts                   Mantine 主题与组件默认配置
src/core/plugins.tsx           前端插件契约与微内核注册表
src/plugins/builtin.tsx        内置 UI 插件装配
src/hooks/                     对话、模型、项目、RAG、MCP、Skills、Ops 等状态逻辑
src/components/                聊天区、侧栏、设置页、观测面板、Ops 工作台等 UI
src/lib/                       系统提示、工具解析、格式化和安全封装
src-tauri/src/lib.rs           Tauri command 注册、应用状态和启动流程
src-tauri/src/cli.rs           nano 终端交互、模型选择和项目问答上下文
src-tauri/src/bin/nano.rs      nano 命令行二进制入口
src-tauri/src/core/plugin.rs   后端插件契约、清单与 Agent 工具扩展点
src-tauri/src/plugins.rs       内置后端插件装配
src-tauri/src/db.rs            主业务 SQLite schema、迁移与共享数据库入口
src-tauri/src/db/              条目、配置、会话、RAG、记忆、画像和项目索引的领域存储
src-tauri/src/code_index.rs    项目代码实体、关系和片段索引
src-tauri/src/project_index.rs 项目文档片段索引与通用项目索引查询
src-tauri/src/runtime.rs       Agent run/step/tool call 运行时存储
src-tauri/src/observability.rs 观测 sink/pipeline 与观测库
src-tauri/src/logging.rs       按天写入并自动清理的系统操作日志
src-tauri/src/llm.rs           Chat、streaming 和 embeddings 请求
src-tauri/src/memory.rs        长期记忆 embedding 编排与混合召回入口
src-tauri/src/profile.rs       用户画像候选过滤、异步 Worker、上下文注入与管理命令
src-tauri/src/db/profile_store.rs 用户画像状态机、预算、租约、Reducer 与删除屏障
src-tauri/src/ops.rs           Ops SSH/SCP、交互终端与 AI 辅助命令
src-tauri/src/mcp.rs           MCP client manager 与传输实现
src-tauri/src/agent_runner.rs  XML tool_call 解析与运行时结果模型
scripts/build-installer.ps1    Windows 打包脚本
scripts/install-cli.ps1        构建 nano.exe、安装到用户目录并配置 PATH
docs/                          系统设计、运维和学习路线文档
```

## 设计原则

- 本地优先：对话、记忆、项目元数据和运行时记录默认保存在本机。
- 数据隔离：业务数据、Agent 运行时、观测数据分库保存，降低互相影响。
- 显式路径：项目路径、会话 ID、模型 ID、tool call ID 等跨层标识显式传递。
- 可点击资源：模型输出项目文件名或相对路径时，前端基于当前项目文件索引生成可点击链接；裸文件名只在能从项目索引解析时补全。
- 项目优先检索：代码类问题优先使用代码实体/关系索引，文档和普通文件问题优先使用项目文档索引，再回退到普通文件列表或工具读取。
- 用户审批：高风险工具调用必须先形成可见的 tool call，再由用户确认执行。
- 微内核：应用壳、状态、权限和能力调度保持稳定；主视图、设置页与 Agent 工具通过显式插件注册表扩展。
- 可审计插件：插件随应用静态编译，启动时校验 ID/工具冲突，工具执行仍统一经过策略和用户审批。
- 非阻塞观测：观测写入失败只记录错误，不阻断主业务流程。
