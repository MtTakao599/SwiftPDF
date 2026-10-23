# SwiftPDF v0.9.0 相互参照（xref）安全化（STEP 112）

状態: **修正済み。署名検出の抜け（STEP 111 の 3 ケース）は塞いだ。案 A（利用者が承認）に従い、ページツリー復元ファイルの回転拒否に合わせて上流テスト 2 件を変更した。全体テストの失敗は既知の 2 件だけ。**

実施日: 2026-10-23。ブランチ `feature/page-rotate-v0.9.0`。基準 HEAD `d6c8f8084ef43445b46f39e5fc93f026e30621b1`。コミット・push はしていない。

ログは `D:\dev\SwiftPDF-rotate-validate\step112\` にある。

改訂履歴:
- 2026-10-23 初版（STEP 112、停止報告）
- 2026-10-23 案 A の承認を受けて上流テスト 2 件を変更し、ワークスペース一括テストを実行した
- 2026-10-23 STEP 113: GUI 確認、拒否文言の日本語の修正、検証ログの出力先の修正（15 節）

## 1. STEP 111 で見つかった問題

STEP 111 では、ページ回転ガード（`crates/engine/src/rotate_guard.rs`）が cos の解釈だけを見ていた。そのため、別のリーダー（レンダラーや他社ビューア）が別のオブジェクトを読む可能性のある PDF で、署名を見落としていた。

見落としていた 3 ケース:
- **hybrid 衝突**: 表と `/XRefStm` が同じオブジェクト番号に別の位置を与えている。
- **xref ストリームの修復**: hybrid の `/XRefStm` が読めず、cos が表だけで読んだ。
- **重複エントリ**: 1 つの xref セクションに同じ番号が 2 回載っている。

いずれも、cos から見えるオブジェクトは署名なし（`/FT /Tx`）だった。一方、他のリーダーが選びうるオブジェクトは `/FT /Sig` と `/Type /Sig` の辞書だった。それでも回転が許可されていた。

## 2. 検出方法

新しいパーサーは書いていない。cos が既に記録している診断を使う。

`rotate_guard::assess` は、最初に次の関数を評価する。

```rust
fn cross_references_unclear(doc: &Document) -> bool {
    !doc.repair_log().is_empty() || !doc.xref_oddities().is_empty()
}
```

これが真なら `Assessment::Indeterminate` を返し、回転を拒否する。判定できない場合は署名なしとして扱わない。

古いリビジョン（`open_with_same_credentials` で開き直した接頭辞）にも同じ判定を適用する。

## 3. cos の診断との対応

| 要求された拒否条件 | cos の診断 |
|---|---|
| xref 再構築が実行された | `repair_log`（「cross-reference … reconstructed」など） |
| xref 修復が記録された | `repair_log`（チェーンのループ、ヘッダー相対の再試行、`/Size` 超過など） |
| hybrid 衝突 | `xref_oddities`（表と `/XRefStm` の不一致） |
| 曖昧な参照（重複エントリ） | `xref_oddities`（同一セクション内の重複） |
| xref を評価できない | `repair_log`（hybrid `/XRefStm` が読めない、古いセクションが読めない） |

テストでは、cos がそれぞれの診断を実際に記録していることも確認している（7 節）。

## 4. 生バイト走査の扱い

STEP 110 で入れた生バイト走査は削除した（`RAW_REFUSED_NAMES`、`raw_names_clear`、`ends_name`）。これは「再構築ファイルに署名名が見つからなければ回転を許可する」ための根拠だった。

今は再構築されたファイルを常に拒否するので、走査は許可の根拠として不要になった。削除しても、xref に問題のある PDF が回転可能になることはない（7 節の拒否テストで確認）。

STEP 110 の単体テスト `a_reconstructed_file_without_signature_names_rotates` は、名前と期待値をより厳しい方向に変えた（`…_is_still_refused`、3 つとも Indeterminate）。これは STEP 110 で自分が追加したテストで、上流のテストではない。

## 5. 署名検出との関係

STEP 105/106 の、解析済みオブジェクトでの署名検出はそのまま残している。
- `/Perms`
- `/SigFlags`
- `/FT /Sig`
- `/Type /Sig` と `/DocTimeStamp`
- 古いリビジョンの検査
- 暗号化ファイルを同じ認証情報で開き直す処理

xref の判定はその前に置いた。xref が明確なときだけ、cos の解釈が他のリーダーの解釈と一致するという前提が成り立つ。その前提があって初めて、署名検出の結果を信用できる。

## 6. 変更したフィクスチャ

テストの補助関数 `with_xref` を追加した。xref を持たないインラインのフィクスチャに、正しい xref 表、trailer、startxref を付ける。

| テスト | 変更 |
|---|---|
| `crates/ui-egui/tests/camera.rs` | `harness()` が `with_xref(FIXTURE)` を開く（6 テスト） |
| `crates/ui-egui/tests/fill_sign.rs` | `image_signature_preview_follows_pointer_at_page_size_with_zoom_and_rotation` |
| `crates/ui-egui/tests/ui.rs` | `a_middle_click_closes_the_tab` |

次の点は変えていない。
- ページ数、内容、レイアウト
- 操作、assertion、テスト名
- 製品の読み込み処理

これらのテストは、いずれも xref の破損を意図的に試すものではない。`ui.rs` の共有 `FIXTURE` 定数と、意図的に切り詰めるテストには手を付けていない。

## 7. 追加したセキュリティテスト

`crates/engine/tests/rotate_security_review.rs` に次のテストを追加・変更した。

`ambiguous_or_repaired_cross_references_refuse_rotation`（拒否）:
- hybrid 衝突
- hybrid `/XRefStm` が読めない
- 重複エントリ
- `/Prev` のループ
- xref のない再構築ファイル
- 正常な xref と署名辞書
- 古いリビジョンにだけ署名辞書

`well_formed_cross_references_still_rotate`（許可）:
- classic xref（署名なし）
- 正常な xref ストリーム（cos の `write_full` 出力で、`/XRef` があり表がないことを確認）
- 正常な hybrid
- 2 回の通常の増分更新
- 3 リビジョンの暗号化ファイル

`reconstructed_files_refuse_every_signature_shape` の、署名なし・xref なしの 2 ケースは、許可から拒否に変えた。

拒否時には毎回、次の 3 点を確認している。
- バイトが同一（`Arc::ptr_eq`）
- dirty でない
- undo と redo がどちらもない

修正前の失敗も確認した。修正を外すと、7 ケースが誤って回転した（`step112\before-fix.txt`）。内訳は、STEP 111 の 3 ケース、ループ、xref なし、署名なしの再構築 2 ケース。許可側のテストは修正前も後も通る。

engine の単体テストにも追加した。`a_readable_cross_reference_with_a_repair_note_is_refused` は、読める xref に修復記録を加えると拒否されることを確かめる。

## 8. 通常 PDF の回転と保存

- classic、xref ストリーム、hybrid、増分更新、暗号化のどの通常形式でも回転できる。
- `rotate_validation`（4 件）も通る。回転、保存、再オープン、undo、redo を含む。
- STEP 2 の UI 回帰テスト（9 節）もすべて通る。

## 9. 原本保護と undo/redo

拒否はエンジン自体（`Session::apply` から `allow_page_rotate` を経て、ガードへ）で行う。view-only の undo と redo も、`allow_page_rotate_step` でガードを再評価する。

拒否時には、次のものが変わらない。
- バイト（`/Rotate` を含む）
- dirty フラグ
- undo/redo の履歴

新しい PDF も生成されない。`safe_save` の 15 件も通る。原本や既存ファイルへの書き込みはしていない。

## 10. 個別テストの結果

### 上流テストとの衝突と案 A

最初のステージ 1 では、上流のテスト `tests::a_recovered_page_tree_opens_unmodified_and_edits_the_pages_shown`（#1664、cccd55fa 由来）が新しく失敗した。

再現条件:
1. `fixture_without_page_tree()` を使う。xref は正しいが、カタログの `/Pages` が存在しないオブジェクト 9 を指している。
2. このファイルを開くと、レンダラーは走査で 2 ページを見つける。エンジンはそれに合わせて cos のページツリーを作り直し、リビジョンとして載せる。
3. その際、「the page tree is missing; its 2 pages were found by scanning the file」が `repair_log` に記録される。
4. `cross_references_unclear` は空でない `repair_log` をすべて拒否するので、回転は `Invalid(MESSAGE)` になる。

停止して報告し、利用者が **案 A（安全側）** を選んだ。ページツリーを復元したファイルは、cos とレンダラーで構造の解釈が食い違ったファイルである。その回転は拒否のまま保ち、上流テストを次のように変えた。

`crates/engine/src/tests.rs` `a_recovered_page_tree_opens_unmodified_and_edits_the_pages_shown`:
- 復元結果についての最初の 3 つの assertion はそのまま残した。
- 回転の `unwrap()` を、次の確認に替えた。
  - 回転が `Invalid(PAGE_ROTATE_SIGNATURE)` で拒否される。
  - 回転角は `[0, 0]` のまま。
  - dirty でなく、undo もない。
- 「表示どおりのページを編集できる」ことは、2 ページ目へのテキスト追加（`AddText { page: 1 }`）で示し、`added()` のページが `[1]` であることを確かめる。

ワークスペース一括テストでは、同じ原因の CLI 版 `pdfcraft-cli --test edit` `a_document_whose_page_tree_is_missing_edits_the_pages_the_reader_shows` も失敗した。利用者の承認を受けて、こちらにも案 A を適用した。
- `--rotate 2:90` は、次の 4 点を確認するように変えた。
  - 失敗終了する。
  - 標準エラーに「Page rotation is unavailable」が出る。
  - 出力ファイルが作られない。
  - 入力ファイルが元のバイトのまま。
- `--delete 1` のケースは、期待値（`[(300, 200, 0)]`）も含めて変えていない。clippy の「1 要素の for ループ」を避けるため、ループを展開しただけである。

どちらのテストも、ガードの xref 判定を一時的に無効にすると、新しい拒否の assertion で失敗することを確かめた。その後、元に戻している（SHA-256 一致）。

### ステージ 1

`cargo test -p pdfcraft-engine --no-fail-fast -j 2 -- --test-threads=1` を、テスト変更後に再実行した。

| 対象 | 結果 |
|---|---|
| engine lib | 325 通過 / 1 無視 |
| rotate_security_review | 5 通過 |
| rotate_validation | 4 通過 |
| その他の engine 結合テスト | すべて通過 |
| pdfcraft-cli `--test edit` | 12 通過 |

### ステージ 2

`--test-threads=1` で実行し、すべて通った。その後の変更はテストだけなので、結果は変わらない。

| 対象 | 結果 |
|---|---|
| ui-egui lib（日本語テキスト、日付印、会長印を含む） | 539 通過 / 1 無視 |
| camera | 6 通過 |
| fill_sign | 27 通過 |
| ui | 55 通過 |
| commands | 24 通過 |
| view_only_commands | 8 通過 |
| view_only_output | 9 通過 |
| view_only_startup | 4 通過 |
| safe_save | 15 通過 |
| stamps | 4 通過 |
| editing | 129 通過 / 4 無視 |
| content | 15 通過 |

## 11. ワークスペース一括テストの結果

`cargo test --workspace --no-fail-fast -j 2 -- --test-threads=1` を 1 回実行した（約 76 分）。engine テストを変更した後、CLI テストを変更する前の状態である。ログは `step112\workspace.txt`。

| | テストバイナリ | passed | failed | ignored |
|---|---|---|---|---|
| STEP 110 | 178 | 3779 | 2 | 16 |
| STEP 112 | 179 | 3784 | 3 | 16 |

テストバイナリが 1 つ増えたのは、`rotate_security_review` を追加したため。

失敗 3 件の内訳:
- 既知（v0.7 から、理由も同じ）: `combine_grid::arrow_keys_move_by_card_and_by_row_and_shift_extends`
- 既知（v0.7 から、理由も同じ）: `fonts::chinese_ui_uses_an_installed_chinese_face`
- 新規: `pdfcraft-cli edit::a_document_whose_page_tree_is_missing_edits_the_pages_the_reader_shows`。上記のとおり、承認を受けて案 A を適用した。`--test edit` の単独再実行では 12 件すべて通った。

ワークスペース全体は再実行していない。そのため、修正後の全体件数（期待値は 3785 通過 / 2 失敗）は数えていない。

## 12. fmt / check / clippy

| 項目 | 結果 |
|---|---|
| `cargo fmt --check` | 通過 |
| `cargo check -p pdfcraft -j 2` | 通過 |
| clippy `-D warnings --all-targets`: pdfcraft-organize | 通過 |
| clippy `-D warnings --all-targets`: pdfcraft-engine | 通過（テスト変更後に再実行） |
| clippy `-D warnings --all-targets`: pdfcraft-ui-egui | 通過 |
| clippy `-D warnings --all-targets`: pdfcraft-cli | 通過（CLI テストを変更したため追加で実行） |

## 13. 未検証事項とリスク

- ページツリーを復元したファイル（#1656 の種類）は、GUI でも CLI でも回転できなくなった。上流の動作からの意図的な後退である。削除などの他の編集はできる。
- xref と関係のない、エンジン実行時の注記でも回転を拒否する。対象は次の 3 つで、安全側の選択である。
  - 注釈の外観を描けなかった
  - `/Fields` にないフィールドを採用した
  - 挿入ページのフィールド名を変更した
- 正常な hybrid と xref ストリームは、cos が書いたファイルと手で組んだファイルで確認した。他社製ツールが書いたファイルは確認していない。
- 修正後のワークスペース全体の件数は、再実行していないので数えていない。
- 実際の業務 PDF、他社ビューアでの見え方、実プリンターは検証していない。

## 14. GUI とコミットの判定

**条件付きで可。**
- 署名検出の抜けは塞いだ。
- 通常の PDF は回転できる。
- 新しい失敗は、承認済みの案 A で解消した。
- 残る失敗は既知の 2 件だけ。

指示どおり、コミットと push はしていない。

## 15. STEP 113: GUI 確認とコミット前の整理

### GUI 確認の方法

製品の起動設定（`apps/pdfcraft/src/main.rs` と同じく、閲覧専用に 5 つの許可、日本語 UI）の実アプリを、egui_kittest で描画しながらメニュー操作した。

- 一時テストとして `crates/ui-egui/tests/` に置き、確認後に削除した。コミットには含めない。写しとスクリーンショットは `D:\dev\SwiftPDF-rotate-validate\step113\` にある。
- 実ウィンドウの操作はしていない。デスクトップ操作は 1 手ごとに承認が必要で、そのたびにウィンドウの前後が入れ替わるため、利用者の判断でこの方法に切り替えた。
- 別名保存は、ネイティブのファイル選択の代わりに `save_override` を使い、確認ダイアログ（`confirm_save_copy`）からは実際の経路を通した。
- フィクスチャは合成の 3 ファイル: 正常（classic xref）、xref 破損（再構築）、重複エントリ。

### 結果

正常な PDF:
- ページメニューの「このページを右に90°回転」から、90°、180°、270° に回せた。表示も正しい。
- 回すたびに dirty になり、Undo は「Rotate page」になった。Undo で 180°、Redo で 270° に戻った。
- Ctrl+S（`file.save`）は拒否される。別名保存は成功し、開き直すと 270° のままで、dirty も Undo も無い。
- 原本のバイトは変わっていない（SHA-256 一致）。

xref の修復、または曖昧な参照がある PDF（再構築、重複エントリの 2 種類）:
- メニュー項目は灰色になる。ポインタを乗せると、拒否の理由がツールチップに出る。
- 灰色の項目を押しても、`page.rotate`、`page.rotate_ccw`、Ctrl+Z を実行しても、回転角、dirty、Undo、Redo、保持しているバイト、ディスク上のファイルは変わらない。
- 再構築したファイルには、既存の「このファイルは破損していたため修復しました。保存すると修復済みの状態が保持されます。」の帯が出る。重複エントリのファイルは修復記録が無いので、帯は出ない。

他の編集:
- 表示の回転は使える。PDF には書き込まない。
- テキストの追加、Undo、Redo、別名保存、開き直しは、どちらの拒否 PDF でもできた。
- テキストを追加した後も、回転は拒否されたままである。

### 拒否時の文言

日本語だけを、内部構造の修復にも当てはまる文に変えた（`commands.rs` の `page_rotate_signature_message`）。判定ロジックは変えていない。

> ページの回転はできません。この PDF には電子署名があるか、その可能性があります。内部構造が修復された PDF も、署名の有無を確かめられないため、安全のため回転できません。署名が有効かどうかは確認していません。

英語は、エンジンの `PAGE_ROTATE_SIGNATURE` のままにした（エンジンを変えないため）。

### 検証ログの出力先

`rotate_validation.rs`、`rotate_security_review.rs`、`rotate_guard.rs` の性能テストは、検証ログを `D:\dev\SwiftPDF-rotate-validate` に固定で書いていた。
- `rotate_validation.rs` は、D ドライブの無い Windows で `unwrap` が失敗する。
- Linux の CI では、リポジトリ内にゴミのフォルダを作る。

テストの出力先だけを、`SWIFTPDF_VALIDATE_DIR` があればそこ、無ければ OS の一時フォルダに変えた。製品コードと判定は変えていない。

### 残るリスク

- 再構築された PDF にテキストを追加して別名保存すると、全体を書き直した xref の正しい新しいファイルになる。そのコピーは回転できる。
  - 既存の別名保存の動作で、上の帯でも予告されている。
  - コピーはどのリーダーでも同じ読み方になり、ガードはコピーでも署名を検査する。原本は変わらない。
  - 重複エントリのファイルは増分保存なので、コピーも拒否のままである。
- 重複エントリの PDF は、cos の修復記録が無いのに、拒否文言に「内部構造が修復された PDF」と出る。厳密には「食い違いのある」構造である。
- 実ウィンドウ（Windows デスクトップ）での確認は、利用者が行う。

