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
if (-not $RunId) { $RunId = [guid]::NewGuid().ToString("N") }
Set-BuildRunId -RunId $RunId
$Brand = Get-BrandConfig -Root $ProjectRoot
$Version = Get-ReleaseVersion -Root $ProjectRoot

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

# Standalone bundle calls must refresh dist before embedding it.
if (-not $SkipEnv) {
  & (Join-Path $PSScriptRoot "build-frontend.ps1") -ProjectRoot $ProjectRoot -EntryPoint $EntryPoint -RunId $RunId -SkipEnv
}

$OverrideConfig = Get-TauriOverrideConfig -Root $ProjectRoot

$timer = Start-Stage -StageId "bundle"
Write-Host "==> [stage/bundle] tauri build --bundles nsis (beforeBuildCommand: skipped)"
Push-Location $ProjectRoot
try {
  & cmd.exe /d /s /c ('npm.cmd run tauri build -- --bundles nsis --ci --config "' + $OverrideConfig + '" -- --locked')
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
if (-not (Test-Path -LiteralPath $DesktopExe)) {
  throw "Desktop executable was not found: $DesktopExe"
}
$PublishedInstaller = Export-ReleaseArtifact -Root $ProjectRoot -Installer $ExpectedNsis `
  -OutputDirectory (Join-Path $ProjectRoot ".build/releases/v$Version/windows-x64") `
  -Version $Version -Target 'x86_64-pc-windows-msvc'

Complete-Stage -Timer $timer -EntryPoint $EntryPoint -Artifact $ExpectedNsis | Out-Null

Write-Host ""
Write-Host "==> Build completed"
Write-Host "App : $DesktopExe"
Write-Host "NSIS: $ExpectedNsis"
Write-Host "Release: $PublishedInstaller"
