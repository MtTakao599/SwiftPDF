<#
.SYNOPSIS
  Check the compiled MSI's install scope, publisher, shortcuts, native UI and OCR models, without installing it.
.EXAMPLE
  pwsh packaging/windows/test-msi.ps1 dist/release/SwiftPDF-0.1.0-windows-x64.msi
#>
param([Parameter(Mandatory)] [string] $Path)
$ErrorActionPreference = 'Stop'
$Installer = New-Object -ComObject WindowsInstaller.Installer
$Database = $Installer.OpenDatabase((Resolve-Path -LiteralPath $Path).Path, 0)

function Read-Row([string] $Sql, [int] $Columns) {
  $view = $Database.OpenView($Sql)
  try {
    [void] $view.Execute()
    $record = $view.Fetch()
    if (-not $record) { throw "MSI row missing: $Sql" }
    $values = @(for ($i = 1; $i -le $Columns; $i++) { $record.StringData($i) })
    return ,$values
  } finally { [void] $view.Close() }
}

function Assert-Equal($Actual, $Expected, [string] $What) {
  if ($Actual -cne $Expected) { throw "$What`: expected '$Expected', got '$Actual'" }
}

function Assert-NoRow([string] $Sql, [string] $What) {
  $view = $Database.OpenView($Sql)
  try {
    [void] $view.Execute()
    if ($view.Fetch()) { throw "Unexpected MSI row ($What): $Sql" }
  } finally { [void] $view.Close() }
}

# Test the compiled condition with Windows Installer's evaluator, in a restricted session that
# cannot change machine state. Normal installs/repairs work, per-user overrides fail, and removal
# of an older incorrectly scoped installation remains possible (#305).
$scopeMessage = 'SwiftPDF must be installed for all users. Run setup with administrator privileges and ALLUSERS=1; per-user installation is not supported.'
$scopeCondition = Read-Row ('SELECT `Condition` FROM `LaunchCondition` WHERE `Description` = ''' + $scopeMessage + '''') 1
$Installer.UILevel = 2
$session = $null
try {
  $session = $Installer.OpenPackage((Resolve-Path -LiteralPath $Path).Path, 1)
  foreach ($case in @(
      @('machine install', '1', '', '', '', 1),
      @('machine repair', '1', '', '1', '', 1),
      @('forced per-user install', '2', '1', '', '', 0),
      @('empty ALLUSERS', '', '', '', '', 0),
      @('resolved per-user install', '', '1', '', '', 0),
      @('forced per-user repair', '2', '1', '1', '', 0),
      @('machine uninstall', '1', '', '1', 'ALL', 1),
      @('legacy per-user uninstall', '', '1', '1', 'ALL', 1))) {
    $session.Property('ALLUSERS') = $case[1]
    $session.Property('MSIINSTALLPERUSER') = $case[2]
    $session.Property('Installed') = $case[3]
    $session.Property('REMOVE') = $case[4]
    Assert-Equal ($session.EvaluateCondition($scopeCondition[0])) $case[5] $case[0]
  }
} finally {
  if ($session) { [void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($session) }
}
foreach ($sequence in @('InstallUISequence', 'InstallExecuteSequence')) {
  $launch = Read-Row ('SELECT `Condition`, `Sequence` FROM `' + $sequence + '` WHERE `Action` = ''LaunchConditions''') 2
  Assert-Equal $launch[0] '' "$sequence launch conditions are unconditional"
  $cost = Read-Row ('SELECT `Sequence` FROM `' + $sequence + '` WHERE `Action` = ''CostInitialize''') 1
  if ([int] $launch[1] -le 0 -or [int] $launch[1] -ge [int] $cost[0]) {
    throw "$sequence must reject per-user overrides before costing"
  }
}
$manufacturer = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''Manufacturer''' 1
Assert-Equal $manufacturer[0] 'SwiftPDF' 'MSI manufacturer'
foreach ($prop in @('ARPURLINFOABOUT', 'ARPURLUPDATEINFO', 'ARPHELPLINK')) {
  Assert-NoRow ('SELECT `Value` FROM `Property` WHERE `Property` = ''' + $prop + '''') "$prop is omitted"
}
$status = Read-Row 'SELECT `Text` FROM `Control` WHERE `Dialog_` = ''InstallProgress'' AND `Control` = ''Status''' 1
Assert-Equal $status[0] 'Please wait while setup completes.' 'Persistent progress message'
Assert-NoRow 'SELECT `Event` FROM `EventMapping` WHERE `Dialog_` = ''InstallProgress'' AND `Control_` = ''Status''' 'progress text subscription'

# Plain (non-advertised) shortcuts to SwiftPDF.exe, each in its own component; the desktop one is
# gated by INSTALLDESKTOPSHORTCUT, which is secure so the UI choice reaches the
# elevated install.
foreach ($entry in @(@('StartMenuShortcut', 'ProgramMenuFolder', 'SwiftPdfStartMenuShortcut', ''),
                     @('DesktopShortcut', 'DesktopFolder', 'SwiftPdfDesktopShortcut', 'INSTALLDESKTOPSHORTCUT = 1'))) {
  $row = Read-Row ('SELECT `Directory_`, `Target`, `Icon_`, `WkDir`, `Component_` FROM `Shortcut` WHERE `Shortcut` = ''' + $entry[0] + '''') 5
  Assert-Equal $row[0] $entry[1] "$($entry[0]) directory"
  Assert-Equal $row[1] '[#SwiftPdfExe]' "$($entry[0]) target"
  Assert-Equal $row[2] 'SwiftPdfIcon.ico' "$($entry[0]) icon"
  Assert-Equal $row[3] 'INSTALLFOLDER' "$($entry[0]) working directory"
  Assert-Equal $row[4] $entry[2] "$($entry[0]) component"
  $component = Read-Row ('SELECT `Directory_`, `Condition` FROM `Component` WHERE `Component` = ''' + $entry[2] + '''') 2
  Assert-Equal $component[0] $entry[1] "$($entry[2]) directory"
  Assert-Equal $component[1] $entry[3] "$($entry[2]) condition"
}
# Defaults an update keeps (#1058, #305). No static default: a SetProperty after AppSearch turns it on
# for a first install and, for an update from a release that offered it ($Release set), only while what
# it installed is still there ($Found, from AppSearch). A command-line value is left alone. Windows
# Installer's evaluator runs the SetProperty condition and then $Component's own condition.
function Assert-KeptOnUpdate([string] $Property, [string] $Release, [string] $Found, [string] $FoundValue, [string] $Component, [string] $What) {
  Assert-NoRow ('SELECT `Value` FROM `Property` WHERE `Property` = ''' + $Property + '''') "$What static default"
  $action = Read-Row ('SELECT `Action`, `Target` FROM `CustomAction` WHERE `Source` = ''' + $Property + '''') 2
  Assert-Equal $action[1] '1' "$What default value"
  $condition = $null
  foreach ($sequence in @('InstallUISequence', 'InstallExecuteSequence')) {
    $search = Read-Row ('SELECT `Sequence` FROM `' + $sequence + '` WHERE `Action` = ''AppSearch''') 1
    $set = Read-Row ('SELECT `Sequence`, `Condition` FROM `' + $sequence + '` WHERE `Action` = ''' + $action[0] + '''') 2
    if ([int] $set[0] -le [int] $search[0]) { throw "$sequence must set the $What default after AppSearch" }
    $condition = $set[1]
  }
  $installed = Read-Row ('SELECT `Condition` FROM `Component` WHERE `Component` = ''' + $Component + '''') 1
  Assert-Equal $installed[0] "$Property = 1" "$Component condition"
  $old = '{00000000-0000-0000-0000-000000000000}'
  # Name, command-line value, WIX_UPGRADE_DETECTED, $Release, $Found, installed (1) or not (0).
  $cases = @(
    @('first install', '', '', '', '', 1),
    @("first install, $Property=0", '0', '', '', '', 0),
    @('update, still there', '', $old, $old, $FoundValue, 1),
    @('update, removed', '', $old, $old, '', 0),
    @("update, $Property=0", '0', $old, $old, $FoundValue, 0),
    @("update, removed, $Property=1", '1', $old, $old, '', 1))
  # An update from a release from before it existed (WIX_UPGRADE_DETECTED alone) is a first install.
  if ($Release -ne 'WIX_UPGRADE_DETECTED') { $cases += , @('update from a release without it', '', $old, '', '', 1) }
  $session = $null
  try {
    $session = $Installer.OpenPackage((Resolve-Path -LiteralPath $Path).Path, 1)
    foreach ($case in $cases) {
      $session.Property($Property) = $case[1]
      $session.Property('WIX_UPGRADE_DETECTED') = $case[2]
      $session.Property($Release) = $case[3]
      $session.Property($Found) = $case[4]
      if ($session.EvaluateCondition($condition) -eq 1) { $session.Property($Property) = $action[1] }
      Assert-Equal ($session.EvaluateCondition($installed[0])) $case[5] "$What`: $($case[0])"
    }
  } finally {
    if ($session) { [void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($session) }
  }
}
Assert-KeptOnUpdate 'INSTALLDESKTOPSHORTCUT' 'WIX_UPGRADE_DETECTED' 'EXISTINGDESKTOPSHORTCUT' 'C:\Users\Public\Desktop\SwiftPDF.lnk' 'SwiftPdfDesktopShortcut' 'Desktop shortcut'
$secure = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''SecureCustomProperties''' 1
if (($secure[0] -split ';') -notcontains 'INSTALLDESKTOPSHORTCUT') { throw "INSTALLDESKTOPSHORTCUT is not secure: '$($secure[0])'" }
$checkbox = Read-Row 'SELECT `Type`, `Property`, `Text` FROM `Control` WHERE `Dialog_` = ''InstallWelcome'' AND `Control` = ''DesktopShortcut''' 3
Assert-Equal $checkbox[0] 'CheckBox' 'Welcome desktop-shortcut control'
Assert-Equal $checkbox[1] 'INSTALLDESKTOPSHORTCUT' 'Welcome checkbox property'
if ($checkbox[2] -notmatch 'desktop shortcut') { throw "Welcome checkbox label: '$($checkbox[2])'" }
$checked = Read-Row 'SELECT `Value` FROM `CheckBox` WHERE `Property` = ''INSTALLDESKTOPSHORTCUT''' 1
Assert-Equal $checked[0] '1' 'Welcome checkbox value'
$app = Read-Row 'SELECT `KeyPath` FROM `Component` WHERE `Component` = ''SwiftPdfApp''' 1
Assert-Equal $app[0] 'SwiftPdfExe' 'Shortcut executable key path'
$scope = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''ALLUSERS''' 1
Assert-Equal $scope[0] '1' 'Per-machine shortcut scope'

# SwiftPDF's own upgrade removes the previous SwiftPDF before the new one is installed.
# PdfCraft's UpgradeCode is never a related product.
$upgradeCode = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''UpgradeCode''' 1
$pdfcraftUpgrade = 'EA00C367-7270-4C50-9D8B-A7A5C88AD27F'
function Normalize-Guid([string] $Value) { return ($Value -replace '[{}]', '').ToUpperInvariant() }
if ((Normalize-Guid $upgradeCode[0]) -eq $pdfcraftUpgrade) { throw 'UpgradeCode is still PdfCraft''s' }
$swiftUpgrade = '2BE82895-B377-4701-A51F-E03759ECEF56'
Assert-Equal (Normalize-Guid $upgradeCode[0]) $swiftUpgrade 'SwiftPDF UpgradeCode'
$productName = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''ProductName''' 1
Assert-Equal $productName[0] 'SwiftPDF' 'ProductName'
$productCode = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''ProductCode''' 1
if ((Normalize-Guid $productCode[0]) -eq (Normalize-Guid $upgradeCode[0])) { throw 'ProductCode must stay distinct from UpgradeCode' }
if ((Normalize-Guid $productCode[0]) -eq $pdfcraftUpgrade) { throw 'ProductCode reused PdfCraft''s UpgradeCode' }
$template = $Database.SummaryInformation(0).Property(7)
if ($template -notmatch '^x64;') { throw "MSI platform must be x64, got '$template'" }
$validate = Read-Row 'SELECT `Sequence` FROM `InstallExecuteSequence` WHERE `Action` = ''InstallValidate''' 1
$remove = Read-Row 'SELECT `Sequence` FROM `InstallExecuteSequence` WHERE `Action` = ''RemoveExistingProducts''' 1
$initialize = Read-Row 'SELECT `Sequence` FROM `InstallExecuteSequence` WHERE `Action` = ''InstallInitialize''' 1
if ([int] $remove[0] -le [int] $validate[0] -or [int] $remove[0] -ge [int] $initialize[0]) {
  throw "RemoveExistingProducts ($($remove[0])) must run between InstallValidate and InstallInitialize"
}
$upgradeView = $Database.OpenView('SELECT `UpgradeCode` FROM `Upgrade`')
try {
  [void] $upgradeView.Execute()
  $upgradeRows = 0
  while ($record = $upgradeView.Fetch()) {
    $upgradeRows++
    $code = Normalize-Guid $record.StringData(1)
    if ($code -eq $pdfcraftUpgrade) { throw 'Upgrade table still names PdfCraft' }
    if ($code -ne $swiftUpgrade) { throw "Upgrade table uses $code instead of SwiftPDF's UpgradeCode" }
  }
} finally { [void] $upgradeView.Close() }
if ($upgradeRows -lt 1) { throw 'Upgrade table has no rows' }

# "Open with SwiftPDF" is optional, on for a first install, and kept on a SwiftPDF update only
# while SwiftPDF's own menu key remains. It does not set the .pdf default.
$pdfKey = 'Software\Classes\SystemFileAssociations\.pdf\shell\SwiftPDF.Open'
$pdfMenu = Read-Row ('SELECT `Registry`, `Value`, `Component_`, `Root` FROM `Registry` WHERE `Key` = ''' + $pdfKey + ''' AND `Name` IS NULL') 4
Assert-Equal $pdfMenu[1] 'Open with SwiftPDF' 'PDF context menu label'
Assert-Equal $pdfMenu[2] 'SwiftPdfOpenMenu' 'PDF context menu component'
Assert-Equal $pdfMenu[3] '2' 'PDF context menu HKLM root'
foreach ($value in @(@('Icon', '"[#SwiftPdfExe]",0'), @('MultiSelectModel', 'Document'))) {
  $row = Read-Row ('SELECT `Value`, `Component_` FROM `Registry` WHERE `Key` = ''' + $pdfKey + ''' AND `Name` = ''' + $value[0] + '''') 2
  Assert-Equal $row[0] $value[1] "PDF context menu $($value[0])"
  Assert-Equal $row[1] 'SwiftPdfOpenMenu' "PDF context menu $($value[0]) component"
}
$pdfCommand = Read-Row ('SELECT `Value`, `Component_` FROM `Registry` WHERE `Key` = ''' + $pdfKey + '\command''') 2
Assert-Equal $pdfCommand[0] '"[#SwiftPdfExe]" "%1"' 'PDF context menu command'
Assert-Equal $pdfCommand[1] 'SwiftPdfOpenMenu' 'PDF context menu command component'
$pdfComponent = Read-Row 'SELECT `Condition`, `KeyPath` FROM `Component` WHERE `Component` = ''SwiftPdfOpenMenu''' 2
Assert-Equal $pdfComponent[0] 'INSTALLOPENWITH = 1' 'PDF context menu condition'
Assert-Equal $pdfComponent[1] $pdfMenu[0] 'PDF context menu key path'
Assert-KeptOnUpdate 'INSTALLOPENWITH' 'WIX_UPGRADE_DETECTED' 'EXISTINGOPENWITH' 'Open with SwiftPDF' 'SwiftPdfOpenMenu' 'Open with SwiftPDF'
if (($secure[0] -split ';') -notcontains 'INSTALLOPENWITH') { throw "INSTALLOPENWITH is not secure: '$($secure[0])'" }
$openSearch = Read-Row 'SELECT `Root`, `Key`, `Name`, `Type` FROM `RegLocator`, `AppSearch` WHERE `AppSearch`.`Signature_` = `RegLocator`.`Signature_` AND `AppSearch`.`Property` = ''EXISTINGOPENWITH''' 4
Assert-Equal $openSearch[0] '2' 'Open with search HKLM root'
Assert-Equal $openSearch[1] $pdfKey 'Open with search key'
Assert-Equal $openSearch[2] '' 'Open with search reads the default value'
Assert-Equal ([int] $openSearch[3]) 18 'Open with search type'
$pdfCheckbox = Read-Row 'SELECT `Type`, `Property`, `Text` FROM `Control` WHERE `Dialog_` = ''InstallWelcome'' AND `Control` = ''OpenWith''' 3
Assert-Equal $pdfCheckbox[0] 'CheckBox' 'Welcome open-with control'
Assert-Equal $pdfCheckbox[1] 'INSTALLOPENWITH' 'Welcome open-with property'
if ($pdfCheckbox[2] -notmatch 'Open with SwiftPDF') { throw "Welcome open-with label: '$($pdfCheckbox[2])'" }
$pdfChecked = Read-Row 'SELECT `Value` FROM `CheckBox` WHERE `Property` = ''INSTALLOPENWITH''' 1
Assert-Equal $pdfChecked[0] '1' 'Welcome open-with value'
Assert-NoRow 'SELECT `Registry` FROM `Registry` WHERE `Key` = ''Software\Classes\.pdf'' AND `Name` IS NULL' '.pdf default app'
Assert-NoRow 'SELECT `Property` FROM `Property` WHERE `Property` = ''INSTALLPDFCONTEXTMENU'' OR `Property` = ''INSTALLIMAGECONTEXTMENU''' 'edit or create-PDF menu properties'

$installDir = Read-Row 'SELECT `DefaultDir` FROM `Directory` WHERE `Directory` = ''INSTALLFOLDER''' 1
if ($installDir[0] -notmatch '(^|\|)SwiftPDF$') { throw "install directory: '$($installDir[0])'" }
if ($installDir[0] -match 'PdfCraft') { throw "install directory still names PdfCraft: '$($installDir[0])'" }
foreach ($shortcut in @('StartMenuShortcut', 'DesktopShortcut')) {
  $name = Read-Row ('SELECT `Name` FROM `Shortcut` WHERE `Shortcut` = ''' + $shortcut + '''') 1
  if ($name[0] -notmatch '(^|\|)SwiftPDF$') { throw "$shortcut name: '$($name[0])'" }
  if ($name[0] -match 'PdfCraft') { throw "$shortcut still names PdfCraft: '$($name[0])'" }
}

# PdfCraft's published component ids. A SwiftPDF MSI that reuses one can remove PdfCraft's files.
$pdfcraftComponents = @(
  'E724C1C6-091F-4E21-A685-80D089FDBE21',
  'DC028192-C233-4F99-9A77-ED458C60DF26',
  'F6948627-32AB-4D78-8865-0FDF417CE442',
  '439F084F-657D-4AC2-A4CC-F099C518D71F',
  '1B783BF3-12DF-4588-AF80-E2CEA8BC1E5A'
)
$componentView = $Database.OpenView('SELECT `ComponentId` FROM `Component`')
try {
  [void] $componentView.Execute()
  while ($record = $componentView.Fetch()) {
    $id = Normalize-Guid $record.StringData(1)
    if ($pdfcraftComponents -contains $id) { throw "component GUID still belongs to PdfCraft: $id" }
    if ($id -eq $pdfcraftUpgrade) { throw 'a component GUID is PdfCraft''s UpgradeCode' }
  }
} finally { [void] $componentView.Close() }

$registryView = $Database.OpenView('SELECT `Key`, `Name`, `Value` FROM `Registry`')
try {
  [void] $registryView.Execute()
  while ($record = $registryView.Fetch()) {
    $key = $record.StringData(1)
    $name = $record.StringData(2)
    $value = $record.StringData(3)
    foreach ($part in @($key, $name, $value)) {
      if ($part -match 'PdfCraft' -or $part -match 'pdfcraft') { throw "registry row names PdfCraft: $key / $name / $value" }
      if ($part -match 'CreatePdf' -or $part -match '--create-images') { throw "registry row still creates a PDF: $key / $value" }
    }
  }
} finally { [void] $registryView.Close() }
Assert-NoRow 'SELECT `File` FROM `File` WHERE `FileName` = ''pdfcraft.exe'' OR `FileName` = ''pdfcraft-cli.exe''' 'PdfCraft executables'
$shippedExe = $false
$fontLicences = @{
  'OFL-biz-ud-pgothic.txt' = $false
  'OFL-biz-ud-mincho.txt' = $false
  'OFL-shippori-mincho.txt' = $false
  'OFL-noto-sans-arabic.txt' = $false
  'OFL-noto-sans-thai.txt' = $false
}
$fileView = $Database.OpenView('SELECT `FileName` FROM `File`')
try {
  [void] $fileView.Execute()
  while ($record = $fileView.Fetch()) {
    $fileName = $record.StringData(1)
    if ($fileName -match '(^|\|)SwiftPDF\.exe$') { $shippedExe = $true }
    if ($fileName -match '(^|\|)pdfcraft\.exe$' -or $fileName -match '(^|\|)pdfcraft-cli\.exe$') {
      throw "packaged file must not be $fileName"
    }
    foreach ($licence in @($fontLicences.Keys)) {
      $pattern = '(^|\|)' + [regex]::Escape($licence) + '$'
      if ($fileName -match $pattern) { $fontLicences[$licence] = $true }
    }
  }
} finally { [void] $fileView.Close() }
if (-not $shippedExe) { throw 'SwiftPDF.exe is not in the MSI' }
foreach ($licence in @($fontLicences.Keys)) {
  if (-not $fontLicences[$licence]) { throw "font licence missing from the MSI: $licence" }
}

# Negative sequences are Windows Installer's success/user-exit/failure paths. Only full UI
# shows these dialogs: an unattended /qn or /qb install must never wait for a Finish click.
foreach ($exit in @(@('InstallComplete', '-1'), @('InstallCancelled', '-2'), @('InstallFailed', '-3'))) {
  $row = Read-Row ('SELECT `Condition`, `Sequence` FROM `InstallUISequence` WHERE `Action` = ''' + $exit[0] + '''') 2
  Assert-Equal $row[0] 'UILevel = 5' "$($exit[0]) UI level"
  Assert-Equal $row[1] $exit[1] "$($exit[0]) exit path"
  $finish = Read-Row ('SELECT `Type`, `Text` FROM `Control` WHERE `Dialog_` = ''' + $exit[0] + ''' AND `Control` = ''Finish''') 2
  Assert-Equal $finish[0] 'PushButton' "$($exit[0]) Finish control"
  Assert-Equal $finish[1] '&Finish' "$($exit[0]) Finish label"
  $endDialog = Read-Row ('SELECT `Argument` FROM `ControlEvent` WHERE `Dialog_` = ''' + $exit[0] + ''' AND `Control_` = ''Finish'' AND `Event` = ''EndDialog''') 1
  Assert-Equal $endDialog[0] 'Return' "$($exit[0]) Finish event"
}
$title = Read-Row 'SELECT `Text` FROM `Control` WHERE `Dialog_` = ''InstallComplete'' AND `Control` = ''Title''' 1
if ($title[0] -notmatch 'completed successfully') { throw 'Success dialog does not confirm completion' }
# OCR models (#103): installed into models\ beside SwiftPDF.exe, where the app looks for them.
$modelsDir = Read-Row 'SELECT `Directory_Parent`, `DefaultDir` FROM `Directory` WHERE `Directory` = ''ModelsFolder''' 2
Assert-Equal $modelsDir[0] 'INSTALLFOLDER' 'OCR models folder parent'
# DefaultDir is `models`, or `SHORT|models` with a generated 8.3 name.
if ($modelsDir[1] -notmatch '(^|\|)models$') { throw "OCR models folder name: '$($modelsDir[1])'" }
$modelFiles = 0
$view = $Database.OpenView('SELECT `File`.`FileName` FROM `File`, `Component` WHERE `File`.`Component_` = `Component`.`Component` AND `Component`.`Directory_` = ''ModelsFolder''')
try {
  [void] $view.Execute()
  while ($record = $view.Fetch()) { if ($record.StringData(1) -match '\.rten$') { $modelFiles++ } }
} finally { [void] $view.Close() }
if ($modelFiles -lt 2) { throw "expected the OCR models (*.rten) in models\, found $modelFiles" }
$rm = Read-Row 'SELECT `Dialog` FROM `Dialog` WHERE `Dialog` = ''MsiRMFilesInUse''' 1
Assert-Equal $rm[0] 'MsiRMFilesInUse' 'Files-in-use dialog'
[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Database)
[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Installer)
Write-Output 'ok MSI: SwiftPDF product id, distinct UpgradeCode and component GUIDs, Program Files\SwiftPDF, SwiftPDF shortcuts and registry only, optional Open with SwiftPDF, no PdfCraft keys, no CLI, x64, per-machine scope, OCR models'
