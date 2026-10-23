# SwiftPDF v0.9.0 ページ回転の保存基盤

実装日: 2026-10-22。ブランチ `feature/page-rotate-v0.9.0`（基準 `d6c8f8084ef43445b46f39e5fc93f026e30621b1`）。

GUI の `page.rotate` / `page.rotate_ccw` は有効にしていない。削除、並べ替え、抽出も有効にしていない。`set_view_only(false)` は呼んでいない。

## 1. 設計方針

ページは `ObjRef` で識別する。回転は同じページ辞書へ `/Rotate` を直接の整数で書く。表示用の `DocView::rotate_view` は使わない。

閲覧専用は維持する。新しい許可 `Capability::PageRotate` があるセッションだけが `Edit::RotatePages` を適用、取り消し、やり直しできる。アプリの起動時にはこの許可を出さない。テストと、後で GUI を開く処理だけが `Session::grant` する。

保存は既存の `save_edited_copy_bytes` を、履歴の種別がページ回転（と、許可があるときの追加テキスト、セッションスタンプ）のときだけ通す。種別は `Edit` から記録する。ラベル文字列が `"Rotate pages"` であることだけでは通さない。

同じ呼び出しで同じページ番号が繰り返されても、そのページは 1 回だけ回す。範囲外の番号は、どのページも書く前に拒否する。

本家 `19d138e951d4` から採ったのは、間接 `/Rotate` を解決して読むことだけである。あのコミットは重複インデックスを積み上げる。SwiftPDF は重複を 1 回にする。cherry-pick はしていない。

## 2. 変更箇所

| ファイル | 内容 |
|---|---|
| `crates/organize/src/lib.rs` | `rotate_pages` と `page_rotation`。間接参照と継承を解決し、重複ページは 1 回 |
| `crates/organize/src/tests.rs` | 間接参照、継承、重複、不正番号、4 回回転 |
| `crates/engine/src/lib.rs` | `HistoryKind`、閲覧専用の回転許可、署名拒否、別名保存 |
| `crates/engine/src/view_only.rs` | `Capability::PageRotate` とエンジン試験 |
| `crates/ui-egui/src/editing.rs` | コマンドは閉じたまま、新規ファイルだけへ書く試験 |
| `crates/ui-egui/src/stamp_preset.rs` | データー印が同じページに残る試験 |
| `crates/ui-egui/src/seal_store.rs` | 理事長印の承認ページが移らない試験 |

`command_policy.rs` の閲覧専用リストは変えていない。`ROADMAP.md` は変えていない。

## 3. 閲覧専用の制限方法

`Session::apply` は `Edit::RotatePages` を先に判定する。

- 署名フィールドに署名がある文書は `EditError::NotPermitted("page rotation on a signed document")`。閲覧専用がオフのセッションでも拒否する。
- 閲覧専用で `PageRotate` が無いときは `EditError::ViewOnly`。
- 許可があるときは、従来どおり `Permissions::assemble()` を見る。無ければ `EditError::NotPermitted("page changes")`。
- `DeletePages`、`MovePages`、`extract` は、`PageRotate` があっても `ViewOnly` のまま。

`allows_page_rotate` は、閲覧専用、許可、編集可能な構造、未署名、assemble が許されているときだけ true になる。GUI はこの関数をまだ見ていない。

## 4. 回転操作と Undo／Redo

右 90° は `degrees: 90`、左 90° は `-90`。現在角度はページ自身の `/Rotate`、無ければ継承値。間接参照は 1 回たどる。結果は 0、90、180、270 に正規化し、ページ辞書へ直接の整数で書く。共有の角度オブジェクトは書き換えない。

| 現在 | 右 90° | 左 90° |
|---|---|---|
| 0° | 90° | 270° |
| 90° | 180° | 0° |
| 180° | 270° | 90° |
| 270° | 0° | 180° |

成功すると undo に操作前の文書と `HistoryKind::PageRotate` が 1 ステップ積まれる。ラベルは依頼したページ数から `"Rotate page"` または `"Rotate pages"` になる。やり直しはそのステップを戻す。追加テキストを取り消しても回転は残る。回転を取り消しても、同じページのスタンプは残る。

## 5. 保存方式

`save_edited_copy_bytes` は `SaveAs` と、履歴に含まれる種別ごとの許可を要求する。回転だけの履歴には `PageRotate` が要る。追加テキストが履歴にあるときは `EditText` も要る。削除や並べ替えの種別 `Other` は拒否する。

バイト列は `working_bytes` の増分更新である。開いたファイルのバイト列を先頭に残し、新しい `/Rotate` を後ろの改訂に書く。全書き出しにはしない。ページを消さないので、前の改訂に残るのは回転前のページ木であり、削除データの残存とは別である。

書き込みは `write_new_file` のままである。原本と同じパスは `SameAsOriginal`、既存ファイルは `DestinationExists`。失敗しても原本は変わらない。`save_bytes`（上書き保存）は閲覧専用のまま拒否する。

## 6. 署名・権限保護

署名の有無は適用前に `Document::is_signed` で見る。これは「署名フィールドに署名本体が入っている」ことである。署名一覧が空のファイルは未署名として扱う。フィールドを持たないバイト範囲署名や、署名辞書を署名一覧へ載せられなかったファイルは、この拒否の対象外である。構造を読めず `read_only_reason` がある文書は、回転許可があっても `ReadOnly` になる。

権限は assemble ビットである。`Changes::None` の保護は拒否する。所有者パスワードで制限を外す経路は追加していない。閲覧できることだけでは回転できない。

署名済み文書を増分更新して署名を無効化したファイルは作らない。適用が拒否され、dirty にも undo にもならない。

## 7. 日本語テキストと印影

日本語の追加テキストは同じページの追加内容のまま残る。回転は内容ストリームを書き換えない。取り消しはテキストのステップと回転のステップを別々に戻す。保存して開き直すと、`/Rotate` と「東京」の両方が残る（craft-fonts があるとき。無いビルドではその部分だけ試験がスキップする）。

一般スタンプはエンジンのカスタム画像スタンプとして、同じページの注釈のまま残る。データー印は UI の日付スタンプで、回転後もページ 0 のスタンプのまま、承認履歴は空のままである。理事長印の使用履歴のページ番号は配置時の 1 始まりの番号のままである。回転は承認行を追加しない。別ページへ承認を移さない。印の承認許可（`Capability::Stamp`）は `PageRotate` を出さない。

配置後に回した印の見た目の画素比較は、この STEP ではしていない。注釈が同じページに残り、開き直しても残るところまでを確認した。

## 8. テスト結果

2026-10-22 に実行。`CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`、`CARGO_BUILD_JOBS=2`。

| コマンド | 結果 |
|---|---|
| `cargo fmt --check` | 成功 |
| `cargo check -p pdfcraft -j 2` | 成功 |
| `cargo clippy -p pdfcraft-organize --all-targets -j 2 -- -D warnings` | 成功 |
| `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings` | 成功 |
| `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings` | 成功 |
| `cargo test -p pdfcraft-organize --lib` | 128 passed、0 failed、0 ignored |
| `cargo test -p pdfcraft-engine --lib` | 308 passed、0 failed、1 ignored（`probe_edit_latency_on_a_large_signed_document`） |
| `cargo test -p pdfcraft-ui-egui --test content -- --test-threads=1` | 15 passed、0 failed、0 ignored（7.05 秒） |

追加した試験は次のとおり。すべて成功。

- `rotate_turns_each_page_once_and_reads_indirect_or_inherited_angles`
- `four_quarter_turns_return_to_the_starting_angle`
- `view_only_page_rotation_saves_a_new_file_and_refuses_everything_else`
- `signed_and_restricted_documents_refuse_page_rotation_before_it_changes_them`
- `page_rotate_grant_does_not_enable_the_command_and_writes_only_a_new_file`
- `a_date_stamp_stays_on_its_page_when_that_page_is_rotated`
- `rotating_the_page_does_not_move_a_chairman_approval`

Noto Sans CJK SC を埋め込まない警告は、AGENTS.md §1.1 の既知の警告である。新しい失敗は無い。結合グリッドと中国語 UI フォントの既知の失敗は、今回の試験対象にしていない。

## 9. 残るリスク

- 保存は増分更新である。最新の改訂の `/Rotate` が開き直したときの角度になる。前の改訂の古い角度はファイルの先頭に残る。
- 署名検出は署名フィールドに限る。検出できない署名形式は拒否できない。
- ページラベル、しおり、構造ツリーは回転では書き換えない。宛先は同じページオブジェクトを指したままである。
- 回転後の印影の画面上の位置を、画素で既存の「既に回転しているページへ置く」試験とは別に比較していない。
- `PageRotate` をアプリが許可しても、`page.rotate` コマンドは `command_policy` が拒否する。試験は `apply_edit` を直接呼んでいる。
- 本家が重複インデックスを 2 回回す挙動は取り込んでいない。

## 10. GUI 有効化に向けた次の課題

1. アプリが `Capability::PageRotate` を出すタイミングを決める。印の承認や `Stamp` 許可から出してはならない。
2. `command_policy` に `page.rotate` と `page.rotate_ccw` だけを足す。削除、並べ替え、抽出、整理グリッドは足さない。
3. 閉じるときの保存と Ctrl+Shift+S が、回転を含む履歴を `publish_new_pdf` へ渡すことを確認する。Ctrl+S は拒否のままにする。
4. 複数ページの選択（`target_pages`）でコマンドが同じ `Edit::RotatePages` を呼ぶことを、GUI を開いた試験で見る。描画を伴う試験は単一スレッドにする。
5. 署名済みと assemble 禁止のときのメッセージを、閲覧専用の汎用拒否と区別して表示する。
