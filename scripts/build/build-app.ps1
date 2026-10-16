#Requires -Version 5.1
# 阶段 2（app）：tauri build --no-bundle 编译产出 nano-desk.exe。
# 必须与阶段 3（bundle）使用完全相同的 tauri 调用形状（--config 覆盖一致），
# 实测普通 cargo build 与 tauri build 的编译缓存双向互不复用（全量 411 crate
# 重编），同形调用才能让阶段 3 的 cargo 退化为秒级 no-op。
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
if (-not $RunId) { Set-BuildRunId -RunId ([guid]::NewGuid().ToString("N")) } else { Set-BuildRunId -RunId $RunId }
$Brand = Get-BrandConfig -Root $ProjectRoot

if (-not $SkipEnv -and -not $DryRun) {
  $envTimer = Start-Stage -StageId "env"
  Initialize-WorkspaceMarker -Root $ProjectRoot
  Initialize-MSVCEnvironment -Root $ProjectRoot
  Complete-Stage -Timer $envTimer -EntryPoint $EntryPoint | Out-Null
}

$StateDir = Join-Path $ProjectRoot ".build"
New-Item -ItemType Directory -Path $StateDir -Force | Out-Null
$OverrideConfig = Join-Path $StateDir "tauri-bundle-config.json"
@{ build = @{ beforeBuildCommand = "" } } | ConvertTo-Json |
  Set-Content -LiteralPath $OverrideConfig -Encoding UTF8

if ($DryRun) {
  Write-Host "==> [dry-run] app  将执行 tauri build --no-bundle（与 bundle 阶段同形调用，缓存必复用）"
  return
}

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
