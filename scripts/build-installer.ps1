$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$Brand = Get-Content -LiteralPath (Join-Path $Root "brand.config.json") -Raw | ConvertFrom-Json
$VcVars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"

if (-not (Test-Path -LiteralPath $VcVars)) {
  throw "Visual Studio Build Tools was not found: $VcVars"
}

Push-Location $Root
try {
  Write-Host "==> Building $($Brand.displayName) installer"
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

  Write-Host "==> Building $($Brand.displayName) desktop app and NSIS installer only"
  & cmd.exe /d /s /c 'npm.cmd run tauri build -- --bundles nsis'

  if ($LASTEXITCODE -ne 0) {
    throw "Tauri build failed. Exit code: $LASTEXITCODE"
  }

  $ReleaseDir = Join-Path $Root "src-tauri\target\release"
  $DesktopExe = Join-Path $ReleaseDir "$($Brand.desktopBinaryName).exe"
  $BundleDir = Join-Path $ReleaseDir "bundle"
  $NsisDir = Join-Path $BundleDir "nsis"
  $Version = (Get-Content -LiteralPath (Join-Path $Root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
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
