# SwiftPDF v0.9.0 回転 UI の整理

実装日: 2026-10-22。ブランチ `feature/page-rotate-v0.9.0`（基準 `d6c8f8084ef43445b46f39e5fc93f026e30621b1`）。

STEP 108 は表示名、ツールチップ、コマンドパレット、回帰テストだけである。ページ回転エンジン、署名ガード、保存処理、印影処理は変えていない。`set_view_only(false)` は呼んでいない。ページの削除、並べ替え、抽出は有効にしていない。

STEP 102〜107 の報告書は残している。`ROADMAP.md` は変えていない。画面の操作確認は行っていない。

## 1. 変更前の問題

STEP 107 で、表示中の 1 ページを回す操作は動くようになった。表示名はまだ、文書全体を回す操作に見えた。

- ページメニューは「ページを右に90°回転」だった。いま見ている 1 ページだけ、と読めなかった。
- コマンドパレットはカタログの "Rotate pages clockwise"（日本語では「ページを時計回りに回転」）のままだった。
- 右下の回転ボタンの説明は「表示を時計回りに回転」だった。PDF に保存されないことが書いていなかった。

右下のボタンは `DocView::rotate_view` のままである。ページメニューは `/Rotate` を書く。同じ「回転」に見えて、保存と Undo の結果が違う。

## 2. 表示回転とページ回転の仕様

| | 表示のみの回転 | ページ自体の回転 |
|---|---|---|
| 操作 | 右下の `rotate-cw` ボタン。表示メニューの既存 2 項目 | ページメニューの `page.rotate` / `page.rotate_ccw` |
| 対象 | 開いている文書の表示全体 | いま表示している 1 ページ |
| PDF の `/Rotate` | 変えない | 90° 単位で変える |
| 未保存状態 | 変えない | 変える |
| Undo / Redo | 対象外 | 対象 |
| 別名保存 | 回転は新しい PDF に入らない | 安全な別名保存に入る |

表示回転のクリック先は `DocView::rotate_view` のままである。ボタンのアイコン、位置、クリック後の処理は変えていない。表示メニューの項目名「Rotate view clockwise」「Rotate view counterclockwise」も変えていない。項目にポインタを乗せた説明だけを足した。

ページ回転のコマンド ID は変えていない。実行はこれまでどおり、表示中ページの 90° と -90° だけである。

## 3. UI 表示名とツールチップ

ページメニューとコマンドパレットは、同じ関数 `page_rotate_menu_label` の文字列を使う。

| コマンド ID | 日本語 | 英語 |
|---|---|---|
| `page.rotate` | このページを右に90°回転 | Rotate current page clockwise 90° |
| `page.rotate_ccw` | このページを左に90°回転 | Rotate current page counterclockwise 90° |

押せるときの説明は「このページを90°回転します。回転結果は新しいPDFに別名保存できます。」である。英語は "Turns the current page by 90°. You can save the result as a new PDF." である。使えないときは、これまでの署名・権限・未読の説明のままである。

右下ボタンの説明:

- 日本語: 表示を右に90°回転（PDFには保存されません）（ショートカット）
- 英語: Rotate the view 90° clockwise (not saved in the PDF) (shortcut)

表示メニューにポインタを乗せた説明は、ショートカット無しで同じ意味である。左回転は「表示を左に90°回転（PDFには保存されません）」。右下に左回転ボタンは無い。

全ツール一覧のカタログ原文 "Rotate pages" は残している。画面に出す名前だけ、上の「このページを右に90°回転」に合わせた。エンジンのコマンド仕様文 "Rotate pages clockwise" も残している。Undo の履歴名 "Rotate page" は変えていない。

## 4. コマンドパレット

パレットの表示名は、メニューと同じ `page_rotate_menu_label` である。英語では "Rotate current page clockwise 90°" と "Rotate current page counterclockwise 90°"。日本語ではメニューと同じ 2 文である。

検索は、表示名に加えてエンジンの英語仕様文も見ている。そのため "rotate pages clockwise" でも、表示は新しい名前のまま、そのコマンドに届く。一括回転の `page.rotate_dialog` は、これまでどおり無効である。クエリ "rotate pages" は、その無効な項目にも当たる。

## 5. 多言語対応

新しい文は `i18n::t` の英語原文である。カタログに訳が無い言語は、その英語文を出す。生のキー名は出ない。日本語だけをソースへ埋め込んではいない。

訳を足したカタログ（各 7 行）:

`ja` `de` `fr` `es` `it` `pt-br` `ru` `uk` `zh-hans` `zh-hant` `pl` `cs` `tr` `hu` `bg` `sl` `ar` `kk` `lt` `vi` `el`

未訳はテルグ語（`te.tsv`）だけである。テルグ語では英語の文が出る。英語自体はカタログではなく原文である。

既存の「ページを時計回りに回転」「表示を時計回りに回転（{key}）」は残している。表示メニューの項目名と、エンジン仕様文の訳はそちらを使う。

## 6. 回転の保存と Undo / Redo

ページ回転のあと、そのページの `/Rotate` だけが変わり、文書は未保存になる。Undo で戻り、Redo で戻せる。別名保存は STEP 103 の `save_edited_copy_bytes` と `write_new_file` のままである。原本と同じパスと、すでにあるファイルへの上書きは拒否する。Ctrl+S の `file.save` は拒否されたままである。

表示回転をそのあと押しても、`/Rotate`、未保存状態、Undo の先頭は変わらない。その状態で別名保存して開き直すと、ページ回転の角度だけが残り、表示回転は残らない。

## 7. 安全性と権限

署名の形がある文書と、ページ編集を許さない権限の文書では、ページ回転は拒否される。文書は変わらない。表示回転は、そのあとでも使える。

閲覧専用は起動時のままである。`PageRotate` の許可は、ページ削除や抽出には広がらない。理事長印の承認状態を変える処理は追加していない。

## 8. テスト結果

品質確認は終了コード 0 である。

- `cargo fmt --check`
- `cargo check -p pdfcraft -j 2`
- `cargo clippy -p pdfcraft-organize --all-targets -j 2 -- -D warnings`
- `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings`
- `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings`

実行したテスト。失敗と ignored は、いずれも 0 である。

| 対象 | 成功 | 除外 |
|---|---:|---:|
| `pdfcraft-ui-egui` lib（表示名、i18n、回転、保存、署名、印、日付スタンプ） | 29 | 511 |
| `pdfcraft-ui-egui` `--test commands` | 24 | 0 |
| `pdfcraft-ui-egui` `--test view_only_commands`（`--test-threads=1`） | 8 | 0 |
| `pdfcraft-engine` lib（`rotate_guard`、署名、権限、閲覧専用のページ回転） | 13 | 307 |
| `pdfcraft-engine` `--test rotate_validation` | 4 | 0 |
| `pdfcraft-organize` lib（`page_rotation` と `rotate_`） | 5 | 123 |
| `pdfcraft-ui-egui` `--test content`（`--test-threads=1`） | 15 | 0 |

`commands` の初回は 23 成功、1 失敗だった。失敗は `palette_arrow_keys_select_results_and_enter_runs_the_selection` である。クエリ "rotate pages" が、無効な一括回転ダイアログを先頭に並べ、矢印 1 回で時計回りを実行した。期待は反時計回り（270°）だった。クエリを表示名の "rotate current page" に変え、270° の確認は残した。再実行は 24 成功である。

organize の doc-tests は、このフィルタに一致するテストが 0 件だった。上の 5 件は lib の結果である。

架空の PDF だけを使った。画面は操作していない。既知の別件（WARP の並列クラッシュ、結合グリッドの矢印キー、中国語 UI フォント）は、この手順では再実行していない。今回のテストに新しい失敗は無い。

## 9. GUI 受け入れ項目

自動テストは上の表である。次は画面で見る項目であり、この手順では未確認である。

確認用の実行ファイルは `D:\dev\SwiftPDF-GUI-Test-v090-step108\SwiftPDF.exe`（debug ビルド、112,667,648 バイト、2026-10-22 17:00:36）である。`portable.txt` は 0 バイトのファイルである。`D:\dev\SwiftPDF-GUI-Test-v090`（112,645,120 バイト）と `v081`（112,615,424 バイト）は上書きしていない。`Data` と `output` は空である。利用中の `seals.bin` は読んでいない。

1. 右下の回転ボタンに、表示だけの回転だと分かる説明がある。
2. 「ページ」メニューに、このページを右・左へ 90° 回転する操作がある。
3. 表示だけの回転は全体に作用し、保存されない。
4. ページ回転は現在ページだけに作用し、Undo / Redo できる。
5. ページ回転後の別名保存が成功する。
6. 保存した PDF を開き直しても角度が残る。
7. 日本語テキストとスタンプが正しく表示される。
8. コマンドパレットの説明が現在ページの操作になっている。

`samples` には、STEP 107 で作った合成の `three-pages.pdf` と `marked.pdf`（「東京」と赤い四角）を入れた。`output` は空である。

## 10. 残る制限

- 電子署名の暗号検証はしていない。`/Filter` だけ、`/Contents` だけでは署名としない。64 を超える古い改訂は、未署名とは扱わない。
- 製品は閲覧専用のままである。ページ削除、並べ替え、抽出、挿入、移動、`page.rotate_dialog` は無効である。
- 右下のボタンは右 90° だけである。左への表示回転は、表示メニューにある。
- 全ツール一覧のカタログ原文は "Rotate pages" のままである。画面上の名前は「このページを右に90°回転」である。
- パレット検索は、古い英語仕様文 "Rotate pages clockwise" にも当たる。
- テルグ語は英語の文になる。
- Undo の履歴名は "Rotate page" のままである。
- この報告書の作成時点で、画面のクリック確認はしていない。
