# SwiftPDF v0.9.0 ページ回転の署名構造保護

記録日: 2026-10-22。ブランチ `feature/page-rotate-v0.9.0`。基準 HEAD `d6c8f8084ef43445b46f39e5fc93f026e30621b1`。

STEP 102〜104 の調査資料は変更していない。`ROADMAP.md` も変更していない。GUI の回転コマンドは有効にしていない。`Capability::PageRotate` は起動時に付与していない。`set_view_only(false)` は呼んでいない。

この保護は、電子署名が暗号学的に有効かどうかを調べるものではない。ページ回転の前に、署名らしい構造があるか、またはその有無を安全に決められないかを見る。合成 PDF の `/Contents` はダミーバイトである。

## 1. 問題の概要

STEP 104 では、次の 4 つの合成 PDF が `Document::is_signed` で未署名になり、ページ回転が適用された。

1. 署名フィールドの `/V` が文字列
2. `/T` がない署名フィールドで、`/V` が署名辞書
3. カタログ `/Perms` の `/DocMDP` だけがある
4. フィールドから参照されない `/Type /Sig` と `/ByteRange` を持つ辞書がある

いずれも暗号学的に有効な署名とは限らない。回転を拒否する理由は、署名らしい構造を安全に未署名と言い切れないことである。

修正前に、この 4 件の期待を「拒否」へ戻した試験は失敗した。`is_signed` は false で、`Edit::RotatePages` は `Ok(())` だった。名前付きで `/V` が辞書のフィールドと、文書タイムスタンプは、従来どおり `NotPermitted("page rotation on a signed document")` で拒否された。空の署名フィールドと `/V null` は回転できた。

## 2. 既存の署名検出の限界

`Document::is_signed` は `signatures` のうち `signed == true` があるときだけ true である。一覧は `pdfcraft_sign::pdf::list_cached` が作る。

- 署名フィールドは `/FT /Sig` かつ `/T` があるものだけが載る。名前のないフィールドは一覧に出ない。
- `signed` は `/V` が辞書に解決できたときだけ true である。`/V` が無い、null、辞書以外なら欄には出ても `signed` は false である。
- `Document::get` は読めない参照を null として返す。壊れた参照は、この経路だけでは空の値に見える。
- カタログの `/Perms` `/DocMDP` `/UR` `/UR3` は、この一覧の対象ではない。
- フィールドの `/V` から参照されない辞書は、文書タイムスタンプの条件を満たすときだけ載る。条件は `/SubFilter /ETSI.RFC3161` と、`/Type` が `/DocTimeStamp` または `/Sig` と、`/ByteRange` である。
- `/Type /Sig` や `/ByteRange` だけでは `is_signed` にならない。

`is_signed` はこの STEP では変えていない。署名パネルや、署名済み文書へのスタンプ拒否の意味は従来のままである。

## 3. 採用した検出方法

ページ回転の直前だけを見る判定を `crates/engine/src/rotate_guard.rs` に置いた。`allow_page_rotate` と `allow_page_rotate_step` が、既存の `is_signed` 判定の後で `blocks_page_rotation` を呼ぶ。

判定は 3 状態である。

- `Unsigned`: 読めた範囲に署名らしい構造がない。回転を許可する。
- `ProtectedOrSuspect`: 署名らしい構造がある。回転を拒否する。
- `Indeterminate`: 参照や古い改訂を読めず、未署名と言い切れない。回転を拒否する。

`is_signed` が true のときは、従来の `EditError::NotPermitted("page rotation on a signed document")` のまま返す。`NotPermitted` の表示文は所有者パスワードの案内を足すため、構造だけの拒否には使わない。構造の拒否は次の文である。

`Page rotation is unavailable because the document contains or may contain a digital signature.`

これは `EditError::Invalid` で返す。暗号学的に有効だとは書いていない。

検査は `Edit::RotatePages` を文書へ適用する前に終わる。検出の途中で読めないオブジェクトがあっても、そこまでに回転は書いていない。

## 4. 走査対象のPDFオブジェクト

本文のバイト列を文字列検索しない。開いた `pdfcraft_cos::Document` のオブジェクトを見る。

現在の改訂で歩く番号は `object_numbers()` である。ファイル上の番号と overlay の番号を合わせ、解放済みの番号は除く。`ScanLimit` に個数、最小番号、最大番号、`revisions()` の件数を残す。未参照の `/Type /Sig` を `new_empty` に足した単体試験では、その番号が最小から最大の範囲に入る。

`scan_objects_checked` は読めないオブジェクトを `Err` のまま返す。そのときは `Indeterminate` にする。`Document::get` のように null へ潰さない。

### 署名フィールド

カタログの `/AcroForm` `/Fields` を、`/Kids` を含めて深さ優先で歩く。`/T` は要らない。親の `/FT` は子へ引き継ぐ。循環は見た `ObjRef` で飛ばす。ノード数が 10,000 を超えたら `Indeterminate` である。

`/FT /Sig` の `/V` は次のとおりである。

- キーが無い、または明示の null: 空欄。これだけでは拒否しない。
- それ以外: 辞書、文字列、数値、ぶら下がった参照を含め `ProtectedOrSuspect`。
- 参照の読み取り自体が失敗: `Indeterminate`。
- 配列の要素が参照で、解決結果が null: `Indeterminate`。明示の null 要素は飛ばす。
- フィールドや `/Kids` が辞書・配列でない: `Indeterminate`。

### カタログの `/Perms`

`/DocMDP`、`/UR`、`/UR3` を見る。明示の null は無いものとして扱う。辞書、参照、参照先が null、その他の値は `ProtectedOrSuspect`。参照が読めなければ `Indeterminate`。`/Perms` 自体が辞書でなければ `ProtectedOrSuspect`。明示の null の `/Perms` は無視する。

### 辞書そのもの

現在の辞書とストリーム辞書について、次を署名らしい構造とする。

- `/Type` が `/Sig`、`/DocTimeStamp`、`/SigRef`、`/TransformParams`。直接の名前でも、間接参照の先がこれらの名前でも同じ。
- `/Type` の間接参照が null または読めない。ページを含む任意の辞書が対象である。署名型かどうか決められないため `Indeterminate` にする。
- `/ByteRange` がちょうど 4 個の数の配列で、かつ同じ辞書に `/SubFilter`、`/Contents`、`/Filter`、`/Reference` のいずれかがある。

`/ByteRange` が文字列、要素数が 4 でない、数ではない、または上記のキーが一つもない場合は、この規則だけでは拒否しない。`/Type /Sig` があれば `/ByteRange` が無くても拒否する。

## 5. 増分改訂への対応と限界

現在のオブジェクトだけが `Clear` のとき、`revision_ends()` の末尾以外を古い改訂として開く。末尾はファイル全体であり、overlay を含めて既に見ている。古い接頭辞の中では、さらにその接頭辞の `revision_ends` を辿らない。各接頭辞は、その時点のクロスリファレンスを持つファイルとして開く。

次は未署名にしない。

- `revisions()` が空で、`repair_log()` が空でない。再構成された xref である。
- 古い改訂が 64 を超える。
- 接頭辞が空、範囲外、または `Document::open` に失敗する。
- 開いた接頭辞が、再構成された xref である。
- 接頭辞の中に署名らしい構造、または読めない署名関連の参照がある。

xref が残っていて修復メモもあるファイルは、パーサーが作ったオブジェクトで判定する。増分更新であることだけでは拒否しない。

解放した署名の試験では、署名オブジェクトを増分更新で書き、次の増分更新でその番号を解放した。最新の `object_numbers()` にその番号は無く、`is_signed` も false だった。古い接頭辞に `/Type /Sig` が残るため `ProtectedOrSuspect` になり、回転は拒否された。

`new_empty` からの `write_full` は、到達できないオブジェクトを捨て、番号を振り直す。この経路では「前の改訂に署名が残る」ことを表せない。試験は、改訂を持つファイルへ増分更新で署名を足してから解放している。

同じ番号が後の改訂で別のオブジェクトに上書きされている場合、接頭辞が開けば古いオブジェクトが見える。接頭辞が開けなければ `Indeterminate` として拒否する。未署名にはしない。

## 6. 署名済み・署名疑い・未署名の判定

回転を許可するのは `Unsigned` だけである。

`is_signed` が true の文書は、構造判定より先に従来の `NotPermitted` で拒否する。名前付き署名フィールドの辞書 `/V` と、ETSI.RFC3161 の文書タイムスタンプはここに入った。構造判定でも `/Type /Sig` と `/Type /DocTimeStamp` は拒否対象である。

構造だけで拒否した例は次である。エラー文はすべて上記の `Invalid` 文で、`/Rotate`、バイト列のポインタ、dirty、Undo、Redo は変わらなかった。

- `/V` が文字列の署名フィールド
- `/T` がない署名フィールド
- 親が `/FT /Sig` で、名前のない子の `/V` が辞書
- `/Perms` `/DocMDP`、`/UR`、`/UR3`
- 未参照の `/Type /Sig` と `/ByteRange`
- `/V` がオブジェクト 999 への参照
- 後の改訂で解放された署名辞書

許可した例は次である。`is_signed` は false で、回転は適用された。

- 通常の未署名 PDF（表示・Undo の回帰を含む）
- 空の署名フィールド
- `/V null` の署名フィールド
- `/FT /Tx` で `/V` が文字列のフォーム
- ページ内容の文字列に `/ByteRange`、`/DocMDP`、`/Sig` と書いただけの PDF
- カタログの独自辞書 `/PrivateNote` の文字列が同じ語を含む PDF

## 7. 誤検出対策

次はオブジェクトのキーとして扱わない。

- コンテンツストリームの中の文字 `/ByteRange`
- 文字列オブジェクトの中の `ByteRange`、`DocMDP`、`Sig`
- 署名キーに似た独自メタデータの値

空の署名フィールドと `/V null` は許可する。未署名のテキストフィールドは許可する。

`/ByteRange` だけがある辞書は拒否しない。4 個の数と、`/SubFilter`、`/Contents`、`/Filter`、`/Reference` のどれかが同じ辞書にあるとき拒否する。画像 XObject は `/Filter` を持つが `/ByteRange` は持たないので、この規則には当たらない。

`/Type /TransformParams` は、署名参照の変換パラメータとしてどの辞書でも拒否する。通常のページや画像にはこの型を置いていない。

間接の `/Type` が null または読めない辞書は、ページ辞書であっても拒否する。通常の合成 PDF の `/Type` は直接の名前である。壊れた `/Type` 参照を未署名とは扱わない。

## 8. 拒否時の原子性

`/Perms` `/DocMDP` と未参照の `/Type /Sig` を持つ合成 PDF で、次を確認した。

- 日本語の追加テキスト「東京」は適用できる。
- 一般スタンプも適用できる。このファイルは `is_signed` が false なので、スタンプ禁止には入らない。
- その後の右 90° は `Invalid` で拒否される。
- 拒否の後も `/Rotate` は 0、dirty は true、Undo の先頭は `Add stamp`、バイト列のポインタは拒否前と同じである。
- テキストとスタンプは残る。
- ディスク上の原本バイト列は変わらない。新しい PDF は作っていない。
- Undo はスタンプを戻し、回転は Undo 対象にならない。
- ページ削除は従来どおり拒否され、ページ数は 1 のままである。

理事長印の承認履歴は、`apply_edit` が `RotatePages` では印影履歴を書かない。拒否された回転が履歴を足す経路はない。成功した回転が履歴を増やさないことは、STEP 104 の UI 試験のままである。利用中の `seals.bin` は読んでいない。

## 9. 回転・保存への影響

変えていないもの:

- `Document::is_signed`
- 閲覧、表示だけの回転、日本語追加テキスト、一般スタンプ、日付印、理事長印
- 閲覧専用で許可される既存の編集
- 保存確認、Ctrl+S、Ctrl+Shift+S、別名保存
- 原本と既存ファイルの上書き拒否
- ページ削除、並べ替え、抽出の拒否
- GUI の `page.rotate` と `page.rotate_ccw`。`command_policy` の閲覧専用一覧には入っていない。

ページ回転だけが、署名らしい構造と判定不能で追加拒否される。未署名の通常 PDF は、従来どおり `Capability::PageRotate` を試験が付与したときに回転できる。製品の起動時にはこの capability を付けない。

## 10. 新規テスト結果

修正前の確認: `signature_structures_refuse_rotation_unless_the_field_has_no_value` は、4 件（`sig-field-nondict-value`、`nameless-sig-field-with-value`、`perms-docmdp`、`orphan-byte-range`）で失敗した。`apply=Ok(())`、`is_signed=false`。この期待は戻していない。

修正後の `cargo test -p pdfcraft-engine --test rotate_validation -- --test-threads=1`:

- 成功 4、失敗 0、ignored 0
- `signature_structures_refuse_rotation_unless_the_field_has_no_value`
- `refusing_rotation_keeps_text_stamps_and_the_original_file`
- `rendered_rotation_matches_a_turned_picture`
- `rotation_undo_keeps_text_stamps_and_refuses_other_page_edits`

`rotate_guard` の単体試験は 3 件とも成功、失敗 0、ignored 0。

- 未参照の署名辞書の番号が、パーサーの番号範囲に入る
- 署名語を含む独自メモは `Unsigned`
- 後の改訂で解放した署名は `ProtectedOrSuspect` で、最新の番号一覧にはその番号が無い

署名試験の記録は `D:\dev\SwiftPDF-rotate-validate\signatures\signatures.txt`。リポジトリには入れてない。

## 11. 回帰テスト結果

品質確認の環境は `CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`、`CARGO_BUILD_JOBS=2`。Noto Sans CJK SC を埋め込まない警告は AGENTS.md §1.1 の想定である。

- `cargo fmt --check`: 成功（終了コード 0）
- `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings`: 成功
- `cargo clippy -p pdfcraft-organize --all-targets -j 2 -- -D warnings`: 成功
- organize の回転 5 件: 成功 5、失敗 0、ignored 0。対象外 123。doc-tests は 0 件。
- engine の `view_only_page_rotation_saves_a_new_file_and_refuses_everything_else` と `signed_and_restricted_documents_refuse_page_rotation_before_it_changes_them`: 成功 2、失敗 0、ignored 0。対象外 310。
- `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings`: 成功
- `cargo check -p pdfcraft -j 2`: 成功
- ui-egui のライブラリ試験 4 件（`page_rotate_grant_does_not_enable_the_command_and_writes_only_a_new_file`、`a_date_stamp_stays_on_its_page_when_that_page_is_rotated`、`rotating_the_page_does_not_move_a_chairman_approval`、`chairman_colour_stays_on_the_approved_page_after_rotation`）: 成功 4、失敗 0、ignored 0。対象外 532。`--lib` で実行した。フィルタ無しの `cargo test -p pdfcraft-ui-egui` は統合試験をすべてコンパイルするため、途中で止めてこの範囲に変えた。止めた実行は試験失敗ではない。
- `cargo test -p pdfcraft-ui-egui --test content -j 2 -- --test-threads=1`: 成功 15、失敗 0、ignored 0。6.97 秒。

ワークスペース全体と、ui-egui のライブラリ試験全件は実行していない。

## 12. 未対応の署名形式

次は、今回の判定が暗号検証ではなく、構造も見ていないか、見て意図的に拒否していないものである。

- バイト列の署名値、証明書、タイムスタンプトークンの検証はしない。ダミーの `/Contents` でも構造があれば拒否する。
- `/ByteRange` だけの辞書は拒否しない。`/SubFilter`、`/Contents`、`/Filter`、`/Reference` が無ければ、このキーだけでは署名辞書としない。
- `/SubFilter` だけの未参照辞書は拒否しない。フィールドの `/V` に入っていれば、値がある署名フィールドとして拒否する。
- 64 を超える古い改訂は中身を見ず `Indeterminate` で拒否する。
- 再構成された xref は中身を未署名とは扱わず拒否する。
- 古い接頭辞が開けないときも拒否する。上書きされた古いオブジェクトを、接頭辞を開かずに復元はしない。
- 全体を `write_full` し直したファイルは、到達できないオブジェクトを捨てる。捨てられた未参照の署名は、その書き直し後のファイルには残らない。
- 現在の `ScanLimit` が記録する番号範囲は、最新の `object_numbers()` である。解放済みの番号は範囲に入らない。そのオブジェクトは古い接頭辞を開いて見る。
- 任意の辞書で間接 `/Type` が壊れていると拒否する。署名以外の型であっても、読めなければ未署名にしない。

## 13. GUI有効化の可否

有効化しない。

この STEP の禁止事項として、`page.rotate` と `page.rotate_ccw` は開かず、`Capability::PageRotate` も起動時に付けない。加えて、第 12 節の形はまだ未署名と署名構造のどちらにも取り切れない。`/ByteRange` だけの辞書、`/SubFilter` だけの未参照辞書、64 改訂を超えるファイル、再構成された xref の中身の個別判定は残っている。再構成と上限超過は拒否側なので回転は通らないが、署名が無いことまでは示していない。

試験が要求した拒否と許可は、合成 PDF の上では満たした。それでも GUI の回転コマンドは閉じたままにする。
