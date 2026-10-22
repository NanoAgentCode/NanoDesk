$ErrorActionPreference = 'Stop'
$check = Join-Path $PSScriptRoot 'check-windows-compat-binary.ps1'
$root = Join-Path $env:TEMP ('nanodesk-pe-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$global:CompatTestImports = 'KERNEL32.dll GetTickCount'
function global:dumpbin.exe {
  $global:LASTEXITCODE = 0
  return $global:CompatTestImports
}
function Assert-Rejected([scriptblock]$Action, [string]$Message) {
  $rejected = $false
  try { & $Action } catch { $rejected = $true }
  if (-not $rejected) { throw $Message }
}
try {
  $fixture = Join-Path $root 'fixture.exe'
  foreach ($arch in @('x86', 'x64')) {
    $bytes = New-Object byte[] 256
    $bytes[0] = 0x4d; $bytes[1] = 0x5a
    [BitConverter]::GetBytes([int]64).CopyTo($bytes, 0x3c)
    [BitConverter]::GetBytes([uint32]0x4550).CopyTo($bytes, 64)
    $machine = if ($arch -eq 'x86') { [uint16]0x14c } else { [uint16]0x8664 }
    [BitConverter]::GetBytes($machine).CopyTo($bytes, 68)
    [BitConverter]::GetBytes([uint16]6).CopyTo($bytes, 64 + 24 + 48)
    [BitConverter]::GetBytes([uint16]1).CopyTo($bytes, 64 + 24 + 50)
    [IO.File]::WriteAllBytes($fixture, $bytes)
    & $check -Path $fixture -Architecture $arch
    $wrongArch = if ($arch -eq 'x86') { 'x64' } else { 'x86' }
    Assert-Rejected { & $check -Path $fixture -Architecture $wrongArch } 'Must reject wrong architecture'
    foreach ($bad in @('bcryptprimitives.dll', 'ProcessPrng', 'GetDpiForWindow', 'VCRUNTIME140.dll', 'MSVCP140.dll', 'api-ms-win-crt-runtime-l1-1-0.dll', 'api-ms-win-core-winrt-l1-1-0.dll', 'CoIncrementMTAUsage')) {
      $global:CompatTestImports = "`n    $bad`n"
      Assert-Rejected { & $check -Path $fixture -Architecture $arch } "Must reject $bad"
    }
    $global:CompatTestImports = 'KERNEL32.dll GetTickCount'
    $global:CompatTestImports += "`nSection contains the following delay load imports:`n api-ms-win-core-winrt-l1-1-0.dll`n CoIncrementMTAUsage"
    & $check -Path $fixture -Architecture $arch
    [BitConverter]::GetBytes([uint16]10).CopyTo($bytes, 64 + 24 + 48)
    [IO.File]::WriteAllBytes($fixture, $bytes)
    Assert-Rejected { & $check -Path $fixture -Architecture $arch } 'Must reject Windows 10 subsystem baseline'
  }
  Write-Host 'PASS: PE architecture, subsystem and import screening rejects incompatible fixtures.'
} finally {
  Remove-Item Function:\dumpbin.exe -ErrorAction SilentlyContinue
  # Delete only this test-created, fully resolved temporary directory.
  $resolved = (Resolve-Path -LiteralPath $root).Path
  $tempRoot = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
  if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected test directory' }
  Remove-Item -LiteralPath $resolved -Recurse -Force
}
