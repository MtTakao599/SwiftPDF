# SwiftPDF 1.0 — final UX and SWSEAL2 I/O diagnostics (STEP 122)

Status: implemented and tested on synthetic data; **not committed**; GUI acceptance on the
verified EXE **not done** (kittest and unit tests only).

Revision history:
- 2026-10-25 — first version (STEP 122, on top of WIP commit `5279fa96`).
- 2026-10-26 — final verification. Every gate and test was re-run on the final working tree (§11,
  §12), the storage-location map was added (§5), and the home wording was checked against the
  granted capabilities (§10). No code changes.

Throughout, **[WIP]** marks code that was already in commit `5279fa96` and **[新規]** marks what
STEP 122 added or changed in the working tree on top of it.

## 1. 開始時の Git 状態

- HEAD: `5279fa965d0d920a53a9d58975a83e5a2d6abd32`. The user approved it as the resume point; we
  did not go back to `4eb07ee3`.
- Untracked: `docs/images/SwiftPDFiconORG.png`. It was not touched (not deleted, moved,
  overwritten or added to Git).
- No reset, clean, stash, rebase, amend, commit or push was used.

WIP audit (the four-way classification requested on resume):

| WIP item | Classification |
|---|---|
| `staging.rs` `AtomicWriteStage` / `write_atomically_detailed` | Implemented and tested (the write stage itself has no test) |
| Placement prompt (`place_close_prompt`, `editing.rs` helpers, six tests) | Implemented but under-tested; two close paths bypassed it |
| Data folder display (`data_store.rs`, About) | Implemented but under-tested (unit tests only) |
| SWSEAL2 `io_note` diagnostics | **Incomplete or unsafe**: the commit did not compile (3 errors), the fault-injection test was tautological, the elapsed time was measured from process start, and a reread I/O error was logged as Verify without its OS code |
| Home subtitle | Implemented and tested |

## 2. 仮置きスタンプの終了経路

Every way a document or the app can close, and how each one treats a placed stamp that is not yet
in the PDF:

| Path | Behaviour |
|---|---|
| Window × / quit (`request_quit`, `guard_quit`) | Placement question **[WIP]** |
| Tab close (`request_close_tab`) | Placement question **[WIP]** |
| Close all (`close_all`, `close_and_continue`) | Placement question, one tab at a time **[WIP]** |
| Opening or switching to another PDF | The placement stays on its own tab; nothing closes **[WIP]** (test) |
| Signed-version notice bar "Close" | **Fixed [新規]**: it called `close_tab` directly when nothing was unsaved, dropping the placement. It now goes through `request_close_tab`. |
| Automatic reload when the file changes on disk | **Fixed [新規]**: it reloaded under the placement. A pending placement now counts like unsaved work, so the notice bar is shown and the document is not reloaded. |
| The notice bar's manual "Reload" button | Not changed. This is an explicit "replace with the disk version", and it drops the placement as it drops unsaved edits. Left as P1 (§14). |

## 3. 仮置き確認の実装結果

- **[WIP]** The `PlaceClose` prompt layer sits after `SealWork` and before the save prompt. Its body
  is 「理事長印が仮置きのままです。」 or 「スタンプが仮置きのままです。」, and its buttons are 「編集に戻る」
  and 「仮置きを破棄して閉じる」. Escape keeps the placement. It is a separate prompt from the save
  prompt: after a discard, a document that also has unsaved edits gets the usual save question.
- **[新規]** `discard_pending_place` closes the approval dialog (ConfirmSeal) only when it belongs to
  the tab being discarded, which is the active tab. Another tab's dialog stays open.
- **[新規]** While the placement question is showing, 確定 (`commit_stamp_place`) does nothing,
  because `front_blocks_stamp_commit` already blocks it. A test proves this.
- Asking about the same tab twice does not discard it (a WIP test).

## 4. 理事長印の監査保護

- Seal work always closes first. `seals.blocking()` covers registration, update, deletion,
  migration, approval and preparation, commit, reconcile and abandon. During any of these, every
  close path is refused with 「理事長印の処理が終わるまで終了できません。」, and no placement question
  is shown. **[新規 test]** `seal_work_refuses_close_before_the_placement_question` covers ×, tab
  close and close-all while the preparation is waiting on the store lock; the stamp is then
  committed with `use_approved`, `use_prepared` and `use_committed`.
- A discard writes nothing to the store. The approval is held only in memory until 確定 starts the
  Prepare job (record_approval plus prepare_use), so discarding before that point leaves no
  audit line to finish. **[新規 test]**
  `quitting_with_an_unconfirmed_chairman_seal_asks_and_writes_nothing` checks that the file is
  unchanged byte for byte, that there are no `use_*` lines, and that the approval and the dialog
  are cleared.
- A document held for recovery (要復旧, `hold_output`) still refuses save and print. This is the
  existing test `the_close_button_waits_for_seal_work_and_asks_about_unsaved_or_held_documents`,
  which passes unchanged.
- Not changed: the approval requirements, the audit event format, reconcile and `hold_output`.

## 5. 実際のデータ保存先

- **[WIP]** About shows 「データ保存先」, the mode (通常 / ポータブル / フォールバック) and the absolute path
  in monospace, read-only. The path is exactly what `settings_dir()` chose, which is the same
  folder passed to `set_seal_dir`. If no description is available, `displayed_data_path()` falls
  back to the seal store's own folder.
- **[新規 test]** `the_displayed_data_folder_is_where_the_seals_are_written` starts in portable
  mode in a synthetic executable folder, registers a seal, and checks three things:
  `displayed_data_path() == seals.store_dir()`, `Data/seals.v2.bin` exists, and no fallback
  warning was shown.
- Tests never touch the real `%LOCALAPPDATA%\SwiftPDF`. The test folders are asserted to be outside
  it.

Where each kind of data lives (from the code; not changed in STEP 122):

| Data | Regular mode | Portable mode (`Data` writable) |
|---|---|---|
| Settings (`app.ron`) | `%LOCALAPPDATA%\SwiftPDF` (`settings_dir`) | `<exe>\Data` |
| Chairman seals (`seals.v2.bin`, lock, `seals.bin` tombstone) | Same folder as settings (`set_seal_dir`) | `<exe>\Data` |
| Seal migration and set-aside backups | The same store folder (names recorded in the audit line) | `<exe>\Data` |
| Logs | `%LOCALAPPDATA%\SwiftPDF` | `<exe>\Data` |
| Crash recovery | `%LOCALAPPDATA%\SwiftPDF\Recovery` | `<exe>\Data\Recovery` |
| Single-instance socket | `%LOCALAPPDATA%\SwiftPDF` | `%LOCALAPPDATA%\SwiftPDF`, named `portable-*`. It is not under `Data`. |

On a portable fallback, everything in the table except the socket uses the regular column. About
shows the settings and seal folder: one path covering settings, seals, their backups and the logs.
It does not show the recovery subfolder or the socket.

## 6. ポータブルフォールバック

- Not changed: the detection itself (`portable.rs`, a marker file plus a probe write).
- **[WIP]** On fallback, the app shows a toast with the path and the OS reason, and About shows the
  fallback mode and the reason.
- Tests:
  - `Data` cannot be created because a file is in the way (existing test).
  - `Data` exists but refuses a file **[新規]**. The probe name is taken by a folder; this stands in
    for a read-only share, which fails at the same write.
  - `Data` is missing and gets created (existing test).
  - The fallback notice built from the real probe result **[新規]**.

## 7. I/O 診断の実装内容

- **[WIP]** The atomic write reports which stage failed: `Create`, `Write`, `Sync` or `Rename`. A
  first-time creation records `create`, `write` or `sync` separately.
- **[新規]** The test-only fault injection now goes through the same error mapping as a real
  failure (`AtomicWriteError { stage, source }` leads to `last_stage` and then the log). The WIP
  injection logged the injected string directly, so its test proved nothing.
- **[新規]** The reread distinguishes three outcomes:
  - the file is missing: `Verify`;
  - an I/O error: `Io`, with its OS code;
  - the file is not readable as a store: `Unreadable`.

  Previously all three were logged as Verify.
- The verify steps distinguish `verify_bytes` (the bytes read back differ), `verify_decode` and
  `verify_undecodable`. **Note:** the last two cannot be reached in practice, because identical
  bytes decode identically. They have no test, and no artificial hook was added to fake one.
  Decryption failure at load is tested by the existing
  `damaged_or_foreign_ciphertext_is_refused_and_kept`.
- **[新規]** The elapsed time is measured per store operation (`io_note::begin()`), not from
  process start.
- **[新規]** The WIP compile errors were fixed: a missing `op` argument in the tombstone write
  (now `"migrate"`) and two `&str` mismatches in the dialog. A clippy `question_mark` lint was
  also fixed.
- Not changed: the lock scope, atomic replacement, DPAPI scope, use_status, commit_use reconcile,
  hold_output, the append-only audit, the Busy timeout and the retry conditions (only
  PermissionDenied and os 32/33, with the existing delays). A Verify failure is always an error.

## 8. ログに含める情報

Each failure produces one `log::warn!` line:

```
swseal2_io stage=<create|write|sync|rename|reread|verify_bytes|verify_decode|verify_undecodable>
  class=<Io|Unreadable|Verify> op=<register|update|delete|record_approval|prepare_use|…|migrate>
  gen=<generation> wrote=<bytes> back=<bytes|-> file=seals.v2.bin kind=<io ErrorKind|-> os=<code|-> ms=<elapsed>
```

The line never contains a seal image, ciphertext, plaintext, a key, approval contents, history
lines, a seal name, a seal id or hash, or any folder path. The file name is the constant
`seals.v2.bin`. **[新規 test]** `the_log_line_names_the_store_file_only` captures the real log
output and checks all of this.

## 9. 障害注入結果

| Injection | Result |
|---|---|
| Create, Write, Sync and Rename, each with an OS code that is not retried (Windows 112/39/29/17) | `Io(WRITE_ERROR)`. The log line has the right stage, `class=Io`, `op=record_approval` and the OS code. The file is unchanged byte for byte, the history is unchanged, and the approval is not recorded, so `prepare_use` is refused with `NotApproved` (the stamp is refused). |
| Bytes changed after the write | `Verify`, logged as `verify_bytes`. The stamp is refused afterwards. |
| File removed after the write | `Verify`, logged as `reread` with class Verify. |
| A folder in place of the file after the write | `Unreadable`, logged as `reread` with class Unreadable. |
| Damaged ciphertext at load | Refused, and the file is kept (existing test). |

**Each test was shown to fail first.** With the `last_stage` copy removed, the stage test fails
(every stage is logged as `create`). With either bypass fix reverted, its regression test fails
(notice bar Close, automatic reload).

## 10. ホーム画面字幕

**[WIP]** The view-only (SwiftPDF) subtitle is 「PDF の閲覧・編集・スタンプ・安全な別名保存ができます。」 /
"View, edit, stamp, and save a safe copy of a PDF." The word "scriptable" no longer appears. Both
languages are tested in `tests/detailed_view.rs`. The full PdfCraft build keeps its own subtitle.

The wording matches what `main.rs` grants in SwiftPDF mode: Save As, adding text, print, image and
chairman stamps, and rotating the current page. In this mode, 「編集」 means adding text and
rotating the current page, not general content editing. In view-only mode, Home hides the
promotion card and the "Recommended tools" (editing tools); Open file, folders and the recent
list stay.

## 11. テスト結果

Final run (2026-10-26) on the final working tree, with
`CARGO_TARGET_DIR=D:\dev\SwiftPDF-seals-validate\step122\target`, `CRAFT_FONTS_DIR`,
`CRAFT_FONTS_REQUIRED=1`, `CARGO_BUILD_JOBS=2` and `CARGO_INCREMENTAL=0`.

| Suite | Passed | Failed | Ignored |
|---|---|---|---|
| `pdfcraft-ui-egui --lib` | 621 | 0 | 4 (fixture writing, large-store timings) |
| `pdfcraft-ui-egui` `tests/commands.rs` (includes the About data-folder test) | 25 | 0 | 0 |
| `pdfcraft-ui-egui` `tests/detailed_view.rs` (Home subtitle) | 9 | 0 | 0 |
| `pdfcraft-platform` (all, including staging) | 25 | 0 | 0 |

Earlier in STEP 122, seal_store_v2 plus seal_flow ran 8 times, with 70 passed every time.

**These are automated tests, including kittest; they are not a GUI check by a person on Windows.**
That check has not been done.

## 12. fmt・check・clippy

Final run (2026-10-26):

- `cargo fmt --check`: OK.
- `cargo clippy -p pdfcraft-platform --all-targets -j 2 -- -D warnings`: OK.
- `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings`: OK.
- `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings`: OK.
- `cargo check -p pdfcraft -j 2`: OK in both the step122 and step117a targets.

During implementation, rustc twice crashed with `STATUS_HEAP_CORRUPTION` while compiling the
third-party `windows` 0.62.2 crate, and an incremental-cache compiler panic occurred once. With
`CARGO_INCREMENTAL=0` the final run did not reproduce either. They were toolchain or environment
failures, not errors in our code.

The three WIP compile errors are resolved and the build passes: the missing operation argument
in the tombstone write, and the two `&str` mismatches in the placement dialog.

## 13. 未解決の I/O リスク

- **The parallel WRITE_ERROR/Verify failure seen earlier did not reproduce here (8 of 8 green). It
  is not fixed and not explained.** The new log line (stage, class, OS code, ms) is what should
  identify it the next time it happens.
- `verify_decode` and `verify_undecodable` have no test (§7).
- Write-stage injection covers the replace path only. First-time creation is covered for the
  create, write and sync labels at the code level, not by injection.

## 14. 残る P0 / P1

- P0: none known.
- P1:
  - The manual Reload button drops a placement (§2). Suggestion: ask with the same placement
    question first.
  - The fallback warning appears as a toast at start-up and in About only. It is easy to miss.
    Suggestion: a persistent line on Home.
  - When `settings_dir()` returns None, About shows no data folder.
  - About does not show the recovery folder or the socket location (§5). In portable mode the
    socket stays in `%LOCALAPPDATA%\SwiftPDF`, as designed.
  - The GUI acceptance on the verified EXE is not done.

## 15. 次工程への提案

1. Run the GUI acceptance on a separately built EXE in an isolated folder (×, tab close and close
   all with a placed seal; About in portable and fallback mode). Do not overwrite the 117E/117K
   EXEs.
2. Watch the next parallel failure using the `swseal2_io` lines.
3. Decide on the P1 items in §14.

## 16. `git diff --stat`

```
 crates/ui-egui/src/canvas.rs              |   7 +-
 crates/ui-egui/src/data_store.rs          |  37 ++++++++--
 crates/ui-egui/src/dialogs.rs             |   5 +-
 crates/ui-egui/src/editing.rs             |  27 +++++--
 crates/ui-egui/src/file_watch.rs          |  39 +++++++++-
 crates/ui-egui/src/lib.rs                 |   2 +-
 crates/ui-egui/src/portable.rs            |  16 ++++
 crates/ui-egui/src/seal_flow_tests.rs     |  91 +++++++++++++++++++++++
 crates/ui-egui/src/seal_store_v2.rs       | 120 +++++++++++++++++++++---------
 crates/ui-egui/src/seal_store_v2_tests.rs | 118 +++++++++++++++++++++----
 10 files changed, 392 insertions(+), 70 deletions(-)
```

(Plus this report, which is untracked.) The changes to `lib.rs` and parts of `data_store.rs` are
`cargo fmt` fixes to WIP code.

## 17. `git status`

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
?? docs/SWIFTPDF_V1_FINAL_UX_AND_IO_DIAGNOSTICS.md
?? docs/images/SwiftPDFiconORG.png
```
