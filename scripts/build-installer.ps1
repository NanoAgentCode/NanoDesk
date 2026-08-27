$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$VcVars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"

if (-not (Test-Path -LiteralPath $VcVars)) {
  throw "Visual Studio Build Tools was not found: $VcVars"
}

Push-Location $Root
try {
  Write-Host "==> Building NanoAgent installer"
  Write-Host "==> Workspace: $Root"

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
  $damagedExcptHeader = Get-ChildItem -LiteralPath "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC" -Recurse -Filter "excpt.*" -ErrorAction SilentlyContinue |
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

  $TauriDir = Join-Path $Root "src-tauri"
  Write-Host "==> Building nano CLI"
  Push-Location $TauriDir
  try {
    & cargo.exe build --release --bin nano
  } finally {
    Pop-Location
  }

  if ($LASTEXITCODE -ne 0) {
    throw "CLI build failed. Exit code: $LASTEXITCODE"
  }

  Write-Host "==> Building NanoAgent desktop app and installers"
  & cmd.exe /d /s /c 'npm.cmd run tauri build'

  if ($LASTEXITCODE -ne 0) {
    throw "Tauri build failed. Exit code: $LASTEXITCODE"
  }

  $ReleaseDir = Join-Path $Root "src-tauri\target\release"
  $CliExe = Join-Path $ReleaseDir "nano.exe"
  $DesktopExe = Join-Path $ReleaseDir "nano-agent.exe"
  $BundleDir = Join-Path $ReleaseDir "bundle"
  $NsisDir = Join-Path $BundleDir "nsis"
  $CliInstallerDir = Join-Path $BundleDir "cli"
  $Version = (Get-Content -LiteralPath (Join-Path $Root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
  $CliInstaller = Join-Path $CliInstallerDir "NanoAgent-CLI_${Version}_x64-setup.exe"
  $NsisRoot = Join-Path $env:LOCALAPPDATA "tauri\NSIS"
  $MakeNsis = Join-Path $NsisRoot "makensis.exe"

  $StandardNsis = Get-ChildItem -LiteralPath $NsisDir -Filter "*.exe" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notlike "*-offline-setup.exe" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
  if (-not $StandardNsis) {
    throw "Standard NSIS installer was not found in: $NsisDir"
  }

  $OfflineNsisName = $StandardNsis.Name -replace '-setup\.exe$', '-offline-setup.exe'
  if ($OfflineNsisName -eq $StandardNsis.Name) {
    throw "Unexpected standard NSIS installer name: $($StandardNsis.Name)"
  }
  $OfflineNsisPath = Join-Path $NsisDir $OfflineNsisName
  $StandardNsisBackup = [System.IO.Path]::GetTempFileName()
  Copy-Item -LiteralPath $StandardNsis.FullName -Destination $StandardNsisBackup -Force
  try {
    Write-Host "==> Building offline NSIS installer with embedded WebView2 Runtime"
    & cmd.exe /d /s /c 'npm.cmd run tauri bundle -- --bundles nsis --config src-tauri/tauri.offline.conf.json'
    if ($LASTEXITCODE -ne 0) {
      throw "Offline NSIS bundle failed. Exit code: $LASTEXITCODE"
    }

    if (-not (Test-Path -LiteralPath $StandardNsis.FullName)) {
      throw "Offline NSIS installer was not found after bundling: $($StandardNsis.FullName)"
    }
    Copy-Item -LiteralPath $StandardNsis.FullName -Destination $OfflineNsisPath -Force
  } finally {
    Copy-Item -LiteralPath $StandardNsisBackup -Destination $StandardNsis.FullName -Force
    Remove-Item -LiteralPath $StandardNsisBackup -Force -ErrorAction SilentlyContinue
  }
  $OfflineNsis = Get-Item -LiteralPath $OfflineNsisPath
  $StandardNsis = Get-Item -LiteralPath $StandardNsis.FullName
  if ($OfflineNsis.Length -le $StandardNsis.Length) {
    throw "Offline NSIS installer does not appear to contain WebView2 Runtime: $OfflineNsisPath"
  }

  if (-not (Test-Path -LiteralPath $CliExe)) {
    throw "CLI executable was not found: $CliExe"
  }
  if (-not (Test-Path -LiteralPath $MakeNsis)) {
    throw "NSIS compiler was not found after Tauri build: $MakeNsis"
  }

  New-Item -ItemType Directory -Path $CliInstallerDir -Force | Out-Null
  Write-Host "==> Building NanoAgent CLI installer"
  & $MakeNsis /V2 "/DCLI_EXE=$CliExe" "/DPATH_HELPER=$(Join-Path $Root 'scripts\update-user-path.ps1')" "/DOUTPUT_FILE=$CliInstaller" "/DPRODUCT_VERSION=$Version" "/DAPP_ICON=$(Join-Path $Root 'src-tauri\icons\icon.ico')" (Join-Path $Root "scripts\nano-cli-installer.nsi")

  if ($LASTEXITCODE -ne 0) {
    throw "CLI installer build failed. Exit code: $LASTEXITCODE"
  }

  $Msi = Get-ChildItem -LiteralPath (Join-Path $BundleDir "msi") -Filter "*.msi" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1

  Write-Host ""
  Write-Host "==> Build completed"
  Write-Host "CLI : $CliExe"
  Write-Host "CLI installer: $CliInstaller"
  Write-Host "App : $DesktopExe"
  Write-Host "NSIS: $($StandardNsis.FullName)"
  Write-Host "NSIS offline: $($OfflineNsis.FullName)"
  if ($Msi) {
    Write-Host "MSI : $($Msi.FullName)"
  }
} finally {
  Pop-Location
}
