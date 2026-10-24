# SwiftPDF v1 最終回帰（STEP 117K）

状態：自動試験済み。STEP 117K の Windows 実GUI（K1〜K7）は利用者が OK と報告。開発ブランチへコミットする。v1.0.0 の一般公開は未承認。E4／E5 は未実施。

通常の `%LOCALAPPDATA%\SwiftPDF`、利用中の `seals.bin`、実理事長印、実業務 PDF には触れていない。合成印影と一時フォルダー、および `D:\dev\SwiftPDF-seals-validate\step117k` のみ。

## 1. 検証対象のコード状態

- リポジトリ：`D:\dev\SwiftPDF`
- ブランチ：`feature/seal-data-safety-v1.0.0`
- HEAD：`c740330f9237fd83abe4bf5508bce1f348f1f757`
- 対象：STEP 117B〜117J の未コミット製品コード・試験・報告書と、本工程の再照合監査（`/NM` の一意性とページ照合の締め）
- 禁止した操作：`git add` / `commit` / `push` / `reset` / `clean` / `stash` / `rebase`、`main` マージ、タグ、Release、MSI
- `ROADMAP.md` は変更していない

## 2. `/NM` 識別の監査

エンジンは注釈追加時に ISO 32000 の `/NM`（UUID）を付ける。再照合は「一覧の先頭の `/NM` を採用」しない。

保持の条件（すべて必要）：

1. 対象 `DocId` が操作の文書である
2. その文書上で、捕捉した `/NM` を持つ Stamp 注釈が **ちょうど 1 件**
3. その注釈のページが `StampOp.place.page` と一致する
4. その注釈の `ObjRef` が、この操作が追加した参照と一致する
5. その参照が今も `present_owned_stamps` にある
6. 保管庫の `store_id` / `op_id` / 印影 ID / revision / 印影 SHA-256 / 準備 PNG の SHA-256 が一致する
7. 使用状態が `Committed(InDocument)` である

生成時の一意性（`chairman_stamps_each_receive_a_unique_annotation_name`）：

- 同じページの 2 押印は別 `/NM`
- 別ページも別 `/NM`
- 別文書も別 `/NM`
- Undo 後の `ObjRef` 再利用では新しい `/NM` が付く

重複 `/NM` を PDF に注入する公開 API は無い。そのため「重複を作って hold する」注入はしていない。重複が存在する場合は上記 2 により保持しない。

配置矩形の数値比較は、回転後の矩形が既存 API で安定に取れないため入れていない。

## 3. 再照合の結果

| ケース | 結果 | 根拠 |
|---|---|---|
| 正常 commit | 押印を保持 | 既存の準備→確定試験 |
| commit 着地・Verify 失敗 | 同一操作と証明できれば保持 | `a_landed_commit_reported_as_error_is_kept_after_reconciliation` |
| commit 未適用 | PDF 押印を取り消し、監査は `Cancelled` | `a_failed_commit_takes_the_stamp_back_and_redo_cannot_restore_it` |
| 再照合読み取り失敗 | `hold_output` | `a_store_that_vanishes_after_commit_holds_the_document` |
| 再照合 Busy | `hold_output` | `a_busy_store_during_reconciliation_holds_the_document` |
| `store_id` 不一致 | `hold_output` | `a_store_id_mismatch_after_commit_holds_the_document` |
| `Committed` だが PDF に押印が無い | 成功扱いせず `hold_output` | `a_landed_commit_without_its_stamp_holds_the_document` |
| `op_id` / 印影 ID / revision / PNG SHA-256 不一致 | 成功扱いしない | `stamp_status_matches` の等式。合成での属性改変は公開 API が無いためコード確認 |
| `/NM` 不一致・空 | 成功扱いしない | `annot_nm` 空または名前不一致は hold |
| `/NM` 重複 | 成功扱いしない | ちょうど 1 件でなければ hold |
| 注釈種別不一致 | 成功扱いしない | Stamp 以外は数えない |
| `ObjRef` 再利用 | 別印影を誤認しない | `/NM` が変わる。既存の再利用試験 |
| 別文書 | 誤った文書に反映しない | `DocId` 固定。タブ切替試験 |

`Committed(InDocument)` を読んでも、PDF 上の該当 Stamp が一意に残っていなければ成功にしない。

## 4. Undo／Redo 監査

`undo_redo_and_removal_add_lines_by_op_id_and_a_failed_line_puts_the_step_back` と、再照合で保持したあとの延長：

1. 理事長印を押す → `use_committed`
2. Undo → `use_undone` が 1 回
3. Redo → `use_redone` が 1 回
4. Verify 失敗後に保持した押印でも同じ（着地試験の末尾）
5. 一般スタンプの Undo／Redo では理事長印の監査行が増えない
6. `ObjRef` 再利用でも対象を取り違えない
7. 監査書き込み失敗時は編集を成功扱いせず、文書を戻す
8. 巻き戻しにも失敗した場合は要復旧
9. 要復旧中の保存・印刷・書き出し・編集は拒否
10. 監査イベントは追記のみ。物理削除しない

## 5. SWSEAL2 競合・移行

既存試験（`seal_` 96 件、プロセス 3 件）で確認した範囲：

- 登録・更新・削除、古い revision の拒否、古い承認の拒否
- `op_id` / `step_id` の重複防止
- 2 プロセス同時追加（40 イベント保持）
- Busy 時の入力保持
- SWSEAL1 からの明示移行、延期時の不変、失敗時の原本保持
- バックアップと墓標、旧版書き戻しの退避・通知
- 監査の追記専用、容量上限 64MiB、48MiB 超の警告
- 未完了の `use_prepared` 表示
- DPAPI 復号失敗時の拒否（暗号文は置き換えない）

64MiB 実体と 63MiB タイミングは `--ignored` のまま。本工程では再実行していない。

## 6. 障害注入結果

合成フックのみ（`cfg(test)` の `Point::{BeforeWrite,AfterWrite}`）。製品の常設スイッチは無い。

- 着地＋Verify 失敗 → 証明できれば保持、できなければ hold
- 未着地 → 取り消し
- 保管庫消失・Busy・`store_id` 不一致・押印欠落 → hold
- 取り消し書き込み失敗（PDF は空）→ hold せず、未完了の準備として通知

Verify 失敗を無条件再試行していない。不明な状態は成功にしない。

## 7. 全体回帰テスト

描画系は `--test-threads=1`。環境：`CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`、`CARGO_BUILD_JOBS=2`。

| 対象 | 成功 | 失敗 | 無視 |
|---|---|---|---|
| `pdfcraft-ui-egui` `--lib` | 602 | 0 | 4 |
| うち `seal_` | 96 | 0 | 3 |
| `seal_store_v2_processes` | 3 | 0 | 0 |
| `pdfcraft-platform` `--lib` | 24 | 0 | 0 |
| `pdfcraft-engine` `--lib` | 325 | 0 | 1 |
| `combine_grid` | 58 | 1（既知） | 0 |
| `fonts` | 10 | 1（既知） | 0 |

SWSEAL2 に結びつく新しい失敗は無い。ワークスペース全件はディスクと時間の理由で回していない。

## 8. 既知の失敗

- `combine_grid::arrow_keys_move_by_card_and_by_row_and_shift_extends` — 結合グリッドの矢印。SWSEAL2 と無関係。
- `fonts::chinese_ui_uses_an_installed_chinese_face` — 中国語 UI 面。SWSEAL2 と無関係。

## 9. 未解決の並列 I/O リスク

STEP 117D〜117F の全体並列 46 回中 1 回：

- 失敗：`WRITE_ERROR` と `Verify`
- OS コードは不明
- 原因は未確定
- 成功率が上がっても原因確定とはしない

本工程では無制限の全体並列を繰り返していない。STEP 117H の障害注入で、失敗時にデータを捨てないことは別項目として確認済み。

## 10. 実GUI検証環境

| 項目 | 値 |
|---|---|
| フォルダー | `D:\dev\SwiftPDF-seals-validate\step117k` |
| exe | `...\step117k\SwiftPDF.exe` |
| SHA-256 | `18A227FC138F3509A4DFD6F89A631CBD95C698E6409264DCCD9EE8104EA68392` |
| 117E exe | 上書きしていない。SHA-256 `F5FA5862EB596C823D3161C0475D8B6ED60DE78638B643E9CDBC807F2C768BBD` |
| 隔離 | 子プロセスの `LOCALAPPDATA` / `APPDATA` / `TEMP` / `TMP` を `step117k\env\*`。`portable.txt` あり |
| 起動確認 | `launch-isolated.ps1 -Check` → 隔離 OK、**起動していない** |
| データ | 合成 PDF と合成 PNG。`seals.v2.bin` は検証用 Data のみ（人が起動したあと） |

Cursor が exe を置いたことは、実GUI合格ではない。

## 11. 人による実GUI確認結果

STEP 117K の K1〜K7 は、利用者が Windows 実ウィンドウで **すべて OK** と報告した（2026-10-24）。Cursor の起動や kittest では合格にしていない。

1. 理事長印の登録・承認・押印 — OK（利用者）
2. Undo／Redo — OK（利用者）
3. 一般スタンプを混ぜた Undo／Redo — OK（利用者）
4. 別名保存 — OK（利用者）
5. 再オープン — OK（利用者）
6. ×ボタンでの終了 — OK（利用者）
7. 移行・競合への明らかな影響 — OK（利用者）

E4（印刷プレビュー）と E5（その他の周辺操作）は **未実施** のまま。これを K1〜K7 の言い換え以上には数えない。

チェックリスト：検証フォルダー `step117k\TEST_CHECKLIST.md`（リポジトリ外）

## 12. コミット対象候補

追跡済み変更（`git diff --stat` に出る）：

- `apps/pdfcraft/src/main.rs`
- `crates/engine/src/lib.rs`
- `crates/engine/src/optimizer.rs`
- `crates/platform/src/dpapi.rs`
- `crates/platform/src/lib.rs`
- `crates/ui-egui/src/canvas.rs`
- `crates/ui-egui/src/commands.rs`
- `crates/ui-egui/src/control.rs`
- `crates/ui-egui/src/dialogs.rs`
- `crates/ui-egui/src/editing.rs`
- `crates/ui-egui/src/lib.rs`
- `crates/ui-egui/src/print_ui.rs`
- `crates/ui-egui/src/seal_store.rs`
- `crates/ui-egui/src/stamp_preset.rs`
- `crates/ui-egui/src/stamps_ui.rs`

未追跡（`git diff --stat` に出ない。別途確認した）：

- `crates/platform/src/lock.rs`
- `crates/ui-egui/src/seal_flow.rs`
- `crates/ui-egui/src/seal_flow_tests.rs`
- `crates/ui-egui/src/seal_service.rs`
- `crates/ui-egui/src/seal_store_v2.rs`
- `crates/ui-egui/src/seal_store_v2_tests.rs`
- `crates/ui-egui/tests/seal_store_v2_processes.rs`
- `docs/SWIFTPDF_V1_GUI_ACCEPTANCE_PREP.md`
- `docs/SWIFTPDF_V1_PARALLEL_TEST_INVESTIGATION.md`
- `docs/SWIFTPDF_V1_RELEASE_READINESS.md`
- `docs/SWIFTPDF_V1_SEAL_COMMIT_RECONCILIATION.md`
- `docs/SWIFTPDF_V1_SEAL_CONCURRENCY_DESIGN.md`
- `docs/SWIFTPDF_V1_SEAL_FAULT_TOLERANCE.md`
- `docs/SWIFTPDF_V1_SEAL_GUI_VALIDATION.md`
- `docs/SWIFTPDF_V1_SEAL_OPERATION_IDENTITY.md`
- `docs/SWIFTPDF_V1_SEAL_STORAGE_SPEC.md`
- `docs/SWIFTPDF_V1_SEAL_STORE_IMPLEMENTATION.md`
- `docs/SWIFTPDF_V1_SEAL_UI_INTEGRATION.md`
- `docs/SWIFTPDF_V1_FINAL_REGRESSION.md`（本ファイル）

障害注入フックは `cfg(test)` のみ。製品バイナリには入らない。

## 13. コミットから除外すべきファイル

- `D:\dev\SwiftPDF-seals-validate\**`（117E/117K の exe、Data、合成印影、隔離 env、ログ）
- `CARGO_TARGET_DIR` の生成物
- 実印影画像、実業務 PDF、利用中の `seals.bin` / 暗号文
- 一時フォルダーへの実行時絶対パス（試験は `temp` 下に動的作成）
- デバッグ用の常設設定や、製品に残る障害注入スイッチ（存在しない）
- `docs/SWIFTPDF_V1_GUI_ACCEPTANCE_PREP.md` の検証者プロファイル絶対パスは、コミット前に `%LOCALAPPDATA%` 表記へ置き換えた

## 14. 残存リスク（解決済みにしない）

1. 全体並列試験 46 回中 1 回の `WRITE_ERROR`／`Verify`。原因未確定。成功率が上がっても原因確定とはしない
2. E4／E5 の Windows 実GUI試験は未実施
3. PDF 注釈内の画像バイト再ハッシュは未対応。PNG SHA-256 は準備画像と監査イベントの対応だけである
4. 重複 `/NM` の直接障害注入は未実施（公開 API が無い。重複時は hold）
5. 大容量ストアでは処理が数秒から約 10 秒になる
6. ストア上限は 64MiB。履歴は追記専用。64MiB 実体試験は ignored
7. 配置矩形は再照合に使っていない
8. 単一起動ソケットはポータブルでも既知フォルダー名を使う。隔離ランチャーが子の `LOCALAPPDATA` を向ける前提
9. `commit` が `Ok` を返した経路は、SealWork 中に文書を直接 undo しない前提（UI ではできない）

## 15. 開発ブランチへのコミット可否

**可。** STEP 118 で対象ブランチへコミット・push する。`main` マージ、タグ、Release、MSI はしない。

## 16. v1.0.0 公開可否

**未承認。** 並列 I/O 未解決、E4／E5 未実施、PDF 内画像の再ハッシュ未対応。一般公開しない。

## 17. 次の工程の提案

1. 開発ブランチのレビュー
2. 並列 I/O は無制限再試行ではなく、失敗時の OS コード記録を別途
3. E4／E5 が必要なら、117K と同じ隔離 exe で人が確認する
4. `main` マージ、タグ、GitHub Release、MSI は、公開判断の明示指示のあと

## 改訂

- 2026-10-24：STEP 117K 初版。自動回帰とコミット整理。実GUIは未確認。
- 2026-10-24：STEP 118。K1〜K7 を利用者 OK として記録。プロファイルパスを伏せた。既知リスクは解決済みにしない。
