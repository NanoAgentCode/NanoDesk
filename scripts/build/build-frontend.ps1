#Requires -Version 5.1
# 阶段 1（frontend）：brand:check + tsc + vite build -> dist/
# 指纹未变化时跳过；-Force 强制重建；-DryRun 只输出计划。
param(
  [string]$ProjectRoot = "",
  [string]$EntryPoint = "package:frontend",
  [string]$RunId = "",
  [switch]$SkipEnv,
  [switch]$Force,
  [switch]$DryRun
)
$ErrorActionPreference = "Stop"
Import-Module (Join-Path $PSScriptRoot "common.psm1") -DisableNameChecking
if (-not $ProjectRoot) { $ProjectRoot = Get-ProjectRoot }
if (-not $RunId) { Set-BuildRunId -RunId ([guid]::NewGuid().ToString("N")) } else { Set-BuildRunId -RunId $RunId }

if (-not $SkipEnv -and -not $DryRun) {
  $envTimer = Start-Stage -StageId "env"
  Initialize-WorkspaceMarker -Root $ProjectRoot
  Initialize-MSVCEnvironment -Root $ProjectRoot
  Complete-Stage -Timer $envTimer -EntryPoint $EntryPoint | Out-Null
}

$fresh = Test-FrontendUpToDate -Root $ProjectRoot
$willSkip = -not $Force -and $fresh.Fresh
if ($DryRun) {
  $action = if ($willSkip) { "跳过：$($fresh.Reason)" } else { "将执行 npm.cmd run build（$($fresh.Reason)）" }
  Write-Host ("==> [dry-run] frontend  {0}" -f $action)
  return
}

$timer = Start-Stage -StageId "frontend"
if ($willSkip) {
  Write-Host ("==> 前端阶段跳过：{0}" -f $fresh.Reason)
  Complete-Stage -Timer $timer -EntryPoint $EntryPoint -Skipped | Out-Null
  return
}

Write-Host "==> [stage/frontend] brand:check + tsc + vite build"
Push-Location $ProjectRoot
try {
  & cmd.exe /d /s /c "npm.cmd run build"
  if ($LASTEXITCODE -ne 0) {
    throw "Frontend build failed. Exit code: $LASTEXITCODE"
  }
  # 先于任何 dist 之后的写入保存指纹，保证指纹记录总是新于 dist。
  Save-FrontendFingerprint -Root $ProjectRoot
} finally {
  Pop-Location
}
Complete-Stage -Timer $timer -EntryPoint $EntryPoint | Out-Null
