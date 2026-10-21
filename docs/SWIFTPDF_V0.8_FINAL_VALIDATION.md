# SwiftPDF v0.8.0 最終検証

検証のみ。製品コードの変更、コミット、push、マージ、インストーラ作成は行っていない。

## 1. 検証日時・環境

- 日時: 2026-10-21 16:24 +09:00（テスト実行は同日 15:05 頃から）
- OS: Windows 10.0.26200
- rustc: 1.96.0 (ac68faa20 2026-05-25)
- フォント: `CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`
- ビルド: `CARGO_TARGET_DIR=D:\dev\SwiftPDF\target`、`CARGO_BUILD_JOBS=2`、cargo `-j 2`
- Noto Sans CJK SC Regular を埋め込まない警告は、AGENTS.md §1.1 の想定どおり
- 実業務 PDF、実印影、利用中の `seals.bin`、実プリンターは使っていない
- Windows GUI は起動していない

## 2. 対象ブランチ・コミット

- ブランチ: `feature/upstream-safety-v0.8.0`
- HEAD: `b9aa25c0d74dcd00fa598e352d747bcec7ee52d4`（作業開始時から不変）
- 作業ツリー: STEP 92〜95 の未コミット変更と、調査報告書 2 件
  - `docs/SWIFTPDF_FEATURE_AUDIT_V0.8.md`
  - `docs/SWIFTPDF_UPSTREAM_FIX_AUDIT_V0.8.md`
- この 2 件と `ROADMAP.md` は、今回編集していない
- 調査報告書は STEP 92〜95 を実装する前の記録である。実装後の状態は本レポートが正

## 3. 変更差分の監査

`git diff --stat`（本レポート作成前）: 10 files, +1289 / −74。`git diff --check` は空。

| ファイル | 対応 |
|---|---|
| `crates/ui-egui/src/files.rs` | STEP 92 出力名と `write_files` |
| `crates/ui-egui/src/lib.rs` | STEP 93 最前面プロンプト |
| `crates/ui-egui/src/dialogs.rs` | STEP 93 Enter / Escape |
| `crates/ui-egui/src/updates.rs` | STEP 93 更新ダイアログ |
| `crates/ui-egui/src/stamps_ui.rs` | STEP 93 押印確定の待機 |
| `crates/ui-egui/tests/ui.rs` | STEP 93 の画面重なり |
| `crates/engine/src/lib.rs` | STEP 94 印刷コピーの `/D` |
| `crates/engine/src/tests.rs` | STEP 94 |
| `crates/ui-egui/src/canvas.rs` | STEP 95 `anchored_scroll` |
| `ROADMAP.md` | 進捗 4 行 |

`Cargo.toml` と `Cargo.lock` は差分に無い。依存クレートの追加は無い。本家コミットの一括マージは無い。

呼び出し経路:

- 閲覧専用は維持されている。差分に `set_view_only(false)` は無い。既存の呼び出しは、今回触っていないテスト（`view_only.rs`、`detailed_view.rs`、`view_only_commands.rs`、`view_only_output.rs`）に限る。新規テストは `set_view_only(true)`。
- 原本の別名保存は `publish_new_pdf` のまま。`editing.rs` は差分に無い。
- `write_files` は名前を一括検査してから `write_new_file` する。既存のファイル、ディレクトリ、ジャンクションは書き始めない。
- 印刷は `allows_printing` と閲覧専用の `Capability::Print` を先に見る。その後、開いている文書ではなく印刷コピーの `/D` だけを書く。
- `commit_stamp_place` は、最前面が理事長印の確認以外なら戻る。承認時は SHA-256 を再照合する。DPAPI と履歴の実装ファイルは差分に無い。
- `PageXform` の変換関数は差分に無い。`canvas.rs` の追加テストが `PageXform` を組み立てているだけである。

## 4. STEP 92〜95 の検証結果

### STEP 92 出力パス

`output_names_are_one_file_in_the_chosen_folder` と `write_files` のテストは、ui-egui の lib 実行で失敗一覧に入っていない。

拒否を確認した名前: `../outside.txt`、`..\outside.txt`、ドライブレター、`C:outside.txt`、UNC、`/tmp/outside.txt`、代替データストリーム、`\\.\`、空、`.`、`..`、`CON` / `NUL` / `COM1` / `LPT9`、先頭・末尾の空白、末尾のピリオド、改行、U+202E、256 文字。受け入れる名前: `資料.pdf`、`報告書.txt`、`COM10.txt`、`Console`。Windows では `a.txt` と `A.txt` を同一視し、後続が失敗したときは今回作ったファイルだけ消す。

既存ファイルを読み取り専用にしても上書きしない。ディレクトリ名も置き換えない。ジャンクション `mklink /J` はテストが `status.success()` を要求しており、この lib 実行は通過した。中のファイルは残る。

ファイルシンボリックリンクは未検証。このセッションで `cmd /c mklink` は `You do not have sufficient privilege`（終了コード 1）だった。テストは作成に成功したときだけ中身を確認するので、権限不足ではその分岐をスキップする。

範囲外: 出力先フォルダ自体がジャンクションである場合は、名前検査の対象にしていない。`write_files` 以外（OCR、書き出し、印刷の一部、`editing.rs` の `write_atomically` など）は従来の書き込みのまま。

### STEP 93 ダイアログ

通過したテスト（抜粋）:

- `a_link_prompt_above_a_save_prompt_owns_its_keys`
- `a_dialog_above_a_save_prompt_keeps_the_prompt`
- `the_seal_effect_window_above_a_save_prompt_takes_escape`
- `processed_chairman_seals_ask_for_approval_and_a_direct_commit_without_it_is_refused`
- `enter_does_not_skip_seal_approval` 系（失敗一覧に無い）
- `chairman_seal_ui_undo_and_redo_in_both_modes`

`shortcuts` は最前面が保存確認のときだけその Enter / Escape を処理し、それ以外のプロンプトがあるときは文書の Ctrl+S / Ctrl+Shift+S に進まない。

決定的に失敗した lib テスト 5 件（`--test-threads=1` でも同じ）:

| テスト | 結果 |
|---|---|
| `chairman_seal_comes_back_on_redo_without_another_approval` | `ConfirmSeal` にならない（`seal_store.rs:846`） |
| `chairman_seal_redo_stops_safely_when_history_cannot_be_written` | 同じ |
| `scale_and_date_stamp_round_trip_without_touching_the_original` | 同じ（`stamp_preset.rs:1372`） |
| `a_processed_chairman_seal_commits_the_preview_png` | 押印されず toast も無い（`stamps_ui.rs:3692`） |
| `manual_rotation_commits_and_a_later_change_is_refused` | 未承認メッセージが `None`（`stamps_ui.rs:3949`） |

原因: `start_chairman_seal` が `Dialog::CreateStamp` を開く。`save_custom_stamp` はそれを閉じない。画面の OK は `dialogs.rs` で `close` のあと `save_custom_stamp` するので、通常操作ではダイアログは先に消える。この 5 件は保存関数を直接呼び、`CreateStamp` を残したまま `ask_stamp_confirm` / `commit_stamp_place` する。STEP 93 の `front_blocks_stamp_commit` は、最前面が理事長印の確認以外なら何もせず戻る。そのため承認画面も「承認されていないため、押印しませんでした。」も出ない。文書への押印は起きていない。

修正候補（未実施）: テストが UI と同じく、`save_custom_stamp` のあと `dialog = None` にしてから配置する。製品側で `save_custom_stamp` が自分でダイアログを閉じる案もある。画面の OK は既に先に閉じている。

Escape の残課題（コード確認のみ。GUI では未再現。未修正）:

- ダイアログが開いているとき、`shortcuts` は `canvas::shortcuts` の Escape に進まない。
- ページ上のテキスト編集は `document_area` の中にあり、`dialogs::show` より前に描かれる。`content_ui.rs` の編集欄はフォーカスや `modal_open` を見ずに `key_pressed(Escape)` で下書きを捨てる。ダイアログ側も同じフレームの Escape を見る。1 回で下書きの取消とダイアログの閉じが両方起き得る。
- 検索欄がフォーカスを持つときも、Escape を消費せず検索を閉じる。
- ダイアログが無いとき、`canvas::shortcuts` の Escape は `egui_wants_keyboard_input` を見ない。選択解除や検索閉じが、編集欄の Escape と重なり得る。
- ページ整理グリッドの Escape は `egui_wants_keyboard_input` を見ている。

原本上書きや未承認押印の迂回ではない。

### STEP 94 非表示レイヤー

engine の lib は 306 成功、0 失敗、1 ignored。失敗一覧に印刷レイヤーのテストは無い。印刷禁止と閲覧専用の拒否テストもそこにある。

`/AS` の PrintState は評価していない。hayro は `/D` だけを読む。印刷コピーの `/D` に `/OFF` を書くと SwiftPDF の印刷では隠れる。`/AS` は残す。`/AS` の印刷状態だけが OFF の、パネルでは非表示にしていないレイヤーは、この実装では印刷に残る。`/AS` を実装した別のビューアでは、`/OFF` に入れたグループが印刷で再び出ることがある。実プリンターには出していない。

### STEP 95 ズーム

`anchored_scroll` は両軸を `0..=content-viewport` に収め、中身がビューポートより小さいときは 0。呼び出しはズームアンカーのスクロールだけ。`PageXform` は変えていない。本家 #3362 のスクロール親は入れてない。

lib のズームテストは失敗一覧に無い。統合テスト `zoom_keeps_the_point_under_the_cursor_still` と `trackpad_pinch_zooms_the_pdf_under_the_pointer` は成功。高 DPI の実ウィンドウは見ていない。テストはスクロールを egui ポイントで計算し、`pixels_per_point` はラスタの倍率にだけ入る。

## 5. v0.7.0 回帰テスト結果

自動テストで成功を確認した範囲:

- PDF を開く、ページ移動、ズーム、別名保存、既存の保存先の拒否、印刷禁止の拒否は、engine と ui-egui の該当テストが失敗一覧の外
- 日本語 UI フォント `japanese_ui_text_uses_craft_fonts` は成功
- 一般スタンプとデーター印の登録・配置・倍率・透明度・Undo は、失敗した 5 件以外のスタンプテストが成功。`chairman_seal_ui_undo_and_redo_in_both_modes` は成功
- 署名済み PDF と注釈禁止の拒否テストは失敗一覧に無い
- 理事長印の「チェック無しでは押さない」テスト `processed_chairman_seals_ask_for_approval_and_a_direct_commit_without_it_is_refused` は成功

自動テストが最後まで到達しなかった範囲:

- 加工済み印影の確定、手動回転の確定と事後変更の拒否、履歴書き込み失敗時のやり直し、やり直しだけで再承認しないこと。これらは上記 5 件が `CreateStamp` の段階で止まっている
- データー印の往復テストも、末尾の理事長印確認で同じ理由により失敗

GUI での日本語入力、印影の見た目、保存後の再オープンは未実施。

## 6. 全テスト件数

公式の `test result:` 行だけを足した。推測で埋めていない。

| 実行 | 成功 | 失敗 | ignored | 備考 |
|---|---:|---:|---:|---|
| `cargo test --workspace`（ui-egui lib で停止） | 2678 | 5 | 12 | 93 バイナリ。ui-egui の統合テストより後は未実行 |
| `pdfcraft-xfa` / `pdfcraft-xfdf` / `xtask` | 144 | 0 | 0 | 61 + 34 + 49 |
| ui-egui 統合テスト 45 本 | 811 | 4 | 4 | `--test` を明示。`--no-fail-fast` |
| 合計 | 3633 | 9 | 16 | |

品質ゲート（workspace テストの前に完了）:

- `cargo fmt --check`: 差分なし
- `cargo check -p pdfcraft -j 2`: Finished
- `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings`: Finished
- `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings`: Finished

lib の 5 失敗は単スレッドでも再現した。並列の揺れではない。

統合テストの 4 失敗（いずれも今回の差分外のファイル）:

- `combine_grid::arrow_keys_move_by_card_and_by_row_and_shift_extends` — カードのラベルが見つからない
- `control::control_quit_retains_blocked_content_typing_after_cancel` — 下書き「Keep 世界」があるのに `ui.quit` が `{"quitting":true,"needs_confirmation":false}`。期待は確認ありで終了しないこと。`control.rs` は差分外
- `fonts::chinese_ui_uses_an_installed_chinese_face` — インストール済み中国語フォントの並びに Inter / JetBrains Mono が無い。`fonts.rs` は差分外
- `ui::files_and_quit_from_the_operating_system` — タブ数が 0（期待 1）。追加した 3 テストは成功。この関数は差分の外

`tests/content.rs` は `STATUS_ACCESS_VIOLATION`（0xc0000005）でプロセスが落ち、`test result:` が無い。11 件は ok を印字し、次の 3 件は結果が出る前に落ちた。

- `text_tool_deleting_a_draft_hides_the_old_added_glyphs_and_cancel_restores_them`
- `dragged_text_follows_the_pointer_before_release`
- `released_text_does_not_jump_back_before_the_page_is_redrawn`

このバイナリは合計に入れていない。`content.rs` は差分外。再実行はしていない。

ignored 16 件は、ネットワーク、corpus、タイミング計測、個人の中国語 PDF、Windows 証明書ストア、低 DPI ヒント計測、トリミング済み私有 PDF。環境やデータが無いためのスキップであり、失敗ではない。

doctest: `pdfcraft-ui-egui`、`pdfcraft-xfa`、`pdfcraft-xfdf` は 0 件で成功。他クレートの doctest は、最初の workspace 実行が lib 失敗で止まった時点で `Doc-tests` 行が一度も出ておらず、未実行。

`cargo test -p pdfcraft-ui-egui --tests` は lib も実行して 5 件で止まった。統合テストの件数は、その後 `--test` を 46 個指定した実行のものである。

## 7. Windows GUI 確認結果

起動していない。実施済みとはしない。普段の設定と `seals.bin` は読んでいない。

手動確認が残っている項目:

1. Ctrl+ホイールのズーム
2. 保存確認と設定画面の重なり
3. Ctrl+S / Ctrl+Shift+S
4. 理事長印の承認画面
5. 非表示レイヤーの印刷プレビュー
6. 日本語テキストの表示
7. 保存後の再オープン

使うなら、空のポータブル `Data` と合成 PDF / 合成 PNG。実印影、実業務 PDF、実プリンターは使わない。

## 8. 既知の制限・未対応事項

- `/AS` の印刷用途は評価しない。別ビューアと印刷結果がずれ得る
- 本家 #3362 のスクロール親は未取り込み
- ページ上の編集欄が開いていると、Escape がダイアログと下書きの両方に届き得る。未修正
- `write_files` 以外の書き出しは、新しい名前検査を通らない
- 増分しない別名保存は今回の対象外
- CID 0 の修正は未実施
- Windows のファイルシンボリックリンクは権限不足で未検証。ジャンクションはテストが通過
- 出力先ディレクトリ自体がジャンクションである場合は未検証
- 登録ダイアログを閉じない直接呼び出しでは、理事長印の承認も未承認の拒否メッセージも出ない（上記 5 件）
- ui-egui 統合テスト 4 失敗と `content` のアクセス違反は、原因を今回の差分とは断定していない
- 他クレートの doctest は未実行
- GUI 受け入れは未実施

## 9. リリース阻害要因の有無

閲覧専用の解除、原本上書き、既存ファイル上書き、権限チェックの回避、署名済み PDF の編集、未承認のまま印影がページに入ること、は差分と失敗内容からは見つかっていない。5 件の失敗は押印が完了したのではなく、確認に入る前に戻っている。

それでも次は、このツリーを緑のリリースとは扱えない。

- lib の 5 件は毎回失敗する。理事長印の確定、失効、履歴失敗、やり直しの自動確認がそこにある
- 統合テスト 4 件が失敗し、`content` が異常終了した。差分外だが、ベースライン比較はしていない
- GUI は未確認

## 10. v0.8.0 の完了可否

完了とはしない。コミットは推奨しない。

製品の安全境界を外す変更は見当たらない。一方、テストが赤のままでは STEP 93 の理事長印経路を検証し終えたとは言えない。先に、5 件が UI と同じく登録ダイアログを閉じてから配置するテスト修正で通るか、指示を待つ。製品コードは今回直していない。統合テストの 4 失敗とアクセス違反は、その指示と別に切り分ける。

## 11. 最終 Git 状態

HEAD は `b9aa25c0d74dcd00fa598e352d747bcec7ee52d4`。ブランチは `feature/upstream-safety-v0.8.0`。

本レポート追加後の `git status --short`:

```
 M ROADMAP.md
 M crates/engine/src/lib.rs
 M crates/engine/src/tests.rs
 M crates/ui-egui/src/canvas.rs
 M crates/ui-egui/src/dialogs.rs
 M crates/ui-egui/src/files.rs
 M crates/ui-egui/src/lib.rs
 M crates/ui-egui/src/stamps_ui.rs
 M crates/ui-egui/src/updates.rs
 M crates/ui-egui/tests/ui.rs
?? docs/SWIFTPDF_FEATURE_AUDIT_V0.8.md
?? docs/SWIFTPDF_UPSTREAM_FIX_AUDIT_V0.8.md
?? docs/SWIFTPDF_V0.8_FINAL_VALIDATION.md
```
