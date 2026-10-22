#Requires -Version 5.1
# common.psm1 - 模块化打包公共模块
# 提供：MSVC 环境注入、workspace 标记、计时观测、品牌配置读取
# 供 build-frontend / build-app / build-bundle / build-all 共享

$script:TimingLogFile = Join-Path $PSScriptRoot "..\..\.build\build-timing.json"

# ── 品牌配置 ────────────────────────────────────────────────
function Get-BrandConfig {
  param([string]$Root)
  return Get-Content -LiteralPath (Join-Path $Root "brand.config.json") -Raw | ConvertFrom-Json
}

# ── Workspace 标记 ────────────────────────────────────────────
# Cargo 与 Tauri 的生成文件包含工作区绝对路径。仓库移动或改名后继续复用
# 原 target 会让权限清单仍指向旧目录，因此在路径变化时执行一次干净构建。
function Initialize-WorkspaceMarker {
  param([string]$Root)

  $CargoTarget = [System.IO.Path]::GetFullPath((Join-Path $Root "src-tauri\target"))
  $ExpectedTargetRoot = [System.IO.Path]::GetFullPath((Join-Path $Root "src-tauri"))
  if (-not $CargoTarget.StartsWith($ExpectedTargetRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to clean Cargo target outside the workspace: $CargoTarget"
  }
  $WorkspaceMarker = Join-Path $CargoTarget ".nanodesk-workspace-root"
  $RecordedWorkspace = if (Test-Path -LiteralPath $WorkspaceMarker) {
    # 兼容历史 BOM 标记：PS5.1 的 UTF8 编码会写入 BOM，不剥离会导致每次构建误清 target。
    (Get-Content -LiteralPath $WorkspaceMarker -Raw).TrimStart([char]0xFEFF).Trim()
  } else {
    ""
  }
  if ((Test-Path -LiteralPath $CargoTarget) -and $RecordedWorkspace -ne $Root) {
    Write-Host "==> Cargo cache belongs to another workspace; cleaning: $CargoTarget"
    Remove-Item -LiteralPath $CargoTarget -Recurse -Force
  }
  New-Item -ItemType Directory -Path $CargoTarget -Force | Out-Null
  [System.IO.File]::WriteAllText($WorkspaceMarker, $Root, (New-Object System.Text.UTF8Encoding($false)))
}

# ── MSVC 环境注入 ─────────────────────────────────────────────
function Initialize-MSVCEnvironment {
  param([string]$Root, [ValidateSet("x64", "x86")] [string]$Architecture = "x64")

  $VcVars = $env:VCVARS64
  if (-not $VcVars) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path -LiteralPath $vswhere) {
      $vsInstall = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
      if ($vsInstall) {
        $VcVars = Join-Path ($vsInstall | Select-Object -First 1) "VC\Auxiliary\Build\vcvars64.bat"
      }
    }
    if (-not $VcVars) {
      foreach ($base in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
        $candidate = Join-Path $base "Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
        if (Test-Path -LiteralPath $candidate) { $VcVars = $candidate; break }
      }
    }
  }

  if (-not $VcVars -or -not (Test-Path -LiteralPath $VcVars)) {
    throw "MSVC x64 build tools were not found. Install Visual Studio Build Tools with 'Desktop development with C++' (MSVC x64/x86 and Windows SDK), then retry. For a custom installation, set VCVARS64 to the full path of vcvars64.bat. Download: https://aka.ms/vs/17/release/vs_BuildTools.exe"
  }

  if ($Architecture -eq "x86") {
    $VcVars = Join-Path (Split-Path $VcVars -Parent) "vcvarsamd64_x86.bat"
    if (-not (Test-Path -LiteralPath $VcVars)) { throw "MSVC x86 build environment was not found: $VcVars" }
  }

  # vcvars64.bat 仅在 cmd 上下文有效，先在 cmd 中采集完整的 MSVC 环境。
  $envDump = & cmd.exe /d /s /c ('"' + $VcVars + '" && set')
  $msvcEnv = @{}
  foreach ($line in $envDump) {
    $idx = $line.IndexOf('=')
    if ($idx -gt 0) {
      $msvcEnv[$line.Substring(0, $idx)] = $line.Substring($idx + 1)
    }
  }

  # 关键修复：Git for Windows 的 /usr/bin/link.exe 会抢占 MSVC 的 link.exe，
  # 导致 Rust 链接失败（报 "/usr/bin/link: missing operand"）。这里在 PATH
  # 最前面加上 MSVC、SDK 与 cargo，并移除任何 Git\usr\bin / Git\mingw...\bin。
  $cleanPath = $msvcEnv['PATH']
  $cargoHome = $env:CARGO_HOME; if (-not $cargoHome) { $cargoHome = Join-Path $env:USERPROFILE '.cargo' }
  $cargoBin = Join-Path $cargoHome 'bin'
  $segments = $cleanPath -split ';' | Where-Object {
    $_ -and ($_) -notmatch 'Git[\\/]+(usr|mingw\d+)[\\/]+bin' -and $_ -ne $cargoBin
  }
  $msvcEnv['PATH'] = (@($cargoBin) + $segments) -join ';'

  $msvcInclude = Join-Path $Root ".build\msvc-include-shims"
  $excptHeader = Join-Path $msvcInclude "excpt.h"
  $MsvcTools = [System.IO.Path]::GetFullPath((Join-Path (Split-Path $VcVars -Parent) "..\..\Tools\MSVC"))
  $damagedExcptHeader = Get-ChildItem -LiteralPath $MsvcTools -Recurse -Filter "excpt.*" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -ne "excpt.h" -and (Get-Content -LiteralPath $_.FullName -TotalCount 1 -ErrorAction SilentlyContinue) -contains "//" } |
    Select-Object -First 1
  if ($damagedExcptHeader -and -not (Test-Path -LiteralPath (Join-Path $damagedExcptHeader.DirectoryName "excpt.h"))) {
    New-Item -ItemType Directory -Path $msvcInclude -Force | Out-Null
    Copy-Item -LiteralPath $damagedExcptHeader.FullName -Destination $excptHeader -Force
    $msvcEnv['INCLUDE'] = "$msvcInclude;$($msvcEnv['INCLUDE'])"
    Write-Host "==> Using local MSVC header shim: $excptHeader"
  }

  foreach ($key in $msvcEnv.Keys) { Set-Item -Path ('Env:' + $key) -Value $msvcEnv[$key] }

  Write-Host "==> Using linker: $((Get-Command link.exe -ErrorAction SilentlyContinue).Source)"
}

# ── 阶段计时 ─────────────────────────────────────────────────
$script:StageRecords = [System.Collections.Generic.List[object]]::new()
$script:CurrentRunId = ""

function Set-BuildRunId {
  # 每次构建入口调用一次，用于计时日志按"次"保留。
  param([string]$RunId)
  $script:CurrentRunId = $RunId
}

function Start-Stage {
  param([string]$StageId)
  return @{
    StageId = $StageId
    Start   = [DateTime]::UtcNow
  }
}

function Complete-Stage {
  param(
    [hashtable]$Timer,
    [string]$EntryPoint,
    [switch]$Skipped,
    [string]$Artifact
  )
  $end = [DateTime]::UtcNow
  $durationMs = [long]($end - $Timer.Start).TotalMilliseconds
  $skipLabel = if ($Skipped) { "skipped" } else { "done" }
  Write-Host ("==> [stage/{0}] {1} {2} ({3:N0} ms)" -f $Timer.StageId, $Timer.Start.ToString("HH:mm:ss"), $skipLabel, $durationMs)

  $script:StageRecords.Add(@{
    runId      = $script:CurrentRunId
    stage      = $Timer.StageId
    entry      = $EntryPoint
    startedAt  = $Timer.Start.ToString("o")
    durationMs = $durationMs
    skipped    = [bool]$Skipped
    artifact   = $Artifact
  }) | Out-Null

  return $durationMs
}

function Write-BuildSummary {
  Write-Host ""
  Write-Host "==> Stage timing summary"
  Write-Host ("  {0,-12} {1,10}  {2}" -f "Stage", "Duration", "Skipped")
  Write-Host ("  " + ("-" * 36))
  foreach ($r in $script:StageRecords) {
    Write-Host ("  {0,-12} {1,8:N0} ms  {2}" -f $r.stage, $r.durationMs, $r.skipped)
  }
  Save-TimingLog
}

function Save-TimingLog {
  $dir = Split-Path $script:TimingLogFile -Parent
  if (-not (Test-Path -LiteralPath $dir)) {
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
  }
  $existing = @()
  if (Test-Path -LiteralPath $script:TimingLogFile) {
    try { $existing = Get-Content -LiteralPath $script:TimingLogFile -Raw | ConvertFrom-Json } catch { $existing = @() }
  }
  if ($existing -isnot [array]) { $existing = @($existing) }
  $combined = @($existing) + @($script:StageRecords)
  # 保留最近 20 次运行（按 runId 去重取末尾 20 个）。
  $runIds = @($combined | ForEach-Object { $_.runId } | Select-Object -Unique)
  if ($runIds.Count -gt 20) {
    $keep = @($runIds | Select-Object -Last 20)
    $combined = @($combined | Where-Object { $keep -contains $_.runId })
  }
  $combined | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $script:TimingLogFile -Encoding UTF8
}

function Get-StageRecords { return $script:StageRecords }

# ── 项目根路径 ────────────────────────────────────────────────
function Get-ProjectRoot {
  return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

. (Join-Path $PSScriptRoot "frontend-cache.ps1")
. (Join-Path $PSScriptRoot "release-artifacts.ps1")

function Get-TauriOverrideConfig {
  param([string]$Root)
  $stateDir = Join-Path $Root ".build"
  New-Item -ItemType Directory -Path $stateDir -Force | Out-Null
  $path = Join-Path $stateDir "tauri-bundle-config.json"
  $content = '{"build":{"beforeBuildCommand":""}}'
  if (-not (Test-Path -LiteralPath $path) -or [System.IO.File]::ReadAllText($path) -cne $content) {
    [System.IO.File]::WriteAllText($path, $content, (New-Object System.Text.UTF8Encoding($false)))
  }
  return $path
}

Export-ModuleMember -Function `
  Get-ReleaseVersion, Export-ReleaseArtifact, `
  Get-TauriOverrideConfig, Get-BrandConfig, `
  Initialize-WorkspaceMarker, `
  Initialize-MSVCEnvironment, `
  Set-BuildRunId, `
  Start-Stage, Complete-Stage, Write-BuildSummary, Save-TimingLog, Get-StageRecords, `
  Get-ProjectRoot, `
  Get-FrontendFingerprint, Test-FrontendUpToDate, Save-FrontendFingerprint
