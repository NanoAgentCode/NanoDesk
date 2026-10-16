$ErrorActionPreference = "Stop"
$Workspace = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$global:InstallerTestRoot = Join-Path $Workspace (".build\installer-test-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force (Join-Path $global:InstallerTestRoot "scripts"), (Join-Path $global:InstallerTestRoot "src-tauri") | Out-Null
Copy-Item -LiteralPath (Join-Path $Workspace "scripts\build-installer.ps1") -Destination (Join-Path $global:InstallerTestRoot "scripts\build-installer.ps1")
Copy-Item -LiteralPath (Join-Path $Workspace "brand.config.json") -Destination (Join-Path $global:InstallerTestRoot "brand.config.json")
Set-Content -LiteralPath (Join-Path $global:InstallerTestRoot "src-tauri\tauri.conf.json") -Value '{"version":"0.2.1"}'

# Simulate a successful build while another process changes the source version.
# No compiler, installer generator, or external shell is invoked by this fixture.
function global:cmd.exe {
  param([Parameter(ValueFromRemainingArguments = $true)] [string[]] $CmdArgs)
  if (($CmdArgs -join " ") -like '*npm.cmd run tauri build*') {
    $bundle = Join-Path $global:InstallerTestRoot "src-tauri\target\release\bundle\nsis"
    New-Item -ItemType Directory -Force $bundle | Out-Null
    Set-Content -LiteralPath (Join-Path $bundle "NanoDesk_0.2.1_x64-setup.exe") -Value "test installer"
    Set-Content -LiteralPath (Join-Path $global:InstallerTestRoot "src-tauri\tauri.conf.json") -Value '{"version":"0.2.2"}'
    $global:LASTEXITCODE = 0
    return
  }
  "PATH=$env:PATH"
  $global:LASTEXITCODE = 0
}

$PreviousVcVars = $env:VCVARS64
try {
  $env:VCVARS64 = Join-Path $global:InstallerTestRoot "vcvars64.bat"
  Set-Content -LiteralPath $env:VCVARS64 -Value "@echo off"
  & (Join-Path $global:InstallerTestRoot "scripts\build-installer.ps1")
  if (-not (Test-Path -LiteralPath (Join-Path $global:InstallerTestRoot "src-tauri\target\release\bundle\nsis\NanoDesk_0.2.1_x64-setup.exe"))) {
    throw "Regression fixture did not create the original-version installer"
  }
  Write-Host "PASS: artifact verification uses the build-start version even when the configuration changes during the build."
} finally {
  $env:VCVARS64 = $PreviousVcVars
  Remove-Item Function:\cmd.exe -ErrorAction SilentlyContinue
  Remove-Variable InstallerTestRoot -Scope Global -ErrorAction SilentlyContinue
}
