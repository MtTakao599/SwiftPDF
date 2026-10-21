# SwiftPDF v0.8.0 本家修正の詳細調査

調査と比較と設計案だけである。製品コード、本家ローカル、`docs/SWIFTPDF_FEATURE_AUDIT_V0.8.md` は変更していない。pull、merge、rebase、cherry-pick はしていない。

## 1. 調査日時・対象コミット

| 対象 | 値 |
|---|---|
| 調査日 | 2026-10-21（JST） |
| SwiftPDF | `D:\dev\SwiftPDF`、`feature/seal-effects-v0.7.0`、`b9aa25c0d74dcd00fa598e352d747bcec7ee52d4` |
| 開始時の作業ツリー | `docs/SWIFTPDF_FEATURE_AUDIT_V0.8.md` のみ未追跡。それ以外の変更は無かった |
| 本家ローカル | `D:\dev\PdfCraft-Original`、`9e5b70d1c231d7db558e278a2622b519ca063494`。この調査では読んでいない。変更していない |
| 本家 GitHub | https://github.com/storytold/pdfcraft |
| 再確認した `main` | `22bedbf2af25bd4cdc1c188b5eff6ec3d2d089fb`。`git ls-remote` の結果は STEP 90 と同じ。進んでいない |
| 差分の取り方 | GitHub Commits API の各コミット JSON（ファイル一覧と patch）。ローカルへの fetch はしていない |
| 一時領域 | `C:\Users\yasuh\AppData\Local\Temp\swiftpdf-upstream-audit`。コミット JSON と patch の写し。リポジトリの外 |

6件はすべて、SwiftPDF の分岐点 `9e5b70d1`（2026-10-16T17:51:58Z）より後である。SwiftPDF の該当関数を読み、本家 patch がそこに入っていないことを確認した。

後続コミットは、各ファイルの Commits API（修正の1秒後から `22bedbf2` まで）で見た。同じ関数を書き換えた後続は、この範囲では見つかっていない。同じファイルの別関数を触ったものは、各節に書いた。

対象6コミットの変更ファイルに、`LICENSE`、`Cargo.toml`、`Cargo.lock`、フォントのライセンスファイルは無い。この6件だけを取り込むなら、新しいクレートは要らない。144件全体の依存差分は、この調査の対象外であり未確認。

## 2. 6修正の一覧

| 記号 | 修正 | 本家コミット | 日時 (UTC) | PR | SwiftPDF | 推奨 |
|---|---|---|---|---|---|---|
| A | 入力文字を CID 0 で書かない | `f0c275669ad80a4d643965b97ae8b859c333c00d` | 2026-10-18T13:48:14Z | #3223 #3235 | 未反映。製品の日本語追加は別経路 | 見送り |
| B | 別名保存は過去の増分を落とす | `bd0b3fdb65d2c57a22c003d834720ecd6e96eee6` | 2026-10-18T07:48:35Z | #2165 #2174 | 未反映。製品の保存は別関数 | 見送り |
| C | パネルで隠したレイヤーを印刷しない | `4ff53a8d8ebb850259fec3884949ed0312f2999a` | 2026-10-18T07:12:26Z | #3046 #3050 | 未反映。レイヤー切替は閲覧専用でも動く | 取り込み |
| D | ズームアウトで表示が飛ばない | `9d23505dd7c5a6bb238abf053a4ed1c2167b1644` | 2026-10-19T06:48:06Z | #3266 #3350 | 未反映。ジャンプ先のクランプは既にある | 取り込み |
| E | 別ダイアログの下で保存確認のキーを処理しない | `84bd488db377425aa73265d474f301f28f0b8325` | 2026-10-19T17:01:58Z | #3387 | 未反映。Ctrl+S 自体は別の理由で止まる | 取り込み |
| F | 選んだフォルダの外に名前で書き出さない | `08b44e858b8761bda1b0931087ea41b0748435eb` | 2026-10-16T18:03:46Z | メッセージに番号は無い | 未反映。別名保存の `publish_new_pdf` は別の保護 | 取り込み |

コミットの対応は、STEP 90 の題名と、今回取得した patch の本文が一致したものだけである。

## 3. 該当する本家コミット・PR

### A. CID 0

https://github.com/storytold/pdfcraft/commit/f0c275669ad80a4d643965b97ae8b859c333c00d

Word の Type 0 サブセットは、本当の空白グリフとは別に CID 0（`.notdef`）も ToUnicode で U+0020 に結び、幅も持たせる。`encode` は各文字のいちばん小さいコードを残すので、編集した行の空白がすべて `<0000>` になり、空白の位置に箱が描かれた。CID 0 は ToUnicode が何と言っても `.notdef` なので、`has_glyph` が CID 0 を拒否し、空白は本当の CID を使う。

問題になる PDF は、複合フォント（Type 0）で、ToUnicode が CID 0 と別の CID の両方を同じ文字（典型は空白）に写しているものである。標準14フォントはこの分岐に入らない。

`crates/fonts/src/pdf.rs` の `Metrics::has_glyph` に、`self.composite` の中で `self.cid(code) == 0` なら `false` を足した。CMap もフォントプログラムも書き換えない。`encode` が選ぶコードが変わる。`decode` は変えていない。

このファイルを後から触ったコミットは、`22bedbf2` まで無い。

### B. 別名保存と増分

https://github.com/storytold/pdfcraft/commit/bd0b3fdb65d2c57a22c003d834720ecd6e96eee6

保存先が別ファイルで、読み取り専用でなく、署名も無いとき、`save_doc_to` が `save_bytes`（増分）ではなく `save_full_bytes`（全書き出し）を使う。同じファイルへの Save As、署名済み、読み取り専用は増分のまま。署名を全書き出しすると、署名が覆っていたバイト列が変わり、検証が壊れる。テストは、削除したページの `(Page 2)` と2つ目の `%%EOF` が別名ファイルに残らないこと、署名済みは元の署名バイトで始まること。

同じファイルの後続は、日付ピッカーのフィールド追加（`a30dde2225`）とページ整理の Ctrl-ドラッグ（`932bf9c6bf`）で、`save_doc_to` は patch に出てこない。

別件として `ecffce69ee`（#1937 #2029）が、損傷して xref を組み直したファイルの上書き保存を全書き出しにする。署名済みで無編集なら、損傷したバイトのままにする。これは別名保存の修正ではない。SwiftPDF の `working_bytes` に `reconstructed()` は無い。

### C. 非表示レイヤーの印刷

https://github.com/storytold/pdfcraft/commit/4ff53a8d8ebb850259fec3884949ed0312f2999a

印刷用 PDF が `/OCProperties` をそのまま写していた。レイヤーパネルで隠しても、紙では既定状態に戻った。画像として印刷する経路は、もともと描画設定のレイヤーを使う。

レイヤーを一度でも変えたあと（`doc.config.layers` が空でない）、印刷用コピーの既定設定 `/D` を、`/BaseState /ON`、見えているものを `/ON`、隠れているものを `/OFF` にする。`/Order`、`/RBGroups`、`/AS` は残す。一度も切り替えていなければ、ファイル自身の設定のまま印刷する。`/OCProperties` が無いファイルは何もしない。

対象は Optional Content Group（OCG）である。テストは合成 PDF の Layer A / Layer B で、赤と緑の矩形が、隠した側だけ印刷結果から消えることを数える。

`crates/engine/src/lib.rs` の後続は11件ある。`set_default_layer_state` と `print_pdf` を含む patch は、確認した `ecffce69ee` には無かった。他の10件の印刷関数までは未確認。関数が後で消えた、という題名は無い。

### D. ズーム

https://github.com/storytold/pdfcraft/commit/9d23505dd7c5a6bb238abf053a4ed1c2167b1644

ズームの錨は、ページ上の点をビューポートの同じ相対位置に置くスクロール量である。egui は範囲外のスクロールを描いたあとで戻す。縮小でページがビューに収まると、範囲外の値が1フレーム出てから跳ねた。

`anchored_scroll` が、コンテンツよりビューが大きい軸は 0、それ以外は `0..=content-viewport` に収める。ページ上の分数 `(fx, fy)` の計算は変えていない。

12秒後の `66f1085688`（#3362）は、スクロール領域の親を文書 ID に固定する `page_scroll` で、`anchored_scroll` は触っていない。モードをまたいでスクロール位置を保つ別修正である。SwiftPDF には `page_scroll` も無い。

### E. 保存確認のキー

https://github.com/storytold/pdfcraft/commit/84bd488db377425aa73265d474f301f28f0b8325

「変更を保存しますか」が出ているあいだ、その上にリンク確認、リモートリンク、全画面確認、更新、別ダイアログ、ロック解除があると、Escape と Return がその保存確認を決めない。テストは、未保存のタブを閉じる確認の上にリンク確認を置き、Escape でも Enter でも文書が閉じず、保存確認が残ること。Enter ではリンク確認が残る。

Ctrl+S と Ctrl+Shift+S の割り当ては、この commit では変わっていない。`crates/ui-egui/src/lib.rs` を後から触ったコミットは、`22bedbf2` まで無い。

### F. 出力パス

https://github.com/storytold/pdfcraft/commit/08b44e858b8761bda1b0931087ea41b0748435eb

添付をタブとして開くと、タブ名が後の Split やページ抽出のファイル名になる。証明書の書き出しは、証明書の主体をファイル名にしていた。主体が `../x` だと、選んだフォルダの親へ `../x.cer` を書く。

`is_plain_file_name` は、空、`.`、`..`、`/`、`\`、ドライブレター付き、Windows では `:` を含む名前を拒否する。`:` の拒否は代替データストリームも止める。`write_files_then` は、バッチのどれか1つでもこれを外れると、1つも書かない。証明書は `export_certificate` が `attachment_file_name` を通してから `write_files` に渡す。添付を開くタブ名も `attachment_file_name` を通す。

テストは、`../x.pdf`、`..`、`.`、空、`sub/x.pdf`、`sub\x.pdf`、`C:x.pdf`、絶対パスを混ぜるとフォルダが空のままであること、証明書主体 `../x` が選んだフォルダの中の `.._x.cer` になること。

シンボリックリンクとジャンクションそのものを解決する処理は、この commit には無い。拒否するのは「名前がフォルダの直下の1ファイルではない」ことだけである。利用者が選んだフォルダがジャンクションなら、その中へ書く。

`crates/ui-egui/src/files.rs` の後続 `cef2255da5` は、読み込んだフォントを編集中の段落へ当てるもので、`is_plain_file_name` は patch に無い。

## 4. 変更ファイル・関数

| 記号 | ファイル | 関数 |
|---|---|---|
| A | `crates/fonts/src/pdf.rs` | `Metrics::has_glyph`。呼び出し元は `Metrics::shows`、`Metrics::encode`、`keep`。テスト `composite_fonts_never_encode_with_cid_0`。既存の CMap 往復テストはコード 0 を逆引きから外した |
| B | `crates/ui-egui/src/editing.rs` | `PendingOverwrite` に `target`。`save_doc_to_then`、`overwrite_changed_file`、`save_doc_to`。判定は `SaveTarget::As`、`same_file`、`read_only_reason`、`is_signed`。実体は `Session::save_full_bytes` / `save_bytes` |
| B | `crates/ui-egui/tests/editing.rs` | `save_as_to_a_new_file_drops_earlier_revisions` |
| B | `crates/ui-egui/tests/signing.rs` | `save_as_keeps_a_signed_document_incremental` |
| B | `docs/architecture.md` | 全書き出しの条件に「別ファイルへの Save As。署名済みを除く」 |
| C | `crates/engine/src/lib.rs` | 新関数 `set_default_layer_state`。`Session::print_pdf` のベクトル印刷の直前 |
| C | `crates/engine/src/tests.rs` | `printing_keeps_hidden_layers_hidden` |
| D | `crates/ui-egui/src/canvas.rs` | 新関数 `anchored_scroll`。`document_area` の `zoom_anchor` 分岐。テスト `zoom_anchor_scroll_stays_within_the_content` |
| E | `crates/ui-egui/src/lib.rs` | `PdfCraftApp::shortcuts`。`dialogs::save_prompt_key` を呼ぶ前の `covered` |
| E | `crates/ui-egui/tests/ui.rs` | `a_link_prompt_above_a_save_prompt_owns_its_keys` |
| F | `crates/ui-egui/src/files.rs` | 新関数 `is_plain_file_name`。`write_files_then` の先頭。`attachment_file_name` の説明。テスト `write_files_refuses_names_that_leave_the_folder` |
| F | `crates/ui-egui/src/sign_ui.rs` | 新メソッド `PdfCraftApp::export_certificate`。テスト `an_exported_certificate_is_written_inside_the_chosen_folder` |
| F | `crates/ui-egui/src/dialogs.rs`、`panels.rs` | 証明書の書き出しを `export_certificate` に差し替え |
| F | `crates/ui-egui/src/lib.rs` | `attachment_action` がタブを開くとき `attachment_file_name` |

## 5. SwiftPDF 側の対応箇所

### A

`crates/fonts/src/pdf.rs` の `Metrics::has_glyph`（798行付近）に CID 0 の拒否は無い。`Metrics::cid` は、CID マップが空ならコードをそのまま返し、マップがあって範囲外なら 0 を返す。

製品が日本語を足す経路は `crates/edit/src/added.rs` の `draw_japanese` である。WinAnsi に入る文字は標準フォント、それ以外は `embed_added_type3` と `added_type3_show` である。Type3 の `FirstChar` は 1、コード空間は `<01> <FF>` で、CID 0 を使わない。

既存の Type 0 へ書き戻すのは `crates/edit/src/text.rs` の `source_show` と `source_encodes` で、ここが `Metrics::encode` を呼ぶ。これは既存行の置換であり、製品は `edit.edit_text` を許可していない。

### B

閲覧専用の「名前を付けて保存」は `crates/ui-egui/src/commands.rs` の `file.save_as` が `begin_save_copy` に入る。書き込みは `editing.rs` の `publish_new_pdf` が `pdfcraft_platform::save_path::write_new_file` を呼ぶ。原本と同一、既存の保存先、解決できないリンクは、バイトを作る前ではなく公開の時点で拒否する。失敗時はステージングを消し、公開はハードリンクだけで、リネームで既存を置き換えない。

中身は、編集が無ければ `Session::save_copy_bytes`（開いたバイトそのもの）、テキストかスタンプがあれば `save_edited_copy_bytes` から `working_bytes` の `write_incremental` である。`save_full_bytes` は `check_not_view_only` で閲覧専用から拒否される。`save_doc_to` は `SaveTarget` を受け取らない。製品の保存確認はこちらを通らない。

### C

`Session::print_pdf_with_pending_text`（`crates/engine/src/lib.rs`）は、画像印刷なら `impose_raster`、それ以外は `opened_cos` に未確定テキストを焼いて `pdfcraft_print::impose` する。`set_default_layer_state` は無い。

`set_layer_visible` は閲覧専用を見ていない。右パネル（`panels.rs`）から隠せる。隠すと `use_layer_choices` が `doc.config.layers` を埋め、画面の再描画はそれを使う。ベクトル印刷は `/OCProperties` のままなので、パネルで隠した層が紙に戻る。印刷禁止の権限は、その前に `allows_printing` で拒否する。原本には書かない。

グレースケールのチェックは、プリンタジョブの `grayscale` と、「画像として印刷」のラスタに分かれる。通常印刷はベクトルなので、この不具合の対象である。画像として印刷する経路は、描画設定を使うため、本家の説明どおり既にパネルの状態を見る。

スタンプと理事長印は、追加したコンテンツであり、OCG のメンバーではない。この修正は印影の画素を変えない。印影の下にあるレイヤーが、隠していれば印刷からも消える。

### D

`document_area` の `zoom_anchor`（`canvas.rs` 2579行付近）は `(point - rel).max(0)` だけで、上側を切っていない。同じ関数の `goto_point`（2698行付近）は、既に `content - viewport` で切っている。`content_h` は 2545行で求めており、錨の処理より前にある。本家の `anchored_scroll` を置く位置はある。

錨の分数は、表示中のページ矩形に対する割合である。回転は `layout` が矩形へ反映したあとの割合なので、このクランプは PDF 座標を計算し直さない。印影と追加テキストの位置は、ポインタとページ矩形から取る。スクロールが1フレームずれて戻ると、そのフレームの見た目とクリック位置がずれる。クランプは、描画と同じ範囲に先に収める。

高 DPI は、この式ではポイントのままである。ラスタの倍率は後段の `pixels_per_point` で、スクロール量とは別である。

### E

`shortcuts`（`lib.rs` 3008行付近）は、`close_request` があると `save_prompt_key` を処理して戻る。その上に別のダイアログがあっても、である。本家の `covered` は無い。

`modal_open` は、ダイアログ、閉じる確認、パスワード、ロック解除、リンク確認、更新で真になる。印影効果は `Dialog::SealEffects` なので、開いているあいだ `modal_open` は真で、登録ショートカットには進まない。Ctrl+S は `file.save` で、閲覧専用では `command_allowed` が拒否する。Ctrl+Shift+S は `begin_save_copy` で、`publish_new_pdf` が原本を拒否する。

残る穴は、閉じる確認が出たあとに別プロンプトも立っているとき、Escape と Return が閉じる確認を先に決めることである。印影効果ウィンドウ自身は Escape で `dialog` を消す。承認のチェックはスタンプバーにあり、それだけでは `modal_open` にならない。

`pending_full_screen` は本家の `covered` にあり、SwiftPDF の `modal_open` には無い。全画面確認が保存確認と重なるかは未確認。

### F

`is_plain_file_name` は無い。`write_files_then`（`files.rs`）は `dir.join(name)` のあと `write_atomically` する。名前の検査は無い。

証明書の書き出しは、`dialogs.rs` と `panels.rs` が `format!("{}.cer", c.display_name())` を `write_files` に渡す。主体に `../` があれば、選んだフォルダの外へ出る。署名パネルは閲覧専用で開ける。電子署名の作成は拒否されるが、この書き出しはコマンドではなくパネル操作である。

添付を開くタブは、まだ `att.name` のままである（`lib.rs` の `attachment_action`）。保存ダイアログの初期名だけ `attachment_file_name` を通る。ページ抽出と分割は閲覧専用で拒否されるので、タブ名が今すぐ `write_files` に届く製品操作は、証明書の書き出しが確認できたものに限る。他の `write_files` 呼び出し（画像の保存、アクセシビリティ報告）は、生成した名前を使う。それらが PDF 由来の区切り文字を含み得るかは未確認。

`publish_new_pdf` はこの関数を使わない。ここへ `is_plain_file_name` を入れても、原本拒否と既存拒否は残る。

添付の「ディスクへ保存」は、利用者が選んだフルパスへ `write_atomically` する。本家のこの commit はそこを変えていない。`write_atomically` は既存ファイルをリネームで置き換える。別名保存の「既存があれば拒否」とは別経路である。

## 6. 競合・依存関係

| 記号 | SwiftPDF 独自機能との関係 | 依存 |
|---|---|---|
| A | 日本語 Type3 追加、標準フォントの WinAnsi 追加、印影とは別関数。既存 Type 0 の編集を解禁したときに初めて製品へ出る | 後続無し。CMap の書き換えはしない |
| B | `publish_new_pdf`、閲覧専用、`save_full_bytes` の拒否、増分で原本バイトを残す方針と衝突する。patch を `save_doc_to` に載せても、製品の保存はそこを通らない | 署名は増分のまま、という条件が必須。#1937 は別修正で、`reconstructed()` が SwiftPDF に無い |
| C | 印影は OCG ではない。印刷禁止、原本を書かない、未確定テキストをコピーにだけ焼く処理は残す。`print_pdf` 全体の差し替えは、SwiftPDF が足した引数と衝突する | `/AS` の印刷時自動状態は、本家が残すと書いている。パネルの OFF と `/AS` のどちらが勝つかは、印刷クレートの未確認 |
| D | 表示のスクロールだけ。印影の PDF 矩形、日本語の `Tm` は計算しない。`canvas.rs` は印影で大きく変わっているので、patch の適用は手で行う | `66f1085688` の `page_scroll` は同時に取り込まない |
| E | 原本への Ctrl+S は、この修正が無くても拒否される。印影効果は既にモーダルとしてショートカットを止める。閉じる確認と別プロンプトの重なりだけが穴 | `lib.rs` の `shortcuts` はこのあと本家でも変わっていない。SwiftPDF 側の関数は分岐後に変わっているので、行のコピーではなく `covered` の条件を足す |
| F | `publish_new_pdf` とは独立。証明書名と `write_files` だけを守る。ポータブルの `Data` や `seals.bin` は、この書き出し先ではない | `attachment_file_name` は SwiftPDF に既にある。タブ名へ使う1行と、証明書の呼び出し2か所を足す |

ライセンスと依存は、6コミットとも新しいクレートもライセンスファイルも無い。

## 7. 安全性評価

優先の順は、データの消失、原本、権限と書き込み先、保存と印刷の正しさ、日本語、操作、差分の大きさである。原本を守ることは、本家の別名保存をそのまま載せることではない。SwiftPDF の `write_new_file` の方が、同一ファイルと既存ファイルを拒否する。

| 記号 | データの消失 | 原本 | 権限と書き込み先 | 正確さ | リスク |
|---|---|---|---|---|---|
| F | 拒否したバッチは1ファイルも書かない | `publish_new_pdf` を変えない限り原本はそのまま | フォルダ外への証明書名を止める。これがこの6件でいちばん直接の穴 | 正規のファイル名は今までどおり | 低。`write_atomically` の上書きは、この修正では残る |
| E | 閉じる確認を、上のダイアログのキーで決めなくなる | Ctrl+S で原本を書かないことは既にある | ダイアログの下の操作を止める | 保存確認のキーが、上に何も無いときだけ働く | 低。条件を広くしすぎると、保存確認の Enter が届かなくなる |
| C | 印刷用のコピーだけを変える。開いている文書は変えない | 印刷は原本を置き換えない | 印刷禁止は今の検査のまま。隠した層を紙に出さない | 画面と紙の OCG が揃う | 中。`/AS` の印刷指定を崩す可能性は未確認。関数全体の移植は未確定テキストと衝突する |
| D | なし | なし | なし | 縮小したフレームでページが跳ねない。座標の式は同じ | 低。`content_h` の定義が連続表示と単ページで違うので、テストは両方 |
| A | なし。書くコードが `.notdef` から実グリフへ変わる | なし | なし | 既存 Type 0 を編集したときの空白 | 低だが、今の製品経路では効果が出ない。CID マップ範囲外を 0 とみなす既存の `cid` と組み合わさり、今は通っていたコードが Type3 へ落ちることがある。既存編集を出す前に、そのテストが要る |
| B | 全書き出しは、増分に残っていた削除済みオブジェクトを新しいファイルから落とす。ガベージコレクションは、未知のオブジェクトを落とすことがある | 本家の `save_doc_to` は、同じパスなら今までどおり上書きする。SwiftPDF の拒否と逆 | 署名済みは増分のまま。権限の変更はしない | 別ファイルは1つの `%%EOF` | 高。製品経路に乗せず、`save_full_bytes` を閲覧専用で呼べるようにすると、印影と日本語を含むファイルの再オープンを別途証明する必要がある。v0.8 ではやらない |

## 8. テスト計画

実印、業務 PDF、`seals.bin`、実プリンタは使わない。合成 PDF と合成 PNG だけにする。

| 記号 | 取り込むなら実行するもの |
|---|---|
| F | 本家の `write_files_refuses_names_that_leave_the_folder` に相当する検査。証明書主体 `../x` がフォルダの中の `.._x.cer` になる検査。`publish_new_pdf` の既存テスト（原本拒否、既存拒否）がそのまま成功すること |
| E | 本家の `a_link_prompt_above_a_save_prompt_owns_its_keys`。加えて、印影効果を開いた状態で Ctrl+S が原本を書かないこと、Ctrl+Shift+S がコピー確認に入り原本パスを拒否すること。承認チェックを付けただけでは印影が変わらないこと |
| C | 本家の `printing_keeps_hidden_layers_hidden` を、SwiftPDF の `print_pdf_with_pending_text` に対して行う。レイヤーを触っていないファイルは従来どおり両方出ること。印刷禁止の文書は今までどおり拒否すること。スタンプを置いた合成 PDF で、隠していないページの印影が印刷用 PDF に残ること。実機には出さない |
| D | `zoom_anchor_scroll_stays_within_the_content`。単ページと連続、90度回転した合成ページで、錨の分数がクランプの前後で変わらないこと。印影の配置矩形が、ズームの前後で PDF 座標として同じであること |
| A | v0.8 では取り込まない。後で入れるなら `composite_fonts_never_encode_with_cid_0` と、日本語 Type3 の既存テスト、WinAnsi の `encode("Café €")` |
| B | v0.8 では取り込まない。後で全書き出しを検討するなら、削除ページが新しいファイルの過去リビジョンに残らないこと、署名済みは増分のままであること、原本パスと既存パスは `write_new_file` が拒否したままであること、日本語と印影を含む合成ファイルが `check_save` を通ること |

各ステップのあと、`cargo fmt --check` と、触ったクレートだけの `cargo clippy -p … --all-targets -- -D warnings` と、そのクレートの該当テスト。ワークスペース全体は回さない。

## 9. 移植優先順位

1. F。選んだフォルダの外への書き込み。原本保護の経路とは別で、証明書パネルから到達する。
2. E。保存確認のキーが、上のダイアログをすり抜けない。原本の Ctrl+S 拒否は既にあるので、この順位は「確認を誤って消さない」ためである。
3. C。画面で隠したレイヤーが紙に戻らない。印刷禁止と原本不変更は維持する。
4. D。ズームの見た目。座標計算は変えない。
5. A。見送り。既存テキスト編集を製品にする版の前に、単独で入れる。
6. B。見送り。ページ削除を製品にする版で、全書き出しが未知データを落とさないと確認できるまで入れない。`save_doc_to` 経由では入れない。

## 10. 推奨する実装ステップ

一度にまとめない。各ステップは前がテストを通ってから始める。A と B はこの版のステップに入れない。

### ステップ 1。出力名をフォルダの中に閉じる

- 対象は `is_plain_file_name`、`write_files_then` の拒否、`export_certificate`、`dialogs.rs` と `panels.rs` の呼び出し、`attachment_action` のタブ名。
- 取り込み方は、本家 patch の該当関数を手で移す。`files.rs` の後続フォント修正は移さない。`publish_new_pdf` と `save_path.rs` は触らない。
- テストは第8節の F。
- 完了は、区切りや `..` やドライブレターを含む名前でフォルダの外にファイルができないこと、普通の `x.pdf` は今までどおり書け、別名保存の原本拒否が残っていること。
- 次へ進む条件は、そのテストと clippy が通ること。

### ステップ 2。閉じる確認のキーを、上のダイアログに渡す

- 対象は `PdfCraftApp::shortcuts` の `close_request` 分岐だけ。
- 取り込み方は、`pending_link`、`pending_remote_link`、`pending_full_screen`、`updates.open`、`dialog`、`unlock_prompt` があるときは `save_prompt_key` を呼ばない、という条件を足す。`file.save` を許可しない。
- テストは第8節の E。
- 完了は、リンク確認が上にあるあいだ Escape と Enter で文書が閉じないこと、印影効果を開いた Ctrl+S が原本を書かないこと。
- 次へ進む条件は、ステップ 1 のテストがまだ通ること。

### ステップ 3。印刷コピーの既定レイヤーを、パネルに合わせる

- 対象は `set_default_layer_state` と、`print_pdf_with_pending_text` のベクトル分岐。画像印刷の early return より後ろ、`impose` の直前。`config.layers` が空なら何もしない。
- 取り込み方は関数の追加と、その1か所の呼び出し。`print_pdf` 全体は置き換えない。未確定テキストを焼く処理、印刷権限、閲覧専用の印刷許可は残す。
- テストは第8節の C。
- 完了は、隠した OCG が印刷用 PDF に描かれず、隠していない OCG と印影は残ること。実プリンタは使わない。
- 次へ進む条件は、既存の `swift_print` テストが通ること。`/AS` の印刷指定を持つ合成 PDF で、意図と違う層が出るなら、このステップは戻して原因を記録し、先へ進まない。

### ステップ 4。ズーム錨のスクロールをコンテンツ内に収める

- 対象は `anchored_scroll` と、`zoom_anchor` の1行。
- 取り込み方は、その関数と呼び出しだけ。`page_scroll`（#3362）は入れない。
- テストは第8節の D。
- 完了は、範囲外の錨が最大スクロールで止まり、ページ矩形上の分数が変わらないこと。
- 次へ進む条件は、印影の配置テストが通ること。

### この版でやらないもの

- A。`has_glyph` の CID 0 拒否。既存テキスト編集を出す作業の最初のコミットにする。それまで `source_show` は製品から呼ばれない。
- B。`save_doc_to` の全書き出し。ページ削除を許可する作業で、増分に残る削除内容が問題になってから、`working_bytes` とは別に設計する。署名済みは増分のまま、公開は `write_new_file` のままにする。#1937 の `reconstructed()` も、この版では足さない。

## 11. 未確認事項

- `/AS` の Print イベントが、`set_default_layer_state` のあと印刷クレートで `/OFF` に勝つかどうか。commit 本文は「その他の項目は残す」とだけ書いている。
- `crates/engine/src/lib.rs` の後続11件のうち、`ecffce69ee` 以外が `print_pdf` を触ったか。題名には印刷レイヤーの再修正は無い。
- `pending_full_screen` と閉じる確認が、SwiftPDF の操作で同時に立つか。
- 画像保存とアクセシビリティ報告のファイル名が、PDF 由来の区切りを含み得るか。
- 添付の「ディスクへ保存」が、利用者が選んだ既存ファイルを `write_atomically` で置き換えるか。本家の F はそこを直していない。
- 利用者が選んだフォルダ自体がジャンクションやシンボリックリンクのとき、フォルダの中へ書く挙動。F は名前の検査だけで、リンク先の解決はしない。SwiftPDF の `write_new_file` は、保存先そのものがリンクなら既存として拒否する。フォルダがリンクである場合は未確認。
- #1937 の `reconstructed()` が、分岐後の `crates/cos` のどこで増えたか。SwiftPDF の `working_bytes` には無い。
- 144件全体の `Cargo.lock` と `LICENSE`。対象6コミットには含まれない。
- 本家最新ツリーを checkout していない。patch は GitHub のコミット API が返したものである。API が patch を省略したファイルは、この6件には無かった。
