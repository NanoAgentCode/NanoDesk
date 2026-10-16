$ErrorActionPreference = "Stop"
$Workspace = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$global:InstallerTestRoot = Join-Path $Workspace (".build\installer-test-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force (Join-Path $global:InstallerTestRoot "scripts\build"), (Join-Path $global:InstallerTestRoot "src-tauri"), (Join-Path $global:InstallerTestRoot "src") | Out-Null
Copy-Item -Path (Join-Path $Workspace "scripts\build\*") -Destination (Join-Path $global:InstallerTestRoot "scripts\build") -Force
Copy-Item -LiteralPath (Join-Path $Workspace "scripts\build-installer.ps1") -Destination (Join-Path $global:InstallerTestRoot "scripts\build-installer.ps1")
Copy-Item -LiteralPath (Join-Path $Workspace "brand.config.json") -Destination (Join-Path $global:InstallerTestRoot "brand.config.json")
Set-Content -LiteralPath (Join-Path $global:InstallerTestRoot "src-tauri\tauri.conf.json") -Value '{"version":"0.2.1"}'
Set-Content -LiteralPath (Join-Path $global:InstallerTestRoot "src\app.tsx") -Value "export const app = 1;"

# Simulate successful builds without a compiler, installer generator, or network.
# The stage scripts Push-Location to the fixture root before invoking cmd.exe,
# so the mocks create artifacts relative to the current location.
function global:cmd.exe {
  param([Parameter(ValueFromRemainingArguments = $true)] [string[]] $CmdArgs)
  $command = $CmdArgs -join " "
  if ($command -like '*npm.cmd run build*') {
    New-Item -ItemType Directory -Force "dist" | Out-Null
    Set-Content -LiteralPath "dist\index.html" -Value "<html>fixture</html>"
    $global:LASTEXITCODE = 0
    return
  }
  if ($command -like '*npm.cmd run tauri build*') {
    if ($command -notmatch '--config "([^"]+)"') {
      throw "Bundle must provide a beforeBuildCommand override"
    }
    $override = Get-Content -LiteralPath $Matches[1] -Raw | ConvertFrom-Json
    if ($override.build.beforeBuildCommand -cne "") {
      throw "Bundle must disable the duplicate frontend build"
    }
    Import-Module (Join-Path $global:InstallerTestRoot "scripts\build\common.psm1") -DisableNameChecking
    if (-not (Test-FrontendUpToDate -Root $global:InstallerTestRoot).Fresh) {
      throw "Bundle must only embed a current frontend"
    }
    # Verify beforeBuildCommand was overridden away: the bundle stage must not
    # reach the frontend build command itself (separate branch above).
    $version = (Get-Content -LiteralPath "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
    $bundle = Join-Path (Get-Location) "src-tauri\target\release\bundle\nsis"
    New-Item -ItemType Directory -Force $bundle | Out-Null
    Set-Content -LiteralPath (Join-Path $bundle "NanoDesk_${version}_x64-setup.exe") -Value "test installer"
    Set-Content -LiteralPath "src-tauri\tauri.conf.json" -Value '{"version":"0.2.2"}'
    $global:LASTEXITCODE = 0
    return
  }
  # vcvars64 dump fallback
  "PATH=$env:PATH"
  $global:LASTEXITCODE = 0
}

$global:StageOutput = ""
function Invoke-Build {
  # 注意：不要用数组 splat 传 "-Fast"——splat 的数组元素按位置绑定，
  # 会被当成 $EntryPoint 的值而不是开关。
  param([string]$File, [switch]$Fast)
  if ($Fast) {
    $global:StageOutput = (& $File -Fast 6>&1 | Out-String)
  } else {
    $global:StageOutput = (& $File 6>&1 | Out-String)
  }
}

function Get-FrontendRecords {
  # 不带 -Force：构建脚本已在本会话加载 common 模块，重导入会清空计时记录。
  Import-Module (Join-Path $global:InstallerTestRoot "scripts\build\common.psm1") -DisableNameChecking
  return @(Get-StageRecords | Where-Object { $_.stage -eq "frontend" })
}

$PreviousVcVars = $env:VCVARS64
try {
  # Regression: hashing must work even when Get-FileHash cannot be resolved
  # in a PowerShell host after the MSVC environment has been loaded.
  function global:Get-FileHash { throw "Get-FileHash is unavailable in this test host" }
  $env:VCVARS64 = Join-Path $global:InstallerTestRoot "vcvars64.bat"
  Set-Content -LiteralPath $env:VCVARS64 -Value "@echo off"

  # ── 场景 1：package:win 全流程 + 版本固定校验回归 ──
  Invoke-Build -File (Join-Path $global:InstallerTestRoot "scripts\build-installer.ps1")
  if (-not (Test-Path -LiteralPath (Join-Path $global:InstallerTestRoot "src-tauri\target\release\bundle\nsis\NanoDesk_0.2.1_x64-setup.exe"))) {
    throw "Regression fixture did not create the original-version installer"
  }
  if ($global:StageOutput -notlike "*Stage timing summary*") {
    throw "package:win should print the stage timing summary"
  }
  $records = @(Get-FrontendRecords)
  if ($records.Count -ne 1 -or $records[0].skipped) {
    throw "package:win must always rebuild the frontend (full mode)"
  }
  Write-Host "PASS: package:win full flow; artifact verification uses the build-start version even when the configuration changes during the build."

  # ── 场景 2：package:fast 增量复用——前端应被跳过 ──
  $distIndex = Join-Path $global:InstallerTestRoot "dist\index.html"
  $distBefore = (Get-Item -LiteralPath $distIndex).LastWriteTimeUtc
  Start-Sleep -Milliseconds 50
  Invoke-Build -File (Join-Path $global:InstallerTestRoot "scripts\build\build-all.ps1") -Fast
  $records = @(Get-FrontendRecords)
  if (-not $records[-1].skipped) {
    throw "package:fast should skip the frontend when the fingerprint is unchanged"
  }
  if ((Get-Item -LiteralPath $distIndex).LastWriteTimeUtc -ne $distBefore) {
    throw "package:fast must not rewrite dist when the frontend is skipped"
  }
  Write-Host "PASS: package:fast skipped the frontend via fingerprint."

  # ── 场景 3：指纹失效后应重建 ──
  Set-Content -LiteralPath (Join-Path $global:InstallerTestRoot "src\app.tsx") -Value "export const app = 2;"
  Invoke-Build -File (Join-Path $global:InstallerTestRoot "scripts\build\build-all.ps1") -Fast
  $records = @(Get-FrontendRecords)
  if ($records[-1].skipped) {
    throw "package:fast must rebuild the frontend after a source change"
  }
  Write-Host "PASS: frontend fingerprint invalidation triggers a rebuild."

  # ── 场景 4：dry-run 输出阶段计划与跳过原因，且不产生副作用 ──
  Set-Content -LiteralPath (Join-Path $global:InstallerTestRoot "src\app.tsx") -Value "export const app = 3;"
  $dryRunOutput = (& (Join-Path $global:InstallerTestRoot "scripts\build\build-all.ps1") -Fast -DryRun 6>&1 | ForEach-Object { "$_" }) -join "`n"
  foreach ($marker in @("dry-run", "frontend", "bundle")) {
    if ($dryRunOutput -notlike "*$marker*") {
      throw "dry-run output should mention '$marker'"
    }
  }
  if ($dryRunOutput -notlike "*已变化*") {
    throw "dry-run output should explain why the frontend will rebuild"
  }
  if ($dryRunOutput -like "*[stage/frontend] brand:check*") {
    throw "dry-run must not execute the frontend build"
  }
  Write-Host "PASS: dry-run prints the stage plan with skip reasons and performs no build."

  # Standalone bundle must refresh stale frontend rather than embed old dist.
  & (Join-Path $global:InstallerTestRoot "scripts\build\build-bundle.ps1") | Out-Null
  $records = @(Get-FrontendRecords)
  if ($records[-1].skipped) { throw "Standalone bundle must refresh stale frontend" }
  Write-Host "PASS: standalone bundle refreshes stale frontend before packaging."
} finally {
  Remove-Item Function:\Get-FileHash -ErrorAction SilentlyContinue
  $env:VCVARS64 = $PreviousVcVars
  Remove-Item Function:\cmd.exe -ErrorAction SilentlyContinue
  if ($global:InstallerTestRoot -and (Test-Path -LiteralPath $global:InstallerTestRoot)) {
    Remove-Item -LiteralPath $global:InstallerTestRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
  Remove-Variable InstallerTestRoot -Scope Global -ErrorAction SilentlyContinue
}
