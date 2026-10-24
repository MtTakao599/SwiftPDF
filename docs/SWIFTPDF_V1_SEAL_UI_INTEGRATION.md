# SwiftPDF v1.0 理事長印 UI 接続（STEP 117C）

状態: SWSEAL2 ストア（`seal_store_v2`）を GUI に接続した。ストアの読み書きはすべてウィンドウのスレッドの外で行う。押印は「承認と準備の記録 → 文書の変更 → 確定の記録」の順に限る。検証は合成データと一時フォルダーだけで行った。コミットと push は行っていない。

| 日付 | 変更 |
|---|---|
| 2026-10-23 | 初版（STEP 117C） |

正とする設計は `docs/SWIFTPDF_V1_SEAL_STORAGE_SPEC.md`（117A）、`docs/SWIFTPDF_V1_SEAL_CONCURRENCY_DESIGN.md`、`docs/SWIFTPDF_V1_SEAL_STORE_IMPLEMENTATION.md`（117B）である。この文書は、UI 側の接続と、旧 UI からの変更点を記録する。

## 1. 接続前の UI の順序（コードで確認した内容）

| 段階 | 旧実装（SWSEAL1、117B まで） |
|---|---|
| 選択 | `QuickTool::ChairmanSeal(index)`。一覧の**位置**で印影を指す |
| 仮置き | `build_stamp_place` が `SealStore` の `index` 番目から PNG を読む |
| 承認画面 | `ask_stamp_confirm` → `Dialog::ConfirmSeal`。チェックで `approve_current_seal` が `SealApproval`（文書・ページ・矩形・透明度・プレビュー世代・画像 SHA-256）をメモリにだけ持つ |
| 確定 | `commit_stamp_place`。承認と画像 SHA-256 を照合し、**文書を変える前に** `confirmed` 行を同期で追記し、編集が失敗したら `pop_event` でその行を**物理削除**していた |
| 元に戻す・やり直し | `history_step` → `note_seal_history_step`。文書を先に変え、`undone`/`redone` 行を同期で追記。失敗したら文書を逆方向に戻す |
| 注釈の削除 | `DeleteAnnotation` の前に `removed` 行を書き、失敗時は `pop_event` |
| 名前を付けて保存・印刷・終了 | 印影ストアとの関わりは無い |

設計との差は次の 4 点で、いずれも 117C で解消した。

- 一覧の位置で選ぶため、別ウィンドウで削除・並べ替えがあると別の印影を指し得た。
- `pop_event` で監査行を物理削除していた。
- 書き込みがウィンドウのスレッドで行われ、ロック待ちや大きな復号の間は描画が止まった。
- 確定の行が文書の変更より前にあり、「行はあるが押印は無い」状態を `pop_event` で消していた。

## 2. 承認・押印の流れ

```text
仮置き（seal_ref = id, revision, sha256）
  → 承認画面（チェック → SealApproval をメモリに保持）
  → 確定
      同期の確認: 承認の一致 / 画像 SHA-256 / ストアが Ready / seals.matches(seal_ref)
      新しい op_id を発行
  → [ワーカー] record_approval + prepare_use     （ここまで文書は変わらない）
  → [ウィンドウ] 同じ文書・同じページ・同じ矩形か、署名や注釈禁止になっていないかを再確認
      → apply_edit(AddCustomStamp)
      → 新しく現れたスタンプがちょうど 1 つかを確認
  → [ワーカー] commit_use
  → 成功: SealUse { op_id, doc, オブジェクト番号 } を記録し、仮置きを消す
```

- 準備の記録に失敗したら、文書には触れない。
- 文書の変更に失敗したら、`cancel_use` で取り消す。
- 確定の記録に失敗したら、文書から取り除き（undo）、`cancel_use` で取り消す。
- 押印の対象は `op.doc`（文書 ID）と op_id で固定している。応答までにタブ・ページ・位置が変わったら押印せず、取り消しを記録する。

## 3. ワーカー設計（`seal_service.rs`）

- 1 回の操作ごとに `std::thread` を 1 本起こす。同時に動かすのは 1 件だけ。結果は `mpsc` で返す。新しいランタイムや依存は追加していない。
- ウィンドウは毎フレーム `poll_seals` で結果を確認し、操作を始めたときの `Flow` から続きを実行する。ワーカーは終わると `ctx.request_repaint()` でウィンドウを起こす。
- 読み込み（`Flow::Load`）以外はすべて「入力を止める操作」である。動いている間は `PromptLayer::SealWork` が最前面に出て、処理名と「完了するまで、文書の編集・保存・終了はできません。」を表示する。
- 止める操作が動いている間は、次をすべて断る（理由を表示する）。
  - 編集（`apply_edit`）
  - 元に戻す・やり直し（`history_step`）
  - コマンド（`execute_unguarded`）
  - 終了（`request_quit`、`quit`）
  - ウィンドウを閉じる操作（`guard_quit` が `CancelClose` を送る）
- 読み込み中でも、理事長印に関わる元に戻す・やり直し・削除は断る。
- 二重実行は `SealService::start` が拒否する。
- Busy（ロック待ちが 3 秒を超えた）は失敗として扱い、成功とは表示しない。入力は残し、利用者が明示的にやり直せる。
- ワーカーが応答せずに終わった場合は失敗（`Io`）として扱う。時間切れを成功とみなす経路は無い。

## 4. ID による選択

- 仮置き（`StampPlace`）は `seal_ref = { id, revision, sha256 }` を持つ。名前は表示用だけである。
- 一覧を読み直すと、`QuickTool::ChairmanSeal` の位置を ID で引き直す。削除されていれば選択を外し、「選んでいた理事長印は削除されました。」と表示する。
- 確定時に `seals.matches(seal_ref)` が偽なら、押印せず「仮置きの後に変更または削除されたため…仮置きをやり直してください。」と表示する。
- ウィンドウが古い一覧を持っていても、ストアが `record_approval` / `prepare_use` で revision と SHA-256 を照合して拒否する（`Conflict`）。このとき承認は消え、最新の一覧を表示する。
- 自動での再承認や自動での押印は行わない。
- 削除して同じ名前で登録し直した印影は新しい ID になる（試験で確認）。

## 5. 登録・変更・削除

| 操作 | 開始 | 成功時 | 失敗時 |
|---|---|---|---|
| 登録 | `begin_seal_register(draft)` | 新しい ID を選択状態にする | 登録画面を入力したまま開き直す |
| 倍率・加工の既定化 | `begin_seal_update(seal_ref, change)` | 自分の仮置きの revision を進める（画像は同じ） | 仮置きの入力は残す |
| 削除 | 確認画面の `seal_delete`（id, revision, sha256）→ `begin_seal_delete` | 仮置きを消す | 何も消さない |

- 一覧の再読み込みは「最新の内容を読み込む」で行う。処理中は登録・削除・再読み込みのボタンを無効にする。
- 失敗を成功と表示する経路は無い。成功の表示は、ストアが書き込みを確認した後だけである。

## 6. 競合時の表示

- 競合（`Conflict`）では、ストアが返した最新の一覧を表示し、何も上書きしない。
- 利用者の入力（登録画面の内容、仮置きの倍率など）は残す。
- 削除の競合では「削除しませんでした。最新の内容を表示しました。削除する場合は、もう一度選んで確認してください。」と表示する。
- 承認画面は、仮置きが最新の印影を指していないときに警告を出し、確定ボタンを無効にする。

## 7. prepare / commit / cancel

| 失敗した段階 | 文書 | ストア | 表示 |
|---|---|---|---|
| 準備の記録（書き込み失敗・確認失敗） | 変更しない | 開いた準備があれば `cancel_use` | 使用履歴を記録できないため、押印しませんでした |
| 準備の記録（競合・容量・Busy） | 変更しない | 何も書かない | 押印しませんでした + 理由 |
| 応答までに文書・位置が変わった | 変更しない | `cancel_use` | 押印する文書か位置が変わったため… |
| 文書の変更 | 変更されない | `cancel_use` | 押印は文書に入っていません |
| 新しいスタンプを特定できない | undo で戻す | `cancel_use` | 押印を確認できないため、押印を取り消しました |
| 確定の記録 | undo で戻す | `cancel_use`（既に確定済みなら `use_undone`） | 押印を記録できないため、押印を取り消しました |
| 確定の記録の後、undo もできない | 残る | 準備のまま残す | 押印を記録できず、文書からも取り消せませんでした…（op ID 表示） |

確定に失敗して取り除いたスタンプは `seal_orphans` に記録し、やり直しでは戻さない（やり直すとすぐに元に戻し、その旨を表示する）。

新しいスタンプの特定は、押印前後の「ページ上にある自分のスタンプ」の差で行う。エンジンは undo で空いたオブジェクト番号を次のスタンプに再利用するため、「最新のスタンプ」だけで判定すると、取り消した印影の後の押印を誤って失敗と判定していた（117C の検証中に見つけて修正）。

## 8. Undo / Redo の監査

監査行は追記だけで、物理削除しない。`pop_event` は削除した。文書を先に変え、その後に op ID と新しい step ID で行を書く。行を書けなければ、文書を逆方向に戻す。

| 利用者の操作 | 文書 | 追加される行（同じ op_id） |
|---|---|---|
| 確定 | スタンプを追加 | `use_approved`, `use_prepared`, `use_committed` |
| 元に戻す（スタンプを追加） | スタンプが消える | `use_undone` |
| やり直し（スタンプを追加） | スタンプが戻る | `use_redone` |
| 注釈の削除 | スタンプが消える | `use_removed` |
| 元に戻す（注釈の削除） | スタンプが戻る | `use_removal_undone` |
| やり直し（注釈の削除） | スタンプが消える | `use_removed` |
| 確定の記録に失敗 | スタンプを取り除く | `use_cancelled` |
| 記録に失敗した印影をやり直す | すぐに元に戻す | なし |
| 一般スタンプ・テキスト | 通常どおり | なし |

- 「取り消しました」「やり直しました」の表示は、行を書いた後にだけ出す。
- 元に戻した印影は、やり直しでしか戻らない。新しい編集でやり直しが空になった時点で、その印影の記録を保持一覧から外す（ストアの行は残る）。空いたオブジェクト番号を再利用した一般スタンプを、印影と取り違えないためである。
- undo の意味（何が戻るか）は変えていない。

## 9. 未完了の準備（`use_prepared`）

- 確定も取り消しもされていない準備は、パネルに件数と op ID を表示する。
- 別のウィンドウで押印中の場合もあるため、自動では取り消さず、削除もしない。自分が処理中の op は表示から除く。
- 成功としては扱わない。

## 10. 旧版の書き戻し

- 移行後に旧版が `seals.bin` に書いた場合、次の書き込みでストアがそのファイルを `seals.bin.legacy-g…bak` へ移し、`legacy_write_set_aside` 行（バックアップ名と SHA-256）を記録する。中身は取り込まない。
- ウィンドウは「以前の SwiftPDF が理事長印の旧ファイルに書き込んでいました。その内容は取り込まず、… として保管し、記録に残しました。」と表示する。
- 記録できなかった場合はストアがエラーを返し、操作は成功しない。

## 11. 移行ダイアログ

- 旧形式だけがある場合、状態は `NeedsMigration` になり、理事長印は使えない。読み込みでは何も作らない（空の v2 ファイルも作らない）。
- パネルの「移行について確認…」で `Dialog::SealMigrate` を開く。次の 6 点を表示する。
  1. 旧形式のデータの件数（理事長印、使用履歴）
  2. このバージョンで使うには変換が必要なこと
  3. 移行後は以前の SwiftPDF で使えないこと
  4. 元の暗号化ファイルは削除せず、バックアップとして残すこと
  5. 移行は一方向であること
  6. 延期でき、延期してもデータを変更しないこと
- 「移行する」でだけ移行する。「延期」は状態を `Deferred` にし、再読み込みしても延期のままである。
- 失敗したときは `NeedsMigration` に戻り、理由と「元のファイルは削除していません」を表示する。元のファイルはそのままである。

## 12. 大きなストア・Busy の表示

- 使用履歴が 1 万件を超えると、パネルに件数と「読み書きに時間がかかります」を表示する。
- 48 MiB を超えると、パネルに「上限に近づいています（x / 64 MiB）」を表示する。
- 上限で拒否された押印は、文書を変更せずに理由を表示する。
- Busy、競合、復号できない、未完了の準備、旧版の書き戻しは、それぞれ通知またはパネルに表示する。
- 処理中の表示は処理名だけで、経過秒数は出していない。

## 13. 試験結果

### 13.1 新規の試験（`crates/ui-egui/src/seal_flow_tests.rs`、15 件）

| 区分 | 試験 | 確認すること |
|---|---|---|
| A 状態 | `a_format_1_store_is_migrated_only_from_the_dialog_and_a_failure_keeps_the_original` | kittest でダイアログを操作。6 項目の表示、延期で無変更、失敗で原本が残る、移行するでだけ移行 |
| A 状態 | `an_unreadable_store_is_not_used_or_replaced` | 読めないファイルは使わず、置き換えない |
| B 押印 | `a_stamp_waits_for_its_prepared_line_and_holds_the_window_until_committed` | 準備の記録まで文書は変わらない。処理中の編集・コマンド・終了・二重実行を拒否。全行が同じ op ID と印影の revision・SHA-256 |
| B 押印 | `a_stamp_whose_tab_or_page_changed_while_preparing_is_cancelled` | 別タブ・別ページへの押印を防ぎ、取り消しを記録 |
| B 押印 | `a_failed_preparation_places_nothing` | 承認または準備の書き込み失敗で何も置かない |
| B 押印 | `a_failed_edit_cancels_the_prepared_attempt` | 文書の変更が失敗したら `use_cancelled` |
| B 押印 | `a_failed_commit_takes_the_stamp_back_and_redo_cannot_restore_it` | 確定の記録の失敗で取り除き、やり直しでも戻らない |
| B 押印 | `a_full_store_refuses_the_stamp_before_the_page_changes` | 容量上限で文書を変更しない |
| C 履歴 | `undo_redo_and_removal_add_lines_by_op_id_and_a_failed_line_puts_the_step_back` | 削除・元に戻す・やり直しの行、step ID が重複しない、行を書けなければ文書を戻す |
| C 履歴 | `a_number_freed_by_an_undone_seal_is_not_mistaken_for_it` | 空いたオブジェクト番号の再利用で、一般スタンプを印影と取り違えない |
| D 非同期 | `the_window_keeps_drawing_while_the_store_is_held_and_busy_keeps_the_input` | ロック保持中も 1 フレーム 1 秒未満で描画、終了を拒否、3 秒待って Busy、入力を保持、再試行で成功 |
| D 非同期 | `the_panel_shows_size_warnings_and_open_preparations_without_closing_them` | 件数・容量の警告、未完了の準備を op ID で表示、自動で閉じない |
| E 複数窓 | `two_windows_keep_each_others_seals_and_a_stale_change_is_refused` | 2 つのウィンドウの登録が両方残る。同名は競合で入力を保持。古い変更・削除は拒否 |
| E 複数窓 | `an_approval_from_before_another_windows_change_does_not_stamp` | 古い承認で押印しない。削除後に同名で登録した印影は別 ID |
| E 複数窓 | `a_late_format_1_write_is_set_aside_recorded_and_announced` | 旧版の書き戻しを退避・記録・通知し、取り込まない |

既存の理事長印の試験（`seal_store.rs`、`stamps_ui.rs`、`stamp_preset.rs`）は、SWSEAL2 と一時フォルダーで動くように移した。期待する結果は弱めていない。

### 13.2 試験が欠陥を検出できることの確認（意図的に修正を外した）

| 外した処理 | 失敗した試験 |
|---|---|
| やり直しが空になったときの保持一覧の整理 | `a_failed_commit_…`（一般スタンプのやり直しが拒否された）、`a_number_freed_…`（一般スタンプに `use_redone` が記録された） |
| 確定時の `seals.matches` | `an_approval_from_before_another_windows_change_does_not_stamp` |
| 準備後の文書・位置の再確認 | `a_stamp_whose_tab_or_page_changed_while_preparing_is_cancelled`（別の文書に押印された） |
| `request_quit` の処理中拒否 | `a_stamp_waits_…`、`the_window_keeps_drawing_…` |
| 確定失敗時の取り除き | `a_failed_commit_takes_the_stamp_back_and_redo_cannot_restore_it` |
| 押印前後の差によるスタンプの特定（旧: 最新のスタンプ） | `stamps_ui::effect_preview_tests::a_processed_chairman_seal_commits_the_preview_png` |

### 13.3 全体の結果

すべて `--test-threads=1 --no-fail-fast` で実行した。一時フォルダーは `TMP`/`TEMP` で検証用フォルダーに向けた。

| 対象 | 結果 |
|---|---|
| `pdfcraft-ui-egui` 単体（lib） | 583 合格、1 失敗、2 無視 |
| `pdfcraft-ui-egui` 結合試験（`seal_store_v2_processes` を含む全対象） | 既知の 2 件（`combine_grid` の矢印キー、`fonts` の中国語 UI）以外はすべて合格 |
| `pdfcraft-engine` | 全対象合格（単体 325、1 無視） |
| `pdfcraft-platform` | 24 合格 |
| `pdfcraft-render`（結合試験を含む） | 全対象合格（単体 300） |

lib の 1 件は `control::transport_tests::an_unauthenticated_connection_is_closed_at_the_deadline`。ローカルの制御チャネルが 3 秒以内に切断することを壁時計で測る試験で、全体実行の負荷の下で時間を超えた。単独で 3 回実行して 3 回とも合格した。理事長印のコードには触れない。

`cargo fmt --check`、`cargo check -p pdfcraft -j 2`、`cargo clippy -p pdfcraft-platform -p pdfcraft-ui-egui -p pdfcraft-engine --all-targets -- -D warnings` は、いずれも警告なしで通った。

## 14. 残るリスク

1. 平坦化・一括処理・保存せずに閉じる操作で印影が文書から消えても、記録しない（旧実装と同じ）。
2. 確定の記録に失敗し、さらに undo もできない場合、押印は文書に残り、準備は開いたままになる。通知と op ID の表示だけで、自動では直さない。
3. 監査行は作業中の文書の状態を表す。保存したファイルにその押印が含まれるかは記録しない。
4. フォルダーにある `seals.bin.legacy-*.bak` は、作った版にかかわらず記録の対象になる。
5. 読み込み中は、理事長印に関わる元に戻す・やり直しを Busy として断る。
6. 処理中の表示に経過秒数は無い。
7. 実際の GUI（`pdfcraft.exe`）は起動していない。通常のデータフォルダーに届くため。kittest と一時フォルダーで代わりに確認した。ウィンドウの × ボタンによる終了の拒否（`guard_quit`）は試験で再現していない。
8. 2 つのウィンドウの試験は同じプロセス内の 2 つのストアで行った。別プロセスの試験は 117B の `seal_store_v2_processes` による。

## 15. STEP 117D へ進めるかの評価

停止条件（記録の無い確定、古い承認での他の印影の押印、undo/redo と記録の食い違い、終了時のデータ消失、保存・原本保護の弱化、大きな形式変更、新しい試験の失敗、GUI 全体の作り直し）に当たる事象は無い。

117D へ進めると判断する。ただし、14 節の 2 と 7（undo もできなかった場合の扱い、実 GUI での × ボタン）は 117D の確認項目に含めることを推奨する。
