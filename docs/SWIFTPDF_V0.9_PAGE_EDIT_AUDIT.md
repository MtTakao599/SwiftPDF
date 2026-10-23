# SwiftPDF v0.9.0 ページ編集の調査

調査のみ。製品コード、既存テスト、`ROADMAP.md` は変更していない。機能は有効化していない。

## 1. 調査日時と対象コミット

| 対象 | 値 |
|---|---|
| 調査日 | 2026-10-22（JST） |
| SwiftPDF ブランチ | `feature/unsaved-text-v0.8.1` |
| SwiftPDF HEAD | `d6c8f8084ef43445b46f39e5fc93f026e30621b1` |
| 作業ツリー（調査開始時） | クリーン |
| 比較用ワークツリー | `D:\dev\swiftpdf-v070-baseline`（detached `b9aa25c0`）。未変更 |
| 本家ローカル | `D:\dev\PdfCraft-Original` は `9e5b70d1c231d7db558e278a2622b519ca063494`（2026-10-16、PR #2067）。SwiftPDF の分岐点と同じ。pull していない |
| 本家 GitHub HEAD | `ffaf312358e4f059e9c746004308e87583407a02`（committer 2026-10-22T00:30:58Z、「M6: Prepare a form … Calculation order」#3733）。`git ls-remote` とコミット API で記録した。オブジェクトは取得していない |

本家のページ操作コードは、分岐点 `9e5b70d1` の SwiftPDF ソースで読んだ。分岐後の整理機能は、GitHub のコミット一覧と `19d138e951d4` の差分だけで確認した。

## 2. ページ操作4機能の実装状況

4機能とも、分岐点のエンジンには実装がある。SwiftPDF は閲覧専用のまま、UI コマンドと `Session::apply` の両方で拒否する。表示だけの回転は別機能で、ページの `/Rotate` を変えない。

| 機能 | エンジン | SwiftPDF での到達 |
|---|---|---|
| 保存される回転 | `pdfcraft_organize::rotate_pages` がページ辞書へ `/Rotate` を書く | コマンド `page.rotate` / `page.rotate_ccw` は閲覧専用リストに無い。`apply` も `EditError::ViewOnly` |
| 削除 | `delete_pages`。全ページ削除は `WouldRemoveAllPages` | `page.delete` は同じ拒否。整理グリッドの `pending_edit` も `apply` に届いて失敗する |
| 並べ替え | `move_pages`。同じページオブジェクトを `/Kids` の順だけ変える | 整理グリッドと `Edit::MovePages`。コマンド許可も `apply` も拒否 |
| 抽出 | `extract_pages` が新しい `Document` を作る。元の `Document` は関数内で変更しない | `Session::extract` は `write_unchecked` が閲覧専用を拒否する。UI の「抽出後に元から削除」は元文書を変える |

表示回転は `DocView::rotate_view`（`crates/ui-egui/src/canvas.rs`）。`rotation` を 90° 刻みで回し、`Edit` を作らない。表示メニューはコマンド許可を通さずこの関数を直接呼ぶ。保存しても `/Rotate` は変わらない。ページ回転として有効化しない。

## 3. 本家との差分

SwiftPDF の整理エンジンは分岐点のままである。GitHub HEAD はフォームの計算順序（#3733）であり、ページ操作そのものではない。分岐後に `crates/organize/src/lib.rs` へ入ったページ関連は次のとおり（API で確認、未取り込み）。

| コミット | 日付 | 内容 | v0.9.0 |
|---|---|---|---|
| `eeadc79d1504` | 2026-10-18 | ページの逆順 `organize.reverse-pages`（#1720） | 対象外 |
| `19d138e951d4` | 2026-10-18 | 同じ呼び出しでページ番号が重なったとき、回転を現在値から積む。間接 `/Rotate` を `resolved_rotation` で読む | 回転を実装するときに、この挙動だけを分岐点の関数へ書き直す。コミットの cherry-pick はしない |
| `932bf9c6bfdd` | 2026-10-18 | 整理グリッドの Ctrl ドラッグでページを複製（#3092） | 対象外 |

分岐点の `rotate_pages` は、走査時点の辞書から `Dict::int(b"Rotate")` を読む。間接参照の角度は `int` では取れず、同じ配列に同じページが二度あると二度目も走査時の角度から計算する。通常の UI は `DocView::target_pages` が選択集合か現在ページを渡すので、同じ番号の重複は起きにくい。間接 `/Rotate` の取りこぼしは残る。

本家の上書き保存、整理グリッド全体、複製、逆順、ページの挿入、分割、結合は有効化しない。抽出ダイアログの範囲入力（調査時点の監査資料が未マージと記した PR #3750）は、この調査ではマージ状態を再確認していない。v0.9.0 の前提にしない。

## 4. SwiftPDF の禁止箇所

禁止は二層ある。

**UI。** `crates/ui-egui/src/command_policy.rs` の `VIEW_ONLY` に `page.rotate`、`page.rotate_ccw`、`page.delete`、`page.extract`、`page.organize`、`page.split`、`page.combine` は入っていない。テスト `editing_saving_printing_and_exporting_are_not_listed` がそれを固定する。`option_allowed_in_view_only("organize", "on")` も拒否する。`commands.rs` の `an_unsecured_pdf_opens_the_stamp_tool_without_a_security_refusal` は、閲覧専用でも `page.delete` が拒否トーストになることを確認する。

**エンジン。** `Session::apply` は、追加テキストとこのセッションのスタンプ以外を `allow_view_only_stamp` の既定枝から `check_not_view_only` へ送る。`Edit::RotatePages`、`DeletePages`、`MovePages` はそこへ落ちる。テスト `view_only_refuses_edits_and_leaves_the_document_alone` は `DeletePages` が `EditError::ViewOnly` で、バイト列もページ数も undo も変わらないことを確認する。`Session::extract` は新しい文書を `write_unchecked` で書き、その先頭が `check_not_view_only` である。同じファイルのテストが `extract` の `ViewOnly` を確認する。

`Document::allows_assembly` は `editable()` を要求する。`editable()` は `editor` があり、かつ閲覧専用でないことである。閲覧専用では、PDF 自身がページ編集を許していても `allows_assembly()` は false になる。

`set_view_only(false)` は使わない。閲覧専用を外すと、既存の `file.save` が元のパスへ戻る。

## 5. UI・エンジン・保存処理の呼び出し経路

### 回転（保存されるもの）

1. `commands.rs` の `page.rotate` が `Edit::RotatePages { pages: targets, degrees: 90 }`、`page.rotate_ccw` が `-90`。`targets` は `DocView::target_pages`（選択、無ければ現在ページ）。
2. 整理グリッド（`canvas.rs`、`organize_panel.rs`）も同じ `Edit` を `pending_edit` に置く。
3. `Session::apply` → `run_edit` → `rotate_pages`。
4. 角度は 90 の倍数だけ。現在の `/Rotate`（ページに無ければ継承）に加算し、`rem_euclid(360)` で 0、90、180、270 にする。結果はページ辞書へ直接の整数で書く。
5. 成功時、undo に `"Rotate page"` または `"Rotate pages"` が積まれ、redo は消える。`scope_of` の既定は `Scope::Full` なので、ページ一覧の作り直しに含まれる。

表示回転はこの経路に入らない。

### 削除

1. `page.delete`、整理グリッド、ページパネルが `Edit::DeletePages { pages }`。
2. `delete_pages` は全ページ削除を拒否する。残すページで `rebuild` し、`/Kids` と `/Count` を書き、継承属性を各ページへ下ろして `/Parent` を根にする。
3. ページラベルは `labels::drop_deleted_pages` が残ったページへずらす。
4. 消えるページのウィジェットをフォームから外す。しおり、残ったページのリンク、カタログの `/OpenAction`、名前付き宛先のうち消えたページを指すものを外す（`drop_destinations_to`、`drop_named_destinations`）。
5. 消えたページオブジェクトは `Document::free`。次の世代で空きになる overlay である。
6. undo ラベルは `"Delete page"` / `"Delete pages"`。UI は成功後にページ選択を空にする（`editing.rs`）。

確認ダイアログは、この経路には無い。削除は即 `apply` である。

### 並べ替え

1. 整理グリッドが `Edit::MovePages { pages, to }`。
2. `move_pages` は番号をソートして重複を除き、相対順を保ったまま `to`（残りの長さで切り詰め）へ挿入し、`rebuild` する。
3. ページのオブジェクト番号は変わらない。安定した識別子は、別のページ ID 型ではなく、この `ObjRef` である。
4. undo ラベルは `"Move page"` / `"Move pages"`。成功後、UI は選択を移動先の番号へ追従する（`editing.rs`）。
5. ページラベルはインデックス範囲のまま動かない。削除と違い、`drop_deleted_pages` は呼ばれない。

### 抽出

1. `page.extract` → `open_extract_dialog` → `extract_selection`（`files.rs`）。
2. `Session::extract` は `assemble` 権限を確認し、`extract_pages` で空の文書へ `import_pages_mapped` する。文書情報、PDF/X の識別、使っている XObject の剪定を行う。
3. 単一ファイルの抽出は `open_created` で未保存タブを開く。ページごとの抽出は `write_files_then`。
4. `extract_draft.delete` が真のとき、書き込み後に元文書へ `DeletePages` を掛ける。元文書を変えない、という SwiftPDF の抽出には使えない。
5. `write_new` は全書き出しのあと、開き直しとページ数の一致を確認する。`publish_new_pdf` ではない。

### 保存

v0.8.1 の安全な別名は `save_edited_copy_bytes` と `publish_new_pdf` である。`copy_history_ok` は undo と redo のラベルが追加テキスト（`"Add text"`、`"Edit content"`、`"Delete content"`）か、このセッションのスタンプだけであることを要求する。ページ操作のラベルはどちらでもない。ページ編集が入った履歴は、現行の別名保存が拒否する。

`working_bytes` は、改訂列が空、暗号化の変更、`full_save_required` のときだけ全書き出しになり、それ以外は `write_incremental` である。`delete_pages` と `move_pages` は `require_full_save` を呼ばない。現行の別名経路でページ削除を書くと、増分更新になる。

`file.save`（Ctrl+S）は閲覧専用のまま拒否する。本家の `save_bytes` をページ操作のために開かない。

## 6. Undo／Redo の整合性

`Session::apply` は成功時に操作前の COS を undo へ積み、redo を空にする。回転、削除、並べ替えはそれぞれ 1 ステップである。エンジンのテスト `failed_edit_changes_nothing` は、全ページ削除の失敗が dirty も undo も変えないことを確認する。

閲覧専用の undo / redo は、追加テキスト（`view_only_text_step`）と、このセッションのスタンプ（`allow_stamp_step`）だけを通す。ページ操作のラベルは通らない。

ページ操作を別名保存できるようにすると、同じ undo スタックに追加テキストとページ操作が混ざる。現行の `copy_history_ok` はその混合を拒否する。v0.9.0 で回転だけを許す場合は、履歴の許可を「追加テキスト、セッションのスタンプ、ページ回転」に広げるか、回転だけの一時セッションでバイト列を作り、成功後にだけ本セッションへ同じ `Edit` を適用する。失敗時は本セッションを変えない。v0.8.1 の「書いてから確定」と同じ順序にする。

削除や並べ替えの undo は、操作前のページ木ごと戻す。それはエンジンのスナップショットであり、アプリ側の理事長印の使用履歴（`SealUse.page`）は戻さない。ページ操作の undo で印の承認記録を別ページへ書き換えてはならない。

## 7. 原本・権限・署名の保護

原本は、ページ操作を有効にしても上書きしない。既存の保存先も上書きしない。成功した書き込みは `write_new_file` の新規ファイルだけにする。失敗とキャンセルは既存ファイルを変えない。

権限はエンジンが既に見る。`check_permission` は回転、削除、移動、挿入、複製、置換、しおり、ページ番号に `Permissions::assemble()` を要求し、無ければ `EditError::NotPermitted("page changes")` を返す。テスト `restricted_documents_refuse_changes_unless_opened_by_the_owner` は、印刷だけの保護文書で `DeletePages` と `extract` が拒否され、dirty にならないこと、所有者パスワードでは削除できることを確認する。SwiftPDF は所有者パスワードで制限を外す操作を、ページ編集の実装時にも追加しない。開いた時点の権限が assemble を拒む文書は、ページ操作ごと拒否する。

署名は、現行の拒否がページ操作を覆いきらない。`apply` が署名済みを拒むのは、その編集が新たに全書き出しを必要とするとき（`EditError::SignedRewrite`）と、改訂列が空のファイルを変更するときである。回転、削除、並べ替えは `require_full_save` を呼ばないので、閲覧専用を外した本家の経路では増分更新として通り得る。SwiftPDF では、署名済み文書のページ操作を、この判定に頼らず、適用の前に拒否する。抽出は元文書を変えないので、元の署名バイト範囲は抽出関数だけでは壊れない。抽出結果は別ファイルであり、元の署名の有効性を引き継ぐ、と表示してはならない。

## 8. 理事長印と日本語テキストへの影響

追加テキストとスタンプ（一般スタンプ、データー印、合成 PNG の理事長印）は、ページオブジェクトの内容ストリームまたは注釈として、そのページに付いている。

| 操作 | ページオブジェクト | テキストと印影 | 承認記録 |
|---|---|---|---|
| 回転 | 同じ `ObjRef`。`/Rotate` だけ変わる | 同じページに残る。表示上はページと一緒に回る。矩形を別ページへ移さない | `SealUse.page` は配置時のページ番号のまま。回転を新しい承認にしない |
| 削除 | `free` され、ページ木から外れる | そのページの内容と注釈は作業中の木から外れる | 消したページの承認を、残ったページへ付け替えない |
| 並べ替え | 同じ `ObjRef` が新しい `/Kids` 順に入る | オブジェクトに付いたまま、見た目のページ番号が変わる | 履歴のページ番号を、別ページが承認されたように書き換えない |
| 抽出 | 元のオブジェクトはそのまま。先の文書はコピー | 選んだページの注釈と内容がコピーされ得る | 抽出先のファイルに、承認済みの使用履歴を新しく作らない。見た目の印はコピーであり、新しい承認ではない |

配置時の向きは、既にエンジンが回転済みページを考慮している。`custom_image_stamp_keeps_displayed_orientation_on_rotated_pages`、`rubber_stamps_point_left_as_displayed_on_rotated_pages`、`typed_text_reads_across_as_displayed_on_rotated_pages`、`explicit_image_stamps_keep_displayed_orientation_on_rotated_pages`、`pdf_page_stamps_keep_displayed_orientation_on_rotated_pages`。これは「既に `/Rotate` があるページへ置く」試験であり、「置いたあとにページを回す」試験ではない。後者は v0.9.0 の回転試験で、合成フィクスチャに対して行う。

日本語の追加テキストは Type3 アウトラインである（`pdfcraft-edit` の `added_japanese_text_uses_type3_outlines_and_keeps_halfwidth_latin`）。回転は内容ストリームを書き換えない。削除はそのページごと外す。並べ替えは同じページオブジェクトを運ぶ。抽出は Type3 のグリフ資源を残す試験がある（`extract_keeps_images_type3_glyphs_may_draw` ほか）。ページ操作の undo ラベルが履歴に入ると、現行の別名保存は日本語テキストがあっても `copy_history_ok` で拒否する。回転を保存する実装は、この履歴条件を同時に設計する。

`seals.bin` は読んでいない。承認スナップショットのページ番号は `crates/ui-egui/src/seal_store.rs` の `SealUse.page` / `SealEvent.page`（`u32`）としてソース上で確認した。

## 9. ページ参照・PDF 内部構造のリスク

`rebuild` はページ木を一つの平坦な `/Pages` にする。各ページへ継承項目（`/Resources`、`/MediaBox`、`/CropBox`、`/Rotate` を含む継承キー）をコピーし、`/Parent` を根にし、根の `/Kids` と `/Count` を書き、根に残っていた継承項目を外す。テスト `deleted_pages_are_not_kept_by_what_points_at_them` は、全書き出しのあとで消したページの内容ストリームとフォームフィールドが残らないこと、残ったページのリンクとしおりの宛先が外れることを確認する。`deleting_the_opening_page_drops_the_open_action` は `/OpenAction` を確認する。

残るリスク:

- ページラベルは削除時だけ詰められる。並べ替え後は、ラベルが古いインデックスに付いたままになり得る。
- 宛先の除去は、しおり、残ったページのリンク、`/OpenAction`、`/Names /Dests` と古い `/Dests` を歩く。構造ツリー、アーティクル、JavaScript のページ参照までは確認していない。
- `move_pages` は宛先を書き換えない。宛先がページオブジェクトを指す限り、並べ替え後も同じページを開く。ページ番号で記憶しているアプリ側のデータ（印の使用履歴）は追従しない。
- 抽出は選んだページと、それが描く資源を新しいファイルへコピーする。元のページ木は変えない。抽出後削除を有効にすると、この性質は失われる。
- 増分更新で書いたファイルは、前の改訂に古い `/Kids` とページ本体を保持する。ビューアは最新のページ木を表示する。ファイルサイズとバイト列は、表示から消えたページを含み得る。

## 10. 増分保存と削除済みデータの残存リスク

`Document::free` は overlay に空きを記録する（`crates/cos/src/document.rs`）。`write_incremental` は元のバイト列を先頭に残し、変更分を後ろへ足す。コメントが述べるとおり、全書き出しが必要なのは改訂列が無いファイル、暗号化の変更、`full_save_required`（墨消しなど）である。ページ削除はこのフラグを立てない。

したがって:

- 画面と最新のページ木からページが消えること。
- 全書き出し（`write_full`）の新しいファイルから、そのオブジェクトが再出力されないこと。`full_roundtrip` を使う organize の削除テストは、この形で内容が残らないことを見ている。
- 増分更新のファイルから、前の改訂のバイト列まで消えること。これは現行の削除実装では保証されない。

利用者向けの文言は「削除したページは、新しいファイルのページ一覧に出ません」までにする。「ファイルから完全に消去した」「復元できない」とは書かない。削除を保存する実装では、新規ファイルを `write_full` で書き、増分更新を使わない。原本は触らないので、原本の中にページが残るのは仕様である。

回転はページを消さない。増分でも全書き出しでも、最新の `/Rotate` は保存される。テスト `encrypted_documents_open_edit_and_save_encrypted` は、保護された文書の増分保存のあとでページ 0 の回転が 90 であることを見ている。これは本家の編集可能セッションの結果であり、SwiftPDF の閲覧専用別名経路ではまだページ回転を書けない。

## 11. 4機能の比較評価

| 評価項目 | 回転 | 削除 | 並べ替え | 抽出 |
|---|---|---|---|---|
| 機能 | `/Rotate` を 90° 単位で合成 | 指定ページをページ木から外す | 同じページオブジェクトの順序を変える | 選んだページの新しい PDF |
| 本家実装 | `organize::rotate_pages`、`Edit::RotatePages`、`commands.rs` `page.rotate` | `delete_pages`、`Edit::DeletePages`、`page.delete` | `move_pages`、`Edit::MovePages`、整理グリッド | `import::extract_pages`、`Session::extract`、`files.rs` `extract_selection` |
| SwiftPDF の禁止 | UI コマンドと `apply` の閲覧専用。表示回転は別 | 同左。確認ダイアログは無い | 同左。グリッドも `apply` で拒否 | UI コマンドと `write_unchecked` の閲覧専用 |
| 既存テスト | engine の回転配置一式、`encrypted_documents_open_edit_and_save_encrypted`。間接回転の蓄積試験は分岐後の本家にだけある | `failed_edit_changes_nothing`、`cannot_delete_every_page_or_missing_pages`、`deleted_pages_are_not_kept_by_what_points_at_them`、`restricted_documents_refuse_changes_unless_opened_by_the_owner`、`view_only_refuses_edits_and_leaves_the_document_alone` | `move_pages_reorders` | `extract_keeps_images_type3_glyphs_may_draw` ほか。閲覧専用は `extract` が `ViewOnly` |
| 安全な別名保存 | 現行の `copy_history_ok` では拒否。履歴条件を広げる必要がある | 同左。さらに全書き出しが必要 | 同左。ページは消えないので増分でも表示順は保存できる | 抽出自体は新しいバイト列。`publish_new_pdf` ではなく `write_new`。閲覧専用では書けない |
| Undo／Redo | エンジンは 1 ステップ。閲覧専用の undo はラベルが通らない | 同左。失敗時はスタックを汚さない | 同左。選択の追従は UI | 元を変えない抽出には undo が要らない。抽出後削除は元の undo になる |
| 日本語テキスト | 同じページに残る | そのページと一緒に作業木から外れる | 同じページオブジェクトに残る | Type3 資源を残す試験がある。実日本語での抽出は未実施 |
| スタンプ・理事長印 | 同じページに残る。承認記録は動かさない | 印影はページと一緒に外れる。承認を他ページへ移さない | 印影はページオブジェクトに残る。履歴のページ番号は古いまま | 見た目はコピーされ得る。新しい承認記録は作らない |
| PDF 内部参照 | `/Rotate` のみ。ページ木は維持 | `/Kids`、`/Count`、宛先、フォーム、ラベル。構造ツリーは未確認 | `/Kids` の順。ラベルは古いまま | 新しい木。元の参照は維持。抽出後削除は削除と同じ |
| 実装難易度 | 中 | 高 | 高 | 中 |
| 推奨 | 条件付き採用。v0.9.0 の対象 | 条件付き採用。v0.9.0 には入れない | 条件付き採用。v0.9.0 には入れない | 条件付き採用。削除より先。v0.9.0 には入れない |

結合グリッドの既知の失敗（`combine_grid::arrow_keys_move_by_card_and_by_row_and_shift_extends`）は、エンジンの並べ替え試験ではない。整理グリッドを v0.9.0 で開かない。

## 12. 推奨実装順

安全性のため、最初の候補（回転 → 削除 → 並べ替え → 抽出）から抽出を前へ移す。抽出は元文書を変えない。削除と並べ替えはページ木を書き換える。

1. **ページ回転**（v0.9.0）
2. **ページ抽出**（元を変えない。抽出後削除は実装しない）
3. **ページ削除**（新規ファイルの全書き出し。完全消去とは言わない）
4. **ページ並べ替え**（オブジェクト同一性と、ラベル・印のページ番号の扱いが決まってから）

逆順、複製、挿入、分割、結合、整理グリッド全体、表示回転の保存化は、この順のどこにも入れない。

## 13. v0.9.0 の推奨開発範囲

v0.9.0 は **保存されるページ回転だけ** にする。4機能すべてを同じ版に入れると、別名保存の履歴条件、署名拒否、削除の残存データ、並べ替え後のラベルと印のページ番号が同時に回帰範囲になる。

### STEP A — v0.9.0 ページ回転

- 実装対象: 現在ページ、または複数選択に対する時計回り 90° と反時計回り 90°。既存の `/Rotate` と合成する。間接 `/Rotate` と、同一呼び出し内の重複インデックスは、分岐後の `19d138e951d4` の挙動に合わせてこの関数だけを直す。cherry-pick はしない。
- 実装しない範囲: 表示回転を保存扱いすること。180° 専用コマンド（90° の二回で足りる）。削除、並べ替え、抽出、整理グリッド、本家の上書き保存、`set_view_only(false)`。
- 変更が必要になるファイル（今回は変更しない）: `crates/organize/src/lib.rs`、`crates/engine/src/lib.rs`（閲覧専用の適用例外、署名拒否、`copy_history_ok` または一時セッション）、`crates/ui-egui/src/command_policy.rs`、`crates/ui-egui/src/editing.rs`、対応するテスト。メニューが既に `page.rotate` を呼ぶなら、コマンド本体の新規実装は不要である。
- 安全性: 閲覧専用は維持する。署名済みは適用前に拒否する。assemble が無い文書は拒否する。保存は新規ファイルのみ。原本と既存保存先は拒否する。失敗時に確定しない。追加テキストと回転が同じ履歴にあっても、閉じるときの保存が新規ファイルへ両方を書く。Ctrl+S は拒否のまま。
- 必要なテスト: 下記 §14 の回転行。既存の閲覧専用テストが、許可していない操作を拒否したままであること。
- 完了条件: 保存した新規 PDF を開き直すと `/Rotate` が合成結果と一致する。表示回転だけではバイト列が変わらない。原本のハッシュが変わらない。署名済みと権限制限はエラーになり、dirty にならない。undo で角度が戻る。

### STEP B — 抽出（v0.9.0 の次）

- 実装対象: 現在ページ、複数選択、連続範囲、非連続選択を、一つの新しい PDF へコピーする。日本語フォント資源、画像、スタンプ注釈を、既存の `extract_pages` がコピーする範囲で保持する。
- 実装しない範囲: `extract_draft.delete`。ページごとのフォルダ書き出し。分割。元タブを dirty にすること。
- 安全性: `Session::extract` の閲覧専用拒否を、新規ファイルへの書き出しに限って通す。元の `Document` は適用しない。assemble が無ければ拒否する。署名済みの元は変更しない。抽出先に承認履歴を追加しない。
- 完了条件: 元ファイルのハッシュが不変。抽出先のページ数が選択と一致し、選んでいないページの本文が無い。開き直しに成功する。

### STEP C — 削除

- 実装対象: 現在ページと複数選択。最後の 1 ページはエンジンの拒否をそのまま出す。確認ダイアログを付けてから `apply` する。保存は新規ファイルの `write_full`。
- 実装しない範囲: 原本への反映。増分更新での削除。「完全に消去した」という表示。構造ツリーの修復（未対応のまま、壊れた参照を新たに作らないことだけを試験する）。
- 安全性: 確認をキャンセルしたら dirty にしない。undo で作業中の木は戻る。承認は残ったページへ移さない。
- 完了条件: 新規ファイルのページ数が減り、全書き出しの再オープンで消したページの本文がオブジェクトとして出ない。原本は不変。最後の 1 ページは削除できない。

### STEP D — 並べ替え

- 実装対象: 選択ページを指定位置へ移す。ページオブジェクトの `ObjRef` を維持する。
- 実装しない範囲: ページラベルの自動振り直し（ラベルがある文書は、実装前に仕様を決める）。印の使用履歴のページ番号の書き換え。整理グリッドの全面有効化。
- 完了条件: 日本語テキストと印影が、移動したページオブジェクト上に残る。しおりが同じオブジェクトを開き続ける。履歴上のページ番号は配置時のままである。

## 14. 回帰テスト計画

実業務の PDF、実印影、利用中の `seals.bin`、実プリンターは使わない。フィクスチャはテスト内で合成する。GUI を描く試験は `--test-threads=1` だけにする。WARP の並列クラッシュは、この試験計画では直さない。

| フィクスチャ | 回転 | 削除 | 並べ替え | 抽出 |
|---|---|---|---|---|
| 通常の複数ページ | 1 ページだけ 90°。保存後に他ページは 0 | 中間ページ。ページ数が 1 減る | 先頭と末尾を入れ替え、現在ページが追従する | 1 ページ、連続、非連続 |
| 最初から `/Rotate` がある | 90 に 90 を足して 180。間接参照は STEP A の修正後に 180 になる | 回転ページを消したあとの残りの角度 | 回転ページがオブジェクトごと移る | 抽出先の `/Rotate` が維持される |
| 日本語の追加テキスト | 同じページに残り、角度だけ変わる | そのページを消すと本文も最新の木から外れる | 移動先の番号で同じ文字列が読める | 抽出先で同じ文字列が読める |
| 一般スタンプ | ページと一緒に回る。別ページへ増えない | ページと一緒に外れる | 同じ注釈が付いたまま | コピーされ、元の注釈は残る |
| 合成 PNG の理事長印 | 承認レコードのページ番号は不変。新しい承認行が無い | 残ったページが承認済みにならない | 履歴のページ番号を書き換えない | 抽出先に承認行を作らない |
| 目次・内部リンク | 宛先のページオブジェクトは不変 | 消したページへの宛先が外れる。残ったリンクは残る | 宛先が同じオブジェクトを指す | 選んだページへの宛先だけが意味を持つ |
| 注釈 | 注釈辞書が残る | 消したページの注釈が最新の木に無い | 注釈がページに付いたまま | 選んだページの注釈がコピーされる |
| 権限制限（assemble 無し） | 拒否。dirty にならない | 同左 | 同左 | 同左 |
| 電子署名済み | 適用前に拒否 | 同左 | 同左 | 元は不変。抽出を許す場合も「署名を引き継いだ」と表示しない |

各 STEP で、原本バイト列の一致、既存パスへの書き込み拒否、失敗時に undo が増えないこと、Ctrl+S が拒否のままであることを見る。表示回転の操作では保存バイト列が変わらないことも、回転 STEP の試験に含める。

既知の課題で、この計画に含めるものは次だけである。

- v0.8.1 の未確定テキストと別名保存。回転の履歴が、この保存を拒否する状態のまま出荷しない。
- 本家の増分更新。削除の保存には使わない。
- WARP。描画を伴う試験は単一スレッド。
- 結合グリッドの失敗と中国語 UI フォントの失敗は、整理グリッドとフォント試験を開かないので対象外。
- 本家 #3362 と `/AS` の PrintState は、ページ操作の経路と別である。同時に直さない。印刷プレビューを実プリンターへ出さない。`/Rotate` がレンダラのページ寸法に反映されることは、既存の `raw_crop_unit_floor_corners_match_nonzero_origin_and_rotations` が既に見ている。

## 15. 未確認事項

- 本家 GitHub HEAD `ffaf312` の作業ツリーは取得していない。`19d138e951d4` 以外のページ操作差分は、コミット題名の範囲でしか見ていない。
- PR #3750（抽出の範囲入力）の現在のマージ状態は、この調査では API で再確認していない。
- 構造ツリー、アーティクル、JavaScript アクションが、削除と並べ替えのあとでビューアを分岐させるかは、合成試験を書くまで未確認である。
- ページを回したあとの印影の見た目は、配置時の回転試験から推定している。配置後に回す試験はまだ無い。
- 並べ替え後のページラベルが、どのビューアでどう読まれるかは未実施である。
- 増分ファイルの前改訂に削除ページが残ることは、`write_incremental` の実装から言える。バイト列を残す回帰試験は、削除 STEP で追加する。
- 印刷プレビューが `/Rotate` と表示回転のどちらを使うかは、実印刷をせず、レンダラの既存試験の範囲に留めた。表示回転は `Edit` を作らないので、印刷がページ辞書を見る限り、表示回転は印刷結果に入らない。この結合は未実施である。
