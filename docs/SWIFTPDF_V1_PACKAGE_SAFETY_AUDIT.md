# SwiftPDF 1.0.0 配布パッケージの安全性監査（STEP 126）

Status: static audit and an isolated portable run are done. The MSI was built and not installed.
Not a release. Not committed.

Revision history:
- 2026-10-26 — STEP 126, on `e503124b`.

Measured results and behaviour read from source or from the MSI database are labelled separately.

## 1. 開始時の Git 状態

| Item | Value |
|---|---|
| Branch | `feature/seal-data-safety-v1.0.0` |
| HEAD | `e503124bd9adf502d7f027f49a0b3b3eed4f3f69` |
| `git status --short` | `?? docs/images/SwiftPDFiconORG.png` only |
| `git diff --check` | clean |

That image was not read, moved, or added. No other change was present. Product code was not modified in this step.

## 2. 実際の配布方式

The Windows package is `packaging/windows/package.ps1` (WiX v5, documented as `dotnet tool install --global wix --version 5.0.2`).

It produces, from `[workspace.package] version` (`1.0.0`):

- `SwiftPDF-1.0.0-windows-<arch>.msi`
- `SwiftPDF-1.0.0-windows-<arch>-portable.zip`

Inputs that are in the tree: `pdfcraft.wxs`, `installer-ui.wxs`, `portable.txt`, `portable-readme.md`, `install-notes.md`, `test-msi.ps1`, `sign.ps1`. Licences come from the repo root and `assets/`. OCR models are downloaded by `cargo xtask models` into the stage folder, checked by SHA-256. `pdfcraft-cli.exe` is refused. Signing is skipped, with a warning, when `WINDOWS_CERTIFICATE` and `AZURE_*` are unset. They were unset.

`wix.exe` is installed but did not start until `DOTNET_ROOT` pointed at the existing user-local .NET 8 runtime. No runtime was installed for this step. After that, `wix --version` was `5.0.2`.

The GitHub workflow `.github/workflows/release.yml` does not call `package.ps1`. It calls `storytold/craft-common-actions` with `app-name: pdfcraft` and `app-title: PdfCraft`. `.github/workflows/windows-arm64.yml` passes `app-dir: PdfCraft`. Those workflows were not run. What the shared workflow prints is not known from this machine.

There is no root `LICENSE` file. The script copies `LICENSE` only when it exists, and it copies `LICENSE-MIT` and `LICENSE-APACHE` always.

## 3. ZIP の構成と生成結果

The app was not rebuilt. `package.ps1 -Arch x64 -SkipBuild` packaged the already measured EXE (`551D6DE1…`, clean clone of `7db6ef69`). STEP 125 changed tests and reports only, so that EXE is the product at `e503124b`.

The local `target\x86_64-pc-windows-msvc\release\SwiftPDF.exe` was a different file (`EA1488BE…`, 94,425,088 bytes). It was moved aside, the known EXE was put in its place for the script, and the aside file was copied back. After the script: local triple EXE `EA1488BE…`, clean-clone EXE and `step124\portable\SwiftPDF.exe` still `551D6DE1…`.

`test-msi.ps1` passed on the built MSI. That script opens the database and a package session with machine state ignored. It does not install.

Output: `D:\dev\SwiftPDF-package-validate\step126\`.

The zip has one root folder, `SwiftPDF-1.0.0-windows-x64-portable\`, and these 22 entries. Nothing else.

| File | Bytes |
|---|---|
| `SwiftPDF.exe` | 97,062,912 |
| `portable.txt` | 511 |
| `README.md` | 5,885 |
| `NOTICE` | 4,441 |
| `ATTRIBUTION.md` | 85,921 |
| `LICENSE-MIT` | 1,100 |
| `LICENSE-APACHE` | 10,174 |
| `LICENSE-brand.txt` | 2,256 |
| `LICENSE-document.txt` | 1,580 |
| `LICENSE-lucide.txt` | 3,208 |
| `OFL-Inter.txt`, `OFL-JetBrainsMono.txt`, `OFL-DancingScript.txt` | shipped |
| `OFL-biz-ud-pgothic.txt`, `OFL-biz-ud-mincho.txt`, `OFL-shippori-mincho.txt` | shipped |
| `OFL-noto-sans-arabic.txt`, `OFL-noto-sans-thai.txt` | shipped |
| `models\text-detection.rten` and `models\text-recognition.rten` | plus each `.LICENCE.txt` and `models\ATTRIBUTION.txt` |

Not present: source files, logs, `seals.v2.bin`, `seals.bin`, PDFs, git metadata, credentials, `.part` downloads, `pdfcraft.exe`, `pdfcraft-cli.exe`. The EXE inside the zip hashes to `551D6DE1…`. `package.ps1` also refused the build when the EXE contained the ArtCraft logo byte strings or `Noto Sans CJK`; the script completed, so those strings were not found.

## 4. ZIP のライセンス文書

`README.md` is `portable-readme.md`. It keeps the PdfCraft / ArtCraft Team copyright line and points at the licence files in the folder. `LICENSE-brand.txt` is the trademark terms. No ArtCraft logo image is in the zip.

Two wording problems, measured from the files that shipped:

- `models\ATTRIBUTION.txt` begins "PdfCraft's text recognition (Scan & OCR) uses these models, unmodified." The author, source and CC-BY-SA-4.0 lines under that are the required credit. The first sentence is a product name, not the model licence.
- `README.md` links to `install-notes.md`. That file is not in the zip. `package.ps1` does not copy it.

## 5. ポータブル保存先

`portable.txt` beside the EXE is the marker. Its contents are ignored by `portable.rs`. The folder `Data` is created on first launch when it can be written.

Measured on the zip, with `LOCALAPPDATA`, `APPDATA` and `TEMP` redirected under `step126\env-*`. The real `%LOCALAPPDATA%\SwiftPDF` timestamp stayed `2026-10-23T22:18:55.4903354Z`. Its contents were not listed.

| Check | Result |
|---|---|
| First launch creates `Data` | Measured. `Data\logs\` and `Data\app.ron`. Log: `portable mode: settings are kept in …\Data` |
| Settings survive a second launch | Measured. `app.ron` was rewritten (window geometry). It was still the settings file. No `seals.v2.bin` |
| EXE-only replace keeps `Data` | Measured. Replacing `SwiftPDF.exe` with the same bytes left `app.ron` hash and the file names unchanged |
| Second extract | Measured. A new folder had no `Data` until its own launch, then its own `logs` and `app.ron`. The first folder's `Data` was unchanged |
| `Data` not writable | Measured. `Data` was a file, not a folder. The log warned that portable mode is off and settings go to the user folder. That folder was the redirected one: `env-c\Local\SwiftPDF\app.ron`. The marker file was left in place |

A synthetic chairman seal was not registered again. The same EXE bytes already had that done by a person in STEP 124 B4 and B6, in `step124\portable`, including save and reopen. This fresh zip did not contain that `Data`.

View-only launch logged that crash recovery is off and `Data\Recovery` is left as it is. No `Recovery` directory was created.

The single-instance socket is not under `Data`. Measured names under the redirected local folder: `portable-instance-*.lock` and `portable-instance-*.sock`. That matches `single_instance.rs`. It does not match the text of `portable.txt`, which says the portable copy writes nothing to `%LOCALAPPDATA%`. `README.md` in the zip says the socket stays there and is not a seal.

## 6. DPAPI の制約

From `seal_store_v2.rs` (`decode`): `seals.v2.bin` is `SWSEAL2\n` plus current-user DPAPI ciphertext. If unprotect fails, the error is `Unreadable` and `open_locked` returns it without writing. `VaultError`'s comment for `Unreadable` is "It was left as it was." A missing v2 file beside a tombstone is `V2Missing`, which does not create a replacement. This was not re-run against another Windows user or another PC, and no real store was opened.

`README.md` in the zip says the same limits: same user may work after reinstall, another user or another PC is not guaranteed, a copy of `Data` is not a migration, and a broken store is not replaced with an empty one. There is no export/import.

`portable.txt` is less careful. It says "Copy the whole folder, including Data, to move SwiftPDF with its settings." It does not mention DPAPI. `app.ron` does move with the folder. A seal store does not decrypt on another machine just because the file was copied. The README states that. The marker file does not.

## 7. MSI のインストール設定

Read from the built MSI (`OpenDatabase` mode 0). Not installed.

| Property | Value |
|---|---|
| ProductName | SwiftPDF |
| Manufacturer | SwiftPDF |
| ProductVersion | 1.0.0 |
| ProductCode | `{467A1396-B078-4787-A371-32B0A92E2455}` |
| UpgradeCode | `{2BE82895-B377-4701-A51F-E03759ECEF56}` |
| ALLUSERS | 1 |
| Platform | `x64;1033` |

`ProductCode` is not in `pdfcraft.wxs`. WiX generated it for this build. It is not the UpgradeCode. The next build is expected to get another ProductCode. That was not given a second build.

Install directory, from the Directory table: `ProgramFiles64Folder` / `SwiftPDF`, plus `models`. Scope is per-machine. The launch condition requires administrator and `ALLUSERS=1`. `test-msi.ps1` evaluated that condition, including a per-user install rejected and a legacy per-user uninstall still allowed.

Shortcuts: Start menu `SwiftPDF`, and a desktop shortcut only when `INSTALLDESKTOPSHORTCUT=1`. Both are plain links, not advertised shortcuts. "Open with SwiftPDF" is optional (`INSTALLOPENWITH`). The `.pdf` default value is not written (`test-msi.ps1` asserts no such registry row). No App Paths or capability key uses the name PdfCraft (`test-msi.ps1` walks the Registry table).

24 components. The EXE, shortcuts, menu, each licence, and each model file have their own GUID in the authoring. `test-msi.ps1` rejects five component GUIDs it treats as PdfCraft's, and PdfCraft's UpgradeCode `EA00C367-7270-4C50-9D8B-A7A5C88AD27F`. This MSI passed that check.

The package is unsigned. `sign.ps1` warned and left it unsigned.

## 8. MSI の更新設定

`MajorUpgrade AllowSameVersionUpgrades="yes"` is in the source. The built Upgrade table has two rows, both for `{2BE82895-B377-4701-A51F-E03759ECEF56}`:

| VersionMin | VersionMax | Attributes | Reading |
|---|---|---|---|
| (empty) | 1.0.0 | 513 (`0x201`, max inclusive + migrate features) | remove this product at 1.0.0 and below |
| 1.0.0 | (empty) | 2 (detect only) | a newer version blocks a downgrade |

No other UpgradeCode is in the table. `RemoveExistingProducts` is between `InstallValidate` and `InstallInitialize` (`test-msi.ps1`). An older SwiftPDF with this UpgradeCode is therefore replaced rather than left beside the new one. That replacement was not executed.

`MsiRMFilesInUse` is in the UI. OK sets `MSIRMSHUTDOWN=2`, which asks Windows to close programs holding the installed files. Cancel stops setup. Not executed. Closing the app during an upgrade can interrupt a seal write; the store's lock is a file lock released when the process ends, and writes go through the existing atomic replace. That interaction was not measured.

User data is not a component and is not under `Program Files\SwiftPDF`. An upgrade, as authored, replaces installed files only.

## 9. MSI のアンインストール設計

The built MSI has no `RemoveFile` table (the query failed because the table is absent). The only `CustomAction` rows are `SetINSTALLDESKTOPSHORTCUT` and `SetINSTALLOPENWITH`, both type 307, target `1`. There is no executable custom action and no action whose target is a user profile.

No directory and no registry key in the authoring is `%LOCALAPPDATA%\SwiftPDF`. Uninstall therefore removes the components Windows Installer owns: files under `Program Files\SwiftPDF`, the shortcuts, and the SwiftPDF registry keys. It does not, in the database, delete seals, settings, audit history, or `Recovery`.

This is the database. It has not been confirmed by uninstalling.

## 10. PdfCraft／ArtCraft との共存

The Upgrade table names only SwiftPDF's UpgradeCode. `test-msi.ps1` fails the build if that table or a component GUID is the PdfCraft value above. Install folder and shortcut names are `SwiftPDF`, not `PdfCraft`.

On this PC the uninstall registry has ArtCraft Launcher 0.5.1 (publisher Learning Machines LLC). It has no PdfCraft entry and no SwiftPDF entry. Nothing was installed, repaired, or removed. Side-by-side with PdfCraft was not launched, because PdfCraft is not installed. Side-by-side with ArtCraft Launcher was not launched either. The MSI database does not name ArtCraft.

Licence text that names PdfCraft contributors and the ArtCraft trademark terms is present on purpose. It was not removed.

## 11. SWSEAL2 への影響

The zip and the MSI do not contain `seals.v2.bin`, a SWSEAL1 file, a tombstone, a backup, or an audit log. Building them did not open a store.

No change was made to the store format, the audit log, the lock, `hold_output`, or re-verification. The portable runs created settings and logs only. They did not create `seals.v2.bin`.

From the source, a failed decrypt does not delete or replace the file (§6). An MSI upgrade or uninstall, as authored, does not list the store as a component (§8, §9). Neither was executed.

## 12. 検証用パッケージの SHA-256

| File | Bytes | SHA-256 |
|---|---|---|
| `SwiftPDF-1.0.0-windows-x64-portable.zip` | 55,311,547 | `FB4A88121F76B787633BC10851F8DC39BA5F62FB110B94A3F7D4AAE55F5063D3` |
| `SwiftPDF-1.0.0-windows-x64.msi` | 46,960,640 | `9663D6AB4E1007AF1D103D7E8924EBDA565DB1C6935CF2C8A498917DD8F7B31A` |
| `SwiftPDF.exe` inside the zip | 97,062,912 | `551D6DE1DFF75FFB525CDCC21C61D45200B8955399727D01B2404C4E1452E318` |

`test-msi.ps1` passed for this MSI. The packages are unsigned.

## 13. 実施した操作と未実施の操作

Done:

- Read the packaging sources and the built MSI tables.
- Ran `package.ps1 -SkipBuild` into `D:\dev\SwiftPDF-package-validate\step126`.
- Ran `test-msi.ps1` as part of that script.
- Launched the zip's EXE three times in redirected environments (first folder twice, second folder once, unwritable `Data` once), then closed those windows.

Not done:

- MSI install, repair, upgrade, or uninstall.
- Any change to Windows Installer registration.
- Opening a real seal store, or copying one to another user or PC.
- Registering a new synthetic seal in this zip (already done by a person on this EXE in STEP 124).
- Confirming the fallback sentence in the GUI. The log line was measured. The toast was not.
- Running `.github/workflows/release.yml`.

## 14. 重大リスクと修正案

No data-deletion action was found, and the MSI does not list PdfCraft as a product to remove. Those two stop conditions are not met. Publishing still should not use the packages until the notes below are accepted.

1. **`portable.txt` overstates what moves.** Measured: a socket is written under `%LOCALAPPDATA%\SwiftPDF`, and the README already says so. The marker also says copying the folder moves settings, without the DPAPI limit the README states. Before the zip is published, make `portable.txt` match the README: the socket stays in the user profile, and `seals.v2.bin` does not decrypt on another user or PC. Do not weaken the README.
2. **Do not ship through the current GitHub release workflow as it stands.** `release.yml` still passes `app-title: PdfCraft`, and `windows-arm64.yml` passes `app-dir: PdfCraft`. The shared workflow was not opened. Change those inputs only after checking what the shared workflow does with them. This step did not change them.
3. **MSI uninstall is unmeasured.** The database has no user-data removal. The next step has to install, upgrade, and uninstall on a disposable profile and show that `%LOCALAPPDATA%\SwiftPDF` is still there, and that ArtCraft Launcher is still installed.
4. **Unsigned.** SmartScreen can warn. Not a data-loss bug.
5. **`models\ATTRIBUTION.txt` still says PdfCraft in the first sentence.** Keep the model author and licence lines. The product name in that sentence can be corrected later. Not done here.
6. Carried from STEP 125, not re-opened: Chinese-UI preview, SWSEAL2 intermittent I/O, two bug checks 0x1A, builds are not byte-identical, and the held / failed-save reload was not done by a person.

## 15. PC の安定性

`cargo xtask models` recompiled `xtask` and a few crates (`crypt`, `filters`, `lopdf`, `cos`, `oracle`) in 2 minutes 8 seconds, one job. WiX and the portable launches followed. No rustc or linker crash, and no bug check, during this step. The earlier 0x1A events are not treated as fixed. The memory was already reduced from four modules to two before STEP 125. SwiftPDF.exe itself was not rebuilt.

## 16. 次 STEP で必要な実機検証

The MSI database is safe enough to install on a test profile, after an explicit approval. Do not install it on the profile that holds real seals.

Suggested checks, none of them done here:

1. Install `9663D6AB…` for all users. Confirm `Program Files\SwiftPDF`, the Start menu name, and that `.pdf`'s current default is unchanged.
2. Confirm About shows `%LOCALAPPDATA%\SwiftPDF`, create a synthetic seal, uninstall, and confirm that folder and ArtCraft Launcher are still present.
3. Install again. The synthetic seal should still open for this Windows user.
4. With the app running, start the same MSI and cancel at the files-in-use dialog. Then repeat and allow it to close the app. Confirm the store is unchanged or, if a write was in progress, that it is `Unreadable` rather than replaced by an empty store.
5. A second build's MSI, with a new ProductCode and the same UpgradeCode, should replace the first entry rather than add a second SwiftPDF.

The zip can be tried as a published package only after `portable.txt` is corrected. The STEP 124 GUI result (B1–B8 passed on `551D6DE1…`) still matches this EXE.

## 17. `git diff --stat`

No tracked file changed. `git diff --stat` is empty. This report is a new untracked file.

## 18. `git status`

After this report was added, and before any commit:

```
?? docs/SWIFTPDF_V1_PACKAGE_SAFETY_AUDIT.md
?? docs/images/SwiftPDFiconORG.png
```

`SwiftPDFiconORG.png` was left untouched. Nothing was committed or pushed.
