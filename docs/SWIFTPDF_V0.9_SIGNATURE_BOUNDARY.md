# SwiftPDF v0.9.0 署名検出の境界条件

記録日: 2026-10-22。ブランチ `feature/page-rotate-v0.9.0`。基準 HEAD `d6c8f8084ef43445b46f39e5fc93f026e30621b1`。

STEP 102〜105 の報告書は変更していない。`ROADMAP.md` も変更していない。GUI の回転コマンドは有効にしていない。暗号学的な署名検証は実装していない。合成 PDF だけを使った。

## 1. 調査対象と環境

- リポジトリ: `D:\dev\SwiftPDF`
- ブランチ: `feature/page-rotate-v0.9.0`
- HEAD: `d6c8f8084ef43445b46f39e5fc93f026e30621b1`
- 開始時の `git diff --check` は空だった。STEP 103〜105 の変更と未追跡の試験・報告書が残っていた。想定外の変更はなかった。
- フォント: `CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`。Noto Sans CJK SC を埋め込まない警告は AGENTS.md §1.1 の想定である。
- ビルド並列: `CARGO_BUILD_JOBS=2`
- 本家と比較用ワークツリーは開いていない。利用中の `seals.bin` は読んでいない。

対象は `crates/engine/src/rotate_guard.rs` のページ回転専用ガードである。`Document::is_signed` は変えていない。

## 2. STEP 105 の署名ガードの現状

判定は 3 状態である。

- `Unsigned`: 読めた範囲に署名らしい構造がない。回転を許可する。
- `ProtectedOrSuspect`: 署名らしい構造がある。回転を拒否する。
- `Indeterminate`: 参照、オブジェクト、または古い改訂を読めない。未署名とは扱わず拒否する。

`blocks_page_rotation` は `Unsigned` 以外で true である。

回転の入口:

- `Session::apply` は、文書を変える前に `allow_page_rotate` を呼ぶ。`is_signed` が true なら従来の `NotPermitted("page rotation on a signed document")`。それ以外でガードが true なら `EditError::Invalid`。閲覧専用でなくても、この検査は適用の前に走る。
- 閲覧専用の Undo／Redo は、戻す段がページ回転のとき `allow_page_rotate_step` で同じ検査をする。製品は閲覧専用なので、この経路に入る。閲覧専用でないセッションの Undo／Redo は、STEP 103 からのとおり検査を足さない。回転そのものを新しく適用する経路ではない。
- `pdfcraft_organize::rotate_pages` を呼ぶ製品経路は `Session::apply` の後だけである。自動化の `page_rotate`、UI の `apply_edit`、CLI の回転編集も `Session::apply` に入る。
- `save_edited_copy_bytes` は現在の作業バイト列を返す。回転はしない。拒否された回転は履歴に入らないので、別名保存のバイト列にも入らない。

`is_signed` が true の拒否文は変えていない。構造だけの拒否文は次のままである。

`Page rotation is unavailable because the document contains or may contain a digital signature.`

## 3. `/ByteRange` の判断

STEP 105 は、`/ByteRange` がちょうど 4 個の数で、かつ同じ辞書に `/SubFilter`、`/Contents`、`/Filter`、`/Reference` があるときだけ拒否していた。キーだけの辞書、短い配列、文字列、壊れた参照は回転できた。修正前の試験で、その 6 件は `apply=Ok(())` だった。

今回の規則: 辞書キーとしての `/ByteRange` があれば拒否する。値の形では未署名にしない。

- 4 個の数だけの辞書: 拒否
- 短い配列、文字列: 拒否。不正な値を「署名ではない」とは扱わない
- 間接参照の先が配列: 拒否
- 参照が読めない: `Indeterminate` として拒否
- `/Contents` または `/Filter` と一緒にある場合: 拒否。これは STEP 105 でも拒否していた
- 後の改訂でその辞書を解放しても、古い接頭辞にキーが残るので拒否
- コンテンツストリームの文字 `/ByteRange` は辞書キーではない。許可する

## 4. `/SubFilter` の判断

`Dict::name` は間接参照を追わない。STEP 105 は `/SubFilter` を、`/ByteRange` が 4 個の数であるときの補助キーとしてしか見ていなかった。名前だけの辞書は回転できた。修正前の試験で、次は `apply=Ok(())` だった。

- `adbe.pkcs7.detached`
- `adbe.pkcs7.sha1`
- `adbe.x509.rsa_sha1`
- `ETSI.CAdES.detached`
- `ETSI.RFC3161`
- それらへの間接参照
- 未知の名前 `Custom.NotASignature`
- 整数

今回の規則:

- 辞書に `/SubFilter` があり、値が読めれば、原則として拒否する。署名用の名前、未知の名前、名前でない値は同じである。未知の値は、周辺が公開鍵暗号の辞書だと確認できない限り安全とは扱わない。
- 間接参照が読めない、または解決結果が null: `Indeterminate`。
- 例外は一つだけ。トレーラの `/Encrypt` が指す辞書で、値が `adbe.pkcs7.s3`、`adbe.pkcs7.s4`、`adbe.pkcs7.s5` のいずれかであり、その辞書に `/ByteRange` がないとき。これは ISO 32000-2 の公開鍵暗号ハンドラであり、署名形式ではない。間接参照の先がこの名前でも同じ。
- 同じ暗号用の名前でも、`/Encrypt` から参照されない辞書は拒否する。どの辞書に付いているか分からない。
- `/Encrypt` 辞書の `/SubFilter` が `adbe.pkcs7.detached` のときは拒否する。暗号用の名前ではない。

ページの画像や内容ストリームが持つ `/Filter /FlateDecode` は `/SubFilter` ではない。それだけでは拒否しない。

## 5. 署名関連オブジェクトの走査範囲

現在の番号は `object_numbers()` である。ファイルと overlay を合わせ、解放済みは除く。`scan_objects_checked` がその番号を歩き、読み取りエラーは null に潰さない。エラーは `Indeterminate` である。

パーサーが列挙できる範囲:

- クロスリファレンスが「ファイル内」または「オブジェクトストリーム内」と示す番号。ハイブリッド参照（表では空き、xref ストリームがオブジェクトストリーム内を示す）も含む。オブジェクトストリーム内の `/Type /Sig` は、ストリームを展開した辞書として拒否した。
- ストリームが圧縮されていても、パーサーが展開できれば対象になる。展開に失敗し、その番号が xref にあるときは `Indeterminate`。
- xref に載っていないオブジェクトは見ない。再構成された xref（`revisions()` が空で `repair_log()` が空でない）は、中身を未署名とは扱わずファイル全体を `Indeterminate` にする。
- カタログの `/AcroForm` `/Fields` と `/Kids`。`/T` は不要。
- カタログの `/Perms` `/DocMDP` `/UR` `/UR3`。
- トレーラ辞書。オブジェクト番号ではないので、オブジェクト走査とは別に見る。
- トレーラ `/Encrypt` が辞書そのもののときは、暗号用 `/SubFilter` の例外判定に使う。

## 6. 増分改訂の扱い

現在のオブジェクトが `Clear` のときだけ、`revision_ends()` の末尾以外を開く。末尾はファイル全体で、すでに見ている。接頭辞の中では、さらにその接頭辞の改訂列を辿らない。各接頭辞はその時点の xref を持つ。

- 改訂 1 件: 古い接頭辞は開かない。
- 改訂 2 件で、変更が `/Lang` だけのとき: `Unsigned`。増分更新だけでは拒否しない。
- 以前の改訂に署名辞書がある、または `/ByteRange` だけの辞書がある: 後の改訂で解放しても拒否する。最新の番号一覧にはその番号がない。
- 古い接頭辞が 64 件を超える（`revision_ends()` が 65 より長い）: それ以上は読まず `Indeterminate`。未署名にはしない。65 回の増分更新を足した合成ファイルで確認した。
- 接頭辞が開けない、または接頭辞自身が再構成された xref: `Indeterminate`。
- ファイル全体の xref が再構成されている: オブジェクトを見る前に `Indeterminate`。

## 7. 誤検出と見逃しの検証

修正前に失敗し、修正後に拒否へ変わったもの:

- `/ByteRange` だけ、短い配列、文字列、間接参照、壊れた参照
- 署名用 `/SubFilter` だけ 5 種、間接参照、未知の名前、整数
- 後の改訂で解放した `/ByteRange` だけの辞書

最初から拒否できていたもの:

- STEP 104 の 4 件（文字列の `/V`、名前のない署名フィールド、`/DocMDP`、未参照の `/Type /Sig`）
- `/ByteRange` と `/Contents`、`/ByteRange` と `/Filter`
- 階層フィールド、`/UR`、`/UR3`、壊れた署名フィールド参照、文書タイムスタンプ

許可のまま残したもの:

- 空の署名欄、`/V null`
- 未署名のテキストフィールド
- 本文に `/ByteRange` と印字した PDF
- 署名語を値に含む独自メタデータ
- `/Filter /FlateDecode` だけで `/ByteRange` がない辞書
- `/Encrypt` の `adbe.pkcs7.s5` と、間接の `adbe.pkcs7.s4`

判断した境界:

- 未知の `/SubFilter` は拒否する。辞書の位置が暗号辞書だと確認できず、署名用でないとも言えない。
- 読めない間接参照は `Indeterminate`。読めたが名前でない値は `ProtectedOrSuspect`。どちらも回転は拒否する。
- 不正な `/ByteRange` は、値が壊れていてもキーがあるので拒否する。

## 8. 拒否時の状態不変

署名構造の統合試験は、拒否のたびに次を見ている。`/Rotate` は変わらない。バイト列のポインタは変わらない。dirty、Undo、Redo は変わらない。エラー文は、`is_signed` のときの `NotPermitted` か、上記の `Invalid` 文だけである。

`/DocMDP` 付きの合成 PDF では、日本語テキストと一般スタンプを適用したあとの回転拒否で、テキスト、スタンプ、原本バイト列が残る。新しい PDF は作らない。この試験は引き続き成功している。理事長印の履歴は `RotatePages` では書かない。利用中の `seals.bin` は読んでいない。

## 9. 通常 PDF の回帰結果

未署名の通常 PDF について、次は成功した。

- 右 90°、左 90°、4 回で元に戻る（organize の 5 件、engine の回転保存）
- Undo／Redo、別名保存、再オープン、日本語テキストと印影（`rotate_validation` の表示・Undo 試験、ui-egui の日付印と理事長印）
- 原本と既存ファイルの上書き拒否、削除・並べ替え・抽出の拒否
- GUI の `page.rotate` は、capability を試験が付与してもコマンドとしては無効

件数は次のとおりである。organize の回転 5 件は成功 5、失敗 0、ignored 0。engine の回転保存と署名済み拒否は成功 2、失敗 0、ignored 0。ui-egui の回転・日付印・理事長印 4 件は成功 4、失敗 0、ignored 0。

## 10. 性能・リソース評価

ガードは回転の適用ごとに 1 回、閲覧専用で回転を Undo／Redo するときにも 1 回走る。ページをまとめて回す 1 回の `Edit::RotatePages` では、ページ数だけ繰り返さない。

古い改訂は、最大 64 個の接頭辞をその場で開き直す。結果はキャッシュしていない。

合成ファイルでの計測（`D:\dev\SwiftPDF-rotate-validate\boundary-perf.txt`）:

- 到達可能なオブジェクト 1002 個を 1 回走査: 1 ms
- 改訂 33 件のファイルを 4 回判定: 47 ms
- 65 回の増分更新を作って 64 件超として拒否する試験は、単体試験全体の 0.13 秒の中に収まっている

この大きさでは停止条件に当たる遅延ではない。大きな実ファイルは測っていない。遅い場合の改善は、判定結果をバイト列の同一性で覚え、未署名と疑わしいの境界は変えないことである。今回は実装していない。

## 11. テスト結果

修正前: `signature_structures_refuse_rotation_unless_the_field_has_no_value` は 14 件で失敗した。失敗したラベルは `byte-range-only`、`byte-range-short`、`byte-range-string`、`byte-range-indirect`、`byte-range-dangling`、`subfilter-pkcs7-detached`、`subfilter-pkcs7-sha1`、`subfilter-x509`、`subfilter-cades`、`subfilter-rfc3161`、`subfilter-indirect`、`subfilter-unknown`、`subfilter-integer`、`byte-range-freed-in-later-revision`。

修正後:

- `cargo fmt --check`: 成功
- `cargo check -p pdfcraft -j 2`: 成功
- `cargo clippy -p pdfcraft-organize --all-targets -j 2 -- -D warnings`: 成功
- `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings`: 成功
- `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings`: 成功
- `rotate_guard` 単体: 成功 11、失敗 0、ignored 0
- `rotate_validation`: 成功 4、失敗 0、ignored 0
- organize の回転 5 件: 成功 5、失敗 0、ignored 0。対象外 123
- engine の `view_only_page_rotation_saves_a_new_file_and_refuses_everything_else` と `signed_and_restricted_documents_refuse_page_rotation_before_it_changes_them`: 成功 2、失敗 0、ignored 0。対象外 318
- ui-egui `--lib` の回転・日付印・理事長印 4 件: 成功 4、失敗 0、ignored 0。対象外 532
- `content` を `--test-threads=1` で実行: 成功 15、失敗 0、ignored 0。7.20 秒

ワークスペース全体と ui-egui のライブラリ全件は実行していない。

## 12. 残る検出限界

- 暗号値は検証しない。ダミーの辞書でもキーがあれば拒否する。
- `/Filter` だけ、`/Contents` だけでは拒否しない。画像と通常の文字列が広く `/Filter` を持つ。
- xref にないオブジェクトは、ファイルが再構成されない限り見ない。再構成されたファイルは全体を拒否する。
- オブジェクトストリームは、パーサーが xref から番号を得て展開できる範囲だけである。
- 古い接頭辞が 64 件を超える分は読まない。拒否はする。
- 閲覧専用でない Undo／Redo はガードを再実行しない。製品は閲覧専用である。
- `write_full` は到達できないオブジェクトを捨てる。
- 公開鍵暗号以外の独自 `/SubFilter` が正当な非署名用途だった場合、今回の例外には入らず拒否する。確認できた非署名用途は `/Encrypt` の `adbe.pkcs7.s3`、`s4`、`s5` だけである。

## 13. GUI有効化の可否

有効化しない。

要求した合成 PDF の拒否と、空欄・本文の文字列・暗号辞書の許可は満たした。第 12 節の限界が残っている。`page.rotate` は閉じており、`Capability::PageRotate` は起動時に付与していない。

## 14. 次の推奨対応

GUI は開かない。次に見るなら、実在の大きな増分ファイルでの所要時間だけを、判定を弱めずに測ることである。遅いと分かった場合は、接頭辞の再解析結果を文書のバイト列に結び付けて覚える。64 件超を未署名にする変更はしない。
