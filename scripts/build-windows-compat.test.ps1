$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$script = Join-Path $root 'scripts/build-windows-compat.ps1'
$plans = @(& $script -DryRun)
if ($plans.Count -ne 2) { throw 'Must plan both Windows architectures by default' }
foreach ($plan in $plans) {
  if ($plan.Target -notmatch '^(i686|x86_64)-win7-windows-msvc$') { throw 'Must use a Win7 Rust target' }
  if ($plan.RustFlags -notlike '*getrandom_backend="windows_legacy"*') { throw 'Must avoid ProcessPrng on Win7' }
  if ($plan.RustFlags -notlike '*+crt-static*') { throw 'Must statically link the VC runtime' }
  if ($plan.RustFlags -notlike '*/DELAYLOAD:ole32.dll*' -or $plan.RustFlags -notlike '*/DELAYLOAD:api-ms-win-core-winrt-l1-1-0.dll*') { throw 'Must defer modern COM/WinRT entry points until used on supported OS versions' }
  if ($plan.Toolchain -ne 'nightly-2026-05-26') { throw 'Compatibility toolchain must be pinned' }
  if ($plan.InstallerName -notlike '*windows-compat*') { throw 'Candidate installers must not overwrite a published installer' }
}
$x86 = @(& $script -Architecture x86 -DryRun)
if ($x86.Count -ne 1 -or $x86[0].Target -ne 'i686-win7-windows-msvc') { throw 'x86 must select the 32-bit target' }
$x64 = @(& $script -Architecture x64 -DryRun)
if ($x64.Count -ne 1 -or $x64[0].Target -ne 'x86_64-win7-windows-msvc') { throw 'x64 must select the 64-bit target' }
Write-Host 'PASS: both architectures, Win7 targets, pinned toolchain, legacy RNG, static CRT and distinct candidate names.'
