# SwiftPDF v0.8.0 Windows GUI 受け入れ確認（STEP 96-E）

検証と報告のみ。製品コード、既存テスト、比較用ワークツリーは変更していない。コミット、push、マージはしていない。

## 1. 検証日時・環境

| 項目 | 値 |
| --- | --- |
| 日時 | 2026-10-21 午後（JST）。最初の起動ログは 19:58 JST |
| OS | Windows 10.0.26200（セッション名 `RDP-Tcp#0`） |
| リポジトリ | `D:\dev\SwiftPDF` |
| ブランチ | `feature/upstream-safety-v0.8.0` |
| HEAD | `b9aa25c0d74dcd00fa598e352d747bcec7ee52d4` |
| rustc | 1.96.0 (ac68faa20 2026-05-25) |
| cargo | 1.96.0 (30a34c682 2026-05-25) |
| フォント | `D:\dev\craft-fonts`（`CRAFT_FONTS_REQUIRED=1`） |
| 比較用ワークツリー | `D:\dev\swiftpdf-v070-baseline`（この手順では未変更） |
| 実行ファイル | `D:\dev\SwiftPDF-GUI-Test-v080\SwiftPDF.exe` |
| ビルド時刻 | 2026-10-21T10:55:44Z |
| サイズ / SHA-256 | 112,600,064 バイト / `44DB67C043D95451EF40821305C128E97F1F5249B17D0CE77D3F86788FD8E0A4` |

`git diff --check` は空（終了コード 0）。`cargo build -p pdfcraft -j 2` は成功。バイナリ名はパッケージの `[[bin]] name = "SwiftPDF"` に従う。`target\debug\pdfcraft.exe` は 2026-10-17 の別バイナリなので、検証には使っていない。

Noto Sans CJK SC を埋め込まない警告は、AGENTS.md §1.1 の既知の除外。

## 2. 検証用ポータブル環境の隔離確認

判定は `crates/ui-egui/src/portable.rs` の通り。実行ファイルのディレクトリに `portable.txt` または `SwiftPDF.portable` が**ファイル**としてあり、その隣の `Data` を作成して書き込めるときだけポータブルモードになる。作業ディレクトリを変えるだけでは有効にならない。設定の置き場所は `apps/pdfcraft/src/settings.rs` がポータブルの `Data` を先に返す。印影は `apps/pdfcraft/src/main.rs` が `settings_dir()/seals.bin` を開く。

配置したマーカーは `D:\dev\SwiftPDF-GUI-Test-v080\portable.txt`（ファイル、37 バイト）。

| 確認 | 結果 |
| --- | --- |
| 起動プロセスのパス | 成功。`D:\dev\SwiftPDF-GUI-Test-v080\SwiftPDF.exe` |
| ログのポータブル宣言 | 成功。`portable mode: settings are kept in D:\dev\SwiftPDF-GUI-Test-v080\Data` |
| 検証用 `Data` | 成功。ログのみ。`seals.bin` と `app.ron` はできていない（プロセスは保存前に終了した） |
| `%LOCALAPPDATA%\SwiftPDF\seals.bin` | 成功（未変更）。内容は読んでいない。長さ 2,013,998、最終更新 2026-10-20T21:20:48.1296673Z のまま |
| `%LOCALAPPDATA%\SwiftPDF\app.ron` | 成功（未変更）。長さ 1,389,594、最終更新 2026-10-20T21:21:09.7623330Z のまま |
| インストール版の `instance-Console.sock` | 成功（未変更）。最終更新 2026-10-20T21:18:40.7812795Z |
| `portable-instance-RDP-Tcp0.sock` | 更新された。長さ 0、最終更新 2026-10-21T11:00:09.4794029Z |

最後のソケットは、ポータブル起動がインストール版へファイルを渡さないための単一インスタンス用ソケット（`apps/pdfcraft/src/single_instance.rs`）。印影でも設定でもない。ロックファイルの時刻は変わっていない。

起動後、検証用プロセスは終了済み。`SwiftPDF` プロセスは残っていない。

## 3. テストPDF・PNGの準備結果

すべて `D:\dev\SwiftPDF-GUI-Test-v080` 内の合成物。実業務の氏名、実印影、実プリンターは使っていない。出力先は `output\`（空）。

最初の `pages-rotated.pdf` には相互参照表がなく、起動ログが再構成を記録し、画面に修復バナーが出た。その後、相互参照表付きに作り直し、同じファイルを開き直した。2回目のログに再構成の警告はない。ウィンドウ題名は `pages-rotated.pdf — SwiftPDF`。作り直した画面の目視は未実施。

| ファイル | 内容 | SHA-256 | 起動後 |
| --- | --- | --- | --- |
| `fixtures\pages-rotated.pdf` | 4ページ。回転 0° / 90° / 180° / 270°。各ページに TOP / MID / BOTTOM | `2D80929F2D57734392E241AD94A7F6147F54AE799AE6E2C8534E30BBA60FFA38` | 未変更 |
| `fixtures\layers-two.pdf` | OCG `LayerA`（赤）と `LayerB`（青）。既定は両方表示。`/AS` なし | `D2355BED0B76A7CB33D4B5BC4FE4160EC4403AA06977DF95618DF44CAE4B7CED` | 未変更 |
| `fixtures\blank-for-text.pdf` | 日本語入力用の1ページ | `50EC6AE5F80578F45CEBB1D1F29F1751650CBAA2B7457F4641E5BD87C83CF4C0` | 未変更 |
| `fixtures\original-protect.pdf` | 上と同じバイト列の原本 | 同じ | 未変更 |
| `fixtures\already-exists.pdf` | 上書き拒否の確認用ダミー | `434365F10B7ED515177791C32FE87AFB031FC5602C590F593909EC4E100A7DC5` | 未変更 |
| `images\stamp-mark.png` | 青い四角と `MARK` | `AB1B90A557781CF9F0CB894C4E4480B33787944AC313984AD163BED56606F9D2` | 未使用 |
| `images\chairman-mark.png` | 赤い円と `TEST` | `211276F958E510BC430BDCB4EFD50A19112A8658061F04684F0683DB6B2E8A71` | 未登録 |

## 4. 自動検証結果

指定の4つは、この作業ツリーで本日成功した。

| コマンド | 結果 |
| --- | --- |
| `cargo fmt --check` | 成功 |
| `cargo check -p pdfcraft -j 2` | 成功 |
| `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings` | 成功 |
| `cargo test -p pdfcraft-ui-egui --test content -j 2 -- --test-threads=1` | 成功。15 passed、0 failed、13.19s |

`content` の単一スレッド成功は、WARP の並列クラッシュを直したという意味ではない。8スレッドと、描画する3テストだけの並列は再実行していない。

関連テストも本日成功した。0件一致の `--exact` は成功に数えていない。

| 対象 | テスト | 結果 |
| --- | --- | --- |
| ズーム位置 | `zoom_anchor_scroll_stays_within_the_content`、`zoom_anchor_holds_across_pages_rotation_and_the_pointer`、`zooming_out_keeps_the_point_and_stops_at_both_ends`、`zoom_commands_anchor_the_centre_and_fit_stays_in_range` | 成功 |
| ダイアログのキー | `a_dialog_above_a_save_prompt_keeps_the_prompt`、`a_link_prompt_above_a_save_prompt_owns_its_keys`、`the_seal_effect_window_above_a_save_prompt_takes_escape`、`save_shortcuts_wait_until_the_dialog_closes` | 成功 |
| Escape と下書き | `a_front_prompt_keeps_an_added_text_draft`、`a_front_prompt_keeps_a_form_draft`、`a_front_prompt_keeps_a_comment_draft`、`a_front_prompt_keeps_a_paragraph_draft` | 成功 |
| 保存と上書き | `view_only_save_as_is_refused_until_granted_and_grants_nothing_else`、`output_names_are_one_file_in_the_chosen_folder`、`a_plain_name_is_created_and_a_bad_name_writes_nothing`、`an_existing_file_directory_or_link_is_left_unchanged`（ジャンクション）、`repeated_names_and_a_failed_later_file_leave_nothing_new` | 成功 |
| 理事長印 | `enter_does_not_skip_seal_approval_and_escape_clears_it`、`a_covering_dialog_refuses_the_stamp_commit`、`manual_rotation_commits_and_a_later_change_is_refused` | 成功 |
| 非表示レイヤーの印刷コピー | `a_layer_hidden_in_the_panel_stays_hidden_when_printed`、`a_hidden_layer_stays_off_when_the_file_also_has_a_print_usage`、`a_hidden_layer_stays_out_of_colour_and_grayscale_image_prints`、`uncommitted_text_on_a_hidden_layer_document_prints_without_the_layer` | 成功 |

v0.7.0 からある3件（結合グリッドの9枚目、描画できる下書きを終了確認が数えない、中国語フォント族のテスト）は、本日再実行していない。新たな失敗は、本日実行した範囲にはない。

## 5. GUI各項目の確認結果

マウスとキーによる操作は行っていない。起動してログとウィンドウ題名を見ることと、相互参照表が無い最初のファイルでページ文字列が見えたことだけが、画面で確認できた事実。7項目の操作そのものは未実施。

| 項目 | 自動テスト | Windows GUI |
| --- | --- | --- |
| 1. ズーム時の表示位置 | 成功 | 未実施 |
| 2. ダイアログの入力優先順位 | 成功 | 未実施 |
| 3. Escape と未確定下書き | 成功 | 未実施 |
| 4. 保存ショートカットと原本保護 | 成功（経路と拒否） | 未実施 |
| 5. 理事長印の承認・回転・かすれ | 成功（承認、前面、手動回転の無効化） | 未実施。印影は登録していない |
| 6. 非表示レイヤーの印刷プレビュー | 成功（印刷コピーの OCG） | 未実施。プレビュー画素は未確認 |
| 7. 日本語テキスト | 該当なし（この手順の自動テストは日本語の見た目を判定していない） | 未実施 |

最初の起動では、修復前の `pages-rotated.pdf` が開き、`Page 1 rotate 0 TOP` と `Page 1 MID`、倍率 `181%` が見えた。これはズーム操作の確認ではない。本家 #3362（表示モード切り替え時のスクロール親）は、今回のズーム固定と別件のまま。

## 6. ユーザーが手動で確認する項目

次の実行ファイルだけを使う。普段の SwiftPDF は終了したままにする。

`D:\dev\SwiftPDF-GUI-Test-v080\SwiftPDF.exe`

保存先は `D:\dev\SwiftPDF-GUI-Test-v080\output\` だけ。`fixtures\` のファイルは上書きしない。実プリンターへは出さない。理事長印の画像は `images\chairman-mark.png` だけを選ぶ。

1. **ズーム。** `fixtures\pages-rotated.pdf` を開く。修復バナーが出ないことを見る。ページ上端・中央・下端で Ctrl+ホイール。ポインタ付近の文字が別の場所へ飛ばないこと。右端の倍率、ズームイン、表示メニューの「ページ幅に合わせる」「ページ全体」（`Fit to width` / `Zoom to page level`）を使う。2〜4ページ（90° / 180° / 270°）でも同じ。スクロールが急に跳ばないこと。
2. **ダイアログ。** 文字を入れて未保存にし、ウィンドウを閉じる。保存確認の上に設定を開けた場合だけ、Enter が背後の保存を確定しないこと、Escape が前面だけを閉じること、1回のキーで両方が閉じないこと、Ctrl+S と Ctrl+Shift+S が文書を動かさないことを見る。設定を閉じたあと、通常のキーが戻ること。保存確認の上に別画面を開けない場合は、無理に重ねず「手動では重ねられない」と記録する。
3. **下書き。** `文字入力` で文字を入れ、確定しないまま設定を開く。Escape 1回で設定だけが閉じ、下書きと元に戻す履歴が残ること。続けて入力できること。ダイアログが無い状態の Escape で下書きが取り消せること。
4. **保存。** 文字かスタンプを入れる。Ctrl+S は閲覧専用として拒否されること。Ctrl+Shift+S または「名前を付けて保存」で確認が開くこと。キャンセルでは `output\` にファイルができないこと。新しい名前で保存できること。保存後に `original-protect.pdf` の SHA-256 が `50EC6AE5F80578F45CEBB1D1F29F1751650CBAA2B7457F4641E5BD87C83CF4C0` のままであること。保存先に `fixtures\already-exists.pdf` を指定すると拒否され、その SHA-256 が `434365F10B7ED515177791C32FE87AFB031FC5602C590F593909EC4E100A7DC5` のままであること。`output\` の新しい PDF を開き直せること。
5. **理事長印。** `images\chairman-mark.png` だけを登録する。未承認では確定できないこと。チェック後に確定できること。手動 -30° / +30°、ランダム回転、かすれ、加工前後。角度か位置を変えると再承認になること。Undo / Redo。別名保存して開き直し、印影が残ること。承認画面が前面にあるとき、背後が確定しないこと。
6. **レイヤー。** `fixtures\layers-two.pdf`。LayerA を表示、LayerB を非表示。印刷プレビューで青（LayerB）が無いこと。カラーとグレースケールを切り替える。LayerB を戻すとプレビューにも戻ること。閉じて開き直し、元のレイヤー状態であること。印刷しない。
7. **日本語。** `fixtures\blank-for-text.pdf` に、ひらがな、カタカナ、漢字、英数字混在を入れる。フォントは BIZ UDPGothic、BIZ UDPGothic Bold、Shippori Mincho、BIZ UDMincho。表示、位置、ズーム後も同じ PDF 位置、別名保存、開き直し、印刷プレビュー。豆腐、欠け、位置ずれを記録する。

操作後、`%LOCALAPPDATA%\SwiftPDF\seals.bin` と `app.ron` の最終更新が、上の時刻のままであることを確認する。検証用の印影は `D:\dev\SwiftPDF-GUI-Test-v080\Data\seals.bin` にだけできる。

## 7. 原本・既存ファイル保護の結果

| 確認 | 結果 |
| --- | --- |
| アプリからの保存 | 未実施。`output\` は空 |
| `original-protect.pdf` のハッシュ | 成功（起動の前後で同じ）。アプリの別名保存ではまだ検証していない |
| `already-exists.pdf` のハッシュ | 成功（未変更）。拒否ダイアログは未実施 |
| 自動テストの新規作成、既存拒否、ジャンクション、閲覧専用の上書き拒否 | 成功 |
| ファイルシンボリックリンク | 未実施（権限が必要で、この環境では未確認） |

## 8. 既知の制限

- `/AS` の PrintState は評価しない。非表示のまま印刷コピーへ出ることは自動テストで確認済み。
- 本家 #3362 の表示モード切り替えは対象外。
- 検索バーとアウトラインの名前変更は、ダイアログの背後でも Escape を見る経路が残っている。今回は直していない。
- 出力パスの検査は `write_files` の経路。ファイルシンボリックリンクは未確認。
- `content` を複数スレッドで描画すると、v0.7.0 と v0.8.0 の両方で WARP（`d3d10warp.dll`）が落ちることがある。単一スレッドの成功は、その修正ではない。
- v0.7.0 から同じ3件の UI テスト失敗がある。本日は再実行していない。
- ポータブルモードでも、単一インスタンス用ソケットだけは `%LOCALAPPDATA%\SwiftPDF\portable-instance-*.sock` を使う。

## 9. 未確認事項

- 7項目のマウス、ホイール、キーボードによる GUI 操作。
- 作り直した PDF の画面（修復バナーが消えたことの目視）。2回目はログとウィンドウ題名だけ。
- 印刷プレビューの画素、印影の見た目、日本語グリフ。
- 通常の UI で、保存確認の上に設定を重ねられるか。
- 権限パスワード画面が下書きのフォーカスを奪う操作。自動テスト `a_front_prompt_keeps_an_added_text_draft` は成功。
- 実プリンター。使っていない。
- クリーン終了時に検証用 `Data\app.ron` ができること。今回はプロセスを強制終了したため、そのファイルは無い。

## 10. v0.8.0 の最終判定に必要な残作業

セクション5までの「未実施」は、2026-10-21 時点で Cursor が画面操作をしていなかった記録である。ユーザーによる手動確認はセクション11に分けて記録する。

自動テストで見えている既知の制限（PrintState、#3362、検索バーとアウトラインの Escape、シンボリックリンク、WARP の並列、v0.7.0 からある3失敗）は、GUI 確認が終わっても別件のまま残る。

## 11. ユーザー手動確認（2026-10-22）

ユーザーから、Windows GUI の受け入れは問題なしとの報告を受けた。ここは画面で見た結果であり、セクション4の自動テストとは別である。

| 項目 | ユーザーの画面確認 | 自動テストとの区別 |
| --- | --- | --- |
| ズーム操作 | 問題なし | アンカーの単体テストは別途成功。画面のホイールとフィットはユーザー確認 |
| ダイアログ操作 | 問題なし | 保存確認の上に別画面を重ねたキーの単体テストは別途成功 |
| 下書き保護 | GUI 未確認 | `a_front_prompt_keeps_an_added_text_draft` など自動テストは成功。通常のメニュークリックで文字が確定するため、背後に下書きを残した Escape は画面では再現していない |
| Ctrl+S、Ctrl+Shift+S、別名保存 | 問題なし | 閲覧専用の拒否と既存ファイル拒否の単体テストは別途成功 |
| 理事長印の承認、回転、かすれ、Undo、Redo | 問題なし | 承認を飛ばさないテストと手動回転後の再承認拒否は別途成功 |
| 非表示レイヤーの印刷プレビュー | 合格 | 最初の `layers-two.pdf` は使っていない。修正版で確認した |
| 日本語テキストの追加、保存、再オープン | 問題なし | この項目の見た目は自動テストでは判定していない |

### レイヤー試験PDF

最初の `D:\dev\SwiftPDF-GUI-Test-v080\fixtures\layers-two.pdf` は、コンテンツストリームに `endstream` が無く、ページが白紙だった。レイヤー名だけが表示された。原因は検証用 PDF の生成ミスであり、SwiftPDF の製品コードではない。詳細は `docs/SWIFTPDF_V0.8_OCG_FIXTURE_ANALYSIS.md`。

ユーザーが合格とした印刷プレビューは、修正版 `D:\dev\SwiftPDF-GUI-Test-v080\fixtures\layers-two-fixed.pdf` である。元の `layers-two.pdf` は上書きしていない。
