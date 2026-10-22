# Windows 兼容构建

目标范围：Windows 7 SP1、Windows 8/8.1、Windows 10 的 x86（32 位）与 x64（64 位），以及 Windows 11 x64。Windows 11 没有 32 位系统版本。

这是独立的候选构建流程。成功构建不等于已在所有系统验证兼容，不能将现有 GitHub Release 的普通 x64 包直接当作 Win7/8 兼容包。

## 构建

在现代 Windows x64 构建机安装 VS 2022 C++ Build Tools（同时安装 x86/x64 工具与 Windows SDK），再安装固定 Rust nightly 及标准库源码：

```powershell
npm ci
rustup toolchain install nightly-2026-05-26 --profile minimal --component rust-src
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows-compat.ps1
# 单独构建一个架构：追加 -Architecture x86 或 -Architecture x64
# 仅查看计划：追加 -DryRun
```

脚本使用 `i686-win7-windows-msvc` / `x86_64-win7-windows-msvc`，通过 `-Z build-std=std,panic_abort,test` 重建 Win7 标准库及测试库。普通 `*-pc-windows-msvc` 的当前 Rust 标准库最低为 Windows 10，不能仅通过修改安装器系统版本解决。使用锁文件中的依赖版本；升级 Tauri、Rust 或原生依赖后需要重新验证兼容性。

Tauri build 对 Tier 3 目标的 rustup 校验无法通过，因此脚本先以 Cargo 编译（保留 `tauri/custom-protocol`），再用 Tauri bundle 生成 NSIS 包。目标架构的 Rust 和 C/C++ 代码静态链接 CRT；随机数使用 `windows_legacy` 后端，避免引入 Win7 不存在的 `bcryptprimitives.dll` / `ProcessPrng`。

候选包、SHA-256 文件和导入表位于 `.build/windows-compat/<版本>/`，名称带 `windows-compat`，不覆盖普通发布包。编译输出分别保存在 `src-tauri/target/<目标>/release/`。与现有安装包一致，编译并打包桌面程序及 `nano.exe`，不生成独立 CLI 安装器。

构建前验证 npm、Cargo、Tauri 三处版本一致。`BUILD-INFO.json` 的 `artifacts` 和 `SHA256SUMS.txt` 只列出本次选择并成功构建的架构，包含实际安装包名称、目标、大小及 SHA-256；单架构重建不会把目录内其他旧包记为本次产物。

兼容流程关闭 Tauri 的混合 CRT 链接辅助逻辑（该逻辑仍动态依赖 UCRT），改为完整静态 CRT；延迟加载 WinRT 激活与 `ole32.dll`，避免 Win7 启动时解析不存在的 WinRT / `CoIncrementMTAUsage` 入口。相关功能仍需实际系统测试；延迟加载不能证明所有调用路径都有旧系统回退。

`ctor` 0.8.0 的本地补丁只让 Windows CRT 初始化宏识别 `vendor=win7`，原始许可证及修改说明保存在 `src-tauri/vendor/ctor/`。通知启用插件的 `windows7-compat` 功能，现有 `builder().show()` 会由插件在内部自动分发到 Win7 兼容通知路径。

`sqlite-vec` 的 MSVC x86 构建引用了仅 x64 提供的 `__popcnt64`，兼容模块用 Rust 的可移植位计数实现相同的 C ABI，并通过真实 SQLite Hamming 距离查询覆盖 64 位数据的高低半部；不要求额外的 POPCNT CPU 指令。

命令执行的 UTF-8 前置脚本通过 .NET `Activator.CreateInstance` 创建编码器，避免 Win7/8 自带旧版 PowerShell 无法解析 `::new()` 构造语法，也避免为创建编码器自动加载 PowerShell 模块而产生 CLIXML 进度输出。模型生成的具体命令及外部 Python/CLI 工具仍需符合目标系统实际安装的软件版本。

## WebView2 前置条件

- Win7 SP1 / Win8 / Win8.1：先从可信的微软分发来源安装对应系统架构的 **WebView2 Runtime 109**。它已停止安全更新，用户已明确接受这一限制。当前未随包提供运行时。兼容安装器发现旧系统未安装 109 时会停止并提示，不尝试运行已不支持旧系统的最新版引导程序。
- Win10 / Win11：使用正常维护的 WebView2；运行时缺失时沿用 Tauri 下载与安装流程。
- 不建议从不明镜像下载运行时。微软 Fixed Runtime 的解压目录与注册安装的 Runtime 不同，本流程检查的是已注册安装的运行时，不会自动识别任意 Fixed Runtime 目录。

## 验证与验收

自动检查：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows-compat.test.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-windows-compat-binary.test.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-installer.test.ps1
```

构建时检查 EXE 的真实 PE 架构、子系统最低版本及已知 Win7 不兼容导入，并保存完整导入表。这只是已知阻断项筛查，不是完整 Windows API 兼容性证明。`src-tauri/tests/windows_compat.rs` 验证 Tauri 的 CRT 启动构造器确实初始化了当前程序路径。

双架构均运行后端单元测试、启动构造器测试和 CLI 版本命令。后端测试串行运行，避免现有自动化恢复测试共用系统临时目录下的进程锁而互相干扰；不修改自动化恢复的业务逻辑。

模拟模型服务的测试在接受连接后显式恢复阻塞模式，避免 Win7 目标标准库下继承监听套接字的非阻塞状态，导致读取请求时偶发 `WouldBlock`。这只修改测试服务器，不改变产品网络请求逻辑。

2026-10-22 本机验证：Windows 11 专业版 x64（10.0.26300）上，两种架构均通过 277 项后端测试和 1 项启动构造器测试；桌面程序与 CLI 均通过 PE/已知导入阻断项检查，CLI 版本命令运行成功。构建计划测试、导入检查器测试、原有安装构建回归及 NSIS 兼容检查编译也通过。这些结果不等于旧系统、安装过程或 GUI 已完成验收。

当前没有 Win7/Win8 测试机和 WebView2 109 运行时，以下各项必须逐系统、逐架构实测后，才能标记对应系统正式支持：

| 测试系统 | 架构 | 安装/启动/功能/升级验收 |
| --- | --- | --- |
| Windows 7 SP1 | x86、x64 | 待实测 |
| Windows 8 | x86、x64 | 待实测 |
| Windows 8.1 | x86、x64 | 待实测 |
| Windows 10 | x86、x64 | 待实测 |
| Windows 11 | x64 | 待实测 |

每个环境至少检查：

1. 干净环境缺少运行时时的提示、安装取消、正确运行时下安装与启动、同版本重装、升级和卸载。
2. 主窗口、桌面助手、文档右键窗口、托盘、文件夹定位和剪贴板。
3. 模型请求、中文流式响应、审批与暂停恢复、通知、文档导入/检索/摘要、交付物分类与折叠。
4. WebView2 109 下的浅色/深色主题、弹窗、滚动和布局。现有 `color-mix()` 等较新 CSS 的降级效果需专项检查，不能仅凭 JS 编译成功声称界面完全兼容。
5. 旧系统的 TLS 根证书、代理、模型服务连接与语音识别；外部服务是否仍支持旧系统由实际连接决定。

安装及启动测试应使用独立测试用户或虚拟机快照，避免覆盖开发者的真实会话与配置。

依据：[Rust Win7 目标](https://doc.rust-lang.org/rustc/platform-support/win7-windows-msvc.html)、[Rust Windows 基线变更](https://blog.rust-lang.org/2024/02/26/Windows-7/)、[微软 WebView2 旧系统支持终止说明](https://blogs.windows.com/msedgedev/2022/12/09/microsoft-edge-and-webview2-ending-support-for-windows-7-and-windows-8-8-1/)、[Tauri Windows 安装器](https://v2.tauri.app/distribute/windows-installer/)。
