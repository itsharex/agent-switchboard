[CmdletBinding()]
param(
  [switch]$Apply,
  [ValidateRange(0, 1024)][double]$MaxSandboxGiB = 0.5,
  [ValidateRange(0, 1024)][double]$MaxRustGiB = 8,
  [ValidateRange(1, 365)][int]$MinSandboxAgeDays = 3,
  [ValidateRange(1, 365)][int]$MinRustAgeDays = 1
)

$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (((Get-Item -LiteralPath $repository -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
  throw "Repository root is a reparse point: $repository"
}
$sandboxCutoff = [DateTime]::UtcNow.AddDays(-$MinSandboxAgeDays)
$rustCutoff = [DateTime]::UtcNow.AddDays(-$MinRustAgeDays)

function Get-CacheEntry {
  param([Parameter(Mandatory)][string]$RelativePath)

  $parts = $RelativePath -split '[\\/]'
  if ($parts | Where-Object { $_ -in @('', '.', '..') }) { throw "Invalid cache path: $RelativePath" }
  $path = [IO.Path]::GetFullPath((Join-Path $repository $RelativePath))
  if (-not $path.StartsWith($repository + [IO.Path]::DirectorySeparatorChar,
      [StringComparison]::OrdinalIgnoreCase)) { throw "Cache path leaves the repository: $path" }
  if (-not (Test-Path -LiteralPath $path -PathType Container)) { return $null }

  $cursor = $repository
  foreach ($part in $parts) {
    $cursor = Join-Path $cursor $part
    $segment = Get-Item -LiteralPath $cursor -Force
    if (($segment.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "Cache path contains a reparse point: $cursor"
    }
  }
  $resolved = (Resolve-Path -LiteralPath $path).Path
  if (-not $resolved.StartsWith($repository + [IO.Path]::DirectorySeparatorChar,
      [StringComparison]::OrdinalIgnoreCase)) { throw "Resolved cache path leaves the repository: $resolved" }
  $items = @(Get-ChildItem -LiteralPath $resolved -Recurse -Force)
  if ($items | Where-Object { ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 }) {
    throw "Cache contains a reparse point: $resolved"
  }
  $files = @($items | Where-Object { -not $_.PSIsContainer })
  $bytes = [long](($files | Measure-Object Length -Sum).Sum)
  $latest = if ($files.Count) { ($files | Measure-Object LastWriteTimeUtc -Maximum).Maximum }
    else { (Get-Item -LiteralPath $resolved).LastWriteTimeUtc }
  return [pscustomobject]@{ RelativePath = $RelativePath; Path = $resolved; Bytes = $bytes; Latest = $latest }
}

function Remove-VerifiedEntries {
  param([object[]]$Entries, [DateTime]$Cutoff)

  foreach ($entry in $Entries) {
    $current = Get-CacheEntry $entry.RelativePath
    if ($null -eq $current -or $current.Path -ne $entry.Path -or $current.Bytes -ne $entry.Bytes -or
      $current.Latest -gt $Cutoff) {
      throw "Cache changed after review; nothing further removed: $($entry.RelativePath)"
    }
    Remove-Item -LiteralPath $current.Path -Recurse -Force
    Write-Output "Removed $($entry.RelativePath) ($([math]::Round($current.Bytes / 1MB, 1)) MiB)"
  }
}

$sandboxPaths = @(
  '.tmp\readme-screenshot-sandbox\headless-edge-profile',
  '.tmp\readme-screenshot-sandbox\appdata\local\dev.agent-switchboard.sandboxdocs\EBWebView',
  '.tmp\claude-ui-review\sandbox\applocal\Microsoft\Edge\User Data',
  '.tmp\web-sandbox\localappdata\Microsoft\Edge\User Data',
  '.tmp\gateway-live-20260917\localappdata\dev.agent-switchboard.desktop.sandbox\EBWebView'
)
$sandboxes = @($sandboxPaths | ForEach-Object { Get-CacheEntry $_ } | Where-Object { $null -ne $_ })
$sandboxBytes = [long](($sandboxes | Measure-Object Bytes -Sum).Sum)
$sandboxBudget = [long]($MaxSandboxGiB * 1GB)
$sandboxToRemove = @()
foreach ($entry in @($sandboxes | Sort-Object Latest)) {
  if ($sandboxBytes -le $sandboxBudget) { break }
  if ($entry.Latest -gt $sandboxCutoff) { continue }
  $sandboxToRemove += $entry
  $sandboxBytes -= $entry.Bytes
}

$rustGroups = @(
  @{ Name = 'desktop development'; Paths = @('target-dev\debug') },
  @{ Name = 'development'; Paths = @('target\debug') },
  @{ Name = 'release intermediates'; Paths = @('target\release\build', 'target\release\deps',
      'target\release\incremental', 'target\release\.fingerprint') },
  @{ Name = 'alternate release intermediates'; Paths = @('target-dev\release\build', 'target-dev\release\deps',
      'target-dev\release\incremental', 'target-dev\release\.fingerprint') }
)
$rustToRemove = @()
$rustBudget = [long]($MaxRustGiB * 1GB)
foreach ($group in $rustGroups) {
  $entries = @($group.Paths | ForEach-Object { Get-CacheEntry $_ } | Where-Object { $null -ne $_ })
  if (-not $entries.Count) { continue }
  $bytes = [long](($entries | Measure-Object Bytes -Sum).Sum)
  $latest = ($entries | Measure-Object Latest -Maximum).Maximum
  if ($bytes -gt $rustBudget -and $latest -le $rustCutoff) { $rustToRemove += $entries }
  Write-Output "$($group.Name): $([math]::Round($bytes / 1GB, 2)) GiB; latest $latest"
}

Write-Output "Known verification browser profiles: $([math]::Round((($sandboxes | Measure-Object Bytes -Sum).Sum) / 1GB, 2)) GiB"
Write-Output "Eligible sandbox recovery: $([math]::Round((($sandboxToRemove | Measure-Object Bytes -Sum).Sum) / 1GB, 2)) GiB"
Write-Output "Eligible Rust recovery: $([math]::Round((($rustToRemove | Measure-Object Bytes -Sum).Sum) / 1GB, 2)) GiB"
foreach ($entry in @($sandboxToRemove) + @($rustToRemove)) {
  Write-Output "  $($entry.RelativePath) — $([math]::Round($entry.Bytes / 1MB, 1)) MiB; latest $($entry.Latest) UTC"
}

if (-not $Apply) { Write-Output 'Dry run only. Pass -Apply to remove the listed cache directories.'; return }
Remove-VerifiedEntries $sandboxToRemove $sandboxCutoff
Remove-VerifiedEntries $rustToRemove $rustCutoff
