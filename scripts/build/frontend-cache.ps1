# ── 前端指纹 ─────────────────────────────────────────────────
# 指纹输入清单：新增构建输入（配置文件等）必须同步登记到这里。
$script:FrontendFingerprintInputs = @(
  "src",
  "index.html",
  "package.json",
  "package-lock.json",
  "public",
  "brand.config.json",
  "scripts/sync-branding.mjs"
)

function Get-FrontendFingerprint {
  # 相对路径 + 内容哈希合并为单一 SHA-256；src/ 下排除 *.test.*。
  param([string]$Root, [switch]$Output)
  $inputs = @()
  foreach ($pattern in @("vite.config.*", "tsconfig*.json", ".env*")) {
    $inputs += @(Get-ChildItem -LiteralPath $Root -Filter $pattern -File -ErrorAction SilentlyContinue |
      ForEach-Object { $_.Name })
  }
  $inputs += $script:FrontendFingerprintInputs
  if ($Output) { $inputs = @("dist") }

  $entries = New-Object System.Collections.Generic.List[string]
  foreach ($item in $inputs) {
    $full = Join-Path $Root $item
    if (-not (Test-Path -LiteralPath $full)) {
      $entries.Add("$item|missing")
      continue
    }
    $files = if (Test-Path -LiteralPath $full -PathType Container) {
      Get-ChildItem -LiteralPath $full -Recurse -File
    } else {
      @(Get-Item -LiteralPath $full)
    }
    foreach ($file in $files) {
      if ($file.Name -like "*.test.*") { continue }
      $rel = $file.FullName.Substring($Root.Length).TrimStart('\').Replace('\', '/').ToLowerInvariant()
      # Use .NET directly so hashing does not depend on host module discovery
      # after importing the MSVC environment (PowerShell 5.1/7 interoperability).
      $fileSha = [System.Security.Cryptography.SHA256]::Create()
      $stream = [System.IO.File]::OpenRead($file.FullName)
      try {
        $fileHash = ([BitConverter]::ToString($fileSha.ComputeHash($stream))).Replace("-", "").ToLowerInvariant()
        $entries.Add("$rel|$fileHash")
      } finally {
        $stream.Dispose()
        $fileSha.Dispose()
      }
    }
  }
  $joined = ($entries | Sort-Object) -join "`n"
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($joined)
    return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Test-FrontendUpToDate {
  # 输入及完整产物内容一致才跳过，产物丢失或缓存损坏时自动重建。
  param([string]$Root)
  $stampPath = Join-Path $Root ".build\frontend-fingerprint.json"
  $distIndex = Join-Path $Root "dist\index.html"
  if (-not (Test-Path -LiteralPath $stampPath)) {
    return @{ Fresh = $false; Reason = "尚无前端指纹记录" }
  }
  if (-not (Test-Path -LiteralPath $distIndex)) {
    return @{ Fresh = $false; Reason = "缺少 dist/index.html" }
  }
  try { $stamp = Get-Content -LiteralPath $stampPath -Raw | ConvertFrom-Json }
  catch { return @{ Fresh = $false; Reason = "前端指纹记录损坏" } }
  if ($stamp.fingerprint -ne (Get-FrontendFingerprint -Root $Root)) {
    return @{ Fresh = $false; Reason = "前端输入指纹已变化" }
  }
  if (-not $stamp.output_fingerprint -or $stamp.output_fingerprint -ne (Get-FrontendFingerprint -Root $Root -Output)) {
    return @{ Fresh = $false; Reason = "前端产物缺失或内容已变化" }
  }
  return @{ Fresh = $true; Reason = "前端指纹未变化，dist 为最新" }
}

function Save-FrontendFingerprint {
  param([string]$Root, [string]$ExpectedFingerprint = "")
  $current = Get-FrontendFingerprint -Root $Root
  if ($ExpectedFingerprint -and $ExpectedFingerprint -ne $current) { throw "前端构建期间输入发生变化，请重新打包。" }
  $stateDir = Join-Path $Root ".build"
  New-Item -ItemType Directory -Path $stateDir -Force | Out-Null
  $stampPath = Join-Path $stateDir "frontend-fingerprint.json"
  @{ fingerprint = $current; output_fingerprint = Get-FrontendFingerprint -Root $Root -Output; saved_at = (Get-Date).ToString("o") } |
    ConvertTo-Json | Set-Content -LiteralPath $stampPath -Encoding UTF8
}
