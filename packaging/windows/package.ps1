<#
.SYNOPSIS
  Build, sign and package SwiftPDF for Windows.

.DESCRIPTION
  Produces, in $env:DIST (default: dist/release):
    SwiftPDF-<version>-windows-<arch>.msi            per-machine installer (WiX v5)
    SwiftPDF-<version>-windows-<arch>-portable.zip   SwiftPDF.exe + models\ + portable.txt

  pdfcraft-cli.exe is not built or packaged. The CLI sources stay in the tree for development.

  The binaries link the C runtime statically (+crt-static), so neither the MSI nor the portable
  zip needs the Visual C++ redistributable. Signing is delegated to sign.ps1 (skipped with a
  warning when no signing secrets are set).

  Needs: Rust (MSVC toolchain + the target), curl and the network (the OCR models), the Windows SDK (rc.exe, signtool.exe),
  and WiX v5: dotnet tool install --global wix --version 5.0.2

.EXAMPLE
  pwsh packaging/windows/package.ps1 -Arch x64
  pwsh packaging/windows/package.ps1 -Arch x86 -SkipBuild
  pwsh packaging/windows/package.ps1 -Arch arm64     # cross-compiled; needs the MSVC ARM64 build tools
#>
param(
  [ValidateSet('x64', 'x86', 'arm64')] [string] $Arch = 'x64',
  [switch] $SkipBuild
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

# The version lives in one place: [workspace.package] version in the root Cargo.toml.
$Version = $env:PDFCRAFT_VERSION
if (-not $Version) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
  }
}
if (-not $Version) { throw 'could not read [workspace.package] version from Cargo.toml' }
# MSI ProductVersion is numeric (major.minor.build); pre-release tags are dropped there.
$MsiVersion = ($Version -split '-')[0]

$Target = switch ($Arch) { 'x64' { 'x86_64-pc-windows-msvc' } 'x86' { 'i686-pc-windows-msvc' } 'arm64' { 'aarch64-pc-windows-msvc' } }
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist\release' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

if (-not $env:PDFCRAFT_BUILD_SHA) { $env:PDFCRAFT_BUILD_SHA = (git -C $Root rev-parse HEAD 2>$null) }
if (-not $env:PDFCRAFT_BUILD_DATE) { $env:PDFCRAFT_BUILD_DATE = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd') }

Write-Output "SwiftPDF $Version for Windows $Arch ($Target)"

# Japanese UI text is drawn with BIZ UDPGothic, embedded by crates/fonts/build.rs when
# CRAFT_FONTS_DIR points at a craft-fonts checkout. A release without it shows Japanese as
# boxes: the Windows system fallback only covers Arabic (and Chinese when the UI is Chinese).
# Noto Sans CJK SC is in that checkout and is not embedded (AGENTS.md §1.1).
$CraftFonts = if ($env:CRAFT_FONTS_DIR) { $env:CRAFT_FONTS_DIR } else { Join-Path $Root '..\craft-fonts' }
$CraftFonts = [IO.Path]::GetFullPath($CraftFonts)
foreach ($font in @(
    'fonts\manifest.txt',
    'fonts\biz-ud-pgothic\BIZUDPGothic-Regular.ttf',
    'fonts\biz-ud-pgothic\BIZUDPGothic-Bold.ttf',
    'fonts\biz-ud-pgothic\OFL.txt'
  )) {
  if (-not (Test-Path -LiteralPath (Join-Path $CraftFonts $font))) {
    throw "Japanese fonts were not found ($font). Set CRAFT_FONTS_DIR to a craft-fonts checkout. Looked in $CraftFonts"
  }
}
$env:CRAFT_FONTS_DIR = $CraftFonts
$env:CRAFT_FONTS_REQUIRED = '1'
Write-Output "craft-fonts: $CraftFonts"

if (-not $SkipBuild) {
  # Static CRT: no VC++ redistributable needed. Scoped to the target so host build scripts and
  # proc-macros are unaffected.
  $flagVar = 'CARGO_TARGET_' + ($Target.ToUpper() -replace '-', '_') + '_RUSTFLAGS'
  [Environment]::SetEnvironmentVariable($flagVar, '-C target-feature=+crt-static')
  # Fail the build (rather than warn) if the icon/VERSIONINFO can't be embedded.
  $env:PDFCRAFT_REQUIRE_WINRES = '1'
  Invoke-Native "cargo build ($Target)" { cargo build --release --locked -p pdfcraft --target $Target }
}

$Bin = Join-Path $TargetDir "$Target\release"

# Check the GUI binary's PE header before packaging. Machine (COFF header) must match -Arch, so an
# x64 build can never ship labelled arm64 (#61). Subsystem (optional header): 2 = Windows GUI.
# The app must be GUI (no console window opens with it, #57). pdfcraft-cli.exe is not packaged.
function Get-PeHeader([string] $Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  $pe = [BitConverter]::ToInt32($bytes, 0x3C)
  return @{ Machine = [BitConverter]::ToUInt16($bytes, $pe + 4); Subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C) }
}
$Machine = switch ($Arch) { 'x64' { 0x8664 } 'x86' { 0x14C } 'arm64' { 0xAA64 } }
# The leading comma keeps this one-row list from being unrolled into the file name and the subsystem.
foreach ($check in @(,@('SwiftPDF.exe', 2))) {
  $h = Get-PeHeader (Join-Path $Bin $check[0])
  if ($h.Machine -ne $Machine) { throw "$($check[0]) is for machine 0x$('{0:X}' -f $h.Machine), expected 0x$('{0:X}' -f $Machine) ($Arch)" }
  if ($h.Subsystem -ne $check[1]) { throw "$($check[0]) has PE subsystem $($h.Subsystem), expected $($check[1])" }
  Write-Output "ok $($check[0]): $Arch, PE subsystem $($h.Subsystem)"
}
$Stage = Join-Path $TargetDir "windows-package\$Arch"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $Bin 'SwiftPDF.exe') $Stage
foreach ($forbidden in @('pdfcraft.exe', 'pdfcraft-cli.exe')) {
  if (Test-Path (Join-Path $Stage $forbidden)) { throw "$forbidden must not be packaged" }
}
# The document icon embeds Lucide artwork: carry its ISC notice and the project licences
# beside the binaries in both distributions.
foreach ($f in 'LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE') {
  Copy-Item (Join-Path $Root $f) $Stage
}
Copy-Item (Join-Path $Root 'assets\app-icon\LICENSE-document.txt') $Stage
# OFL-1.1 faces embedded by crates/fonts/build.rs. Noto Sans CJK SC is not one of them.
$FontLicences = @(
  'biz-ud-pgothic',
  'biz-ud-mincho',
  'shippori-mincho',
  'noto-sans-arabic',
  'noto-sans-thai'
)
foreach ($family in $FontLicences) {
  $ofl = Join-Path $CraftFonts "fonts\$family\OFL.txt"
  if (-not (Test-Path -LiteralPath $ofl)) { throw "missing font licence: $ofl" }
  Copy-Item -LiteralPath $ofl (Join-Path $Stage "OFL-$family.txt")
}
$builtExe = Join-Path $Stage 'SwiftPDF.exe'
if (-not ([System.Management.Automation.PSTypeName]'SwiftPdfBytes').Type) {
  Add-Type -TypeDefinition @'
public static class SwiftPdfBytes {
  public static bool Has(byte[] hay, byte[] needle) {
    for (int i = 0; i <= hay.Length - needle.Length; i++) {
      if (hay[i] != needle[0]) continue;
      bool ok = true;
      for (int j = 1; j < needle.Length; j++) if (hay[i + j] != needle[j]) { ok = false; break; }
      if (ok) return true;
    }
    return false;
  }
}
'@
}
$exeBytes = [IO.File]::ReadAllBytes($builtExe)
$gothic = [Text.Encoding]::BigEndianUnicode.GetBytes('BIZ UDPGothic')
if (-not [SwiftPdfBytes]::Has($exeBytes, $gothic)) {
  throw 'SwiftPDF.exe does not embed BIZ UDPGothic, so Japanese UI text would be missing'
}
$cjk = [Text.Encoding]::ASCII.GetBytes('Noto Sans CJK')
if ([SwiftPdfBytes]::Has($exeBytes, $cjk)) { throw 'SwiftPDF.exe embeds Noto Sans CJK, which must not be shipped' }

& (Join-Path $PSScriptRoot 'sign.ps1') (Join-Path $Stage 'SwiftPDF.exe')

# OCR models (#103): the app looks in models\ beside SwiftPDF.exe. `cargo xtask models` fetches every
# ATTRIBUTION.toml `kind = "model"` file (verified by SHA-256) with its licence and ATTRIBUTION.txt;
# the MSI and the portable zip take the whole folder, so new models ship without naming them here.
$Models = Join-Path $Stage 'models'
New-Item -ItemType Directory -Force -Path $Models | Out-Null
Push-Location $Root
try { Invoke-Native 'cargo xtask models' { cargo xtask models $Models } } finally { Pop-Location }
Remove-Item -Force (Join-Path $Models '*.part') -ErrorAction SilentlyContinue
if (-not (Test-Path (Join-Path $Models 'ATTRIBUTION.txt')) -or -not (Get-ChildItem (Join-Path $Models '*.LICENCE.txt'))) {
  throw "no OCR models in $Models after cargo xtask models"
}
# pdfcraft.wxs names each model file so its component GUID cannot collide with PdfCraft.
# A new or renamed file must be added there with a new GUID before it can ship.
$ExpectedModels = @(
  'ATTRIBUTION.txt',
  'text-detection.rten',
  'text-detection.rten.LICENCE.txt',
  'text-recognition.rten',
  'text-recognition.rten.LICENCE.txt'
)
$ActualModels = @(Get-ChildItem -File $Models | ForEach-Object { $_.Name } | Sort-Object)
$ExpectedSorted = @($ExpectedModels | Sort-Object)
if (($ActualModels -join '|') -ne ($ExpectedSorted -join '|')) {
  throw "models folder is $ActualModels; pdfcraft.wxs lists $ExpectedSorted"
}

# ---- MSI ---------------------------------------------------------------------------------------
$Msi = Join-Path $Dist "SwiftPDF-$Version-windows-$Arch.msi"
Invoke-Native 'wix build' {
  wix build (Join-Path $PSScriptRoot 'pdfcraft.wxs') -arch $Arch `
    (Join-Path $PSScriptRoot 'installer-ui.wxs') `
    -d "Version=$MsiVersion" -d "BinDir=$Stage" -d "ModelsDir=$Models" -d "IconPath=$(Join-Path $Root 'assets\app-icon\swiftpdf.ico')" `
    -o $Msi
}
# Inspect the built MSI, not just the XML, before signing/publishing it. In a child process, so
# its Windows Installer database handle is gone before signtool opens the MSI.
Invoke-Native 'test-msi' { & (Get-Process -Id $PID).Path -NoProfile -File (Join-Path $PSScriptRoot 'test-msi.ps1') $Msi }
# wix writes its debug symbols (.wixpdb) next to the MSI; keep them out of the release assets.
Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))
& (Join-Path $PSScriptRoot 'sign.ps1') $Msi

# ---- portable zip ------------------------------------------------------------------------------
$Portable = Join-Path $TargetDir "windows-package\SwiftPDF-$Version-windows-$Arch-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Portable | Out-Null
Copy-Item (Join-Path $Stage 'SwiftPDF.exe') $Portable
foreach ($f in 'NOTICE', 'LICENSE-document.txt') {
  Copy-Item (Join-Path $Stage $f) $Portable
}
Copy-Item -Recurse $Models (Join-Path $Portable 'models')
Copy-Item (Join-Path $PSScriptRoot 'portable-readme.md') (Join-Path $Portable 'README.md')
foreach ($f in 'LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Portable }
}
foreach ($family in $FontLicences) {
  Copy-Item (Join-Path $Stage "OFL-$family.txt") $Portable
}
# portable.txt beside SwiftPDF.exe switches on portable mode: settings, logs, recovery files and new
# digital IDs go to Data\ next to the exe instead of %LOCALAPPDATA% (see crates/ui-egui/src/portable.rs).
# The marker is not a registry change. Data is created on first launch and moves with the folder.
Copy-Item (Join-Path $PSScriptRoot 'portable.txt') $Portable
foreach ($forbidden in @('pdfcraft.exe', 'pdfcraft-cli.exe')) {
  if (Test-Path (Join-Path $Portable $forbidden)) { throw "$forbidden must not be in the portable folder" }
}
if (-not (Test-Path (Join-Path $Portable 'SwiftPDF.exe')) -or -not (Test-Path (Join-Path $Portable 'portable.txt'))) {
  throw 'the portable folder must contain SwiftPDF.exe and portable.txt side by side'
}
$Zip = Join-Path $Dist "SwiftPDF-$Version-windows-$Arch-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zipFile = [IO.Compression.ZipFile]::OpenRead($Zip)
try {
  $names = @($zipFile.Entries | ForEach-Object { [IO.Path]::GetFileName($_.FullName) })
  foreach ($forbidden in @('pdfcraft.exe', 'pdfcraft-cli.exe')) {
    if ($names -contains $forbidden) { throw "$forbidden must not be in the portable zip" }
  }
  if ($names -notcontains 'SwiftPDF.exe' -or $names -notcontains 'portable.txt') {
    throw 'the portable zip must contain SwiftPDF.exe and portable.txt'
  }
} finally { $zipFile.Dispose() }
Get-Item $Msi, $Zip | Format-Table Name, Length
