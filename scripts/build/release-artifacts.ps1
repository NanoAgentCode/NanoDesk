# Shared release validation and artifact staging. No publishing/network side effects.
function Get-ReleaseVersion {
  param([string]$Root)
  $version = (Get-Content -LiteralPath (Join-Path $Root 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
  $packageVersion = (Get-Content -LiteralPath (Join-Path $Root 'package.json') -Raw | ConvertFrom-Json).version
  $cargo = Get-Content -LiteralPath (Join-Path $Root 'src-tauri/Cargo.toml') -Raw
  $packageSection = [regex]::Match($cargo, '(?ms)^\[package\]\s*(.*?)(?=^\[|\z)').Groups[1].Value
  $cargoVersion = [regex]::Match($packageSection, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
  if (-not $version -or $version -ne $packageVersion -or $version -ne $cargoVersion) {
    throw "Release versions differ: Tauri=$version, npm=$packageVersion, Cargo=$cargoVersion"
  }
  return $version
}

function Export-ReleaseArtifact {
  param([string]$Root, [string]$Installer, [string]$OutputDirectory, [string]$Version, [string]$Target)
  $file = Get-Item -LiteralPath $Installer -ErrorAction Stop
  if ($file.Length -eq 0) { throw "Installer is empty: $Installer" }
  New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
  $destination = Join-Path $OutputDirectory $file.Name
  Copy-Item -LiteralPath $Installer -Destination $destination -Force
  # Use .NET hashing so MSVC environment imports cannot affect module discovery.
  $sha = [Security.Cryptography.SHA256]::Create()
  $stream = [IO.File]::OpenRead($destination)
  try { $hash = ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
  finally { $stream.Dispose(); $sha.Dispose() }
  $utf8 = New-Object Text.UTF8Encoding($false)
  [IO.File]::WriteAllText("$destination.sha256", "$hash  $($file.Name)`n", $utf8)
  $commit = $null
  $dirty = $null
  # Do not attribute a staging directory to the enclosing checkout.
  if (Test-Path -LiteralPath (Join-Path $Root '.git')) {
    $commit = & git -C $Root rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot determine release source commit' }
    $changes = @(& git -C $Root status --porcelain)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot determine release source status' }
    $dirty = $changes.Count -gt 0
  }
  $info = @{
    version = $Version; target = $Target; installer = $file.Name
    sha256 = $hash; sizeBytes = $file.Length
    sourceCommit = $commit; sourceHasChanges = $dirty
    builtAtUtc = [DateTime]::UtcNow.ToString('o')
  } | ConvertTo-Json
  [IO.File]::WriteAllText((Join-Path $OutputDirectory 'BUILD-INFO.json'), $info, $utf8)
  $notes = ''
  $changelog = Join-Path $Root 'CHANGELOG.md'
  if (Test-Path -LiteralPath $changelog) {
    $content = [IO.File]::ReadAllText($changelog)
    $section = [regex]::Match($content, ('(?ms)^## v' + [regex]::Escape($Version) + '(?:[ \t][^\r\n]*)?\r?\n(.*?)(?=^## |\z)'))
    if ($section.Success) { $notes = $section.Groups[1].Value.Trim() }
  }
  # Same release shape as the legacy API; no API call is needed to read this file.
  $manifest = @{
    tag_name = "v$Version"; body = $notes; draft = $false; prerelease = $false
    assets = @(@{
      name = $file.Name
      browser_download_url = "https://github.com/NanoAgentCode/NanoDesk/releases/download/v$Version/$($file.Name)"
    })
  } | ConvertTo-Json -Depth 5
  [IO.File]::WriteAllText((Join-Path $OutputDirectory 'latest.json'), $manifest, $utf8)
  return $destination
}
