$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Brand = Get-Content -LiteralPath (Join-Path $Root "brand.config.json") -Raw | ConvertFrom-Json
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

Push-Location $Root
try {
  Write-Host "==> Building $($Brand.displayName) installer"
  Write-Host "==> Workspace: $Root"

  # Cargo 与 Tauri 的生成文件包含工作区绝对路径。仓库移动或改名后继续复用
  # 原 target 会让权限清单仍指向旧目录，因此在路径变化时执行一次干净构建。
  $CargoTarget = [System.IO.Path]::GetFullPath((Join-Path $Root "src-tauri\target"))
  $ExpectedTargetRoot = [System.IO.Path]::GetFullPath((Join-Path $Root "src-tauri"))
  if (-not $CargoTarget.StartsWith($ExpectedTargetRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to clean Cargo target outside the workspace: $CargoTarget"
  }
  $WorkspaceMarker = Join-Path $CargoTarget ".nanodesk-workspace-root"
  $RecordedWorkspace = if (Test-Path -LiteralPath $WorkspaceMarker) {
    (Get-Content -LiteralPath $WorkspaceMarker -Raw).Trim()
  } else {
    ""
  }
  if ((Test-Path -LiteralPath $CargoTarget) -and $RecordedWorkspace -ne $Root) {
    Write-Host "==> Cargo cache belongs to another workspace; cleaning: $CargoTarget"
    Remove-Item -LiteralPath $CargoTarget -Recurse -Force
  }
  New-Item -ItemType Directory -Path $CargoTarget -Force | Out-Null
  Set-Content -LiteralPath $WorkspaceMarker -Value $Root -Encoding UTF8

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

  Write-Host "==> Building $($Brand.displayName) desktop app and NSIS installer only"
  # Keep the version used when starting this build; source files may change while it runs.
  $Version = (Get-Content -LiteralPath (Join-Path $Root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
  & cmd.exe /d /s /c 'npm.cmd run tauri build -- --bundles nsis'

  if ($LASTEXITCODE -ne 0) {
    throw "Tauri build failed. Exit code: $LASTEXITCODE"
  }

  $ReleaseDir = Join-Path $Root "src-tauri\target\release"
  $DesktopExe = Join-Path $ReleaseDir "$($Brand.desktopBinaryName).exe"
  $BundleDir = Join-Path $ReleaseDir "bundle"
  $NsisDir = Join-Path $BundleDir "nsis"
  $ExpectedNsis = Join-Path $NsisDir "$($Brand.displayName)_${Version}_x64-setup.exe"
  if (-not (Test-Path -LiteralPath $ExpectedNsis)) {
    throw "NSIS installer was not found: $ExpectedNsis"
  }

  Write-Host ""
  Write-Host "==> Build completed"
  Write-Host "App : $DesktopExe"
  Write-Host "NSIS: $ExpectedNsis"
} finally {
  Pop-Location
}
