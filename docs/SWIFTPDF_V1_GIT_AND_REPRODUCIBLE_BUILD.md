# SwiftPDF 1.0 — Git save and reproducible build check (STEP 123)

Status: written **before** the STEP 123 commit and included in it. The commit itself, the push,
and the clean-clone build happen after this file is committed. Their results (commit id, push,
SHA-256) are therefore **reported in the chat only**, not added here, so that no extra commit is
made without approval. Sections that depend on them say so.

Revision history:
- 2026-10-26 — first version, written before the STEP 123 commit.
- 2026-10-26 — STEP 124: appended the post-commit results (section "Post-commit results"). The
  earlier sections are unchanged.
- 2026-10-26 — STEP 125: B1–B8, run by a person on the clean-clone EXE, are recorded in that
  section.

## 1. 作業開始時の Git 状態

- Branch `feature/seal-data-safety-v1.0.0`, HEAD `5279fa965d0d920a53a9d58975a83e5a2d6abd32`, and
  `git diff --check` was clean.
- `git log -5`:
  - `5279fa96` wip: preserve incomplete STEP 122 UX and I/O diagnostics
  - `e54d834e` chore(release): prepare SwiftPDF 1.0.0 identity, branding and documentation
  - `4eb07ee3` docs(seals): record SWSEAL2 safety design, faults, and final regression
  - `0623b414` feat(seals): add concurrency-safe SWSEAL2 store and audited stamp flow
  - `c740330f` fix: harden page rotation against repaired xref structures
- Changes: the 11 expected modified files, the 2 expected untracked docs, and
  `docs/images/SwiftPDFiconORG.png` (owned by the user). Nothing else.

## 2. コミット対象一覧

| Kind | Files |
|---|---|
| Modified (11) | `crates/ui-egui/src/{canvas,data_store,dialogs,editing,file_watch,lib,portable,seal_flow_tests,seal_store_v2,seal_store_v2_tests}.rs`, `crates/ui-egui/tests/reload.rs` |
| New (3) | `docs/SWIFTPDF_V1_FINAL_UX_AND_IO_DIAGNOSTICS.md`, `docs/SWIFTPDF_V1_MANUAL_RELOAD_AND_GUI_ACCEPTANCE.md`, this file |

Before staging, the diff and the docs were searched for user names, credentials and private paths:

- No user names or credentials were found.
- The only hit in code is a test asserting that log lines do **not** contain seal data.
- The docs name only the synthetic validation folders under `D:\dev\SwiftPDF-seals-validate`.

The files are staged by name. `git add .` and `git add -A` were not used.

## 3. 除外ファイル

- `docs/images/SwiftPDFiconORG.png` stays untracked: not added, moved, overwritten or deleted.
- Outside the repository, and so never staged:
  - seal stores (`seals.v2.bin`, backups);
  - validation PDFs and EXEs;
  - `target` folders;
  - the `step122` and `step122a` folders;
  - personal settings.

## 4. STEP 122・122A の検証結果

**Automated** (STEP 122A, on the code being committed):

| Check | Result |
|---|---|
| `cargo fmt --check` | Passed |
| `cargo check -p pdfcraft` | Passed |
| clippy `-D warnings` (platform, engine, ui-egui, all targets) | Passed |
| ui-egui `--lib` | 627 passed, 0 failed, 4 ignored |
| `reload` | 11 passed |
| `commands` | 25 passed |
| `detailed_view` | 9 passed |
| pdfcraft-platform | 25 passed |
| accessibility, actions, bookmarks, buttons, camera | 31 passed |
| `combine_grid` | 58 passed, **1 failed**. It fails identically on `4eb07ee3` (§13). |

The remaining ui-egui integration test files were not run (§13).

**Human, real Windows GUI:** A1 to A10 all passed, with the STEP 122A EXE
(`F9B3E2DD…BEF2A`) run through the isolated launcher. The reload while held, during seal work,
and after a failed save were not injected in the GUI and are covered by automated tests only.

The code has not changed since those runs; STEP 123 changes only docs. The heavy suites were
therefore not repeated before the commit.

## 5. コミット ID

Reported in the chat. A commit cannot name its own id.
Message: `fix(release): finalize STEP 122 UX safety and reload guards`.

## 6. GitHub への push 結果

Reported in the chat. The push is `git push origin feature/seal-data-safety-v1.0.0`; force pushes
are never used.

## 7. クリーン clone の場所

`D:\dev\SwiftPDF-repro-step123`:

- a fresh `git clone` of `origin`, checked out at the pushed branch;
- build output in its own `target` folder inside that clone.

The working tree `D:\dev\SwiftPDF` and `D:\dev\SwiftPDF-seals-validate` are not touched.

## 8–11. 再現ビルド・release・SHA-256・ツールチェーン異常

Reported in the chat after the run.

Settings: `CRAFT_FONTS_DIR=D:\dev\craft-fonts`, `CRAFT_FONTS_REQUIRED=1`, `CARGO_BUILD_JOBS=1`,
`RUST_TEST_THREADS=1`, `CARGO_INCREMENTAL=0`.

If rustc or link.exe crash again (access violation, heap corruption, linker 1318), the run stops
without retrying the same heavy step. The crash is recorded as a toolchain fault, not a test
failure.

## 12. 旧コミットのビルド問題

- `e54d834e` removed `artcraft_mark` and `artcraft_logo` from `widgets.rs`, but `dialogs.rs` and
  `home.rs` still call them. That commit cannot compile from a clean checkout.
  - The cause is a source mismatch inside one commit, not a file missing from Git or the build
    environment.
- `5279fa96` no longer has any reference to either function (checked with `git grep`). Whether it
  builds from a clean checkout has not been tested.
- Neither commit is rewritten; both stay as history.
- New commits are judged by the clean-clone build in §7.

## 13. 未完了の結合テスト

- `combine_grid` `arrow_keys_move_by_card_and_by_row_and_shift_extends` fails before and after
  STEP 121/122, so it is not caused by them. It should be handled as its own issue.
- The ui-egui integration test files after `combine_grid` were not run in STEP 122A, because
  rustc and link.exe kept crashing on this machine. They should be run on CI or on a stable
  machine.

## 14. SWSEAL2 の未解決 I/O 問題

- The earlier parallel `WRITE_ERROR` / `Verify` failures have not recurred, and their cause is
  unknown.
- The stage diagnostics added in STEP 122 (`seal_store_v2.rs`) will name the failing stage if it
  happens again.
- `verify_decode` and `verify_undecodable` have no test, because they cannot be reached in
  practice.

## 15. 次工程に向けた課題

1. A full `cargo test -p pdfcraft-ui-egui --tests` on CI or a stable machine.
2. Handle the combine_grid failure.
3. Watch for SWSEAL2 I/O errors, using the new diagnostics.
4. Reword the notice bar for view-only mode (it mentions an in-place 保存).
5. Before any formal release: a release build from a clean clone, then the user's GUI acceptance
   of that exact EXE.

## 16. `git status`

Before the commit: the 11 modified files, the 3 untracked docs, and the untracked
`docs/images/SwiftPDFiconORG.png`. After the commit, only `SwiftPDFiconORG.png` should remain
untracked. The actual result is reported in the chat.

## Post-commit results (appended in STEP 124)

These are the results that §5–§11 left to the chat. They are recorded here without changing the
sections above.

- **Commit:** `7db6ef69530a7e5b5e87d3b61b704ead04b95a08`
  (`fix(release): finalize STEP 122 UX safety and reload guards`). It contains 14 files, staged by
  name. `git diff HEAD^ HEAD --check` was clean.
- **Push:** `git push origin feature/seal-data-safety-v1.0.0` succeeded (`5279fa96..7db6ef69`),
  with no force push. After `git fetch`, the local HEAD and
  `origin/feature/seal-data-safety-v1.0.0` were the same commit.
- **Clean clone:** `D:\dev\SwiftPDF-repro-step123`, a fresh `git clone` of the pushed branch, with
  HEAD at `7db6ef69`.
  - The Cursor sandbox sets `CARGO_TARGET_DIR` to a shared cache. The first `check` there
    therefore reused that cache and is not counted.
  - Every counted build used the new, empty folder
    `D:\dev\SwiftPDF-repro-step123\target-clean`.
- **Checks:**

  | Check | Result |
  |---|---|
  | `cargo fmt --check` | Passed |
  | `cargo check -p pdfcraft -j 1` | Passed: 23 min 13 s, 545 crates compiled from empty |
  | clippy `-D warnings` (platform, engine, ui-egui, all targets) | Passed: 5 min 27 s, no warnings |

- **Release build:** `cargo build -p pdfcraft --release -j 1` succeeded in 38 min 48 s.
  `--version` prints `SwiftPDF 1.0.0`.
- **SHA-256:** `551D6DE1DFF75FFB525CDCC21C61D45200B8955399727D01B2404C4E1452E318`
  (97,062,912 bytes), for `target-clean\release\SwiftPDF.exe`.
- **Bit-for-bit reproducibility: not achieved.**
  - The STEP 122A EXE (`F9B3E2DD…`) is the same size, but its bytes differ.
  - Both EXEs embed their own build folder paths, so two builds from different folders never
    match byte for byte. Their production source is the same; only tests and docs differ.
- **GUI acceptance:** the A1–A10 check in STEP 122A was done on the **STEP 122A EXE**
  (`F9B3E2DD…`). B1–B8 were later run by a person on this clean-clone EXE (`551D6DE1…`), and
  every item passed. See the readiness report §12 and §13.
- **Toolchain:** no rustc or link.exe crash occurred during these builds.
  - On 2026-10-26 the machine itself stopped twice with bug check 0x1A (MEMORY_MANAGEMENT): first
    during the STEP 124 test build, then again after a restart, with no build running.
  - The user reports that the memory has since been checked.
  - See `SWIFTPDF_V1_INTEGRATION_TEST_AND_RELEASE_READINESS.md` §8.
