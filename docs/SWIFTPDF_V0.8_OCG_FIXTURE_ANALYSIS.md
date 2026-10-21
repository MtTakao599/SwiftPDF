# SwiftPDF v0.8.0 OCG 試験PDFの白紙表示（STEP 96-F）

製品コードと既存テストは変更していない。`layers-two.pdf` は上書きしていない。

## 1. 調査日時・環境

| 項目 | 値 |
| --- | --- |
| 日時 | 2026-10-22 午前（JST）。修正版の画面確認は 03:30 |
| リポジトリ | `D:\dev\SwiftPDF` |
| ブランチ | `feature/upstream-safety-v0.8.0` |
| HEAD | `b9aa25c0d74dcd00fa598e352d747bcec7ee52d4` |
| 検証環境 | `D:\dev\SwiftPDF-GUI-Test-v080` |
| 問題のPDF | `fixtures\layers-two.pdf`（747 バイト、SHA-256 `D2355BED0B76A7CB33D4B5BC4FE4160EC4403AA06977DF95618DF44CAE4B7CED`） |
| レンダラー | 現行ツリーの `pdfcraft-cli render`（GUI と同じ `PageRenderer`）と、ポータブルな `SwiftPDF.exe` |

開始時の `git diff --check` は空。検証用 SwiftPDF は調査の開始時には動いていなかった。画面確認に使ったプロセスは終了済み。

## 2. 問題PDFの構造

`layers-two.pdf` の全体は次のとおり。

```
%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R 6 0 R] /D << /Order [5 0 R 6 0 R] >> >> >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 240 140] /Contents 4 0 R /Resources << /Properties << /LA 5 0 R /LB 6 0 R >> >> >> endobj
4 0 obj << /Length 84 >> stream/OC /LA BDC 1 0 0 rg 20 20 80 80 re f EMC /OC /LB BDC 0 0 1 rg 140 20 80 80 re f EMC endobj
5 0 obj << /Type /OCG /Name (LayerA) >> endobj
6 0 obj << /Type /OCG /Name (LayerB) >> endobj
xref
0 7
...
trailer << /Size 7 /Root 1 0 R >>
startxref
544
%%EOF
```

| 項目 | 内容 |
| --- | --- |
| ヘッダー | `%PDF-1.7` |
| xref / trailer | ある。`/Size 7`、`/Root 1 0 R`。`/Prev` はない |
| ページツリー | 1ページ。`/CropBox` はない。`/MediaBox [0 0 240 140]` |
| `/Contents` | オブジェクト 4 を指している |
| `/Resources` | `/Properties << /LA 5 0 R /LB 6 0 R >>` だけ |
| `/OCProperties` | `/OCGs [5 0 R 6 0 R]`、`/D << /Order [5 0 R 6 0 R] >>` |
| `/BaseState` `/ON` `/OFF` | 無い。名前オブジェクトは LayerA と LayerB |
| 描画命令の文字列 | 赤 `1 0 0 rg 20 20 80 80 re f`、青 `0 0 1 rg 140 20 80 80 re f`。どちらも `/OC … BDC` と `EMC` の間 |
| `q` / `Q` | 無い |
| オブジェクト 4 の実体 | `stream` の直後が `/`（`stream/OC`）。改行が無く、ファイル中に `endstream` が一度もない |

座標自体は MediaBox の内側で、二つの四角形は重ならない。命令の文字列はオブジェクト 4 の辞書のうしろに続いているが、PDF のストリームとしては区切られていない。

同じ生成ミスが、作り直した `pages-rotated.pdf`、`blank-for-text.pdf`、`original-protect.pdf` にもある。いずれも `endstream` が無い。元の3ファイルは残してある。

## 3. 白紙表示の原因

原因は試験PDFの生成である。PowerShell の二重引用符が `$nlendstream` を変数名として展開し、`endstream` という語と、その前後の改行が消えた。

`vendor/lopdf/src/parser/mod.rs` のストリーム読取は、`stream` のあとに行末があり、`/Length` バイトのあとに `endstream` があることを要求する。このファイルは両方を欠くので、オブジェクト 4 はコンテンツストリームにならない。ページ辞書と OCG の名前は別オブジェクトなので、ページは開き、レイヤーパネルには LayerA と LayerB が出る。描画命令はストリームとして渡らないため、目のアイコンを切り替えても図形は出ない。

`/Length 84` は、入れるつもりだった命令列の長さである。そのバイト列はストリームの中に入っていない。

STEP 94 の印刷処理はこの白紙の原因ではない。通常表示のラスタがすでに白である。

## 4. 別レンダラーでの確認結果

| 手段 | 結果 |
| --- | --- |
| Edge のヘッドレススクリーンショット | 失敗。問題PDFと修正版の両方が同じ空の灰色画像（2,728 バイト）になり、PDF のページを描いていない |
| Poppler / MuPDF / Ghostscript | 未導入。インストールしていない |
| Windows.Data.Pdf | ページ数はどちらも 1 と読めた。描画ストリームの取得に失敗し、画像は得ていない |
| SwiftPDF の `PageRenderer` | 実施。下記 |

別製品での図形表示は未確認。レイヤー切り替えと印刷プレビューも、それらのビューアでは未確認。

## 5. SwiftPDF v0.7.0／v0.8.0の比較

v0.7.0 の実行ファイルは `D:\dev\swiftpdf-v070-target` に無い。比較用ワークツリーは起動も変更もしていない。

白紙になるファイルは、v0.8.0 の差分に含まれない `lopdf` のストリーム文法で拒否される。同じ HEAD の v0.7.0 もこのオブジェクト 4 から描画命令を取り出せない。v0.7.0 での起動そのものは未実施。

v0.8.0 の現行レンダラーでの結果は次のとおり。72 dpi。

| ファイル | 画像 | 赤 | 青 | 黒 |
| --- | --- | --- | --- | --- |
| `layers-two.pdf` | 240×140 | 0 | 0 | 0 |
| `layers-two-fixed.pdf` | 240×180 | 4,900 | 4,900 | 272 |
| `/OFF [5 0 R]`（LayerA を既定で非表示） | 240×180 | 0 | 4,900 | 272 |
| `/OFF [6 0 R]`（LayerB を既定で非表示） | 240×180 | 4,900 | 0 | 272 |

4,900 は 70×70 の四角形そのものである。黒い 272 画素は、レイヤー外の 16×16 の印と `MARK` である。LayerA を外すと赤だけが消え、LayerB を外すと青だけが消える。印は両方に残る。

## 6. テストPDF修正の有無

修正した。製品コードと既存テストには手を入れていない。生成に使った一時スクリプトはリポジトリに置いていない。誤りは、ストリームを `stream`、改行、本体、改行、`endstream` の順で書いていなかったことである。

## 7. 修正版PDFの場所

`D:\dev\SwiftPDF-GUI-Test-v080\fixtures\layers-two-fixed.pdf`

SHA-256 `B0A154DE7E16E83B09CF892800834D4EA37692664D4EDD4CF0B1CA08358E094A`（948 バイト）。

内容は 1 ページ、パスワードなし、署名なし。`/MediaBox [0 0 240 180]`。`/BaseState /ON`。`/OFF` は無い。

- レイヤー外: 黒の 16pt 四方 `(8, 150)` と Helvetica の `MARK`
- LayerA: 赤の 70pt 四方 `(20, 30)`。`/OC /LA BDC` … `EMC`
- LayerB: 青の 70pt 四方 `(140, 30)`。`/OC /LB BDC` … `EMC`

重ならない。`q` / `Q` は使っていない。`stream` の直後は改行（`0A`）で、`endstream` がある。起動ログに相互参照の再構成は出ていない。

同じ欠落を直した副本も、元のファイルを残したまま追加した。

| ファイル | SHA-256 |
| --- | --- |
| `fixtures\pages-rotated-fixed.pdf` | `9ABFFE69235AC61B798C306AB74D9BF41102651300A311A7C615F5472C5A30CE` |
| `fixtures\blank-for-text-fixed.pdf` | `B8E07DD69C0D30FC7EA48267496E956548307BA588BED5ADFDEBDF39A340F107` |
| `fixtures\original-protect-fixed.pdf` | 上と同じ |

`pages-rotated-fixed.pdf` の 1 ページ目は、`Page 1 rotate 0 TOP`、`Page 1 MID`、`Page 1 BOTTOM` を描く。画面での回転確認は未実施。

## 8. 通常表示とレイヤー操作の結果

| 確認 | 結果 |
| --- | --- |
| 通常表示（GUI） | 成功。`layers-two-fixed.pdf — SwiftPDF` に、黒印、`MARK`、赤い四角形、青い四角形が見えた。修復バナーは無い |
| レイヤーパネル | 成功。`レイヤー 2` に LayerA と LayerB があり、どちらも目のアイコンが付いている |
| LayerA を非表示 | GUI の目の操作は未実施。`/OFF [5 0 R]` の別PDFを同じレンダラーで描くと赤だけが消える |
| LayerB を非表示 | GUI の目の操作は未実施。`/OFF [6 0 R]` では青だけが消える |
| 両方再表示 | GUI 未実施。修正版の既定状態では赤と青が同時に出る |
| 問題の `layers-two.pdf` | 変更なし。レンダラーでは 240×140 の白 |

## 9. 印刷プレビュー確認結果

GUI の印刷プレビューは未実施。非表示レイヤーの印刷コピーについては、既存の4テストが本日も成功した。

- `a_layer_hidden_in_the_panel_stays_hidden_when_printed`
- `a_hidden_layer_stays_off_when_the_file_also_has_a_print_usage`
- `a_hidden_layer_stays_out_of_colour_and_grayscale_image_prints`
- `uncommitted_text_on_a_hidden_layer_document_prints_without_the_layer`

これらはメモリ上の正しいストリームを使っており、壊れた `layers-two.pdf` とは別である。`/AS` の PrintState を評価しない制限はそのまま。

## 10. 未確認事項

- 修正版を開いた状態で、目のアイコンを押して赤だけ、青だけを消す操作
- その状態の印刷プレビュー（カラーとグレースケール）
- 閉じて開き直したあとのレイヤー状態
- Edge など別ビューアでの図形とレイヤー
- v0.7.0 の実行ファイルによる起動
- `pages-rotated-fixed.pdf` の 90° / 180° / 270° の画面確認

## 11. 次に必要な作業

印刷プレビューの手動確認は、`layers-two.pdf` ではなく `layers-two-fixed.pdf` で行う。LayerB を隠すと青だけが消え、プレビューにも青が出ないこと、LayerB を戻すと青が戻ることを見る。実プリンターには出さない。

ズームと日本語の手動確認は、`endstream` の無い `pages-rotated.pdf`、`blank-for-text.pdf`、`original-protect.pdf` を使わない。それぞれ `*-fixed.pdf` を使う。原本ハッシュの基準は、直す前の `original-protect.pdf`（`50EC6AE5…`）ではなく、`original-protect-fixed.pdf`（`B8E07DD6…`）にする。

製品側の描画修正は不要。STEP 94 のコードとテスト結果は、この白紙とは独立している。
