param(
  [Parameter(Mandatory = $true)] [string]$Path,
  [Parameter(Mandatory = $true)] [ValidateSet('x86', 'x64')] [string]$Architecture,
  [string]$ReportPath
)
$ErrorActionPreference = 'Stop'
$bytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Path).Path)
if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) { throw 'Not a Windows PE executable' }
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
if ($pe -lt 64 -or $pe + 96 -gt $bytes.Length -or [BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550) { throw 'Invalid PE header' }
$machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
$expected = if ($Architecture -eq 'x86') { 0x14c } else { 0x8664 }
if ($machine -ne $expected) { throw "Wrong PE architecture: $machine, expected $Architecture" }
$subsystemMajor = [BitConverter]::ToUInt16($bytes, $pe + 24 + 48)
$subsystemMinor = [BitConverter]::ToUInt16($bytes, $pe + 24 + 50)
if ($subsystemMajor -gt 6 -or ($subsystemMajor -eq 6 -and $subsystemMinor -gt 1)) { throw "PE subsystem requires newer than Win7: $subsystemMajor.$subsystemMinor" }
$imports = & dumpbin.exe /imports $Path 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) { throw 'dumpbin /imports failed' }
if ($ReportPath) { [IO.File]::WriteAllText($ReportPath, $imports, [Text.UTF8Encoding]::new($false)) }
# Known load-time blockers; this is a screening check, not a full Win7 API audit.
if ($imports -match '(?im)^\s*(bcryptprimitives\.dll|api-ms-win-crt-[^\s]+\.dll|VCRUNTIME\d+[^\s]*\.dll|MSVCP\d+[^\s]*\.dll)\s*$' -or
    $imports -match '\b(ProcessPrng|GetSystemTimePreciseAsFileTime|SetThreadDescription|GetDpiForWindow|GetDpiForSystem|IsWow64Process2)\b') {
  throw 'Known Win7-incompatible imports or dynamic VC runtime found; inspect the imports report'
}
$eagerImports = ($imports -split 'Section contains the following delay load imports')[0]
if ($eagerImports -match '(?i)api-ms-win-core-winrt[^\s]*\.dll|\bCoIncrementMTAUsage\b') {
  throw 'WinRT APIs must be delay-loaded: they do not exist on Windows 7'
}
Write-Host "PASS: $Architecture PE header and known Win7 import blockers ($subsystemMajor.$subsystemMinor)"
