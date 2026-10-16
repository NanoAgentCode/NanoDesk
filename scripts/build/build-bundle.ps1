#Requires -Version 5.1
# 阶段 3（bundle）：tauri build --bundles nsis 生成安装包并校验产物。
# 通过 --config 覆盖把 beforeBuildCommand 置空，dist/ 的新鲜度由阶段 1 保证。
param(
  [string]$ProjectRoot = "",
  [string]$EntryPoint = "package:bundle",
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

if ($DryRun) {
  Write-Host "==> [dry-run] bundle  将执行 tauri build --bundles nsis（beforeBuildCommand 经 --config 覆盖置空）"
  return
}

# Keep the version used when starting this build; source files may change while it runs.
$Version = (Get-Content -LiteralPath (Join-Path $ProjectRoot "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version

$StateDir = Join-Path $ProjectRoot ".build"
New-Item -ItemType Directory -Path $StateDir -Force | Out-Null
$OverrideConfig = Join-Path $StateDir "tauri-bundle-config.json"
@{ build = @{ beforeBuildCommand = "" } } | ConvertTo-Json |
  Set-Content -LiteralPath $OverrideConfig -Encoding UTF8

$timer = Start-Stage -StageId "bundle"
Write-Host "==> [stage/bundle] tauri build --bundles nsis (beforeBuildCommand: skipped)"
Push-Location $ProjectRoot
try {
  & cmd.exe /d /s /c ('npm.cmd run tauri build -- --bundles nsis --config "' + $OverrideConfig + '"')
  if ($LASTEXITCODE -ne 0) {
    throw "Tauri build failed. Exit code: $LASTEXITCODE"
  }
} finally {
  Pop-Location
}

$ReleaseDir = Join-Path $ProjectRoot "src-tauri\target\release"
$DesktopExe = Join-Path $ReleaseDir "$($Brand.desktopBinaryName).exe"
$NsisDir = Join-Path $ReleaseDir "bundle\nsis"
$ExpectedNsis = Join-Path $NsisDir "$($Brand.displayName)_${Version}_x64-setup.exe"
if (-not (Test-Path -LiteralPath $ExpectedNsis)) {
  throw "NSIS installer was not found: $ExpectedNsis"
}

Complete-Stage -Timer $timer -EntryPoint $EntryPoint -Artifact $ExpectedNsis | Out-Null

Write-Host ""
Write-Host "==> Build completed"
Write-Host "App : $DesktopExe"
Write-Host "NSIS: $ExpectedNsis"
