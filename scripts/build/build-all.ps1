#Requires -Version 5.1
# 组合入口：阶段 1 前端（指纹跳过）-> 阶段 2+3 tauri build --bundles nsis
# （编译与打包一次完成，形状恒定，稳态零重编译）。
# -ForceFrontend 强制重建前端。
# -DryRun        只输出阶段计划与跳过原因，不执行任何构建。
# package:win 与 package:fast 行为一致（均增量）；-Fast 仅影响计时记录的入口标签。
param(
  [switch]$Fast,
  [switch]$ForceFrontend,
  [switch]$DryRun,
  [string]$EntryPoint = ""
)
$ErrorActionPreference = "Stop"
Import-Module (Join-Path $PSScriptRoot "common.psm1") -DisableNameChecking
$Root = Get-ProjectRoot
$null = Get-ReleaseVersion -Root $Root
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
  $frontendSkip = -not $ForceFrontend -and $fresh.Fresh
  $frontendAction = if ($frontendSkip) {
    "跳过：$($fresh.Reason)"
  } else {
    "将执行 npm.cmd run build（$($fresh.Reason)）"
  }
  Write-Host ("==> [dry-run] env      始终执行：注入 MSVC 环境、校验 workspace 标记")
  Write-Host ("==> [dry-run] frontend {0}" -f $frontendAction)
  Write-Host "==> [dry-run] bundle   将执行 tauri build --bundles nsis（编译+打包一次完成；beforeBuildCommand 经 --config 覆盖置空）"
  return
}

# 阶段 0（env）：每次执行，秒级。
$envTimer = Start-Stage -StageId "env"
Initialize-WorkspaceMarker -Root $Root
Initialize-MSVCEnvironment -Root $Root
Complete-Stage -Timer $envTimer -EntryPoint $EntryPoint | Out-Null

# 阶段 1：前端。所有入口都按指纹跳过；需要强制全量重建前端时传 -ForceFrontend。
& (Join-Path $PSScriptRoot "build-frontend.ps1") -ProjectRoot $Root -EntryPoint $EntryPoint -RunId $RunId -SkipEnv -Force:$ForceFrontend

# 阶段 2+3 合并为一次 tauri build --bundles nsis：编译与打包同形调用。
# 不要在组合流程中插入 --no-bundle 的 app 阶段——它会改变 TAURI_CONFIG，
# 导致下一次构建重链接 nano-desk（实测 ~86s）。package:app 仍可单独使用。
& (Join-Path $PSScriptRoot "build-bundle.ps1") -ProjectRoot $Root -EntryPoint $EntryPoint -RunId $RunId -SkipEnv

Write-BuildSummary
