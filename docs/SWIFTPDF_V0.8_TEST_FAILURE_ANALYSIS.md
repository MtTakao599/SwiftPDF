# SwiftPDF v0.8.0 テスト失敗の原因調査

調査のみ。製品コード、既存テスト、`ROADMAP.md`、既存レポート3件は変更していない。コミットと push はしていない。

## 1. 調査日時・環境

- 日時: 2026-10-21（JST、17:40 以降）
- OS: Microsoft Windows NT 10.0.26200.0
- rustc: 1.96.0 (ac68faa20 2026-05-25)
- cargo: 1.96.0 (30a34c682 2026-05-25)
- フォント: `CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`
- ビルド並列: `CARGO_BUILD_JOBS=2`、cargo `-j 2`
- テスト並列: 単独は `--test-threads=1`。`content` と比較用の UI 一式は `--test-threads=8` も実施
- Noto Sans CJK SC Regular を埋め込まない警告は、AGENTS.md §1.1 の想定どおり
- 実業務 PDF、実印影、利用中の `seals.bin`、実プリンターは使っていない
- 開発ツリーの出力先: `D:\dev\SwiftPDF\target`
- 比較ツリーの出力先: `D:\dev\swiftpdf-v070-target`
- `Cargo.lock` は作業ツリーの差分に無い。依存クレートの版は両環境で同じ
- このセッションのシェルは `CARGO_TARGET_DIR=D:\dev\SwiftPDF\target` を継承する。比較ビルドは変数を明示して分けた

## 2. v0.7.0 比較環境

作成済み。元のブランチは切り替えていない。

| 項目 | 値 |
|---|---|
| パス | `D:\dev\swiftpdf-v070-baseline` |
| 状態 | detached HEAD |
| コミット | `b9aa25c0d74dcd00fa598e352d747bcec7ee52d4` |
| 作業ツリー | 変更なし |
| 出力先 | `D:\dev\swiftpdf-v070-target` |

最初の比較ビルドは継承された `CARGO_TARGET_DIR` のため開発ツリーの出力先を使おうとした。ロック待ちになった時点で停止し、上記の別ディレクトリでビルドし直した。開発ツリーのソースは戻していない。比較用ワークツリーは残してある。

## 3. 統合テスト4件の比較結果

いずれも `--test-threads=1` の単独実行。v0.8.0 は各2回。テストファイル `combine_grid.rs`、`control.rs`、`fonts.rs` は今回の差分に無い。`ui.rs` には STEP 93 のテスト追加があるが、4件目の関数本体は差分の外。

| テスト | v0.7.0 | v0.8.0 | 再現 |
|---|---|---|---|
| `arrow_keys_move_by_card_and_by_row_and_shift_extends` | 失敗 | 2回とも失敗 | 常時。両方同じ |
| `control_quit_retains_blocked_content_typing_after_cancel` | 失敗 | 2回とも失敗 | 常時。両方同じ |
| `chinese_ui_uses_an_installed_chinese_face` | 失敗 | 2回とも失敗 | 常時。両方同じ |
| `files_and_quit_from_the_operating_system` | 成功 | 単独2回成功。UI一式（55件、8スレッド）も成功 | STEP 96 の1回だけ失敗 |

失敗メッセージは両版で一致した。

### combine の矢印

`crates/ui-egui/tests/combine_grid.rs:199`（`card`）。問い合わせラベルは `f8.pdf`。アクセシビリティ木には `f0.pdf` から `f7.pdf` まであり、`f8.pdf` は無い。9枚目が画面外で木に出ていない。

### 終了確認

`crates/ui-egui/tests/control.rs:1453`。下書きは `Keep 世界` のまま。`ui.quit` の結果は `{"quitting":true,"needs_confirmation":false}`。期待は確認ありで終了しないこと。

`has_unsaved_work`（`editing.rs`、差分外）が未確定の追加テキストを未保存に数えるのは、`content.blocked()` が文字を返したときだけ。`blocked` は、今のフォントで PDF に書けない文字。`CRAFT_FONTS_REQUIRED=1` では `世界` は書け、未保存扱いにならない。終了はその下書きを残さない。

### 中国語フォント

`crates/ui-egui/tests/fonts.rs:257`。`expect("the app's own font")`。インストール済みの中国語フォントがあると、`installed_font_definitions` は全ファミリーへ `SYSTEM_FALLBACK` を入れる。craft-fonts の個別ファミリーは `vec![そのフォント名]` だけで、Inter も JetBrains Mono も無い。テストは全ファミリーに Inter または JetBrains Mono を要求する。`theme.rs` は差分外。

### OS からのオープン

単独では両版ともタブが1つ開く。v0.8.0 の UI テスト一式も、この再実行では 55 成功、0 失敗。STEP 96 では同じ関数がタブ数 0（期待 1）で1回失敗している。ファイルはハーネスの4フレームのあと削除する。開く処理が遅れると、削除後に読む。断続的で、今回は再現しなかった。

## 4. content.rs の異常終了

ターゲットは `pdfcraft-ui-egui` の `--test content`。テストは14件。`content.rs` は差分外。

| 実行 | 結果 |
|---|---|
| v0.8.0、1スレッド、14件 | 成功 |
| v0.8.0、問題の3件を1件ずつ | 成功 |
| v0.8.0、8スレッド | `0xc0000005`（`STATUS_ACCESS_VIOLATION`）。STEP 96 とこの再実行の2回 |
| v0.7.0、1スレッド、14件 | 成功 |
| v0.7.0、8スレッド | 2回とも14件成功 |

8スレッドで結果が出る前に落ちたのは、次の2件。単独ではどちらも成功する。

- `dragged_text_follows_the_pointer_before_release`
- `released_text_does_not_jump_back_before_the_page_is_redrawn`

Windows アプリケーションログ（読み取りのみ）:

- 障害アプリケーション: `content-b855e111b305f2e5.exe`
- 障害モジュール: `C:\WINDOWS\SYSTEM32\d3d10warp.dll` 10.0.26100.9711
- 例外: `0xc0000005`
- 時刻: 17:57:07

このバイナリ名は STEP 96 の異常終了と一致する。製品の `unsafe` は禁止されており、スタックは SwiftPDF の関数ではなく WARP（Direct3D のソフトウェア実装）で落ちている。`canvas.rs` の製品差分は、スクロールを両端で止める関数と、その1か所の呼び出し、およびテストである。GPU 資源の確保は差分に無い。

## 5. 原因分析

| テスト | v0.7.0 | v0.8.0 | 再現 | 原因 | 製品への影響 | 対応案 | 優先度 |
|---|---|---|---|---|---|---|---|
| combine の矢印 | 失敗 | 失敗 | 常時 | 確認済み。9枚目がアクセシビリティ木に無い。差分外 | 画面外の結合カードが木に出ない。矢印操作そのものの回帰ではない | テストはカードを表示してから問い合わせる。製品で画面外も公開するかは別判断 | 低 |
| 終了確認 | 失敗 | 失敗 | 常時 | 確認済み。描ける下書きは未保存に数えない。差分外 | フォントで描ける未確定テキストは、終了確認なしに捨てられる | 仕様を決める。未確定ならすべて確認するなら製品修正。書けない文字だけなら、テストの文字を書けないものにする | 中 |
| 中国語フォント順 | 失敗 | 失敗 | 常時 | 確認済み。個別ファミリーに Inter が無い。差分外 | 比例・等幅の並びが壊れた証拠ではない | テストは Proportional と Monospace だけを見る | 低 |
| OS オープン | 成功 | 単独と一式は成功。STEP 96 は1回失敗 | 断続 | 推定。オープンと一時ファイル削除のタイミング | 今回の再実行ではタブは開く | 再現したら、タブを確認してからファイルを消す。assertion は残す | 低 |
| content 8スレッド | 2回成功 | 2回クラッシュ | v0.8.0 では8スレッドで再現。1スレッドでは両方成功 | 未確定。落ちたモジュールは `d3d10warp.dll`。SwiftPDF 側の1行には特定できない | 利用者の1ウィンドウ描画ではなく、並列のヘッドレス描画。14件の動作は1スレッドで通る | このターゲットは `--test-threads=1`。再発するなら ui-egui のフレーム処理を二分する | 中 |

## 6. 製品への影響

v0.8.0 の差分が、この4件の断言を新たに壊した証拠は無い。3件は v0.7.0 でも同じメッセージで失敗する。

終了確認は、v0.8.0 で新しく入った穴ではない。ただし、描ける未確定テキストを確認なしに捨てる動作は、両版にある。印影の承認や原本保護とは別である。

アクセス違反は、v0.8.0 のテストプロセスを8スレッドで2回落とし、v0.7.0 の同じ条件2回では落ちなかった。PDF の保存や押印の失敗としては再現していない。テストプロセスの GPU ドライバ（WARP）で落ちている。

## 7. 推奨する修正順序

1. `content` の統合テストを1スレッドで実行する。8スレッドのクラッシュを、機能失敗と混ぜない。
2. 終了確認の仕様を決める。未確定テキストをすべて残すなら `has_unsaved_work`。書けない文字だけならテストの入力を変える。assertion は削除しない。
3. combine は、9枚目を表示してからラベルを取るか、画面外のカードを木に出す。
4. 中国語フォントのテストは、比例と等幅の並びだけを確認する。
5. OS オープンは、再発したときだけ、ファイル削除をタブ確認の後に移す。

今回は実装していない。

## 8. 再検証に必要なテスト

```powershell
$env:CRAFT_FONTS_DIR="D:\dev\craft-fonts"
$env:CRAFT_FONTS_REQUIRED="1"
$env:CARGO_BUILD_JOBS="2"
# 開発ツリー
$env:CARGO_TARGET_DIR="D:\dev\SwiftPDF\target"

cargo test -p pdfcraft-ui-egui --test combine_grid -- --test-threads=1 --exact arrow_keys_move_by_card_and_by_row_and_shift_extends
cargo test -p pdfcraft-ui-egui --test control -- --test-threads=1 --exact control_quit_retains_blocked_content_typing_after_cancel
cargo test -p pdfcraft-ui-egui --test fonts -- --test-threads=1 --exact chinese_ui_uses_an_installed_chinese_face
cargo test -p pdfcraft-ui-egui --test ui -- --test-threads=1 --exact files_and_quit_from_the_operating_system
cargo test -p pdfcraft-ui-egui --test content -- --test-threads=1
```

8スレッドの `content` は、クラッシュ調査のときだけ、1回に限る。

## 9. 未確認事項

- アクセス違反のコールスタックは、SwiftPDF の関数まで取っていない。障害モジュールは `d3d10warp.dll`
- v0.7.0 の8スレッド成功は2回。それ以上は繰り返していない
- `files_and_quit_from_the_operating_system` の STEP 96 の失敗は、今回の再実行では再現しなかった
- GUI の手動確認はしていない
- 比較用ワークツリーは削除していない

## 10. v0.8.0 のリリース可否

この4件は、v0.8.0 だけが壊した回帰としては扱わない。3件は v0.7.0 から同じ失敗で、1件は今回再現しなかった。

`content` の8スレッド異常終了は、v0.7.0 の2回では起きず、v0.8.0 では2回起きた。1スレッドでは14件とも成功する。PDF を壊す不具合としては特定できていない。テストスイートを8スレッドのまま緑とは扱えない。

リリース判断は、STEP 96 の既知の制限と、この WARP の差を分けて行う。この調査だけでは、製品コードの修正箇所は確定していない。
