#Requires -Version 5.1
# 阶段 2（app）：tauri build --no-bundle 编译产出 nano-desk.exe。
# 独立入口先刷新前端；--no-bundle 与发布入口的配置形状不同，
# 发布时使用 build-all.ps1 合并编译和打包，避免交替调用导致重链接。
param(
  [string]$ProjectRoot = "",
  [string]$EntryPoint = "package:app",
  [string]$RunId = "",
  [switch]$SkipEnv,
  [switch]$DryRun
)
$ErrorActionPreference = "Stop"
Import-Module (Join-Path $PSScriptRoot "common.psm1") -DisableNameChecking
if (-not $ProjectRoot) { $ProjectRoot = Get-ProjectRoot }
if (-not $RunId) { $RunId = [guid]::NewGuid().ToString("N") }
Set-BuildRunId -RunId $RunId
$Brand = Get-BrandConfig -Root $ProjectRoot

if (-not $SkipEnv -and -not $DryRun) {
  $envTimer = Start-Stage -StageId "env"
  Initialize-WorkspaceMarker -Root $ProjectRoot
  Initialize-MSVCEnvironment -Root $ProjectRoot
  Complete-Stage -Timer $envTimer -EntryPoint $EntryPoint | Out-Null
}

if ($DryRun) {
  & (Join-Path $PSScriptRoot "build-frontend.ps1") -ProjectRoot $ProjectRoot -DryRun
  Write-Host "==> [dry-run] app  tauri build --no-bundle"
  return
}
& (Join-Path $PSScriptRoot "build-frontend.ps1") -ProjectRoot $ProjectRoot -EntryPoint $EntryPoint -RunId $RunId -SkipEnv
$OverrideConfig = Get-TauriOverrideConfig -Root $ProjectRoot

$timer = Start-Stage -StageId "app"
Write-Host "==> [stage/app] tauri build --no-bundle"
Push-Location $ProjectRoot
try {
  & cmd.exe /d /s /c ('npm.cmd run tauri build -- --no-bundle --config "' + $OverrideConfig + '"')
  if ($LASTEXITCODE -ne 0) {
    throw "Tauri build (no-bundle) failed. Exit code: $LASTEXITCODE"
  }
} finally {
  Pop-Location
}
$DesktopExe = Join-Path $ProjectRoot "src-tauri\target\release\$($Brand.desktopBinaryName).exe"
if (-not (Test-Path -LiteralPath $DesktopExe)) {
  throw "Desktop executable was not found: $DesktopExe"
}
Complete-Stage -Timer $timer -EntryPoint $EntryPoint -Artifact $DesktopExe | Out-Null
