# 桌面与 CLI 业务能力对齐

## 范围与验收

以桌面注册业务命令为基准，排除托盘、窗口、剪贴板和资源管理器菜单等交互。当前 161 个注册入口中，148 个业务入口可由 CLI 调用，13 个桌面交互入口排除；完整路径清单见 `cli-command-coverage.json`。目录测试从当前 `lib.rs` 注册表计算应有业务命令，检查 CLI 不缺项、不重复。

| 能力 | 代码状态 | 共用实现与验证 |
| --- | --- | --- |
| 模型、供应商与模型连通性 | 完整具备 | `commands/models`；共用配置数据库与模型用途校验 |
| 会话、归档、条目、记忆与画像 | 完整具备 | 原命令与数据库；CLI CRUD、参数校验、重开持久化实测 |
| 智能、固定、四维模型路由 | 完整具备 | `modelRoutingRules.json` 共用任务规则和策略映射；前端与 Rust 分类/回退测试 |
| 聊天、工具、审批、澄清、预算 | 完整具备 | 同一 `background_agent` 执行循环，不再有独立只输出文本的 CLI 流程；无窗口真实工具循环测试 |
| 规划、整体结果、交付物与恢复 | 完整具备 | 同一运行存储、规划协议与交付元数据；CLI 运行文件写入及交付恢复测试 |
| 文档、图片、RAG、ASR、OCR | 完整具备 | 共用读入、索引、预处理、转写与识别命令；不实现终端麦克风/剪贴板 UI |
| 文件、代码与项目索引 | 完整具备 | 原路径边界、索引和检索能力；CLI 现有索引测试与共用命令目录 |
| MCP、Skills、环境检查与安装 | 完整具备 | 共用服务和配置；普通会话恢复启用的 MCP，并提供本地技能描述与工具定义 |
| 自动任务与 SSH 运维 | 完整具备 | `--serve` 使用原调度/执行服务与事件；SSH 凭据、PTY 和上传走原命令 |
| 托盘、桌面助手窗口、资源管理器菜单 | 不适用 | 按约定排除桌面交互 |

这里的“完整具备”指业务入口和实现同源，不表示已对用户所有外部模型、MCP、ASR/OCR、SSH 服务完成真实环境联调。模型/供应商等业务数据共享；桌面路由面板的浏览器本地选择偏好不自动同步到 CLI，CLI 通过参数选择模式和策略模型。

## 命令入口

```powershell
nano --commands
nano --call list_model_configs
nano --call list_model_suppliers
nano --call get_user_profile
nano -C D:\workspace\demo --access-mode ask -p "读取并分析启动流程"
nano -C D:\workspace\demo --access-mode auto -p "生成报告并保存到文件"
nano --routing smart --routing-models '{"quality":"model-a","speed":"model-b","cost":null}' -p "总结项目"
```

`ask` 允许无计划、无工具的普通回答直接完成；提交了规划或请求工具时要求审核规划，并按工具风险审核操作；非交互输入遇到待审核任务时返回任务 ID 和继续方式，不会默许批准。`auto/full` 的风险边界直接复用桌面策略。`--model` 仍可指定固定或兜底模型。策略 ID 应为已保存的聊天模型 ID；不适用当前任务的模型走兜底。

`--call` 参数使用注册命令的 snake_case 名称和原 DTO。未知参数、错误类型和缺失必填项会报错。单次结果是 JSON，失败退出码非零。长任务、索引预处理、SSH PTY、流式聊天、立即执行自动任务和异步画像需要 `--serve` 保持执行宿主存活；单次自然语言任务可直接使用 `-p`。一次 MCP 调用会恢复启用的连接，进程结束时连接关闭。

## 持久会话协议

运行 `nano --serve [--data-dir <目录>]`，每行输入一个请求，每行输出一个 JSON 响应或事件。调用方在任务完成前保持标准输入打开。

请求示例：

```json
{"id":1,"command":"list_model_configs","params":{}}
{"id":2,"command":"create_conversation","params":{"draft":{"title":"CLI task","model_config_id":"model-id","project_path":"D:/workspace/demo"}}}
```

响应为 `{"id":1,"result":...}` 或 `{"id":1,"error":"..."}`；事件为 `{"event":"background-agent","payload":...}`，以及原来的 `chat-stream`、`document-index-progress`、`automation-changed`、`ops-ssh` 等。异步事件可能先于请求响应，使用 `id` 关联响应，不依赖输出行顺序。

典型文件任务调用顺序：`create_conversation` → `append_message` → `create_agent_run` → `start_background_agent`；使用 `respond_background_agent` 审核规划、批准工具、回答澄清或恢复任务。`list_messages` 的交付元数据和 `list_agent_run_timelines` 的工具/产物步骤与桌面一致。附件可通过 `read_document_chat_file`/`extract_uploaded_file`/`save_chat_image_attachment` 后调用对应索引或任务接口。

自动任务通过 `save_automation`、`set_automation_enabled`、`run_automation_now`、`list_automation_runs`、`recover_automation_run` 管理；服务存活时运行调度器，退出后不会继续执行新触发。SSH PTY 通过 `start_ops_ssh_session`、`send_ops_ssh_input`、`resize_ops_ssh_session`、`stop_ops_ssh_session` 和 `ops-ssh` 事件交互。MCP 的连接、刷新和调用可在同一会话连续完成。

## 实现边界与进程安全

`CommandHost` 提供应用数据目录、共用状态与事件；桌面注入真实 Tauri 宿主，CLI 注入无窗口宿主。`CommandState` 仍使用 Tauri 原有的受管状态解析，CLI 则直接借用同一类型的业务状态。命令表由 `scripts/generate-cli-business.py` 从桌面注册表生成，新增业务命令后重新生成。

CLI 附加数据库不自动执行崩溃恢复。持久宿主启动恢复时通过 OS 文件锁识别仍活跃的任务：每个运行和每个会话只允许一个执行者；跨进程暂停/停止请求写入运行记录，由当前执行者在安全检查点消费，已开始的工具先记录结果再暂停。恢复过程跳过仍持有锁的运行；自动任务队列只允许一个调度执行宿主，其他客户端可继续管理同一份配置和记录。进程异常结束后锁由操作系统释放，重启后按原恢复策略处理未知工具结果，不自动重放。

实时事件由执行宿主发出；另一个客户端可查询共享的任务状态和历史，不保证镜像另一个进程的每个流式增量。桌面系统通知、窗口焦点和终端快捷键不属于业务对齐范围。

Windows 统一使用 Tauri 原有 Common Controls v6 清单，应用到桌面、CLI 与测试可执行文件，避免无窗口宿主引用原生适配器后测试/CLI 在加载阶段失败。
