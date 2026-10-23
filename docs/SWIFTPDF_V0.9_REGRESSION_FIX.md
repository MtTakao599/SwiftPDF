# SwiftPDF v0.9.0 回帰修正（STEP 110）

状態: **修正済み。新規の失敗 7 件はすべて解消し、全体テストの失敗は既知の 2 件だけ。§5 の判断について利用者の確認待ち。**

実施日: 2026-10-22。ブランチ `feature/page-rotate-v0.9.0`。基準 HEAD `d6c8f8084ef43445b46f39e5fc93f026e30621b1`。コミット・push はしていない。

STEP 109 で見つかった新規失敗 7 件の原因を調べ、最小限の修正を入れた。変更したのは `crates/engine/src/rotate_guard.rs` と `ATTRIBUTION.toml` の 2 ファイルだけである。ログはリポジトリ外の `D:\dev\SwiftPDF-rotate-validate\step110\` にある。

## 1. 新規失敗 7 件の正確な名前

| # | 群 | クレート / バイナリ | テスト |
|---|---|---|---|
| 1 | A | `pdfcraft-engine --lib` | `tests::saving_a_password_protected_document_rebases_on_it` |
| 2 | B | `pdfcraft-ui-egui --test camera` | `dragging_a_corner_then_enhance_page_straightens_the_page` |
| 3 | B | `pdfcraft-ui-egui --test camera` | `the_clean_up_level_enhances_again_from_the_photo_and_keeps_the_turn` |
| 4 | B | `pdfcraft-ui-egui --test camera` | `the_level_leaves_the_page_alone_when_another_edit_came_in_between` |
| 5 | B | `pdfcraft-ui-egui --test fill_sign` | `image_signature_preview_follows_pointer_at_page_size_with_zoom_and_rotation` |
| 6 | B | `pdfcraft-ui-egui --test ui` | `a_middle_click_closes_the_tab` |
| 7 | C | `xtask --bin xtask` | `assets::tests::the_real_manifest_passes` |

修正前に 7 件すべてを個別に再現した（`step110\before-*.txt`）。

## 2. 再現条件

- **A**: パスワード付きの合成 PDF（AES-256、利用者パスワード `pw`）をパスワードで開く。回転して保存すると、改訂が 2 つになる。そのファイルを基準に戻してもう一度回転すると、`Invalid("Page rotation is unavailable because the document contains or may contain a digital signature.")` が返る。CLI では STEP 109 の `f_enc_inplace.json`（その場保存後の 2 回目の回転）と `h_enc3_reopen.json`（2 改訂の暗号化 PDF を開き直して回転）で再現した。
- **B**: テスト用の手書き PDF には xref 表がなく、`trailer << /Root 1 0 R >>` だけを持つ。開くと cos が xref を再構成し、`revisions()` が空、`repair_log()` が空でない状態になる。この状態で回転を求めると拒否された。開く、表示、タブ、カメラ補正、署名プレビュー自体は動いており、止まっていたのは回転だけである。CLI では `a_noxref.json` で再現した。
- **C**: STEP 108 で 21 言語の `.tsv` に 7 行ずつ訳を追加したが、`ATTRIBUTION.toml` の `sha256` を更新していなかった。`the_real_manifest_passes` が、ちょうどこの 21 ファイルについて SHA-256 の不一致を報告した。

## 3. 根本原因

- **A**: `rotate_guard::assess` は古い改訂を読むとき、各改訂の終わりまでのバイト列を `Document::open` で開いていた。`Document::open` はパスワードを受け取らない。そのため暗号化 PDF の古い改訂は必ず開けず、「読めない」として `Indeterminate` になった。結果、暗号化された PDF は 2 回目以降の回転がすべて拒否された。STEP 105〜106 は暗号化 PDF の複数改訂を focused テストで扱っておらず、全体テストで初めて見つかった。
- **B**: STEP 106 で「xref を再構成したファイルは一律に `Indeterminate`」という規則を入れた。再構成した読み取りは、同じ番号のオブジェクトの最後の定義だけを残す。そのため古い定義（たとえば上書きされた署名辞書）が見えなくなる、という理由だった。この規則は安全側だが、署名と無関係な手書き PDF もすべて拒否した。カメラ、フィル&サイン、タブのテストは、そうした PDF で回転を使っていた。
- **C**: STEP 108 の手順漏れ。訳の追加後に `cargo xtask assets` を実行していなかった。

## 4. 修正方針と理由

- **A**: 古い改訂を `cos.open_with_same_credentials(...)` で開くよう 1 行変えた。
  - これは cos の既存 API で、署名検証が「署名が覆うバイト列」を開くために使っている。今の文書と同じ `/Encrypt` と `/ID` を持つデータだけを、今の文書がすでに持っているファイル鍵で開く。
  - パスワードを受け取らず、保持もしない。ログやテスト出力にも出ない。
  - 暗号化されていないデータは `Document::open` と同じように開く。別の鍵で暗号化されたデータは `NeedsPassword` で失敗する。この挙動は cos の既存テスト `crates/cos/tests/encryption.rs` が確かめている。
  - 失敗した場合の扱いは前と同じで、`Indeterminate`（回転を拒否）になる。
  - 大きな再設計は不要だった。
- **B**: 一律の `Indeterminate` を、パーサーに依存しない安全確認に置き換えた。対象は再構成された文書のときだけである。
  - `raw_names_clear(cos.bytes())` がファイルの生のバイト列を先頭から最後まで走査し、`/` で始まる名前をすべて調べる。
  - 次のどれかが 1 つでもあれば、今までどおり `Indeterminate` を返す:
    - `#` を含む名前（エスケープでどの名前でも綴れるため）
    - `Sig` で始まる名前（`/Sig`、`/SigFlags`、`/SigRef` など）
    - `ByteRange`、`SubFilter`、`Perms`、`DocMDP`、`UR`、`UR3`、`DocTimeStamp`、`TransformParams`、`ObjStm`、`Encrypt`
  - 走査が通った場合だけ、通常の検査（`assess_current`、フォームの `/FT /Sig`、`/Perms`、`/ByteRange`、`/SubFilter`、全オブジェクト）に進む。
  - この確認が安全である理由は、cos の再構成（`document.rs` の `reconstruct`）の仕組みにある。
    - 再構成は、生のバイト列の行頭にある `N G obj` だけを索引に入れる。
    - 例外は、`/Type` がそのままの名前 `/ObjStm` であるストリームの中身だけである。
    - したがって、リーダーがたどれる辞書はすべて、`ObjStm` を除いて生のバイト列に名前として現れる。最後の定義に隠された古い定義も同じである。`ObjStm` は拒否する。
    - 文字列やコメントの中の名前は区別しない。誤って拒否する側（安全側）に倒れる。
- **C**: 21 件の `sha256` の行だけを、今のファイルのバイト列から計算し直した値に置き換えた（§7）。

## 5. xref のない PDF の扱い

- 開く、表示、タブ、カメラ、署名プレビューは、修正の前も後も止めない。回転を拒否しても、表示やタブは塞がない。
- 再構成された PDF が回転できるのは、生の走査と通常の検査の両方が通った場合だけである。「安全確認なしに回転可能にする」変更ではない。
- 署名の名前を含む再構成 PDF は、今までどおり `Indeterminate` で回転を拒否する。上書きされた古い署名定義があるもの、`/ObjStm` や `/Encrypt` を含むもの、名前に `#` があるものも同じである。
- 既存の STEP 106 のテスト `a_reconstructed_xref_is_not_treated_as_unsigned` は、アサーションを変えずにそのまま通る（`Indeterminate`）。そのフィクスチャには `/ObjStm` が含まれるからである。生の走査を無効にするとこのテストは失敗した。今これを拒否しているのは新しい走査だ、ということを確認した。
- **ユーザー確認事項**: これは STEP 106 の「再構成なら一律拒否」という規則を、安全性の根拠つきで狭めたものである。拒否条件のうち署名に関わるものは緩めていない。ただし、署名と無関係な再構成 PDF が回転できるようになる点は、仕様上の判断としてレビューしてほしい。一律拒否に戻す場合、B 群の 5 テストはそのフィクスチャに xref 表を付ける必要がある。

## 6. 暗号化 PDF の古い改訂

- 認証済みの文書は、自分の古い改訂を同じファイル鍵で読む。古い改訂に残った署名の形も、暗号化されていない PDF と同じく検出する。
- 認証できない改訂は「署名なし」として扱わず、`Indeterminate` にする。別の `/Encrypt` や `/ID` を持つもの、壊れたもの、65 個以上あるものがこれに当たる。
- 暗号化 PDF でもガードは無効にしていない。パスワードはログ、テスト出力、ファイルのどこにも出していない。テストの固定値 `pw` は合成データだけで使う。

## 7. 翻訳のハッシュとライセンス

- 対象は STEP 108 で 7 行ずつ追記した 21 ファイルである: `ar bg cs de el es fr hu it ja kk lt pl pt-br ru sl tr uk vi zh-hans zh-hant`。`te.tsv` は STEP 108 で触っていないので、ハッシュも変えていない。
- 21 エントリーすべてについて次を確認した:
  - `licence = "MIT OR Apache-2.0"`
  - `kind = "translation"`
  - `source` がクリーンルーム翻訳であること
- 追記した行はこのブランチで英語の UI ラベルから書いた貢献者オリジナルの訳であり、独自の翻訳資源は使っていない。既存の出所とライセンスに合っている。
- `author`、`source`、`licence` は変えていない。変更行は `-sha256` 21 行と `+sha256` 21 行だけである（`git diff --numstat`: 21 / 21）。
- 新しい値は今のファイルのバイト列から `Get-FileHash -Algorithm SHA256` で求めた（`step110\attribution-hashes.txt`）。`ATTRIBUTION.md` にはハッシュがないので、再生成は不要だった。
- 訳を削除したものはない。他の資産には触れていない。
- **注記**: STEP 108 の追記行は AI の支援で書いたもので、母語話者のレビューは受けていない。
  - `lt` と `vi` の `source` は「2026-10-15 の追加分は AI 支援で未レビュー」と書いている。
  - `pt-br` の `source` は「PdfCraft contributors が補完・レビュー」と書いている。
  - 指示どおり `source` の文言は変えていない。文言を追記するかは利用者の判断に委ねる。

## 8. 変更ファイル

| ファイル | 変更 |
|---|---|
| `crates/engine/src/rotate_guard.rs`（未追跡のまま） | 古い改訂の読み込みを `open_with_same_credentials` に変更（77 行目付近）。再構成ファイルのときだけ `raw_names_clear` を確認（54 行目）。`RAW_REFUSED_NAMES`、`raw_names_clear`、`ends_name` を追加。テストを 5 件追加 |
| `ATTRIBUTION.toml` | 翻訳 21 件の `sha256` だけ |
| `docs/SWIFTPDF_V0.9_REGRESSION_FIX.md` | この報告書（新規） |

製品の UI コード、コマンド、閲覧専用の設計、保存処理、`ROADMAP.md`、テストのアサーションは変えていない。

## 9. 新規テストと既存テストの結果

新規テスト（すべて `rotate_guard::tests`）。修正を戻したときに失敗することを確認済み（`step110\revert-*.txt`）:

| テスト | 内容 | 修正を戻すと |
|---|---|---|
| `an_authenticated_encrypted_file_reads_its_older_revisions` | 2 改訂・署名なしの暗号化 PDF は `Unsigned` | A を戻すと失敗 |
| `a_signature_freed_from_an_encrypted_file_is_still_refused` | 暗号化 PDF で、後の改訂で解放した署名辞書が古い改訂に残る場合は `ProtectedOrSuspect` | A を戻すと失敗 |
| `a_reconstructed_file_without_signature_names_rotates` | 署名名のない再構成 PDF は `Unsigned`。文字列の中の `/Sig` は拒否（安全側） | 一律拒否に戻すと失敗、走査を無効にしても失敗 |
| `a_reconstructed_file_hiding_an_older_signature_definition_is_refused` | 同じ番号の後の定義に隠された `/Type /Sig` は拒否。リーダーからは見えないことも確認 | 走査を無効にすると失敗 |
| `a_reconstructed_file_with_signature_shaped_names_is_refused` | `/S#69g`、`/ByteRange`、`/SigFlags`、`/Perms`、`/Encrypt`、`/ObjStm` はそれぞれ拒否 | 走査を無効にすると失敗 |

7 件の対象テストの修正後の結果は、7 件とも ok:

- `saving_a_password_protected_document_rebases_on_it`
- camera 3 件
- `fill_sign` 1 件
- `ui` 1 件
- `the_real_manifest_passes`

Focused 回帰はすべて成功した（`--test-threads=1`）:

| 対象 | 結果 |
|---|---|
| `pdfcraft-engine` lib | 324 passed / 1 ignored（`rotate_guard` 17 件を含む） |
| `pdfcraft-engine` `rotate_validation` | 4 passed |
| `pdfcraft-engine` その他の結合テスト | すべて ok |
| `pdfcraft-organize` | 128 passed |
| `pdfcraft-ui-egui` lib | 539 passed / 1 ignored |
| camera / fill_sign / ui | 6 / 27 / 55 passed |
| commands / view_only_commands / view_only_startup / view_only_output | 24 / 8 / 4 / 9 passed |
| safe_save / signing / stamps / editing | 15 / 11 / 4 / 129 passed（editing 4 ignored） |
| viewer / view / reload / recovery / control | 51 / 46 / 11 / 12 / 50 passed |
| content（`--test-threads=1`） | 15 passed |
| `cargo xtask assets` | ok（275 資産） |

品質ゲート:

| 項目 | 結果 |
|---|---|
| `cargo fmt --check` | ok |
| `cargo check -p pdfcraft -j 2` | ok |
| clippy `-D warnings --all-targets`（pdfcraft-organize、pdfcraft-engine、pdfcraft-ui-egui） | ok |

STEP 109 の CLI 再現スクリプトも、修正後のビルドで再実行した。対象は `a_noxref`、`c_plain`、`f_enc_inplace`、`g_plain_inplace`、`h_enc3_reopen` で、5 本すべて回転できた。`f` と `g` は STEP 109 と同じくその場保存なので、合成ファイル `enc3.pdf` と `plain3.pdf` に改訂が 1 つ増えた。

## 10. 全体テスト結果

`cargo test --workspace --no-fail-fast -j 2 -- --test-threads=1` を 1 回実行した。22:12〜23:31 の約 79 分で、ログは `step110\workspace.txt`。

| | テストバイナリ | passed | failed | ignored |
|---|---|---|---|---|
| STEP 109（修正前） | 178 | 3767 | 9 | 16 |
| STEP 110（修正後） | 178 | 3779 | 2 | 16 |

失敗した 2 件は、v0.7 からの既知の失敗である。理由も前と同じだった:

- `pdfcraft-ui-egui --test combine_grid` `arrow_keys_move_by_card_and_by_row_and_shift_extends`（ラベル `f8.pdf` が見つからない）
- `pdfcraft-ui-egui --test fonts` `chinese_ui_uses_an_installed_chinese_face`（"the app's own font"）

STEP 109 の 3767 + 9 = 3776 件に対し、今回は 3779 + 2 = 3781 件である。差の 5 件は §9 の新規テストに当たる。新規の失敗 7 件はすべて成功に変わった。

## 11. 新規の回帰

なし。全体テストの失敗は既知の 2 件だけで、新しい失敗は出ていない。

## 12. 署名保護と原本保護への影響

- 署名保護:
  - `is_signed` による `NotPermitted` は変えていない。
  - ガードの判定順と拒否条件（`/V`、`/Perms`、`/ByteRange`、`/SubFilter`、署名辞書、`ObjStm` 内の辞書、古い改訂、64 改訂の上限）も変えていない。
  - A によって、暗号化 PDF の古い改訂も実際に読んで検査するようになった。前は「読めない」で終わっていた。古い改訂に署名があれば `ProtectedOrSuspect` を返すことをテストで確認した。
  - B で回転できるようになるのは、生のバイト列に署名に関わる名前が 1 つもない再構成 PDF だけである。
- 原本保護と閲覧専用:
  - 保存経路、別名保存、`write_new_file`、閲覧専用の拒否、`set_view_only`、ページの削除・並べ替え・抽出には触れていない。
  - 関連テスト（safe_save、view_only_*、commands、rotate_validation）はすべて成功した。

## 13. 残課題

- §5 の判断（再構成 PDF に対する一律拒否を、安全確認つきの許可に狭めたこと）について、利用者の確認が必要。
- §7 の注記（STEP 108 の訳は AI 支援で未レビュー。`source` の文言は変えていない）。
- STEP 109 の監査で挙げた低リスク項目は、この手順の範囲外なので未対応のまま。
  - 1 ページ制限は UI 側だけにある。
  - 閲覧専用中の undo は組み立て系を再確認しない。
  - 閲覧専用でない undo とバッチにはガードがない。
- 既知の失敗 2 件（v0.7 から）は変わらず:
  - `combine_grid::arrow_keys_move_by_card_and_by_row_and_shift_extends`
  - `fonts::chinese_ui_uses_an_installed_chinese_face`

## 14. コミット可否

**条件付きで可。** テストと品質ゲートの観点ではコミットを妨げるものはない（新規の失敗は 0 件、既知の失敗 2 件だけ）。

ただし §5 の判断（再構成 PDF への一律拒否を、生のバイト列の確認つきで狭めたこと）は、利用者の承認を前提とする。承認されない場合は、B 群の 5 テストのフィクスチャに xref 表を付ける別案に切り替える。指示どおり、コミットと push はしていない。
