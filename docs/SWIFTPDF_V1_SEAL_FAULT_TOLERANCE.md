# SwiftPDF v1.0.0 STEP 117H — SWSEAL2 耐障害性検証

状態：**検証工程。製品の保存契約は維持。Class C（置換後の応答喪失）では、窓が PDF を Undo し、ストアへ `use_undone` を 1 行足して一致させる。二重の commit は起きない。`use_status` で成功確定はしない。製品コードは修正していない。並列 I/O 1/46 は未解決。v1.0.0 の公開は承認しない。**

改訂：2026-10-24（117H）。`ROADMAP.md` は変更していない。

検証 exe は再ビルドしていない。実 GUI 確認に使ったバイナリは `D:\dev\SwiftPDF-seals-validate\step117e\SwiftPDF.exe`（SHA-256 `F5FA5862EB596C823D3161C0475D8B6ED60DE78638B643E9CDBC807F2C768BBD`）。`step117e` は削除も上書きもしていない。

通常の `%LOCALAPPDATA%\SwiftPDF`、利用中の `seals.bin` / `seals.v2.bin`、実理事長印、実業務 PDF には触れていない。合成印影と一時フォルダーのみ。

---

## 1. 保存処理の実際の順序

`SealVault::apply` → `store` の実順。依頼書の 1〜12 と一致しない箇所は、次のとおり実順に並べた。

1. フォルダー解決（必要なら作成）
2. `seals.bin.lock` を `LockFileEx` で取得（上限は既定約 3 秒。失敗は `Busy` またはフォルダー I/O）
3. ロック下で最新ストアを読む（`open_locked`：`seals.v2.bin`、必要なら移行の続き、後書きされた format-1 の退避）
4. メモリ上で操作を適用する（generation / seal revision / `op_id` / `step_id` / 使用状態）
5. 変更がなければ（かつ退避行がなければ）書き込まずに返す
6. `generation = current + 1`。新しい行の `seq` をその世代にする
7. `store`：
   1. スナップショット検証
   2. JSON 化 + 現在ユーザーの DPAPI 暗号化 + `SWSEAL2\n` 付与（`encode`）
   3. 予算（64 MiB と未完了押印の予約）の確認
   4. テスト専用 `Point::BeforeWrite`
   5. 既存ファイルなら `write_atomically`（一時ファイル作成 → 書き込み → `sync_all` → 対象へ rename）。初回は `write_new_file` / `create_new`
   6. 共有違反（Win32 32/33）と `PermissionDenied` だけ、約 2 秒まで再試行
   7. テスト専用 `Point::AfterWrite`
   8. 書き戻し後の再オープン（`read_opt`）
   9. バイト一致、復号、スナップショット一致
8. 初回作成なら `seals.bin` を墓標にする
9. ロック解放（ガードの drop）
10. ワーカーが `Result` を窓へ返す。窓は `finish_seal_*` で通知する

依頼書の「generation 確認 → イベント更新 → DPAPI → 一時ファイル」は、ロック取得と最新読み込みの**後**である。DPAPI は一時ファイル作成より前である。

ロック範囲は手順 2 から 9 まで。`use_status` も同じロックで読むだけである。

---

## 2. ロック範囲

| 保持するもの | 保持しないもの |
|---|---|
| 1 回の `apply` / `migrate` / `state` / `use_status` | 窓のワーカー待ち、PDF 編集、ユーザー入力 |
| 読み → 変更 → 書き → 読み戻し | 別操作のキュー |

別の `SealVault`（別プロセスを含む）は、同じ `seals.bin.lock` を待つ。待ち切れは `Busy`。ファイルはそのまま。入力は呼び出し側が持つ。

古いメモリ上のスナップショットで上書きすることはない。毎回ロック下で読み直す。

---

## 3. 故障注入位置

製品ビルドのフックは no-op である。公開スイッチはない。

| 位置 | 手段 | 区分 |
|---|---|---|
| ロック取得前・待機中 | 別 vault が `BeforeWrite` でロック保持 | A |
| 最新読み込み | 壊れた v2 / 墓標のみ | A |
| DPAPI 暗号化 | `encode` 内。`BeforeWrite` より前。Windows 現在ユーザーでは注入しない | A（コード順） |
| 一時ファイル作成〜置換直前 | `Point::BeforeWrite`（ファイル未タッチ） | A/B |
| 一時ファイル書き込み途中 / flush / rename | `pdfcraft_platform::staging::write_atomically`。失敗時は staging を消し、対象を残す | B |
| 置換処理の失敗 | 上に同じ。呼び出しは `WRITE_ERROR` | B |
| 置換成功直後 | `Point::AfterWrite` | C |
| 読み戻し失敗 | `AfterWrite` で削除 → `verify_missing` | C |
| Verify 不一致 | `AfterWrite` で最終バイトを反転 | C |
| ロック解放・ワーカー通知 | `AfterWrite` のあと `apply` が `Err` を返す。窓は `finish_seal_*` | C |

`FAIL_SEAL_WRITE`（format-1）は v2 の `store` では使わない。

---

## 4. 書き込み前失敗（区分 A）

対象：ロック失敗、古い revision、`op_id` 競合、DPAPI、`BeforeWrite`、サイズ超過。

観測：

- 既存 `seals.v2.bin` のバイトは変わらない
- 監査行は増えない
- 呼び出しは `Busy` / `Conflict` / `Io` / `Full` / `Rejected`
- 入力は呼び出し側が保持する。自動再試行はない（共有違反の短い再試行を除く）

試験：`a_stop_before_write_leaves_the_file_and_the_attempt_unchanged`、`a_lock_held_by_another_vault_is_busy_and_the_waiter_writes_nothing`、既存の revision / Full / Unreadable 試験。

---

## 5. 置換前失敗（区分 B）

`write_atomically` は staging へ書き、閉じてから rename する。いずれかの失敗で staging を消し、対象は触らない。

`WRITE_ERROR` はこの失敗（および初回 `create_new` 失敗、JSON 化失敗）である。OS の生コードは製品の戻り値に残らない。テスト専用 `io_note` だけが記録する。

観測：`BeforeWrite` 停止後、ファイル・世代・使用状態は操作前のまま。staging 名は残らない。

別 vault がその後に承認を書く。同じ `op_id` の再試行は 1 行だけ追加する。B の変更は消えない。

---

## 6. 置換後失敗（区分 C）— 最重要

`write_atomically` が `Ok` を返したあと、`AfterWrite` / 再オープン / バイト比較 / 復号が失敗すると、呼び出しは `Io` または `Verify` を見る。ファイル側は新しいスナップショットである可能性がある。

ストア契約：

- 成功とは言わない
- 空ストアで置き換えない
- 再照合は `use_status(op_id)`
- 同じ `op_id` の `commit_use` 再試行は `Unchanged`（二重の `use_committed` なし）
- 着地済みの commit に対する `cancel_use` は `Conflict(UseState(Committed))`

窓の現行動作（製品未修正）：

- 準備の `Verify` / `Io` → `start_seal_abandon`。着地していれば `use_cancelled`。未着地なら cancel は競合として閉じる。PDF には未押印。
- 確定の任意エラー → `take_back_seal`（PDF を Undo）→ `Job::Abandon`。`cancel_use` が `Committed(InDocument)` なら、二重の commit や cancel は書かず `use_undone` を 1 行足す。ストアは `Committed(Undone)`、PDF は空。両方一致する。
- 通知は「押印を記録できないため、押印を取り消しました」。着地済みを「未完了の押印準備」とは言わない（abandon が `Ok` のため）。

再現：`a_landed_commit_reported_as_error_is_taken_back_and_recorded_as_undone`。

印影・履歴の消失、監査の二重追記、監査成功なしの押印、原本 PDF の破壊は、この経路では再現していない。残る設計上の選択は、置換後の応答喪失を「成功として残す」（`use_status`）か「取り消して一致させる」（現行）かである。最小修正案は §17。実装はしていない。

---

## 7. Verify 失敗の扱い

`store` が返す `Verify` の下位区分（テスト専用 `io_note`）：

| 記録 | 意味 |
|---|---|
| `verify_missing` | 再オープン時にファイルがない |
| `verify_read` | 再オープンの I/O 失敗（呼び出しは `Io`） |
| `verify_bytes` | 読んだバイトが書いたものと違う |
| `verify_decode` | 復号できたがスナップショットが違う |
| `verify_undecodable` | 復号または内容確認に失敗 |

製品の `VaultError::Verify` はこれらを区別しない。

117F の `a_processed_chairman_seal_commits_the_preview_png` の 1 回の `Verify` は、下位区分も OS コードもログに残っていない。**不明。推測しない。**

不一致のファイルは空ストアで置き換えない（`a_write_that_does_not_read_back_is_an_error_and_the_store_is_not_reset`）。置換後にファイルが消えた場合は `V2Missing` として扱い、空ファイルは作らない。

無条件の Verify 再試行は加えていない。

---

## 8. WRITE_ERROR の解析

返る箇所：

1. `encode` の JSON 化失敗
2. 既存ファイルの `with_retry(write_atomically)` が最終的に失敗
3. 初回の `write_new_file` / `create_new` / `create_new_plain` 失敗

`write_atomically` が触る API：一時ファイル作成、`write_all`、`sync_all`、`rename`。失敗時は staging を削除する。

117F の `a_stamp_whose_tab_or_page_changed_while_preparing_is_cancelled` の `WRITE_ERROR` は、原子的置換の失敗まで分かっている。Win32 の番号、一時ファイルの残存、適用の有無は、当時のログにない。117F の `io_note` は再現しなかったため発火していない。**番号は不明。推測しない。**

安全な再試行：

- 区分 B（置換前）で、同じ `op_id` を送り、`use_status` が未着地なら 1 回だけ
- 共有違反（32/33）と `PermissionDenied` は `with_retry` が約 2 秒だけ行う。それ以上の無条件リトライは承認していない

---

## 9. op_id・step_id の重複防止

| 操作 | 同一内容の再送 | 違う内容の同一 ID |
|---|---|---|
| `record_approval` | `Unchanged` | `OpIdInUse` |
| `prepare_use` | `Unchanged` | `OpIdInUse` |
| `commit_use` / `cancel_use` | 既にその終端なら `Unchanged` | `UseState` 競合 |
| `note_use_step` | 同じ `step_id` と同じ kind なら `Unchanged` | `StepIdReused` |

未確定（Approved / Prepared）を確定済みにはしない。`use_status` で再照合できる。

2 つの vault が同じ `op_id` を同時に送ることは、ロックで直列化される。先着が書き、後着は `Unchanged` または競合。

---

## 10. 複数プロセス障害試験

既存 `crates/ui-egui/tests/seal_store_v2_processes.rs`：

- 2 プロセスが各 20 承認 × 20 試行。40 行すべて残る。staging は残らない
- abort / 即終了したプロセスのロックは OS が解放する

`Point` と `set_hook` は `pub(crate)` のため、統合プロセス試験からは注入できない。製品面にスイッチは出していない。

同じフォルダー上の 2 vault（ロックはプロセス間と同じ）で注入した：

- A がロック中、B は `Busy`。B は書かない
- A が置換前に失敗、B が承認、A が同じ `op_id` で再送 → commit 1 行。B は残る
- A が置換後に応答喪失、B が承認、A が `use_status` のあと再送 → commit は増えない。B は残る

実 GUI の 2 窓は 117E で確認済み。今回の注入を GUI では行っていない。

---

## 11. 安全に再試行できる条件

1. `Busy` / `Full` / `Rejected` / 置換前の `WRITE_ERROR` で、`use_status` が未着地
2. 同じ `op_id` / `step_id` と、同じ内容
3. 利用者が明示的に再操作する
4. 共有違反の短い `with_retry`（実装済み）

---

## 12. 再試行できない条件（または先に再照合が要る条件）

1. `Verify` または置換後に見えうる `Io` — 先に `use_status`
2. 着地済み commit を、`use_status` なしで PDF Undo と `use_undone` にする現行（利用者が望んだ印は残らない）
3. 違う page / 画像 / 承認での同一 `op_id`
4. `Unreadable` / `V2Missing` — ファイルを作り直さない
5. 要復旧ドキュメントに対する保存・印刷・再押印
6. Verify の無条件リトライ

---

## 13. 追加した試験

ストア（`seal_store_v2_tests.rs`）：

- `a_stop_before_write_leaves_the_file_and_the_attempt_unchanged`
- `a_landed_commit_is_unknown_until_use_status_and_cannot_be_cancelled`
- `a_file_removed_after_replace_is_verify_and_is_not_replaced_with_empty`
- `two_vaults_a_lost_commit_reply_does_not_drop_the_other_event_or_duplicate`
- `two_vaults_a_stop_before_write_lets_the_other_change_and_retries_once`
- `a_lock_held_by_another_vault_is_busy_and_the_waiter_writes_nothing`

窓（`seal_flow_tests.rs`）：

- `a_landed_commit_reported_as_error_is_taken_back_and_recorded_as_undone` — 置換後の応答喪失を、PDF Undo と `use_undone` で一致させる現行を記録する

既存のまま使う：`an_unknown_outcome_is_rechecked_by_op_id_and_not_recorded_twice`、`a_write_that_does_not_read_back_is_an_error_and_the_store_is_not_reset`、`use_lines_follow_their_states_and_retries_record_once`、`seal_store_v2_processes`、要復旧の `a_stamp_that_can_neither_be_recorded_nor_taken_back_holds_the_document`。

---

## 14. 自動試験結果

環境：`CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`、`CARGO_BUILD_JOBS=2`、`CARGO_TARGET_DIR=D:\dev\SwiftPDF-seals-validate\step117a\target`。`--test-threads=1`。

| ゲート | 結果 |
|---|---|
| `cargo fmt --check` | 合格 |
| `cargo check -p pdfcraft -j 2` | 合格 |
| `cargo clippy -p pdfcraft-platform --all-targets -j 2 -- -D warnings` | 合格 |
| `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings` | 合格 |
| `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings` | 合格 |

| 試験 | 結果 |
|---|---|
| `seal_store_v2::tests` | 36 合格、0 失敗、2 無視（巨大ファイルの計時） |
| `seal_flow::tests` | 22 合格、0 失敗、1 無視（受け入れ fixture） |
| `seal_store_v2_processes` | 3 合格、0 失敗。2 プロセス × 20 承認 × 20 試行はすべて 40 承認が残った |
| 117H の注入・2 vault・要復旧を含む焦点 12 件 | すべて合格 |

全ワークスペースの長時間試験と、描画の並列は回していない。1/46 の並列失敗は再現していない。原因確定とはしない。

---

## 15. 残存リスク

1. **Class C の窓経路**（§6）。置換後の `Verify` / `Io` を、`use_status` で成功確定せず、PDF と監査の両方を取り消す。一致は保つが、利用者が望んだ押印は残らない
2. **並列 I/O 1/46**（117F）。今回も再現していない。原因未確定
3. E4 / E5 の実 GUI は未実施（自動試験のみ）
4. `WRITE_ERROR` / `Verify` の OS 番号と下位区分は、過去ログからは不明
5. ロックを使わない外部書き込み（別ツール）は、Verify で拒否するが内容は回復しない
6. 置換後に第三者が v2 を消すと `V2Missing`。空ストアは作らない

データ消失（ストアが有効な v2 を空で置き換えること）は、今回の注入では再現していない。監査の二重追記も、ストア API では再現していない。

---

## 16. v1.0.0 の公開判断

**公開しない。**

理由：並列 I/O 1/46 が未解決。E4/E5 の実 GUI がない。Class C は監査と PDF を矛盾させないが、着地済み確定を取り消す現行のままである。117G の G1〜G5 と 117E/F の 4 項目は実 GUI で OK だが、それだけでは足りない。

---

## 17. 次工程への提案

製品を広範囲に直さない。判断待ちの最小案：

1. `finish_seal_commit` の `Err(Verify | Io(_))` で、直ちに `take_back_seal` しない
2. 同じワーカーで `use_status(op_id)` する
3. `Committed(InDocument)` なら成功として扱う（PDF の印を残す）
4. `Prepared` なら現行どおり take-back + cancel
5. `use_status` 自身が `Verify` / `Unreadable` なら take-back せず `hold_seal_doc`
6. 通知文を状態に合わせる。「未完了の押印準備」と着地済み commit を混同しない

実装は別途承認を得ること。共有違反の追加リトライも、確定条件があるときだけ設計する。

117G の実 GUI（G1〜G5）は受け入れ済みとして保持する。1/46 は再現するまで原因確定と書かない。
