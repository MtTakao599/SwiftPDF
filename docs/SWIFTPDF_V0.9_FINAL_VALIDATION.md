# SwiftPDF v0.9.0 最終検証（STEP 109）

状態: **停止。新規のテスト失敗が 7 件ある。コミット不可。**

実施日: 2026-10-22。ブランチ `feature/page-rotate-v0.9.0`。基準 HEAD `d6c8f8084ef43445b46f39e5fc93f026e30621b1`。

この手順では製品コードとテストを変えていない。追加したのはこの報告書だけである。`ROADMAP.md` と STEP 102〜108 の報告書は変えていない。

## 1. 検証対象と環境

- 対象: STEP 103〜108 の未コミット変更（38 ファイル、+1227 / −63）と、未追跡の `rotate_guard.rs`、`rotate_validation.rs`、報告書 7 件
- 環境変数: `CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`、`CARGO_BUILD_JOBS=2`
- Windows、rustc / cargo 1.96、`target` はリポジトリ内
- 開始前確認は想定どおりだった。ブランチ、HEAD、変更ファイル、未追跡 7 件が一致し、`git diff --check` は問題なし
- 再現用の合成 PDF とスクリプトはリポジトリ外の `D:\dev\SwiftPDF-rotate-validate\step109\repro` に置いた。全体テストのログは `D:\dev\SwiftPDF-rotate-validate\step109\workspace.txt`

## 2. STEP 103〜108 の変更概要

| STEP | 内容 |
|---|---|
| 103 | `Capability::PageRotate`、履歴種別 `HistoryKind`、`rotate_pages` の重複除去・間接と継承 `/Rotate`・範囲外の事前拒否、別名保存の対象に回転を追加 |
| 104 | 画像比較と内部構造の検証（製品変更なし） |
| 105 | `rotate_guard.rs`。署名の形がある文書の回転を拒否 |
| 106 | ガードの境界（`/ByteRange`、`/SubFilter`、古い改訂、xref 再構成） |
| 107 | 起動時に `grant_page_rotate()`。ページメニューから表示中の 1 ページだけを回す |
| 108 | 表示名、ツールチップ、コマンドパレット、21 言語の訳 |

## 3. 安全性コード監査

確認したこと:

- `set_view_only(false)` の追加は無い。起動時は `set_view_only(true)` のあとに既存 4 つの許可と `grant_page_rotate()` を呼ぶだけである。
- `command_policy.rs` で開くのは `page.rotate`、`page.rotate_ccw`、`edit.undo`、`edit.redo` だけで、どれも `PageRotate` の許可が条件である。`page.delete`、`page.extract`、`page.organize`、`page.insert`、`page.move`、`file.save` は閉じたままである（`editing.rs` のテストで確認）。
- GUI からの回転はすべて `PdfCraftApp::apply_edit` か `Session::apply` を通る。`Session::apply` は最初に `allow_page_rotate` を呼び、`is_signed`、`rotate_guard`、閲覧専用と許可、読み取り状態を順に確認する。権限は `check_permission` が assemble で確認する。
- `page.rotate` / `page.rotate_ccw` の実行は `views[i].current` の 1 ページだけを渡す。サムネイルの選択は使わない。
- 範囲外のページは `rotate_pages` が書き込み前に `NoSuchPage` で拒否する。重複したページ番号は 1 回だけ回す。
- 表示回転は `DocView::rotate_view` のままで、`/Rotate`、未保存状態、履歴に触れない。
- `apply_edit` 自身は UI 側の制限を持たない。`RotatePages` を作る他の UI（整理グリッドの各ページボタン、整理パネル、回転ダイアログ、カメラ補正）は、閲覧専用では `allows_assembly()` が偽なので押せない。またはコマンドが拒否される。
- `Edit::Batch` に回転を入れても、閲覧専用では `allow_view_only_stamp` が `ViewOnly` で拒否する。

低リスクの指摘（修正していない）:

- 「表示中の 1 ページだけ」は UI 側の制限である。`PageRotate` の許可があれば、エンジンは複数ページの `RotatePages` も受け付ける。署名ガードと権限の確認は、その場合も働く。
- 閲覧専用で回転を Undo / Redo するとき、`allow_page_rotate_step` は署名ガードを再確認するが、assemble 権限は再確認しない。その回転は適用時に権限を通過しているので、実害は確認していない。
- 閲覧専用でないセッション（CLI、自動化）では、Undo / Redo のガード再確認と、`Batch` 内の回転のガードが無い。製品の GUI は閲覧専用なので対象外である。

## 4. 署名・権限制限

`rotate_guard.rs` は次を扱う: `/FT /Sig`（継承を含む）の `/V`、カタログ `/Perms` の `DocMDP` / `UR` / `UR3`、`/Type /Sig` / `DocTimeStamp` / `SigRef` / `TransformParams`、`/ByteRange` キー（値の形を問わない）、`/SubFilter`（例外は、トレーラ `/Encrypt` の公開鍵方式名で `/ByteRange` が無い場合だけ）、読めない参照、古い改訂（64 まで）、xref 再構成。

Undo / Redo の回転は、閲覧専用では毎回ガードを再確認する。その経路での回避は見つからなかった。

暗号学的な署名検証は実装していない。拒否は「署名がある、または否定できない」という意味であり、署名が有効だという意味ではない。

**新しい問題（偽陽性。拒否する側に倒れている）:**

1. **xref の無い、または壊れた PDF:** 署名の無い PDF でも回転が拒否され、「電子署名がある可能性」の文言が出る。STEP 106 で決めた「改訂が空で修復記録がある場合は Indeterminate」による。閲覧専用でないセッションにも効くため、既存の 5 テストが失敗した（§9）。
2. **パスワード付き PDF で改訂が 2 つ以上:** 正しいパスワードで開いても回転が拒否され、同じ署名の文言が出る。古い改訂の切り出しをパスワード無しで開くため、読めない扱い（Indeterminate）になると推定している。改訂が 1 つなら回転できる。暗号化の無い PDF なら、改訂 2 つでも回転できる。

再現手順（`pdfcraft-cli run --script <file> --root D:\dev\SwiftPDF-rotate-validate\step109\repro`）:

| スクリプト | 内容 | 結果 |
|---|---|---|
| `a_noxref.json` | xref の無い 2 ページ PDF を開いて回転 | 拒否（署名の文言） |
| `c_plain.json` | 正常な 2 ページ PDF を開いて回転 | 成功 |
| `f_enc_inplace.json` | パスワード `pw` の PDF を開き、回転、上書き保存（改訂 2）、もう 1 ページ回転 | 4 手目で拒否 |
| `g_plain_inplace.json` | 暗号化無しで同じ手順 | 成功 |
| `h_enc3_reopen.json` | 上の改訂 2 のパスワード付き PDF を開き直して回転 | 拒否 |

SwiftPDF の別名保存は元のバイト列への増分更新なので、パスワード付き PDF を回して別名保存し、そのファイルを開き直すと、次の回転は拒否されると推定する。GUI の保存経路では確かめていない。

どちらも、許すべきでない回転を許す問題ではない。データ消失、原本の上書き、署名ガードの回避は無い。

権限制限の PDF（Changes なし）は、エンジンと UI のテストで拒否を確認した。文書は変わらない。

## 5. 原本・既存ファイル保護

保存経路（`crates/platform/src/save_path.rs`）は STEP 103〜108 で変えていない。`write_new_file` は書き込みの前後に原本と保存先を確かめ、ハードリンクで公開する。原本と同じパス（`SameAsOriginal`）と既存ファイル（`DestinationExists`）は拒否する。`publish_new_pdf` は書いたファイルを読み戻して照合する。

Ctrl+S は `file.save` で、閲覧専用では拒否される。終了確認の保存は `close_uses_save_copy` から別名保存へ進む。`save_bytes` は閲覧専用で拒否のままである。

## 6. 保存と Undo / Redo

`save_edited_copy_bytes` は、履歴の各段の `HistoryKind` で判定する。ラベル文字列では判定しない。追加テキスト、セッションのスタンプ、回転は、それぞれの許可があるときだけ保存できる。それ以外の履歴が 1 つでもあれば拒否する。

テストで確認したこと: 右 90°、左 90°、4 回で元に戻る、間接と継承の `/Rotate`、表示中の 1 ページだけ、Undo / Redo、表示回転と保存回転が別であること、回転と追加テキスト・スタンプの混在保存、別名保存、開き直し、保存失敗後の再試行、原本と既存ファイルが変わらないこと。

## 7. 日本語・印影との整合性

`seal_store.rs` と `stamp_preset.rs` の差分はテストの追加だけである。印影処理は変えていない。

テストで確認したこと: 回転しても理事長印の承認が別ページへ移らない。承認したページに色が残る。日付印がそのページに残る。回転は理事長印の承認を代行しない。テストは一時フォルダの `seals.bin` だけを使い、利用中の `seals.bin` は読み書きしていない。日本語の追加テキストは、`content` と `editing` のテストで回転後も保存・再表示を確認した。

## 8. 全体テスト結果

`cargo test --workspace --no-fail-fast -j 2 -- --test-threads=1` を 1 回実行した（約 69 分、終了コード 101）。

- 結果行 178（空の doc-tests を含む）
- 成功 3767、失敗 9、ignored 16
- 失敗したターゲット 7: `pdfcraft-engine --lib`、`pdfcraft-ui-egui` の `camera`、`combine_grid`、`fill_sign`、`fonts`、`ui`、`xtask --bin xtask`

主な統合テストの結果: `content` 15/0（1 スレッド）、`commands` 24/0、`view_only_commands` 8/0、`view_only_output` 9/0、`view_only_startup` 4/0、`view_only_launch` 1/0、`editing` 129/0（ignored 4）、`safe_save` 15/0、`print` 14/0、`swift_print` 4/0、`recovery` 12/0、`control` 50/0、`stamps` 4/0、`automation` 175/0、`rotate_validation` 4/0。

失敗したテストの単独再実行はしていない。原因は §4 の CLI 再現で確かめた。この報告の件数に二重計上は無い。

## 9. 既知の失敗と新規失敗の区別

| テスト | 区別 | 原因 |
|---|---|---|
| `combine_grid::arrow_keys_move_by_card_and_by_row_and_shift_extends` | 既知（v0.7 から） | ラベル `f8.pdf` が見つからない。差分外 |
| `fonts::chinese_ui_uses_an_installed_chinese_face` | 既知（v0.7 から） | `the app's own font`。差分外 |
| `pdfcraft-engine` `tests::saving_a_password_protected_document_rebases_on_it` | **新規**（STEP 105/106） | 上書き保存で改訂 2 になったパスワード付き PDF の 2 回目の回転が拒否される |
| `camera::dragging_a_corner_then_enhance_page_straightens_the_page` | **新規**（STEP 106） | xref の無いテスト用 PDF。補正後の回転が拒否され、寸法が 270×360 のまま |
| `camera::the_clean_up_level_enhances_again_from_the_photo_and_keeps_the_turn` | **新規**（STEP 106） | 同上 |
| `camera::the_level_leaves_the_page_alone_when_another_edit_came_in_between` | **新規**（STEP 106） | 同上。`Session::apply` が署名の文言で拒否 |
| `fill_sign::image_signature_preview_follows_pointer_at_page_size_with_zoom_and_rotation` | **新規**（STEP 106） | xref の無いテスト用 PDF。`apply_edit(RotatePages)` が偽 |
| `ui::a_middle_click_closes_the_tab` | **新規**（STEP 106） | xref の無いテスト用 PDF。回転が拒否され、タブが「(edited)」にならない |
| `xtask` `assets::tests::the_real_manifest_passes` | **新規**（STEP 108） | 訳を足した 21 カタログの SHA-256 が `ATTRIBUTION.toml` と合わない |

v0.8.1 の最終検証では、全体の失敗は既知の 2 件だけだった。新規の 7 件はすべて STEP 103〜108 の変更が原因である。テストを通すための変更はしていない。

STEP 105〜108 では対象を絞ったテストだけを実行し、ワークスペース全体は実行していなかった。そのため、この 7 件は今回初めて見つかった。

## 10. fmt・check・clippy 結果

いずれも終了コード 0。

- `cargo fmt --check`
- `cargo check -p pdfcraft -j 2`
- `cargo clippy -p pdfcraft-organize --all-targets -j 2 -- -D warnings`
- `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings`
- `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings`

Noto Sans CJK SC を埋め込まない警告は既知である（AGENTS.md §1.1）。

## 11. Windows GUI 受け入れ結果

以下はユーザーによる手動確認であり、自動テストではない。

`D:\dev\SwiftPDF-GUI-Test-v090-step108` で問題なしと報告を受けた項目:

- 右下ボタンの説明
- ページメニューの右 90°・左 90° の表示名
- コマンドパレットの表示名
- ページ回転の Undo / Redo
- ページ回転の別名保存

STEP 107 の GUI（`D:\dev\SwiftPDF-GUI-Test-v090`）で問題なしと報告を受けた項目:

- 現在ページの回転
- 日本語テキストの表示
- スタンプの表示

エージェントは画面を操作していない。

## 12. 未確認事項

GUI で確認されていない項目:

- 保存した PDF を開き直したときの角度
- 表示回転が保存に入らないこと
- 署名・権限制限の PDF での拒否表示
- 終了確認からの別名保存
- 回転後の印刷プレビュー
- 理事長印と回転の組み合わせ

その他:

- パスワード付き PDF を GUI で回して別名保存し、開き直したときの拒否（§4 の推定）
- 失敗した 7 件の単独再実行
- 実プリンターでの印刷（禁止事項のため実施しない）

## 13. 既知の制限

- 電子署名の暗号学的検証は未実装
- 64 を超える増分改訂は回転を拒否する
- xref 再構成時は回転を拒否する。署名の無い壊れた PDF でも、署名の可能性の文言が出る（§4）
- パスワード付き PDF で改訂が 2 つ以上あると回転を拒否する（§4。今回の新規）
- 保存方式は増分更新で、古い改訂が残る
- ページ削除、並べ替え、抽出は無効
- 表示中の 1 ページだけという制限は UI 側にある（§3）
- WARP 並列描画クラッシュは未解決。`content` は 1 スレッドで実行した
- 既知の UI テスト失敗 2 件（§9）
- `/AS` の PrintState は評価していない
- 本家 #3362
- `write_files` 以外の書き出し経路は未監査
- ファイルのシンボリックリンクは未検証
- 一部の未確定編集は保存用コピーの対象外
- テルグ語の回転文言は英語にフォールバックする

## 14. コミット可否

**現状ではコミット不可。** 新規の失敗が 7 件ある。

安全面の評価: 署名ガードの回避、閲覧専用の解除、回転以外の編集権限の開放、原本の上書き、データ消失、理事長印の承認の引き継ぎは見つからなかった。新規の失敗は、回転を拒否しすぎる偽陽性と、カタログのハッシュ記録漏れである。

次に進むには、ユーザーの判断が必要である。考えられる方向（今回は実施していない）:

- カタログ: 21 ファイルの訳の出所（このブランチで書いた、元の英語からの翻訳）を確かめたうえで、`cargo xtask assets --write` などで `ATTRIBUTION.toml` の SHA-256 を更新する。
- xref 再構成: ガードの xref 再構成の規則を閲覧専用のセッションだけに限るか、テスト用 PDF を直すか、現在の拒否を仕様として既存テストの期待を変えるか。最後の 2 つは既存テストの変更になる。
- パスワード付き PDF: 古い改訂を、開いたときと同じ鍵で読む方法を検討する。読めない改訂を未署名とは扱わないという STEP 106 の方針は保つ。

## 15. 最終 Git 状態

- ブランチ `feature/page-rotate-v0.9.0`、HEAD `d6c8f8084ef43445b46f39e5fc93f026e30621b1`
- `git diff --stat`: 38 ファイル、+1227 / −63（STEP 108 終了時と同じ）
- 未追跡: `crates/engine/src/rotate_guard.rs`、`crates/engine/tests/rotate_validation.rs`、`docs/SWIFTPDF_V0.9_*.md` 8 件（この報告書を含む）
- コミットと push はしていない

## 追記（STEP 110）

この報告書の新規失敗 7 件は STEP 110 で修正した。詳細は `docs/SWIFTPDF_V0.9_REGRESSION_FIX.md` にある。修正後の全体テストは 3779 passed / 2 failed / 16 ignored で、失敗は既知の 2 件だけである。再構成 PDF の扱いを変えたことについて、利用者の確認を待っている。
