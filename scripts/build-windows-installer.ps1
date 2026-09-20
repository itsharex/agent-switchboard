[CmdletBinding()]
param(
  # The silent engine the wizard embeds: NSIS (per-user, customizable
  # directory) or MSI (per-machine, Program Files, elevated via UAC).
  [ValidateSet('Nsis', 'Msi')]
  [string]$Engine = 'Nsis',
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TauriArguments
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'The Windows installer must be built on Windows.' }

function Resolve-MSBuild {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  if (Test-Path -LiteralPath $vswhere -PathType Leaf) {
    $resolved = & $vswhere -latest -products * -requires Microsoft.Component.MSBuild -find 'MSBuild\**\Bin\MSBuild.exe'
    if ($LASTEXITCODE -eq 0 -and $resolved) {
      $candidate = $resolved | Select-Object -First 1
      if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }
  }

  $frameworkMsBuild = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\MSBuild.exe'
  if (Test-Path -LiteralPath $frameworkMsBuild -PathType Leaf) { return $frameworkMsBuild }
  throw 'MSBuild with .NET Framework WPF targets is required.'
}

function Assert-WpfTargetingPack {
  $targetingPack = Join-Path ${env:ProgramFiles(x86)} 'Reference Assemblies\Microsoft\Framework\.NETFramework\v4.8.1'
  $requiredAssemblies = @('mscorlib.dll', 'System.Xaml.dll', 'WindowsBase.dll', 'PresentationCore.dll', 'PresentationFramework.dll')
  $missing = @($requiredAssemblies | Where-Object { -not (Test-Path -LiteralPath (Join-Path $targetingPack $_) -PathType Leaf) })
  if ($missing.Count -gt 0) {
    throw '.NET Framework 4.8.1 targeting pack is required to build the custom WPF installer. Missing: ' + ($missing -join ', ')
  }
}

function ConvertTo-VerbatimCSharpLiteral {
  param([Parameter(Mandatory)][string]$Value)
  if ($Value -match '[\r\n]') { throw 'Installer metadata cannot contain newlines.' }
  return '@"' + $Value.Replace('"', '""') + '"'
}

function Assert-InstallerBackground {
  param([Parameter(Mandatory)][string]$Path)
  Add-Type -AssemblyName PresentationCore
  $stream = [IO.File]::OpenRead($Path)
  try {
    $decoder = [System.Windows.Media.Imaging.WmpBitmapDecoder]::new(
      $stream,
      [System.Windows.Media.Imaging.BitmapCreateOptions]::None,
      [System.Windows.Media.Imaging.BitmapCacheOption]::OnLoad)
    if ($decoder.Frames.Count -eq 0) { throw 'Installer background has no image frame.' }
  }
  finally {
    $stream.Dispose()
  }
}

$repository = Split-Path $PSScriptRoot -Parent
Push-Location $repository
try {
  $config = Get-Content -LiteralPath 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
  $package = Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json
  $version = (& node scripts/updater-release.mjs workspace-version --cargo-manifest Cargo.toml).Trim()
  if ($LASTEXITCODE -ne 0) { throw 'Could not read the workspace version.' }
  if ([string]::IsNullOrWhiteSpace($config.productName) -or [string]::IsNullOrWhiteSpace($package.name)) {
    throw 'Installer product metadata is incomplete.'
  }
  $msbuild = Resolve-MSBuild
  Assert-WpfTargetingPack
  Assert-InstallerBackground (Resolve-Path -LiteralPath 'installer/assets/installer-background.wdp').Path

  $targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
  $bundleRoot = [IO.Path]::GetFullPath((Join-Path $targetRoot 'release/bundle'))
  $engineBundle = if ($Engine -eq 'Msi') { 'msi' } else { 'nsis' }
  & node node_modules/@tauri-apps/cli/tauri.js build --config src-tauri/tauri.windows.conf.json --bundles $engineBundle @TauriArguments
  if ($LASTEXITCODE -ne 0) { throw "Tauri engine build failed (exit code $LASTEXITCODE)." }

  $enginePattern = if ($Engine -eq 'Msi') {
    "$($config.productName)_${version}_x64_en-US.msi"
  } else {
    "$($config.productName)_${version}_x64-setup.exe"
  }
  # PowerShell variable names are case-insensitive: the path must not reuse
  # the $Engine parameter's name or the ValidateSet rejects the assignment.
  $enginePath = (Resolve-Path -LiteralPath (Join-Path $bundleRoot "$engineBundle/$enginePattern")).Path
  if ($Engine -ne 'Msi') {
    $engineVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($enginePath).ProductVersion
    if ($engineVersion -ne $version) {
      throw "Installation engine version '$engineVersion' does not match workspace '$version'."
    }
  }
  # An MSI database carries no executable version resource. Its file name is
  # produced from the same workspace version and the Resolve-Path above only
  # succeeds when this exact build emitted it.

  $outputDirectory = Join-Path $bundleRoot 'installer'
  $buildDirectory = Join-Path $targetRoot 'release/agent-switchboard-installer'
  # Rebuild's clean pass deletes what the previous build recorded in the
  # intermediate directory; keeping one per engine lets both setup artifacts
  # coexist in the output directory.
  $intermediateDirectory = Join-Path (Join-Path $buildDirectory 'obj') $engineBundle
  New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
  New-Item -ItemType Directory -Path $buildDirectory -Force | Out-Null
  New-Item -ItemType Directory -Path $intermediateDirectory -Force | Out-Null

  $fileVersion = ($version -split '[-+]')[0] + '.0'
  $buildInfo = [IO.Path]::GetFullPath((Join-Path $buildDirectory 'InstallerBuildInfo.cs'))
  $buildInfoSource = @"
using System.Reflection;
[assembly: AssemblyVersion("$fileVersion")]
[assembly: AssemblyFileVersion("$fileVersion")]
[assembly: AssemblyInformationalVersion($(ConvertTo-VerbatimCSharpLiteral $version))]
[assembly: AssemblyProduct($(ConvertTo-VerbatimCSharpLiteral $config.productName))]

namespace AgentSwitchboard.Installer
{
    internal static class InstallerProductMetadata
    {
        internal const string ProductName = $(ConvertTo-VerbatimCSharpLiteral $config.productName);
        internal const string Version = $(ConvertTo-VerbatimCSharpLiteral $version);
        internal const string ApplicationFileStem = $(ConvertTo-VerbatimCSharpLiteral $package.name);
        internal static readonly bool UsesMsiEngine = $($(if ($Engine -eq 'Msi') { 'true' } else { 'false' }));
    }
}
"@
  [IO.File]::WriteAllText($buildInfo, $buildInfoSource, [Text.UTF8Encoding]::new($false))

  $targetName = if ($Engine -eq 'Msi') {
    "$($config.productName)_${version}_x64-msi-setup"
  } else {
    "$($config.productName)_${version}_x64-setup"
  }
  $outputPath = [IO.Path]::GetFullPath($outputDirectory) + [IO.Path]::DirectorySeparatorChar
  $intermediatePath = [IO.Path]::GetFullPath($intermediateDirectory) + [IO.Path]::DirectorySeparatorChar
  $buildArguments = @(
    'installer/AgentSwitchboard.Installer.csproj',
    '/nologo',
    '/m',
    '/t:Rebuild',
    '/p:Configuration=Release',
    '/p:Platform=x64',
    "/p:OutputPath=$outputPath",
    "/p:IntermediateOutputPath=$intermediatePath",
    "/p:TargetName=$targetName",
    "/p:InstallerEnginePath=$enginePath",
    "/p:InstallerEngineKind=$Engine",
    "/p:InstallerBuildInfoFile=$buildInfo"
  )
  & $msbuild @buildArguments
  if ($LASTEXITCODE -ne 0) { throw "Custom installer compilation failed (exit code $LASTEXITCODE)." }

  $output = Join-Path $outputDirectory "$targetName.exe"
  if (-not (Test-Path -LiteralPath $output -PathType Leaf)) {
    throw 'Custom installer compilation did not produce the expected executable.'
  }
  $compiledVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($output)
  if ($compiledVersion.ProductVersion -ne $version -or $compiledVersion.FileVersion -ne $fileVersion -or $compiledVersion.ProductName -ne $config.productName) {
    throw 'Compiled installer version metadata does not match the workspace contract.'
  }

  if (Test-Path -LiteralPath "$output.sig") { Remove-Item -LiteralPath "$output.sig" }
  if ($env:TAURI_SIGNING_PRIVATE_KEY) {
    & node node_modules/@tauri-apps/cli/tauri.js signer sign $output
    if ($LASTEXITCODE -ne 0) { throw "Custom installer signing failed (exit code $LASTEXITCODE)." }
  }
  Write-Output "Custom installer: $output"
}
finally { Pop-Location }
