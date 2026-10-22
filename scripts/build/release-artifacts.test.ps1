$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
Import-Module (Join-Path $PSScriptRoot 'common.psm1') -DisableNameChecking
$fixture = Join-Path $workspace ('.build/release-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $fixture 'src-tauri') -Force | Out-Null
try {
  Set-Content (Join-Path $fixture 'package.json') '{"version":"1.2.3"}'
  Set-Content (Join-Path $fixture 'src-tauri/tauri.conf.json') '{"version":"1.2.3"}'
  Set-Content (Join-Path $fixture 'src-tauri/Cargo.toml') "[package]`nversion = `"1.2.3`"`n[dependencies]`nother = `"9.0.0`""
  if ((Get-ReleaseVersion -Root $fixture) -ne '1.2.3') { throw 'Matching versions must be accepted' }
  Set-Content (Join-Path $fixture 'package.json') '{"version":"1.2.4"}'
  $rejected = $false
  try { Get-ReleaseVersion -Root $fixture | Out-Null } catch { $rejected = $true }
  if (-not $rejected) { throw 'Mismatched versions must fail before building' }
  Set-Content (Join-Path $fixture 'package.json') '{"version":"1.2.3"}'
  Set-Content (Join-Path $fixture 'src-tauri/Cargo.toml') "[package]`nversion = `"1.2.4`""
  $rejected = $false
  try { Get-ReleaseVersion -Root $fixture | Out-Null } catch { $rejected = $true }
  if (-not $rejected) { throw 'Cargo version mismatches must also fail' }
  $installer = Join-Path $fixture 'NanoDesk_1.2.3_x64-setup.exe'
  Set-Content $installer 'installer bytes'
  $output = Join-Path $fixture 'release'
  Export-ReleaseArtifact -Root $fixture -Installer $installer -OutputDirectory $output -Version '1.2.3' -Target 'x86_64-pc-windows-msvc' | Out-Null
  $hash = (Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()
  $name = Split-Path $installer -Leaf
  if ((Get-Content (Join-Path $output "$name.sha256") -Raw).Trim() -ne "$hash  $name") { throw 'Checksum must match copied bytes' }
  $info = Get-Content (Join-Path $output 'BUILD-INFO.json') -Raw | ConvertFrom-Json
  if ($info.version -ne '1.2.3' -or $info.sha256 -ne $hash -or $info.target -ne 'x86_64-pc-windows-msvc') { throw 'Build metadata must identify artifact' }
  if ($info.sourceCommit -or $info.sourceHasChanges -ne $null) { throw 'Non-repository fixtures must not inherit parent repository provenance' }
  $rejected = $false
  try { Export-ReleaseArtifact -Root $fixture -Installer (Join-Path $fixture 'missing.exe') -OutputDirectory $output -Version '1.2.3' -Target 'x64' } catch { $rejected = $true }
  if (-not $rejected) { throw 'Missing installer must not be exported' }
  $empty = Join-Path $fixture 'empty.exe'
  [IO.File]::WriteAllBytes($empty, [byte[]]@())
  $rejected = $false
  try { Export-ReleaseArtifact -Root $fixture -Installer $empty -OutputDirectory $output -Version '1.2.3' -Target 'x64' } catch { $rejected = $true }
  if (-not $rejected) { throw 'Empty installer must not be exported' }
  Write-Host 'PASS: release version validation, checksums, metadata and missing artifacts.'
} finally {
  $resolved = [IO.Path]::GetFullPath($fixture)
  if (-not $resolved.StartsWith((Join-Path $workspace '.build/release-test-'), [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe fixture cleanup' }
  Remove-Item -LiteralPath $resolved -Recurse -Force
}
