# SwiftPDF 1.0 — full integration tests and release readiness (STEP 124)

Status: STEP 125 corrected the two tests only. Both test files passed. B1–B8 passed, run by a
person on the clean-clone EXE. The Chinese-interface preview issue is recorded and not fixed.
This file is included in the STEP 125 commit; §16 and §17 are the tree just before that commit.

Revision history:
- 2026-10-26 — first version (STEP 124, on `7db6ef69`).
- 2026-10-26 — STEP 125: B1–B8 recorded as passed by a person; the two test fixes, the Chinese-UI
  issue, the memory change and the release judgment are in §6, §7 and §18.

## 1. 開始時の Git 状態

- Branch `feature/seal-data-safety-v1.0.0`, HEAD `7db6ef69530a7e5b5e87d3b61b704ead04b95a08`
  (pushed in STEP 123), `git diff --check` clean.
- Only `docs/images/SwiftPDFiconORG.png` was untracked. It is owned by the user and was not
  touched.
- Before and after the machine stopped (§8), HEAD was unchanged and `git fsck` was clean.

## 2. 結合テスト一覧

The list was read from the source, not estimated:

- `crates/ui-egui/Cargo.toml` declares no `[[test]]` entries, so Cargo finds the test programs
  automatically.
- There are 47 files in `tests/*.rs`, plus `tests/ui_group/main.rs` (15 modules), plus the
  library's own unit tests.
- The source has 931 `#[test]` functions under `tests/`. 12 of them are compiled only on Linux or
  macOS (`control.rs` 10, `ui.rs` 1, `view.rs` 1), so 919 run on Windows.

| Category | Tests |
|---|---|
| Fixtures | `tests/data/form.pdf` (forms). Every other fixture is built in code. |
| Windows-only or Windows-specific parts | `seal_store_v2_processes.rs` (`#![cfg(windows)]`, starts real child processes on a temp-dir store); one `#[cfg(windows)]` part each in `document_links.rs` and `safe_save.rs` |
| Linux or macOS only (not run here) | `control.rs` 10, `ui.rs` 1, `view.rs` 1 |
| Heavy or long | `perf.rs` (frame budgets; skips itself on a busy machine). In the library: the 64 MiB and 63 MiB SWSEAL2 store tests, ignored by default. |
| Ignored on purpose (8) | Library 4: GUI fixture writer, 64 MiB store, store timings, hinting probe. `editing.rs` 4: each needs a private local PDF. |
| Skip themselves when a resource is missing | `ocr.rs` (OCR models), `fonts.rs` (craft-fonts faces or installed fonts), `perf.rs`. They pass when they skip, and their messages are hidden because output is captured. |

## 3. 実行したコマンド

- Run in the clean clone `D:\dev\SwiftPDF-repro-step123`, at the same commit `7db6ef69`.
- Build output went to a separate folder, `D:\dev\SwiftPDF-step124\target-test2`.
- `LOCALAPPDATA`, `APPDATA`, `TEMP` and `TMP` were redirected to `D:\dev\SwiftPDF-step124\env\…`,
  so no test could touch the real `%LOCALAPPDATA%\SwiftPDF`. Fonts are read from `WINDIR\Fonts`
  and are not affected by this.

```powershell
$env:CRAFT_FONTS_DIR="D:\dev\craft-fonts"; $env:CRAFT_FONTS_REQUIRED="1"
$env:CARGO_BUILD_JOBS="1"; $env:RUST_TEST_THREADS="1"; $env:CARGO_INCREMENTAL="0"
cargo test -p pdfcraft-ui-egui --tests -j 1 --no-fail-fast
```

- The log is `D:\dev\SwiftPDF-step124\logs\test-all-2.txt`. The run took 1 h 29 min and exited
  with 101 (tests failed).
- The first attempt (`test-all.txt`, target `target-test`) was cut off by the machine stopping
  (§8). Its partly built folder was not reused.

## 4. テストごとの結果

| Program | Passed | Failed | Ignored |
|---|---|---|---|
| library (`src/lib.rs`) | 627 | 0 | 4 |
| accessibility | 13 | 0 | 0 |
| actions | 2 | 0 | 0 |
| bookmarks | 8 | 0 | 0 |
| buttons | 2 | 0 | 0 |
| camera | 6 | 0 | 0 |
| **combine_grid** | 58 | **1** | 0 |
| commands | 25 | 0 | 0 |
| comments | 46 | 0 | 0 |
| compare | 5 | 0 | 0 |
| content | 15 | 0 | 0 |
| control | 50 | 0 | 0 |
| create | 12 | 0 | 0 |
| date_format | 3 | 0 | 0 |
| detailed_view | 9 | 0 | 0 |
| dialog_chrome | 3 | 0 | 0 |
| document_links | 30 | 0 | 0 |
| editing | 129 | 0 | 4 |
| export | 1 | 0 | 0 |
| fill_sign | 27 | 0 | 0 |
| folders | 9 | 0 | 0 |
| **fonts** | 10 | **1** | 0 |
| forms | 28 | 0 | 0 |
| javascript | 5 | 0 | 0 |
| object_selection | 12 | 0 | 0 |
| ocr | 4 | 0 | 0 |
| perf | 2 | 0 | 0 |
| pickers | 35 | 0 | 0 |
| prepare | 15 | 0 | 0 |
| print | 14 | 0 | 0 |
| reading_colors | 2 | 0 | 0 |
| recovery | 12 | 0 | 0 |
| reload | 11 | 0 | 0 |
| safe_save | 15 | 0 | 0 |
| seal_store_v2_processes | 3 | 0 | 0 |
| signing | 11 | 0 | 0 |
| simple_home | 12 | 0 | 0 |
| stamps | 4 | 0 | 0 |
| swift_print | 4 | 0 | 0 |
| theme_button | 1 | 0 | 0 |
| trust_signer | 1 | 0 | 0 |
| ui | 55 | 0 | 0 |
| ui_group | 78 | 0 | 0 |
| ui_mode | 8 | 0 | 0 |
| view | 46 | 0 | 0 |
| view_only_commands | 8 | 0 | 0 |
| view_only_output | 9 | 0 | 0 |
| view_only_startup | 4 | 0 | 0 |
| viewer | 51 | 0 | 0 |

The programs that STEP 122A never reached have now run for the first time and passed, including
`seal_store_v2_processes`, `safe_save`, `recovery`, `view_only_*`, `swift_print` and `simple_home`.
The programs that also passed in STEP 122A were run again here, and passed again.

## 5. 全体結果

49 programs: **1540 passed, 2 failed, 8 ignored.** No program was cut off, and no compile or
link step failed.

## 6. `combine_grid` の原因分析

**The failing test:** `arrow_keys_move_by_card_and_by_row_and_shift_extends`.

- It opens 9 files in a 760×1200 window.
- Before any key is pressed, it looks up every card by its label to count the cards per row. It
  fails at `card(&h, "f8.pdf")`: "No nodes found".

**How this was measured:** a temporary probe test in the clean clone. Its source is kept as
`D:\dev\SwiftPDF-step124\logs\zz_step124_probe.rs.txt`, and it was removed from the clone
afterwards.

**What happens at 760×1200:**

1. The SwiftPDF mode bar has an extra 「文字入力」 button and wraps onto 2 rows. The search field
   moves to y 99–131, so the bar is 48 px taller than upstream (`e5d7a040`, responsive toolbar).
2. The Tools panel takes the left ~300 px, which leaves 2 columns of 184 × 262 px cards. 9 files
   then need 5 rows.
3. The grid is virtualized on purpose (`show_viewport` + `thumbnail_rows`): only the rows near the
   visible area are laid out. f7 already ends at y 1402, below the 1200 px window. The 5th row
   (f8) is not laid out, so it has no accessibility node.
4. At 760×1400 all 9 cards are laid out. There, **every step of the failing test passes** (2
   columns, Right, Down, Up, Shift+Down, Up on the first row, Alt+Down).
5. **At 760×1200, the keyboard reaches the hidden row.** Click f4, press Down for f6, press Down
   again: the selection becomes f8 and the grid scrolls it into view (y 883–1137).

**The items to check, one by one:**

| Point | Finding |
|---|---|
| Expected key behaviour | Arrows move by card or by row, Shift extends from the anchor, Up on the first row stays, Alt+arrows move the files |
| Actual against expected | Identical in a window tall enough to lay out every row |
| Specification and history | No SwiftPDF commit touched `combine_grid.rs`, `combine_ui.rs` or this test. The mode bar change (`e5d7a040`) made the chrome taller. |
| Windows key handling | Not involved: the test fails before the first key press |
| DPI, focus, size, start state | It depends on window size alone. Focus and DPI play no part. |
| Alone and in the full run | Fails the same way every time, alone or in the full run (now, in STEP 122A, and on `4eb07ee3`), because the layout is deterministic |
| Effect on users | None found. Cards below the window are reached by scrolling or by keyboard, which selects them and scrolls them into view. |
| Blocks the release? | No |

**Classification: B (a fault in the test code).** The test assumes every card is laid out at once
without scrolling. SwiftPDF's taller mode bar broke that assumption. The product behaviour is
unchanged.

**STEP 125 fix (test only).** The window is 760×1400. The comment in the test says why 1200 is
too short. Every assertion is unchanged: several cards per row and several rows, Right, Down by
one row, Up, Shift+Down from the anchor, Up on the first row, and Alt+Down. Re-run on
2026-10-26: this test passed, and all 59 tests in `combine_grid.rs` passed (4.90 s). No product
file changed.

## 7. fonts 系テストの結果

**The failing test:** `chinese_ui_uses_an_installed_chinese_face`. It fails at `fonts.rs:257`:
`expect("the app's own font")`.

**The conditions under which it runs:**

- This build has no bundled Simplified Chinese (Hans) face: `ui_chinese_fonts()` is empty.
- Windows has an installed Chinese face (`WINDIR\Fonts`), so the test does not skip.
- In this setup it fails deterministically. It was not unstable here.

**The cause:** `e5212532` ("extended font selection") added one font family per bundled face, so
that added text can be previewed in exactly the chosen face. The 6 families are
`BIZ UDPGothic Regular`, `BIZ UDPGothic Bold`, `BIZ UDMincho Regular`,
`Shippori Mincho Regular`, `Noto Sans Arabic Regular` and `Noto Sans Thai Regular`.

- These families deliberately do not contain the app's own font, Inter.
- The test still assumes that every family contains it. **That is a test not updated for the new
  families (B).**

**A small product issue the test points at (low severity):**

- In a **Chinese** interface, `installed_font_definitions` puts the installed Chinese face
  *before* the first Japanese face of every family. That includes the per-face families, which
  then become `["system-fallback", "BIZ UDPGothic Regular", …]`.
- `added_face_family` says the editor face is "that face alone, so a missing glyph is not
  silently taken from another family". In a Chinese interface, however, text added in a Japanese
  face is **shown on screen** in the installed Chinese face.
- This is inferred from the code (`added_face_family` selects this family for the preview); it
  was not checked in the GUI.
- The Japanese interface (`--language ja`, SwiftPDF's normal use) is not affected: the installed
  face goes last there and only fills missing glyphs.
- The saved PDF uses the font file chosen for the text, not the screen font stack. That is read
  from the code and was **not** checked on a saved file, so it is not a confirmed result.

Other font tests: 10 of 11 passed in the STEP 124 run. Some of them skip themselves when a face
or installed font is missing. Their messages are hidden because output was captured, so this
report does not count which ones skipped.

**STEP 125 fix (test only).** The "app's own font" check now applies only to the interface
families (`Proportional`, `Monospace`, `medium`, `semibold`): the app's font still has to come
before the installed face, and the installed face still has to come before the Japanese faces.
The Han-character coverage check is unchanged. A per-face family must contain its one bundled
face exactly once, must not mix in Inter or JetBrains Mono, and a non-Japanese preview face must
still come before the installed face. The test does **not** require a Japanese preview face to
come first: that order is the open display issue, and asserting it would only pass after a
product change. Re-run on 2026-10-26: this test passed, and all 11 tests in `fonts.rs` passed
(0.23 s). No product file changed.

## 8. rustc・link.exe の異常

- During this STEP's builds, rustc and link.exe did **not** crash (no access violation, heap
  corruption or 1318 in either log).
- **The machine itself stopped twice** on 2026-10-26, according to the Windows event log:
  - 06:43: bug check **0x1A (MEMORY_MANAGEMENT)**, parameter 1 `0x41202`, during the first test
    build.
  - Between 06:59 and 07:01: bug check 0x1A again, parameter 1 `0x41792`, within minutes of the
    restart, with no build running.
  - It then shut down and restarted normally at 07:03.
- After the restart:
  - the repository, `git fsck`, the STEP 123 EXE, its step124 copy and the step117e EXE were all
    unchanged (hashes checked);
  - the test run was restarted in a new, empty target folder.
- The user later changed the installed memory from four modules to two. That may be more stable.
  It is **not** treated as the cause being found or fixed.
- STEP 125 recompiled and re-ran only `combine_grid` and `fonts` (`CARGO_BUILD_JOBS=1`,
  `RUST_TEST_THREADS=1`). rustc and link.exe did not crash, and the machine did not stop.
- The earlier rustc and link.exe crashes (STEP 122A) are consistent with this memory event. They
  are recorded as a machine fault, not as test failures.

## 9. SWSEAL2 の未解決 I/O 問題

- The cause is still unknown. No stress loop was run.
- **The diagnostics are usable:**
  - A store failure writes one `swseal2_io` line at `warn` level: stage, class, operation,
    generation, written and read-back sizes, `io::ErrorKind`, OS code and elapsed ms.
  - It never logs ciphertext, plaintext, seal images, approvals or user paths. The file name is
    always shown as `seals.v2.bin`. Unit tests check this.
  - The release logger writes `info` for SwiftPDF's own crates, so `warn` lines from SwiftPDF
    reach `<data folder>\logs\pdfcraft.log`.
- **The limit to know about:** each launch moves the log to `pdfcraft.1.log` and `pdfcraft.2.log`.
  After 3 more launches the log of a failure is gone. Collect it right after the failure.
- `seal_store_v2_processes` (3 tests, real child processes on one store) passed in this run.

## 10. クリーンビルド EXE

| Item | Value |
|---|---|
| Built in | STEP 123, from the clean clone (`7db6ef69`) |
| Path | `D:\dev\SwiftPDF-repro-step123\target-clean\release\SwiftPDF.exe` (not overwritten) |
| SHA-256 | `551D6DE1DFF75FFB525CDCC21C61D45200B8955399727D01B2404C4E1452E318`, re-checked in STEP 124 (also after the restart) |
| Copy | `D:\dev\SwiftPDF-seals-validate\step124\portable\SwiftPDF.exe`, with the same hash before and after copying |

## 11. 隔離ランチャーの安全性

`step124\launch-isolated.ps1` was reviewed and adapted from STEP 122A, in portable mode only.

**How SwiftPDF finds its folders on Windows (checked in the code):**

- `apps/pdfcraft/src/main.rs` (`user_dir`, `settings_dir`) and `single_instance.rs` read the
  `LOCALAPPDATA` **environment variable**.
- eframe gets an explicit `persistence_path`, so eframe's own known-folder lookup is not used on
  Windows.
- No `dirs` or `directories` crate is in the app's dependency tree.
- Redirecting the variables for the child process is therefore enough to isolate it.

**What the launcher checks before it starts anything:**

- It lives in `step124`.
- The EXE's SHA-256 is `551D6DE1…`; otherwise it refuses.
- `portable.txt` is present, and `Data` is not a file.
- `Data`, `LOCALAPPDATA`, `APPDATA` and `TEMP` all lie under `step124\portable`. None of them is
  the real `%LOCALAPPDATA%\SwiftPDF` (taken from the known-folder path) or anywhere inside the real
  LocalApplicationData.
- File arguments must lie inside `step124`.

**Other properties:**

- `-Check` prints the paths, creates nothing and launches nothing. The dry run passed: exit 0, and
  no folders were created.
- It does not depend on STEP 122A. The synthetic samples were copied, hash-checked, and the seals
  are registered fresh into `step124\portable\Data`.

## 12. GUI チェックリスト B1〜B8

`D:\dev\SwiftPDF-seals-validate\step124\TEST_CHECKLIST.md`, launched with `START_PORTABLE.cmd`:

| ID | Check | Result |
|---|---|---|
| B1 | The version is 1.0.0 | Passed, by the user |
| B2 | A synthetic PDF displays | Passed, by the user |
| B3 | A general stamp is placed, then cancelled | Passed, by the user |
| B4 | A synthetic chairman seal is committed | Passed, by the user |
| B5 | Undo and Redo | Passed, by the user |
| B6 | Save As and reopen: the seal and the added text remain | Passed, by the user |
| B7 | About shows `step124\portable\Data` | Passed, by the user |
| B8 | Closing a tab with a placed stamp asks first | Passed, by the user |

EXE SHA-256 `551D6DE1DFF75FFB525CDCC21C61D45200B8955399727D01B2404C4E1452E318`. These are a
person's results, not automated results.

## 13. GUI で未確認の項目

- **B1–B8: all passed**, run by the user on
  `D:\dev\SwiftPDF-seals-validate\step124\portable\SwiftPDF.exe`
  (SHA-256 `551D6DE1DFF75FFB525CDCC21C61D45200B8955399727D01B2404C4E1452E318`).
  Recorded in STEP 125. This is a person's result, separate from the automated tests.
- Not part of that GUI check, and still not done by a person:
  - a reload while held, during seal work, or after a failed save (automated tests only);
  - reload and the portable fallback were checked by a person in STEP 122A, on the other EXE
    (`F9B3E2DD…`), in A1–A10.
- The Chinese-interface preview effect in §7 was not looked at in the GUI, and its effect on a
  saved PDF was not checked separately.

## 14. 正式リリースを妨げる問題

- **No product defect found that blocks a Japanese 1.0 release.**
- Done since the first draft of this section:
  1. B1–B8 passed, run by a person on the `551D6DE1…` EXE (§12).
  2. The two test fixes are in STEP 125 (§6, §7). The full 49 programs were not run again; the
     STEP 124 run's only failures were these two, and this change is tests and docs only.
- Still open, and not blockers for the Japanese release:
  1. The Chinese-interface preview issue (§18). Fix it before calling the Chinese UI supported.
  2. SWSEAL2's intermittent I/O errors: cause unknown (§9).
  3. Two bug checks 0x1A on this machine (§8). A release EXE should be built on CI or on a machine
     that has stayed up through a full build. Changing to two memory modules is not proof.
  4. Builds are not reproducible byte for byte (the Git report).
- Not done by a person, and covered by automated tests only: reload while held, during seal work,
  and after a failed save.

## 15. 推奨する修正案

Applied in STEP 125 (tests only; approved by the STEP 125 instruction):

1. **combine_grid:** the window is 760×1400. The assertions are unchanged.
2. **fonts:** the Inter check is limited to the interface families. Per-face families are checked
   for one bundled face and no mixed-in interface font. They are **not** required to put a
   Japanese face before the installed face, because that is the open product issue.

Not applied:

3. **Chinese UI preview (product, separate issue):** in `installed_font_definitions`, do not
   insert the installed face in front of a per-face family (`added-*` and one family per bundled
   face). The preview then uses that face alone, which is what `added_face_family` already says.
   It needs its own regression test. See §18 for whether 1.0 must wait for it.
4. **Diagnostics (optional):** keep more log generations, or tell users to collect
   `logs\pdfcraft.log` right after a seal error.

## 16. `git diff --stat`

`git diff --stat` before this report is added (it is still untracked at that moment):

```
 crates/ui-egui/tests/combine_grid.rs           |  7 +++-
 crates/ui-egui/tests/fonts.rs                  | 54 ++++++++++++++++++++++++--
 docs/SWIFTPDF_V1_GIT_AND_REPRODUCIBLE_BUILD.md | 46 ++++++++++++++++++++++
 3 files changed, 101 insertions(+), 6 deletions(-)
```

This report is a new file in the same commit. `git show --stat` on the commit includes it. The
chat records that figure.

## 17. `git status`

```
 M crates/ui-egui/tests/combine_grid.rs
 M crates/ui-egui/tests/fonts.rs
 M docs/SWIFTPDF_V1_GIT_AND_REPRODUCIBLE_BUILD.md
?? docs/SWIFTPDF_V1_INTEGRATION_TEST_AND_RELEASE_READINESS.md
?? docs/images/SwiftPDFiconORG.png
```

`SwiftPDFiconORG.png` is not added. After the commit, only that image should remain untracked.
The chat reports `git status` after the push.

## 18. 中国語 UI の表示問題（未修正）

**When it happens:**

- The UI language is `zh-hans` or `zh-hant`.
- The build has no bundled Hans face (every current build; `ui_chinese_fonts()` is empty).
- Windows has a Chinese face installed, so it is used as `system-fallback`.
- Added text is previewed in a Japanese bundled face (`BIZ UDPGothic`, `BIZ UDMincho`,
  `Shippori Mincho`).

**Likely cause:** `e5212532` added one family per bundled face, containing only that face, so a
preview would not borrow glyphs from another family (`added_face_family`). The older Chinese-UI
rule in `installed_font_definitions` then walks **every** family and inserts the installed face
before the first Japanese face. On a Japanese per-face family that puts the Chinese face first,
so the preview draws with it. Measured in STEP 124: those stacks were
`["system-fallback", "BIZ UDPGothic Regular", …]`.

**Not confirmed:**

- Not looked at in the real GUI.
- A saved PDF was not checked. The code embeds the chosen font file rather than the screen stack,
  so the file is expected to keep the Japanese face, but that is not a measured result.

**Smallest change, not made:**

- In `installed_font_definitions`, skip the per-face families (`added-*` and one family per
  bundled face) when inserting the installed face. Add a regression test that a Japanese per-face
  family in a Chinese UI starts with that Japanese face.

**Does 1.0 have to wait?**

- No, for the Japanese release. SwiftPDF's normal UI is Japanese (`--language ja`). In that UI the
  installed face is last and only fills glyphs the bundled faces lack, so stamps, seals and
  Japanese text editing are outside this bug.
- Yes, before describing the Chinese UI as supported. `zh-hans` and `zh-hant` are selectable, so a
  person who switches to them and previews Japanese-face text can see the wrong face on screen.
  That is a display bug in a language SwiftPDF does not ship as its primary UI. It does not change
  SWSEAL2, audit data or `hold_output`.
