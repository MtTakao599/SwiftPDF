# SwiftPDF v0.9.0 ページ回転 GUI

実装日: 2026-10-22。ブランチ `feature/page-rotate-v0.9.0`（基準 `d6c8f8084ef43445b46f39e5fc93f026e30621b1`）。

対象は、いま表示している 1 ページを右 90° または左 90° 回し、その結果を新しい PDF へ保存すること。ページの削除、並べ替え、抽出、挿入、移動は有効にしていない。`set_view_only(false)` は呼んでいない。

STEP 102〜106 の報告書は変えていない。`ROADMAP.md` も変えていない。

## 1. GUI の操作仕様

メニューは既存の「ページ」メニュー（`chrome.rs` の `registry_menu(..., "Pages")`）に出る。登録済みの `page.rotate` と `page.rotate_ccw` の表示名を、次に置き換えた。

| コマンド | 日本語 | 英語 |
|---|---|---|
| `page.rotate` | ページを右に90°回転 | Rotate this page 90° clockwise |
| `page.rotate_ccw` | ページを左に90°回転 | Rotate this page 90° counterclockwise |

ポインタを乗せた説明は、PDF に保存できる回転であることと、表示メニューの回転は見た目だけであることを書く。使えないときは、署名の可能性または権限の理由を、灰色の項目の説明に出す。

操作対象は表示中のページだけである。サムネイルで別のページを選んでいても、その選択は回さない。複数ページの一括回転は付けていない。`page.rotate_dialog` は許可しない。

成功すると、そのページの `/Rotate` が 90° 変わり、文書は未保存になる。Undo で戻し、Redo で戻せる。Ctrl+Shift+S と終了確認の「保存」は、v0.8.1 と同じ別名保存へ進む。

## 2. 既存の表示回転との違い

表示メニューの「Rotate view clockwise / counterclockwise」と、ページ欄の回転ボタンは `DocView::rotate_view` のままである。これは画面の向きだけを変え、PDF の `/Rotate` も未保存状態も変えない。

ページメニューの 2 項目は `Edit::RotatePages` を `Session::apply` へ渡す。保存した角度は、開き直したあとも残る。表示回転の角度は、この保存とは別の値として残る。

## 3. `Capability::PageRotate` の付与

起動時の `apps/pdfcraft/src/main.rs` は `set_view_only(true)` のあと、既存の別名保存、テキスト、印刷、スタンプに加えて `grant_page_rotate()` を呼ぶ。この関数は `Session::grant(Capability::PageRotate)` だけを行う。閲覧専用は解除しない。出力禁止も解除しない。

許可はセッションに対するものである。文書を開いた瞬間の判定を、その後の恒久的な許可にはしない。メニューが押せるかは、開いている文書に対する `Session::allows_page_rotate` が `Available` のときだけである。

`command_policy.rs` の閲覧専用リストには入れていない。リストへ入れると、許可が無くてもコマンドが通る。`page.rotate` は `file.save_as` や `edit.text` と同じく、許可があるセッションだけ `command_allowed` が真になる。許可が無いテストアプリでは、これまでどおり拒否される。

閲覧専用の文書は `allows_assembly()` が偽になる。メニューの有効判定は、そこを迂回して `allows_page_rotate` を見る。この判定は署名ガードを飛ばさない。

## 4. 署名と権限の判定

`Session::page_rotate_availability` は、次の順で文書を見る。

1. 文書が無い。
2. `is_signed` が真、または `rotate_guard::blocks_page_rotation` が真。どちらも署名の可能性として拒否する。暗号学的な検証はしない。
3. 編集用の構造が読めない、または読み取り専用。
4. 権限がページ変更（`assemble`）を許さない。
5. それ以外は利用できる。

`Unsigned` 以外は拒否する。空の署名フィールドと、値が明示的な null のフィールドは、STEP 105 のとおり回転できる。`/Filter` だけの画像ストリームと、`/Contents` だけの辞書は署名として扱わない。

利用者向けの文は、日本語では「電子署名があるか、その可能性があります。署名が有効かどうかは確認していません。」である。英語はエンジンの `PAGE_ROTATE_SIGNATURE` と同じ文で、検証できたとは書かない。権限不足は「この PDF の権限ではページを変更できません。」である。

拒否のあと、表示と、表示だけの回転は使える。

## 5. エンジンへの呼び出し

メニュー、ショートカット、コマンドパレット、操作チャネルは、どれも `PdfCraftApp::execute` に入る。

1. `command_allowed` が、`PageRotate` の許可を見る。削除、抽出、整理、挿入、`page.rotate_dialog`、`file.save` はここを通らない。
2. `command_enabled` が、開いている文書の `allows_page_rotate` を見る。
3. `execute` は表示中のページ番号だけを取り、`Edit::RotatePages { pages: vec![current], degrees: 90 または -90 }` を `apply_edit` へ渡す。
4. `Session::apply` が、変更の前に `allow_page_rotate` を再実行する。ここでも `is_signed` と `blocks_page_rotation` を見る。そのあと、従来どおりページ変更の権限を見る。
5. 拒否なら `/Rotate`、dirty、Undo、Redo は変えない。

`Capability::PageRotate` は、この再検査を省略するスイッチではない。自動化の `page_rotate` も `Session::apply` を通るので、同じガードに入る。

## 6. 保存と終了確認

回転は文書を dirty にする。終了確認は既存の `close_uses_save_copy` が dirty を未保存として扱うので、新しい保存処理は足していない。

- Ctrl+S（`file.save`）は拒否する。原本は変わらない。
- Ctrl+Shift+S（`file.save_as`）は新しい PDF の保存へ進む。
- 終了確認の保存も同じ別名保存である。
- 原本と同じパスと、すでにあるファイルは拒否する。
- 保存をキャンセルしても、回転と Undo / Redo は残る。
- 保存に失敗しても、Undo / Redo は残る。別の新しい名前で再試行できる。
- 保存した PDF は開き直せる。角度は残る。

## 7. Undo / Redo

回転は履歴に「Rotate page」として残る。右 90° の Undo は回転前、Redo は回転後へ戻る。左 90° も同じである。同じ向きへ 4 回回すと角度は 0 に戻る。

Undo / Redo は、テキストまたはスタンプの許可に加えて、`PageRotate` の許可でもコマンドとして通る。エンジン側は、閲覧専用の Undo で回転の履歴を戻すとき、署名ガードを再度見る。

確認した組み合わせは次のとおり。

- 日本語テキスト「東京」を入れてから回転し、Undo で回転だけを戻す。文字は残る。
- 回転してから一般スタンプを押し、Undo でスタンプだけを戻す。回転は残る。
- 一般スタンプのあとに回転しても、スタンプのページは変わらない。
- 日付印のあとに回転しても、印は同じページに残り、原本のハッシュは変わらない。
- 理事長印の承認履歴の件数とページは変わらない。回転は承認を追加しない。

## 8. 画面表示とキャッシュ

1 ページだけの回転に成功したときは、`DocView::page_geometry_changed` がそのページの高さだけを更新し、`page_changed` でそのページの画面、サムネイル、印刷プレビュー枠を古くする。他のページのキャッシュは捨てない。ズームは変えない。

Undo と Redo は、これまでの履歴処理のとおり文書全体の再描画になる。複数ページを一度に回す経路は GUI から使わない。使う場合は全体の再描画のままである。

印刷プレビューは、そのページの枠が古い印の付いたあとに、ページ情報の回転と寸法で描き直す。

## 9. テスト結果

環境: `CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`、`CARGO_BUILD_JOBS=2`。Noto Sans CJK SC を埋め込まない警告は、AGENTS.md §1.1 のとおり想定内。

| 確認 | 結果 |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo check -p pdfcraft -j 2` | exit 0 |
| `cargo clippy -p pdfcraft-organize --all-targets -j 2 -- -D warnings` | exit 0 |
| `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings` | exit 0 |
| `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings` | exit 0 |

| テスト | 成功 | 失敗 | ignored | 備考 |
|---|---:|---:|---:|---|
| `pdfcraft-ui-egui --lib` の回転・印・方針 | 10 | 0 | 0 | 530 件はフィルタで未実行 |
| `pdfcraft-engine --lib` のガードと閲覧専用回転 | 13 | 0 | 0 | 307 件はフィルタで未実行 |
| `pdfcraft-engine --test rotate_validation` | 4 | 0 | 0 | |
| `pdfcraft-organize --lib` の `rotate` と `page_rotation` | 5 | 0 | 0 | 2 回に分けて実行 |
| `pdfcraft-ui-egui --test view_only_commands --test-threads=1` | 8 | 0 | 0 | 許可の無いアプリではメニューが灰色 |
| `pdfcraft-ui-egui --test content --test-threads=1` | 15 | 0 | 0 | 7.19 秒。WARP の並列実行はしていない |

ワークスペース全体、`pdfcraft-ui-egui` のフィルタ無し一式、`ui_mode`、`simple_home`、コマンドパレットの統合テストは、この手順では実行していない。未実行の件数は成功に数えない。

`view_only_commands` は、許可を出していないアプリで「Rotate this page 90° clockwise / counterclockwise」が灰色であることを確認する。削除や抽出も灰色のままである。閲覧専用を外した既存テストは、表示中の先頭ページだけが 90° になることを確認している。

## 10. GUI 手動確認項目

画面を操作しての確認は、この手順では行っていない。隔離した実行ファイルは `D:\dev\SwiftPDF-GUI-Test-v090\SwiftPDF.exe` である。これはこの手順の debug ビルドで、リリース用の MSI や ZIP ではない。`portable.txt` は空のファイル、`Data` は空のディレクトリである。利用中の `seals.bin` は読んでいない。v0.8.0 と v0.8.1 の検証フォルダは上書きしていない。

`samples\three-pages.pdf` は合成した 3 ページの白紙である。`samples\marked.pdf` は合成した 1 ページで、文字「東京」と、24×24 の赤い四角の画像スタンプだけを持つ。理事長印ではない。新しい保存先は空の `output` である。

手動で見る項目:

1. 未署名の 3 ページ PDF を開く。ページメニューに右 90° と左 90° がある。
2. 2 ページ目を表示して右へ回す。2 ページ目だけが変わり、1 ページ目と 3 ページ目は変わらない。
3. 表示メニューの回転をしても、保存される角度は変わらない。
4. Undo と Redo。4 回回して元の向きに戻る。
5. 未保存の表示と、Ctrl+S が原本を変えないこと。
6. Ctrl+Shift+S で `output` の新しい名前へ保存し、その PDF を開き直す。
7. 保存をキャンセルし、終了確認でもキャンセルし、回転が残ること。
8. 原本と同じパスと、すでにあるファイルを拒否すること。
9. `samples\marked.pdf` で、日本語と赤い四角のスタンプのあとに回転する。理事長印を試すときは、このフォルダの空の `Data` に試験用の印を作り、利用中の `seals.bin` は開かない。承認の件数が勝手に増えないこと。
10. 署名の形を持つ PDF では項目が使えず、説明が署名を有効だと断定しないこと。拒否のあと、表示だけの回転はできること。
11. 実業務の PDF、実印影、実プリンターは使わない。

## 11. 既知の制限

- 暗号学的な署名検証はしない。構造から署名の可能性が残る文書は拒否する。
- `/Filter` だけ、`/Contents` だけでは拒否しない。
- 相互参照に無いオブジェクトは、ファイル自体が再構成されない限り見えない。再構成されたファイルは判定不能として拒否する。
- 64 より多い過去の増分は、中を読まずに拒否する。
- 閲覧専用でないセッションの Undo は、回転ガードを再実行しない。製品の起動は閲覧専用のままである。
- コマンドパレットの表示名は、カタログの "Rotate pages clockwise" の翻訳のままである。実行される処理は、メニューと同じく表示中の 1 ページだけである。
- 複数ページの一括回転、削除、並べ替え、抽出は、この版では使えない。
- 印刷プレビューの実機確認はしていない。コード上は、回転したページの枠だけを古くする。

## 12. 残るリスク

- 署名に見えない未知の辞書を、署名ではないと判定して回転できる可能性は残る。STEP 106 で閉じたのは、キーとしての `/ByteRange` と `/SubFilter`、既知の署名型、署名フィールド、`/Perms` の DocMDP / UR / UR3、過去の増分、読めない参照である。
- メニューを灰色にしていても、`Session::apply` を直接呼ぶ経路はエンジンの再検査に依存する。GUI の `execute` はその再検査を通る。
- 大きなファイルで、キャッシュの無い増分の再読込が遅くなる可能性は、STEP 106 の合成ファイルでは小さく、実ファイルでは未計測である。
- 画面上のスクロール位置は、高さが変わったページに合わせて自動では中央へ戻さない。ズームは維持する。
