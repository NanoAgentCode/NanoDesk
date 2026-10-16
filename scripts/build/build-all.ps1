#Requires -Version 5.1
# 组合入口：阶段 1 前端 -> 阶段 2 应用 -> 阶段 3 打包。
# -Fast          按需增量：前端指纹未变化时跳过阶段 1；阶段 2 始终执行
#                （cargo 增量缓存，源码未变时为秒级 no-op），避免打包陈旧二进制。
# -ForceFrontend 强制重建前端。
# -DryRun        只输出阶段计划与跳过原因，不执行任何构建。
param(
  [switch]$Fast,
  [switch]$ForceFrontend,
  [switch]$DryRun,
  [string]$EntryPoint = ""
)
$ErrorActionPreference = "Stop"
Import-Module (Join-Path $PSScriptRoot "common.psm1") -DisableNameChecking
$Root = Get-ProjectRoot
$Brand = Get-BrandConfig -Root $Root
if (-not $EntryPoint) {
  $EntryPoint = if ($Fast) { "package:fast" } else { "package:win" }
}
$RunId = [guid]::NewGuid().ToString("N")
Set-BuildRunId -RunId $RunId

Write-Host "==> Building $($Brand.displayName) installer"
Write-Host "==> Workspace: $Root"
Write-Host ("==> Entry: {0}  Run: {1}" -f $EntryPoint, $RunId)

if ($DryRun) {
  Write-Host "==> [dry-run] 阶段计划"
  $fresh = Test-FrontendUpToDate -Root $Root
  $frontendSkip = $Fast -and $fresh.Fresh
  $frontendAction = if (-not $Fast) {
    "将执行 npm.cmd run build（package:win 全量模式）"
  } elseif ($frontendSkip) {
    "跳过：$($fresh.Reason)"
  } else {
    "将执行 npm.cmd run build（$($fresh.Reason)）"
  }
  Write-Host ("==> [dry-run] env      始终执行：注入 MSVC 环境、校验 workspace 标记")
  Write-Host ("==> [dry-run] frontend {0}" -f $frontendAction)
  Write-Host "==> [dry-run] app      将执行 cargo build --release --features tauri/custom-protocol（增量）"
  Write-Host "==> [dry-run] bundle   将执行 tauri build --bundles nsis（beforeBuildCommand 经 --config 覆盖置空）"
  return
}

# 阶段 0（env）：每次执行，秒级。
$envTimer = Start-Stage -StageId "env"
Initialize-WorkspaceMarker -Root $Root
Initialize-MSVCEnvironment -Root $Root
Complete-Stage -Timer $envTimer -EntryPoint $EntryPoint | Out-Null

# 阶段 1/2/3：环境已注入，各阶段脚本在同进程内执行以共享计时记录。
# package:win 为全量基线：前端始终重建；package:fast 才按指纹跳过。
& (Join-Path $PSScriptRoot "build-frontend.ps1") -ProjectRoot $Root -EntryPoint $EntryPoint -RunId $RunId -SkipEnv -Force:(-not $Fast -or $ForceFrontend)
& (Join-Path $PSScriptRoot "build-app.ps1") -ProjectRoot $Root -EntryPoint $EntryPoint -RunId $RunId -SkipEnv
& (Join-Path $PSScriptRoot "build-bundle.ps1") -ProjectRoot $Root -EntryPoint $EntryPoint -RunId $RunId -SkipEnv

Write-BuildSummary
