[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Desktop E2E requires Windows and WebView2.' }
$repository = Split-Path $PSScriptRoot -Parent
$previousTarget = $env:CARGO_TARGET_DIR
$previousDevelopment = $env:ASB_WEB_DEVELOPMENT
Push-Location $repository
try {
  $env:CARGO_TARGET_DIR = Join-Path $repository 'target/e2e-desktop'
  Remove-Item Env:ASB_WEB_DEVELOPMENT -ErrorAction SilentlyContinue
  & node e2e/support/build-fingerprint.mjs capture
  if ($LASTEXITCODE -ne 0) { throw 'Could not capture desktop build inputs.' }
  & node node_modules/@tauri-apps/cli/tauri.js build --no-bundle --features desktop-e2e --config src-tauri/tauri.e2e.conf.json -- --locked
  if ($LASTEXITCODE -ne 0) { throw "Desktop E2E release build failed ($LASTEXITCODE)." }
  & node e2e/support/build-fingerprint.mjs verify
  if ($LASTEXITCODE -ne 0) { throw 'Source files changed during the build; build again before testing.' }
  $release = Join-Path $env:CARGO_TARGET_DIR 'release'
  $executable = Join-Path $release 'Agent Switchboard E2E.exe'
  if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Tauri did not produce the expected E2E executable: $executable"
  }
  Copy-Item -LiteralPath (Join-Path $repository 'src-tauri/bin/WebView2Loader.dll') -Destination $release
  $manifest = @{
    executable = $executable
    sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    builtAt = [DateTime]::UtcNow.ToString('o')
    identifier = 'dev.agent-switchboard.desktop.e2e'
    feature = 'desktop-e2e'
    mode = 'release'
    sourceSha256 = (Get-Content -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'build-inputs.json') -Raw | ConvertFrom-Json).sha256
  }
  $manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'build-manifest.json') -Encoding utf8
  Write-Output "Built real desktop E2E application: $executable"
}
finally {
  $env:CARGO_TARGET_DIR = $previousTarget
  $env:ASB_WEB_DEVELOPMENT = $previousDevelopment
  Pop-Location
}
