#Requires -Version 5.1
# Candidate installers for Win7 SP1 / Win8 / Win8.1 / Win10 x86+x64 and Win11 x64.
# Build hosts require modern Windows, MSVC and the pinned nightly with rust-src.
param(
  [ValidateSet('x86', 'x64', 'all')] [string]$Architecture = 'all',
  [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$version = (Get-Content (Join-Path $root 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
$architectures = if ($Architecture -eq 'all') { @('x86', 'x64') } else { @($Architecture) }
$plans = @($architectures | ForEach-Object {
  [pscustomobject]@{
    Architecture = $_
    Target = if ($_ -eq 'x86') { 'i686-win7-windows-msvc' } else { 'x86_64-win7-windows-msvc' }
    Toolchain = 'nightly-2026-05-26'
    RustFlags = '--cfg getrandom_backend="windows_legacy" --cfg windows_slim_errors -C target-feature=+crt-static -C link-arg=/DELAYLOAD:api-ms-win-core-winrt-l1-1-0.dll -C link-arg=/DELAYLOAD:ole32.dll -C link-arg=delayimp.lib'
    InstallerName = "NanoDesk_${version}_windows-compat_${_}-setup.exe"
  }
})
if ($DryRun) { return $plans }

Import-Module (Join-Path $PSScriptRoot 'build/common.psm1') -DisableNameChecking
$toolchain = $plans[0].Toolchain
& rustup run $toolchain rustc --version
if ($LASTEXITCODE -ne 0) { throw "Install the toolchain first: rustup toolchain install $toolchain --profile minimal --component rust-src" }
$sysroot = & rustup run $toolchain rustc --print sysroot
if (-not (Test-Path (Join-Path $sysroot 'lib/rustlib/src/rust/library/Cargo.toml'))) {
  throw "Install rust-src first: rustup component add rust-src --toolchain $toolchain"
}
& (Join-Path $PSScriptRoot 'build/build-frontend.ps1') -ProjectRoot $root

$output = Join-Path $root ".build/windows-compat/$version"
New-Item -ItemType Directory -Path $output -Force | Out-Null
$override = Join-Path $output 'tauri-compat.json'
$config = @{
  build = @{ beforeBuildCommand = '' }
  bundle = @{ windows = @{ nsis = @{ installerHooks = (Join-Path $root 'src-tauri/windows-compat-hooks.nsh') } } }
} | ConvertTo-Json -Depth 6 -Compress
[IO.File]::WriteAllText($override, $config, [Text.UTF8Encoding]::new($false))

# Isolate process environment; never change the global/default Rust toolchain.
$keys = @('RUSTUP_TOOLCHAIN', 'TAURI_CONFIG', 'CARGO_TARGET_DIR', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'STATIC_VCRUNTIME')
foreach ($plan in $plans) {
  $keys += ('CARGO_TARGET_' + $plan.Target.Replace('-', '_').ToUpperInvariant() + '_RUSTFLAGS')
  $keys += ('CFLAGS_' + $plan.Target.Replace('-', '_')), ('CXXFLAGS_' + $plan.Target.Replace('-', '_'))
}
$saved = @{}
foreach ($key in $keys) { $saved[$key] = [Environment]::GetEnvironmentVariable($key, 'Process') }
Push-Location $root
try {
  $env:RUSTUP_TOOLCHAIN = $toolchain
  $env:CARGO_TARGET_DIR = Join-Path $root 'src-tauri/target'
  $env:TAURI_CONFIG = $config
  # Tauri's STATIC_VCRUNTIME=true intentionally keeps the UCRT dynamic (Win10
  # baseline). Disable that helper and let +crt-static link the entire CRT.
  $env:STATIC_VCRUNTIME = 'false'
  Remove-Item Env:RUSTFLAGS, Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
  foreach ($plan in $plans) {
    Write-Host "==> Compatibility candidate: $($plan.Target)"
    Initialize-MSVCEnvironment -Root $root -Architecture $plan.Architecture
    $flagKey = 'CARGO_TARGET_' + $plan.Target.Replace('-', '_').ToUpperInvariant() + '_RUSTFLAGS'
    [Environment]::SetEnvironmentVariable($flagKey, $plan.RustFlags, 'Process')
    foreach ($prefix in @('CFLAGS_', 'CXXFLAGS_')) {
      $key = $prefix + $plan.Target.Replace('-', '_')
      [Environment]::SetEnvironmentVariable($key, (($saved[$key] + ' /MT').Trim()), 'Process')
    }
    # Tauri build rejects Tier-3 targets because rustup does not distribute std
    # for them. Build std and the app with Cargo, then use Tauri's bundle command.
    # Include libtest up front to share the rebuilt std graph with acceptance
    # tests instead of compiling all target dependencies a second time.
    & cargo build --manifest-path src-tauri/Cargo.toml --release --bins --target $plan.Target -Z build-std=std,panic_abort,test --features 'tauri/custom-protocol,windows-legacy' --locked
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed for $($plan.Target)" }
    $release = Join-Path $env:CARGO_TARGET_DIR "$($plan.Target)/release"
    $exe = Join-Path $release 'nano-desk.exe'
    & (Join-Path $PSScriptRoot 'check-windows-compat-binary.ps1') -Path $exe -Architecture $plan.Architecture -ReportPath (Join-Path $output "$($plan.Architecture)-imports.txt")
    & (Join-Path $PSScriptRoot 'check-windows-compat-binary.ps1') -Path (Join-Path $release 'nano.exe') -Architecture $plan.Architecture -ReportPath (Join-Path $output "$($plan.Architecture)-cli-imports.txt")
    & (Join-Path $release 'nano.exe') --version
    if ($LASTEXITCODE -ne 0) { throw "CLI startup smoke test failed for $($plan.Target)" }
    # Existing automation recovery tests share a lease in the system temp
    # directory; run serially so unrelated tests cannot claim that same lease.
    & cargo test --manifest-path src-tauri/Cargo.toml --release --target $plan.Target -Z build-std=std,panic_abort,test --features 'tauri/custom-protocol,windows-legacy' --lib --test windows_compat --locked -- --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "Compatibility runtime tests failed for $($plan.Target)" }
    & npm.cmd run tauri -- bundle --target $plan.Target --bundles nsis --config $override --ci
    if ($LASTEXITCODE -ne 0) { throw "NSIS bundling failed for $($plan.Target)" }
    $installer = Join-Path $release "bundle/nsis/NanoDesk_${version}_$($plan.Architecture)-setup.exe"
    if (-not (Test-Path -LiteralPath $installer)) { throw "Installer missing: $installer" }
    Copy-Item -LiteralPath $installer -Destination (Join-Path $output $plan.InstallerName) -Force
    $sha = [Security.Cryptography.SHA256]::Create()
    $stream = [IO.File]::OpenRead((Join-Path $output $plan.InstallerName))
    try { $hash = ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
    finally { $stream.Dispose(); $sha.Dispose() }
    [IO.File]::WriteAllText((Join-Path $output "$($plan.InstallerName).sha256"), "$hash  $($plan.InstallerName)`n", [Text.UTF8Encoding]::new($false))
  }
  $sourceCommit = & git rev-parse HEAD
  $sourceChanges = @(& git status --porcelain)
  $info = @{
    version = $version
    toolchain = $toolchain
    sourceCommit = "$sourceCommit"
    sourceHasChanges = ($sourceChanges.Count -gt 0)
    sourceChanges = $sourceChanges
    architectures = @($plans | ForEach-Object { $_.Architecture })
    builtAtUtc = [DateTime]::UtcNow.ToString('o')
    status = 'candidate; operating system acceptance tests pending'
    legacyWebView2 = 'Requires separately installed Microsoft WebView2 Runtime 109 on Windows 7/8/8.1'
  } | ConvertTo-Json -Depth 4
  [IO.File]::WriteAllText((Join-Path $output 'BUILD-INFO.json'), $info, [Text.UTF8Encoding]::new($false))
} finally {
  Pop-Location
  foreach ($key in $keys) { [Environment]::SetEnvironmentVariable($key, $saved[$key], 'Process') }
}
Write-Host "==> Candidates: $output (OS acceptance tests still required)"
