# SwiftPDF 1.0 — manual reload protection and GUI acceptance setup (STEP 122A)

Status: the code fix and its automated tests are done; the validation EXE and checklist are
ready. The user ran the Windows GUI checklist A1–A10, and every item passed. Some ui-egui
integration test files have still not been run (§8).

Revision history:
- 2026-10-26 — first version (STEP 122A, on top of the uncommitted STEP 122 work over `5279fa96`).
- 2026-10-26 — STEP 123: recorded the user's GUI acceptance (§13, §14, §16, §17).

## 1. 開始時の Git 状態

- Branch `feature/seal-data-safety-v1.0.0`, HEAD `5279fa965d0d920a53a9d58975a83e5a2d6abd32`, and
  `git diff --check` was clean.
- The 10 uncommitted STEP 122 files and its report were present and are kept.
- `docs/images/SwiftPDFiconORG.png` (untracked) was not touched.

## 2. 手動再読込の旧動作

The notice bar's 「再読み込み」 queued `watch.reload_request`, and the next frame called
`reload_from_disk` directly, with no question. It dropped all of the following without asking:

- a placed stamp (general or chairman);
- unsaved PDF edits. The bar's text says so, but there was no confirmation.

Two further problems were found during the audit:

- **Hold bypass (pre-existing).** `Session::reload` opens the new bytes as a fresh document object
  and swaps it in. A fresh document has `output_hold: None`, so any reload of a held document
  (要復旧) would have **cleared the hold**. The automatic reload path could reach this: a held,
  clean document whose file changed on disk was reloaded without a question. The new test shows
  this: with the guard removed, the document is reloaded and its hold is gone.
- **No seal-work check.** The manual reload did not check whether seal work was running.

## 3. 再読込の全経路

| Path | In SwiftPDF? | Behaviour now |
|---|---|---|
| Notice bar 「再読み込み」 | Yes (the only manual reload) | `request_reload`: refused during seal work or a hold; otherwise the placement question, then the save question, then the reload |
| Automatic reload (the file changed on disk) | Yes (on by default) | A pending placement or unsaved work: only the notice bar, no reload (STEP 122). Seal work or a hold: no reload, notice bar instead (**new**) |
| Menu, shortcut or command palette | **No reload command exists** | — |
| File ▸ Revert (`file.revert`) | No: not in the view-only allowlist, and the engine refuses it in view-only mode | Not changed (PdfCraft only) |
| Signed-version bar | Has only 「閉じる」, no reload | Closing goes through the close questions (STEP 122) |
| Tab close, opening another PDF | — | The STEP 122 behaviour, unchanged |

Every reload, automatic or manual, now ends in `reload_view`, which applies the same check:
nothing is reloaded during seal work or while the document is held.

## 4. 仮置き確認の実装

- `CloseRequest` gained a variant, `Reload(DocId)`. The placement prompt (`PlaceClose`) and the
  save prompt are **reused**, not duplicated.
- For a reload, the placement prompt's text is 「まだ PDF には入っていません。再読込するとこの仮置きは破棄されます。未保存の編集があれば、続けて確認します。」
  and its buttons are 「編集に戻る」 and 「仮置きを破棄して再読込」.
- 「編集に戻る」 or Escape keeps the placement, the document and the notice bar.
- Discarding clears the placement (and its approval and dialog, as in STEP 122) and then calls
  `request_reload` again. If nothing is unsaved, the document reloads; otherwise the save
  question follows.
- When nothing is placed and nothing is unsaved, the reload happens with no question.

## 5. 未保存編集との共存

- For a reload, the save prompt's title is 「再読込する前に「{name}」の変更を保存しますか？」, with
  the usual 保存 / キャンセル / 保存しない.
- キャンセル: nothing is reloaded, and the edits and their undo history stay.
- 保存しない: the document reloads.
- 保存: the existing save path runs (in SwiftPDF, the Save As copy). The reload runs only in the
  callback after the file has been written (`save_then`, `close_saved_tab`). A cancelled or
  failed save never runs it. Discarding the placement alone never drops the PDF edits; their
  question always follows.
- Double clicks: while either question is up, `request_reload` returns at once, and the bar's
  request is a single `Option`, so nothing is asked or done twice.

## 6. 理事長印監査への影響

- During seal work (`seals.blocking()`: registration, update, deletion, migration, preparation,
  commit, reconcile, abandon and the undo/redo `Step` lines), a reload is refused with
  「理事長印の処理が終わるまで再読込できません。」. No question appears, the placement stays,
  and the stamp then commits normally.
- Not changed: SWSEAL2, the audit format, the approval requirements, the locks and reconcile.

## 7. 要復旧状態の保護

- A manual reload of a held document is refused with the hold's reason (`refuse_held`).
- An automatic reload of a held document is refused in `reload_view`, and the notice bar is set.
- The hold therefore stays until the tab is closed, as before.

## 8. 自動テスト結果

New tests in `crates/ui-egui/src/file_watch.rs`:

| Test | Covers |
|---|---|
| `reload_asks_about_a_placed_stamp_and_return_keeps_it` | General and chairman placements, 編集に戻る, and a second press |
| `discarding_the_placement_reloads_a_clean_document` | 破棄して再読込 |
| `reload_without_a_placement_or_changes_does_not_ask` | No question when nothing is at stake |
| `unsaved_edits_are_asked_about_after_the_placement` | Placement and edits together, Cancel, Don't save |
| `a_failed_save_does_not_reload` | A failed save stops the reload |
| `a_held_document_is_not_reloaded` | Manual and automatic reload of a held document |

In these tests automatic reloading is switched off, so only the Reload under test can reload. The
test harness's inline watcher otherwise reloads on every frame, which would hide a broken Reload.

Other test changes:

- `seal_flow_tests.rs` `seal_work_refuses_close_before_the_placement_question` also checks that a
  reload is refused during seal work.
- `tests/reload.rs` `unsaved_changes_are_kept_until_reload_is_chosen` was **updated on purpose**.
  It expected Reload to drop unsaved edits at once. It now clicks Reload, sees the save question,
  clicks Cancel (nothing changes), then clicks Reload and Don't save, and only then is the
  document replaced. It failed against the new code before the update, as predicted.

Proof that the tests fail without the fixes:

- With the bar going straight to the reload again, the hold and seal-work guard removed, and the
  seal-work check removed, 5 tests failed. The two that still passed are the positive cases,
  which are expected to reload.
- With only the `reload_view` guard removed, the held test fails at its automatic half: 3 pages,
  so the hold was gone.

Results:

| Suite | Passed | Failed | Ignored |
|---|---|---|---|
| ui-egui `--lib` | 627 | 0 | 4 |
| `tests/reload.rs` | 11 | 0 | 0 |
| `tests/commands.rs` | 25 | 0 | 0 |
| `tests/detailed_view.rs` | 9 | 0 | 0 |
| accessibility, actions, bookmarks, buttons, camera | 13, 2, 8, 2, 6 | 0 | 0 |
| `tests/combine_grid.rs` | 58 | **1** | 0 |
| pdfcraft-platform | 25 | 0 | 0 |

- **The combine_grid failure is not ours.** `arrow_keys_move_by_card_and_by_row_and_shift_extends`
  cannot find the "f8.pdf" card. It fails deterministically, and it fails identically on a clean
  export of `4eb07ee3`, before STEP 121/122. (`e54d834e` cannot be built from a clean export: it
  calls `widgets::artcraft_mark` and `artcraft_logo`, which do not exist.)
- **The other ui-egui integration test files were not run.** After combine_grid, this machine's
  toolchain kept crashing: rustc `STATUS_ACCESS_VIOLATION` and `STATUS_HEAP_CORRUPTION`, and
  `link.exe` exit codes 1318 and 0xc0000005, with 20 GB of RAM and 713 GB of disk free. The same
  sources had compiled cleanly shortly before. This is an environment problem, not a code
  failure, but those test files are unverified.

## 9. fmt・check・clippy

Run on the final code with `CARGO_INCREMENTAL=0`:

- `cargo fmt --check`: OK.
- clippy `-D warnings` for pdfcraft-platform, pdfcraft-engine and pdfcraft-ui-egui (all targets,
  including the edited test): OK.
- `cargo check -p pdfcraft -j 2`: OK.

## 10. 検証用 EXE と SHA-256

- `cargo build -p pdfcraft --release` with `CARGO_TARGET_DIR=D:\dev\SwiftPDF-seals-validate\step122a\target`,
  `CRAFT_FONTS_DIR` and `CRAFT_FONTS_REQUIRED=1`. It finished in 22 min 33 s, and
  `--version` prints `SwiftPDF 1.0.0`.
- SHA-256 `F9B3E2DDE11CC33A59A5E5962CA2DF7AF3C177B2C8C3EBAF6C5378CDB96BEF2A`, 97,062,912 bytes.
- The same EXE is at `step122a\portable\SwiftPDF.exe` and `step122a\fallback\SwiftPDF.exe`.
- It includes every production change of STEP 122 and 122A. The only later edit was to
  `tests/reload.rs`.
- The step117e and step117k EXEs and folders were not touched. Nothing was written there; the
  sample files were only read and copied.

## 11. データ隔離方法

`step122a\launch-isolated.ps1`, through `START_PORTABLE.cmd` or `START_FALLBACK.cmd`, does the
following:

- It refuses to run unless it lives in `D:\dev\SwiftPDF-seals-validate\step122a`.
- It redirects `LOCALAPPDATA`, `APPDATA`, `TEMP` and `TMP` **for the child process only**, to
  `<mode>\env\...`.
- It finds the real profile folder through the known-folder API (not the environment), and
  refuses if `Data`, `LOCALAPPDATA` or the EXE would be in the real `%LOCALAPPDATA%\SwiftPDF`, or
  outside the mode folder.
- **Portable mode**: `portable\portable.txt` plus the `portable\Data` folder.
- **Fallback mode**: `fallback\Data` is a **file**, so the portable probe fails, and the launcher
  refuses to start unless it is. SwiftPDF then falls back to `LOCALAPPDATA\SwiftPDF`, which is
  `step122a\fallback\env\Local\SwiftPDF`. The single-instance socket, recovery and logs also live
  under the redirected folders.
- File arguments must be inside `step122a`.
- `-Check` showed the expected paths for both modes without launching. Without the EXE, the
  launcher refused ("NOT LAUNCHED").

Fixtures: synthetic PDFs (`stamp-test`, `unsaved-test`, `normal`, `multi-page`) and synthetic
seals A and B, copied from the STEP 117E synthetic samples. They were generated by the test code;
no real seals or business PDFs are used.

- `RELOAD_TEST.ps1 -Prepare/-Change` writes only `output\reload-test.pdf`, a working copy.
- `output\` is where saves go.
- No access to the real `%LOCALAPPDATA%\SwiftPDF` was needed or made.

## 12. GUI チェックリスト

`D:\dev\SwiftPDF-seals-validate\step122a\TEST_CHECKLIST.md` covers A1 to A10, with exact screen
texts and the paths to expect. Items that cannot be produced through normal use (a hold, a reload
during seal work, a failed save) are listed as "not run in the GUI; covered by automated test X".

## 13. 人が確認した結果

**A1 to A10: all passed.** The user ran them in the real Windows GUI with the EXE in §10
(SHA-256 `F9B3E2DD…BEF2A`), launched through the isolated launcher, and reported the result
before STEP 123.

Not covered by that run: the checklist items marked "covered by automated test" — a reload while
held, a reload during seal work, and a failed save. They were not injected in the GUI and are not
counted as GUI passes.

## 14. 未確認事項

- In the real GUI: the injected-fault cases named in §13 (automated tests only).
- The ui-egui integration test files after combine_grid (§8).

## 15. 既知の SWSEAL2 I/O リスク

As in the STEP 122 report:

- The earlier parallel `WRITE_ERROR` / `Verify` failures did not reproduce, and their cause is not
  known. **Unresolved.**
- `verify_decode` and `verify_undecodable` cannot be reached in practice, so they have no test.

The repeated toolchain crashes on this machine during STEP 122A are a separate, environmental
reason to watch for I/O or memory instability while validating.

## 16. コミット可否

- The code, the targeted tests, fmt, check and clippy are ready for the development branch.
- Before committing, the remaining integration test files should run once on a stable build,
  either on CI or after a reboot.
- The GUI acceptance A1 to A10 has passed (§13).

## 17. 次工程案

1. Done: the user ran A1 to A10 in the real GUI, and every item passed.
2. Re-run `cargo test -p pdfcraft-ui-egui --tests --no-fail-fast` once the toolchain is stable,
   for example after a reboot or with Defender excluding the target folder.
3. Treat the pre-existing combine_grid failure as its own issue.
4. Optionally, reword the notice bar. In SwiftPDF it mentions an in-place 「保存」 that view-only
   mode does not offer, and Reload now asks before dropping changes.
5. The extracted baseline folders `step122a\baseline-4eb07ee3` and
   `step122a\baseline-e54d834e` can be deleted. Removing them automatically was refused.

## 18. `git diff --stat`

```
 crates/ui-egui/src/canvas.rs              |   7 +-
 crates/ui-egui/src/data_store.rs          |  37 +++--
 crates/ui-egui/src/dialogs.rs             |  45 ++++--
 crates/ui-egui/src/editing.rs             |  71 +++++++++-
 crates/ui-egui/src/file_watch.rs          | 222 +++++++++++++++++++++++++++++-
 crates/ui-egui/src/lib.rs                 |   2 +-
 crates/ui-egui/src/portable.rs            |  16 +++
 crates/ui-egui/src/seal_flow_tests.rs     |  97 +++++++++++++
 crates/ui-egui/src/seal_store_v2.rs       | 120 +++++++++++-----
 crates/ui-egui/src/seal_store_v2_tests.rs | 118 ++++++++++++++--
 crates/ui-egui/tests/reload.rs            |  10 ++
 11 files changed, 656 insertions(+), 89 deletions(-)
```

This includes STEP 122. The STEP 122A changes are in `dialogs.rs`, `editing.rs`,
`file_watch.rs`, `seal_flow_tests.rs` and `tests/reload.rs`.

## 19. `git status`

```
 M crates/ui-egui/src/canvas.rs
 M crates/ui-egui/src/data_store.rs
 M crates/ui-egui/src/dialogs.rs
 M crates/ui-egui/src/editing.rs
 M crates/ui-egui/src/file_watch.rs
 M crates/ui-egui/src/lib.rs
 M crates/ui-egui/src/portable.rs
 M crates/ui-egui/src/seal_flow_tests.rs
 M crates/ui-egui/src/seal_store_v2.rs
 M crates/ui-egui/src/seal_store_v2_tests.rs
 M crates/ui-egui/tests/reload.rs
?? docs/SWIFTPDF_V1_FINAL_UX_AND_IO_DIAGNOSTICS.md
?? docs/SWIFTPDF_V1_MANUAL_RELOAD_AND_GUI_ACCEPTANCE.md
?? docs/images/SwiftPDFiconORG.png
```
