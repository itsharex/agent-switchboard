[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'Desktop E2E driver setup requires Windows.'
}

$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$targetRoot = Join-Path $repository 'target'
$toolsRoot = Join-Path $targetRoot 'e2e-tools'
$buildRoot = Join-Path $targetRoot 'e2e-driver-build'
$toolchain = 'stable-x86_64-pc-windows-gnu'
$targetTriple = 'x86_64-pc-windows-gnu'
$driverVersion = '2.0.6'
$definitionUrl = 'https://raw.githubusercontent.com/mingw-w64/mingw-w64/master/mingw-w64-crt/lib-common/ktmw32.def'
$definitionSha256 = '4ad71b531f91462c619651a9c42c54fac184b10b3f5c2362832d01721060e206'

function Assert-WorkspaceTarget([string]$Path) {
  $absolute = [IO.Path]::GetFullPath($Path)
  $prefix = $targetRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
  if ($absolute -ne $targetRoot -and -not $absolute.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Driver output escaped the workspace target directory: $absolute"
  }
  $current = $absolute
  while ($current -and $current -ne $repository) {
    if (Test-Path -LiteralPath $current) {
      $item = Get-Item -LiteralPath $current -Force
      if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Driver output contains a symlink or junction: $current"
      }
    }
    $current = Split-Path $current -Parent
  }
  return $absolute
}

function Get-RequiredCommand([string]$Name) {
  $command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $command) { throw "Required GNU setup command is missing from the current PATH: $Name" }
  return $command.Source
}

function Test-InstalledDriver {
  $binary = Assert-WorkspaceTarget (Join-Path $toolsRoot 'bin/tauri-driver.exe')
  $metadata = Assert-WorkspaceTarget (Join-Path $toolsRoot '.crates2.json')
  if (-not (Test-Path -LiteralPath $binary -PathType Leaf) -or -not (Test-Path -LiteralPath $metadata -PathType Leaf)) {
    return $false
  }
  # tauri-driver 2.0.6 has no --version option. Cargo owns the installed version.
  $installs = (Get-Content -LiteralPath $metadata -Raw | ConvertFrom-Json).installs
  $entry = $installs.PSObject.Properties | Where-Object {
    $_.Name -like "tauri-driver $driverVersion (registry+*)" -and
      $_.Value.target -eq $targetTriple -and $_.Value.bins -contains 'tauri-driver.exe'
  }
  return $null -ne $entry
}

function Get-KtmImportLibrary([string]$Gcc, [string]$Dlltool) {
  $candidate = (& $Gcc '-print-file-name=libktmw32.a').Trim()
  if ($LASTEXITCODE -ne 0) { throw 'GCC could not resolve its Windows import libraries.' }
  if ($candidate -ne 'libktmw32.a' -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {
    return (Get-Item -LiteralPath $candidate).FullName
  }
  $directory = Assert-WorkspaceTarget (Join-Path $toolsRoot 'lib')
  New-Item -ItemType Directory -Path $directory -Force | Out-Null
  $definition = Assert-WorkspaceTarget (Join-Path $directory 'ktmw32.def')
  if (-not (Test-Path -LiteralPath $definition -PathType Leaf)) {
    Invoke-WebRequest -Uri $definitionUrl -OutFile $definition -UseBasicParsing
  }
  $actual = (Get-FileHash -LiteralPath $definition -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $definitionSha256) {
    throw "Official ktmw32.def checksum mismatch; refusing driver build. Expected $definitionSha256, received $actual. File retained: $definition"
  }
  $library = Assert-WorkspaceTarget (Join-Path $directory 'libktmw32.a')
  if (-not (Test-Path -LiteralPath $library -PathType Leaf)) {
    & $Dlltool '--machine' 'i386:x86-64' '--dllname' 'KtmW32.dll' '--input-def' $definition '--output-lib' $library
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $library -PathType Leaf)) {
      throw 'GNU dlltool failed to create the x64 KtmW32.dll import library.'
    }
  }
  return $library
}

$environmentNames = @('CARGO_HOME', 'CARGO_TARGET_DIR', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'TEMP', 'TMP', 'TMPDIR')
$previousEnvironment = @{}
foreach ($name in $environmentNames) { $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
Push-Location $repository
try {
  if (Test-InstalledDriver) {
    Write-Output "tauri-driver $driverVersion is already installed for $targetTriple at $toolsRoot."
    return
  }
  $cargo = Get-RequiredCommand 'cargo.exe'
  $rustup = Get-RequiredCommand 'rustup.exe'
  $gcc = Get-RequiredCommand 'gcc.exe'
  $dlltool = Get-RequiredCommand 'dlltool.exe'
  $installedToolchains = & $rustup 'toolchain' 'list'
  if ($LASTEXITCODE -ne 0 -or -not ($installedToolchains -match "^$([regex]::Escape($toolchain))(\s|$)")) {
    throw "The already-installed $toolchain toolchain is required; setup does not change or install a default toolchain."
  }
  $machine = (& $gcc '-dumpmachine').Trim()
  if ($LASTEXITCODE -ne 0 -or $machine -ne 'x86_64-w64-mingw32') {
    throw "The current GCC must target x86_64-w64-mingw32; received $machine."
  }
  $env:CARGO_HOME = Assert-WorkspaceTarget (Join-Path $toolsRoot 'cargo-home')
  $env:CARGO_TARGET_DIR = Assert-WorkspaceTarget $buildRoot
  $temporary = Assert-WorkspaceTarget (Join-Path $buildRoot 'tmp')
  foreach ($directory in @($toolsRoot, $env:CARGO_HOME, $buildRoot, $temporary)) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
  }
  foreach ($name in @('TEMP', 'TMP', 'TMPDIR')) { [Environment]::SetEnvironmentVariable($name, $temporary, 'Process') }
  $library = Get-KtmImportLibrary $gcc $dlltool
  $dllName = (& $dlltool '--identify' $library).Trim()
  if ($LASTEXITCODE -ne 0 -or $dllName -ine 'ktmw32.dll') { throw "Unexpected Windows DLL import library: $library" }
  $nativeDirectory = Split-Path $library -Parent
  $env:RUSTFLAGS = "-L native=$nativeDirectory"
  # Cargo's encoded form preserves the directory as one argument if it contains spaces.
  $env:CARGO_ENCODED_RUSTFLAGS = '-L' + [char]0x1f + "native=$nativeDirectory"
  & $cargo "+$toolchain" 'install' 'tauri-driver' '--version' $driverVersion '--locked' '--target' $targetTriple '--root' $toolsRoot
  if ($LASTEXITCODE -ne 0) { throw "tauri-driver $driverVersion installation failed ($LASTEXITCODE); build output remains in $buildRoot." }
  if (-not (Test-InstalledDriver)) { throw 'Cargo did not record the expected GNU tauri-driver installation.' }
  Write-Output "Installed tauri-driver $driverVersion at $toolsRoot. Edge WebDriver is matched automatically by @wdio/tauri-service."
}
finally {
  foreach ($name in $environmentNames) {
    if ($null -eq $previousEnvironment[$name]) {
      Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue
    } else {
      [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process')
    }
  }
  Pop-Location
}
