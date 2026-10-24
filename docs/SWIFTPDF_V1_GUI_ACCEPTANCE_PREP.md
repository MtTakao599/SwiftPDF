# SwiftPDF v1.0.0 STEP 117E — Windows 実GUI受け入れテスト環境の準備

状態：**117E/F の実 GUI 4 項目は OK。117G の G1〜G5 は利用者がすべて OK と報告。合わせて実 GUI の主要 9 項目を確認済み。E4/E5 は未実施。並列 I/O 1/46 は未解決。この工程だけで v1.0.0 の公開は承認しない。**

Cursor が検証用ポータブル版を用意したこと、kittest が仮想画面で確認したこと、ストア単体試験、利用者の実ウィンドウ操作は、それぞれ別である。`TEST_CHECKLIST.md` の全項目を OK にはしない。

改訂：2026-10-24（117E 準備）。117F で利用者報告の 4 項目を追記。117G で重点手順を案内。117H で利用者報告の G1〜G5 を追記。`ROADMAP.md` は変更していない。

確認に使った exe は `D:\dev\SwiftPDF-seals-validate\step117e\SwiftPDF.exe`（SHA-256 `F5FA5862EB596C823D3161C0475D8B6ED60DE78638B643E9CDBC807F2C768BBD`）。このバイナリと、以後のコード変更後のバイナリは別である。

## 0. STEP 117F：利用者が実 GUI で確認した項目

報告者：利用者。実施場所：Windows 実ウィンドウ。検証 exe は上の 117E ポータブル版。

| 項目 | 結果 | 根拠 |
|---|---|---|
| 2 つのウィンドウから登録した印影が両方残る | **OK** | 利用者の実 GUI |
| 別ウィンドウで削除した印影を、古い選択から押印できない | **OK** | 利用者の実 GUI |
| SWSEAL1 の移行を延期すると元データが保持される | **OK** | 利用者の実 GUI |
| 移行を実行すると SWSEAL2 とバックアップが作成される | **OK** | 利用者の実 GUI |

これ以外は、この報告では OK にしない。

| 区分 | 項目 |
|---|---|
| 利用者の実 GUI | 上の 4 項目に加え、下の 0b（G1〜G5） |
| Cursor の起動確認（操作なし） | 117E の隔離起動。ページ操作・押印・保存はしていない |
| egui_kittest | 移行画面、ストア待ち中の描画、要復旧バー、× ボタン（117D） |
| ストア単体試験 | 競合拒否、履歴、`use_*` の遷移、遅延書き戻し、要復旧の出力拒否 |
| 未確認（個別の実 GUI 根拠なし） | E4 保存失敗終了、E5 要復旧終了、基本操作 A の各項、使用履歴の間接確認 |

## 0b. STEP 117G：利用者が実 GUI で確認した項目

報告者：利用者。実施場所：Windows 実ウィンドウ。検証 exe は上の 117E ポータブル版（再ビルドなし。SHA-256 は同じ）。製品コードは 117G では変えていない。

細目は `D:\dev\SwiftPDF-seals-validate\step117e\TEST_CHECKLIST.md` の G。利用者は G1〜G5 をすべて OK と報告した。G の各チェック項目を、報告にない個別観察で補っていない。

| 項目 | 結果 | 根拠 |
|---|---|---|
| 1. 検証バイナリと SHA-256 | 確認済み（再ビルドなし） | 117E の exe を継続使用 |
| 2. データの隔離 | ランチャー `-Check` 合格 | 117E/G の隔離確認。実データには触れていない |
| 3. 押印と承認（G1） | **OK** | 利用者の実 GUI |
| 4. Undo／Redo（G2） | **OK** | 利用者の実 GUI |
| 5. 一般スタンプとの混在（G2） | **OK** | 利用者の実 GUI |
| 6. 別名保存（G3） | **OK** | 利用者の実 GUI |
| 7. 再オープン（G3） | **OK** | 利用者の実 GUI |
| 8. × ボタン終了（G4 / G5） | **OK** | 利用者の実 GUI |
| 9. 未保存時の確認（G4） | **OK** | 利用者の実 GUI |
| 10. 故障注入（E4 / E5） | **未実施** | スイッチなし。自動試験で確認済み。実 GUI では再現しない |
| 11. 並列テスト | 未解決リスクとして保持（46 回中 1 回） | 117F。117H でも再現せず、原因確定とはしない |
| 12. 公開判断 | **しない** | 1/46 と E4/E5 実 GUI。117H の Class C は監査/PDF の矛盾ではなく、着地済み確定を取り消す現行 |

117E/F の 4 項目と合わせ、実 GUI の主要 9 項目を確認済み。

---

## 1. テスト環境

| 項目 | 値 |
|---|---|
| 作業リポジトリ | `D:\dev\SwiftPDF` |
| ブランチ | `feature/seal-data-safety-v1.0.0` |
| 基準 HEAD | `c740330f9237fd83abe4bf5508bce1f348f1f757` |
| 検証フォルダー | `D:\dev\SwiftPDF-seals-validate\step117e` |
| OS | Windows 10.0.26200 |
| フォント入力 | `CRAFT_FONTS_DIR=D:\dev\craft-fonts`（必須） |
| ビルド出力 | `CARGO_TARGET_DIR=D:\dev\SwiftPDF-seals-validate\step117a\target` |
| 通常のアプリデータ | `%LOCALAPPDATA%\SwiftPDF`（**触れない**） |

通常フォルダーには利用中の `seals.bin` がある。117E はその中身を開いていない。存在と更新日時の STAT だけ行った。

STEP 117B〜117D の未コミット変更と、STEP 115〜117D の報告書はそのまま残してある。reset / clean / stash / rebase はしていない。コミットと push はしていない。

## 2. ビルド対象とコミット

検証 exe は STEP 117D で作った release をコピーした。117E で製品の機能は増やしていない。

| 項目 | 値 |
|---|---|
| 実行ファイル | `D:\dev\SwiftPDF-seals-validate\step117e\SwiftPDF.exe` |
| 移行試験用 | `step117e\migration-test\SwiftPDF.exe`（同じファイルへのハードリンク） |
| サイズ | 97,083,904 バイト |
| SHA-256 | `F5FA5862EB596C823D3161C0475D8B6ED60DE78638B643E9CDBC807F2C768BBD` |
| 元の成果物 | `step117a\target\release\SwiftPDF.exe`（2026-10-24 4:56） |
| パッケージ名 | Cargo 上は `pdfcraft`。Windows の製品名は `SwiftPDF.exe` |

117D 以降の差分は試験と control の待ち方、117E のフィクスチャ書き出し試験、書式だけである。exe の中身は 117D のままである。

## 3. データ隔離の確認方法

製品の保存先は次のとおり（コード）。

- `portable.txt` が exe の横にあると、設定・ログ・復旧・理事長印は `<exe>\Data`（`portable.rs`、`main.rs` の `settings_dir` → `set_seal_dir`）。
- 単一インスタンス用ソケットだけは、ポータブルでも `%LOCALAPPDATA%\SwiftPDF\portable-instance-*.sock`（`single_instance.rs`）。通常版のソケットと名前は分けるが、フォルダーは通常のアプリデータと同じ。
- そのため、検証起動は子プロセスだけ `LOCALAPPDATA` / `APPDATA` / `TEMP` / `TMP` を `step117e\env\*` へ向ける。ユーザー環境変数とシステム環境変数は変えない。レジストリのアプリ設定も書かない。

起動スクリプトの確認（`launch-isolated.ps1 -Check`）：

```text
exe          <validate>\step117e\SwiftPDF.exe
Data         <validate>\step117e\Data
LOCALAPPDATA <validate>\step117e\env\Local
real user    %LOCALAPPDATA%  (not used by the child)
CHECK ONLY: not launched
```

`migration-test` も、専用の `Data` と `env\Local` で同じ確認をした。

隔離の実行確認（Cursor。人の操作ではない）：

1. 通常フォルダーの `seals.bin` / `app.ron` / ソケットの更新日時を記録した。
2. ランチャーで検証 exe を起動した（pid 2884、`--language ja`）。
3. ログは `step117e\Data\logs\pdfcraft.log` に出た。先頭は `portable mode: settings are kept in D:\dev\SwiftPDF-seals-validate\step117e\Data`。
4. ソケットは `step117e\env\Local\SwiftPDF\portable-instance-Console.sock` に出た。
5. 通常フォルダーの監視対象はすべて未変化。その後プロセスを終了した。

通常の `seals.bin` へは到達していない。隔離できない場合は起動しない、という条件は満たしたうえで、隔離確認のための短い起動だけ行った。ページ操作はしていない。

Windows が Save As の「最近使ったフォルダー」をレジストリへ書くことは、人が別名保存したときに起きうる。アプリ自身の印影と設定の隔離とは別である。

## 4. ポータブル版の構成

```text
step117e\
  SwiftPDF.exe
  portable.txt
  START_SWIFTPDF_TEST.cmd
  START_SWIFTPDF_TEST_WINDOW2.cmd
  START_MIGRATION_TEST.cmd
  launch-isolated.ps1
  README_TEST.md
  TEST_CHECKLIST.md
  Data\                 検証用の設定・ログ・理事長印
  env\Local|Roaming|Temp
  samples\              合成 PDF と合成印影（上書きしない）
  output\               保存先
  screenshots\
  logs\
  migration-test\       移行専用（別 Data・別 env）
```

既存の検証ファイルは無条件に上書きしていない。フィクスチャ書き出しは `create_new` で、既にあるファイルは作らない。

## 5. 起動スクリプト

人が使うのは次の 3 つだけ。`SwiftPDF.exe` を直接開かない。

| ファイル | 用途 |
|---|---|
| `START_SWIFTPDF_TEST.cmd` | 通常の検証窓 |
| `START_SWIFTPDF_TEST_WINDOW2.cmd` | 同じ Data の 2 つ目（`--new-window`） |
| `START_MIGRATION_TEST.cmd` | 合成 SWSEAL1 の移行（`migration-test`） |

`launch-isolated.ps1` が共通の安全確認をする。

- 根が `D:\dev\SwiftPDF-seals-validate\step117e` または `...\migration-test` であること
- `SwiftPDF.exe` と `portable.txt` があること
- `Data` と `env` を検証フォルダー内に作ること
- 本物の LocalApplicationData は `GetFolderPath` で取る（親が `LOCALAPPDATA` をリダイレクトしていても、本物のプロファイルと取り違えない）
- リダイレクト先が通常の `SwiftPDF` フォルダーなら起動しない
- 開く PDF の引数は検証フォルダー内だけ

失敗時は窓を開かず、`NOT LAUNCHED:` と理由を出す。

## 6. 合成 PDF 一覧

実業務 PDF は使っていない。`samples` の原本は上書きしない。保存は `output`。

| ファイル | 内容 | 確認したこと |
|---|---|---|
| `samples\normal.pdf` | 1 ページ。Helvetica。枠と `TOP - SwiftPDF synthetic test` | xref 正常。回転は Available |
| `samples\multi-page.pdf` | 5 ページ（Page ONE … FIVE） | 同上 |
| `samples\rotate-test.pdf` | 2 ページ。2 ページ目は `/Rotate 90` | 回転は Available |
| `samples\repaired-xref.pdf` | startxref を +7 | 開くと修復扱い。回転は Signature（拒否） |
| `samples\stamp-test.pdf` | 2 ページ。押印用 | 回転は Available |
| `samples\unsaved-test.pdf` | 1 ページ。未保存終了用 | 回転は Available |
| `output\save-fail\readonly-target.pdf` | 読み取り専用のコピー | 故障注入スイッチはない。E4 では使わない |

各ページに `synthetic data, not a real document` と書いてある。

## 7. 合成印影一覧

実在の理事長印は使っていない。描画は製品の日付印レンダラー。文字は試験用。

| ファイル | 表示 | 用途 |
|---|---|---|
| `samples\seals\test-seal-A.png` | テスト印A / 合成データ | 登録・押印 |
| `samples\seals\test-seal-B.png` | テスト印B / 検証用 | 見た目の違う 2 つ目 |
| `samples\seals\test-seal-C.png` | テスト印C / 削除再登録用 | 競合 |
| `samples\seals\old-format-seal.png` | 旧形式テスト印 / 移行試験 | 合成 SWSEAL1 |
| `migration-test\Data\seals.bin` | 旧形式 1 件、使用履歴 3 件 | 移行案内 |
| `samples\late-legacy-write\seals.bin` | 旧形式 + 「旧版で追加したテスト印」 | 移行後の書き戻し通知 |

監査と確認画面に「合成」「テスト印」と出る。実在の個人・団体名は入れていない。

## 8. GUI 受け入れテスト項目

手順と結果欄は `step117e\TEST_CHECKLIST.md`。画面の日本語はコード上のラベルどおり。

- A. 開く、ページ移動、ズーム、日本語テキスト、一般スタンプ、データー印、表示回転、保存される回転、修復 xref、Undo/Redo、別名保存、印刷プレビュー（印刷しない）
- B. 合成印影 A/B の登録、選択、承認、押印、Undo/Redo、別名保存、再オープン、使用履歴（一覧 UI はない。間接確認）
- C. 2 窓での登録・削除・古い承認の拒否・同名再登録
- D. 合成 SWSEAL1 の案内、延期、移行、バックアップ、墓石、旧版書き戻しの通知
- E. × ボタン（通常 / 未保存 / 処理中）。E4 保存失敗終了と E5 要復旧終了は、安全な検証スイッチがないため **未実施** とする

結果は OK / NG / 未実施。NG は再現手順と画面表示を書く。画面写真は `screenshots`。合成データだけを写す。

## 9. × ボタンの確認方法

1. `START_SWIFTPDF_TEST.cmd` で開く。
2. 変更していないタブだけなら × で確認なしに閉じる（E1）。
3. `samples\unsaved-test.pdf` を開き、「文字入力」→ 何か書く →「文字入力を終了」→ ×。「閉じる前に「unsaved-test.pdf」への変更を保存しますか？」「保存しないと変更内容は失われます。」ボタンは「保存」「保存しない」「キャンセル」（E2）。
4. 理事長印を承認して「確定」した直後に ×。「理事長印の処理が終わるまで終了できません。」（E3。書き込みは短いので間に合わなければ未実施）。

E4 / E5 は通常 GUI から作らない。自動試験側で確認済み。

## 10. 複数起動の確認方法

1. 1 つ目は `START_SWIFTPDF_TEST.cmd`。
2. 2 つ目は `START_SWIFTPDF_TEST_WINDOW2.cmd`（同じ Data、`--new-window`）。
3. 2 つ目で `テスト印C` を登録する。1 つ目で「最新の内容を読み込む」。
4. 1 つ目で `テスト印B` を「選択中の理事長印を削除…」。
5. 2 つ目の古い選択からは押せない。「この理事長印は仮置きの後に変更または削除されたため、押印しませんでした。仮置きをやり直してください。」
6. 同じ名前で再登録した印には、削除前の承認は使えない。

通常版の SwiftPDF は起動しない。手順は WINDOW2 を使う。

## 11. SWSEAL1 移行の確認方法

**通常の検証窓はすべて閉じてから** `START_MIGRATION_TEST.cmd` を使う。別の `Data` を持つ。

1. スタンプ → 理事長印。「理事長印のデータが旧形式です。」「旧形式のデータ：理事長印 1 件、使用履歴 3 件」
2. 「移行について確認…」。開示 6 文は `seal_migration_disclosures` のとおり（チェックリスト D に全文）。
3. 「延期」→「移行を延期しました。理事長印のデータは変更していません。移行するまで理事長印は使えません。」`seals.v2.bin` はできない。
4. もう一度確認 →「移行する」→「理事長印のデータを新しい形式へ移しました。元の暗号化ファイルは … として残してあります。」
5. 一覧に `旧形式テスト印`。`migration-test\Data` に `seals.v2.bin` と `seals.bin.swseal1-*.bak`。`seals.bin` は墓石。
6. 任意：`samples\late-legacy-write\seals.bin` を墓石の上にコピーし、「最新の内容を読み込む」。「以前の SwiftPDF が理事長印の旧ファイルに書き込んでいました。…」`旧版で追加したテスト印` は増えない。

本物の旧版 SwiftPDF は起動しない。

## 12. 並列試験失敗の調査結果

117D で「ライブラリ全体の並列実行 18 回中 1 回、名前未記録」と残していた失敗は、同じ系列の 40 回ログのうち **run 18** として残っていた（`step117e\logs\parallel\fail-18.txt`）。

### 失敗した試験（2 件、同じ 1 回）

1. `seal_flow::tests::a_stamp_whose_tab_or_page_changed_while_preparing_is_cancelled`
   - 期待：タブ変更後に「文書か位置が変わった」でキャンセル
   - 実際：`使用履歴を記録できないため、押印しませんでした。理事長印のファイルを書き出せませんでした。`
   - 種類：`VaultError::Io(WRITE_ERROR)`。書き込みそのものが失敗
2. `stamps_ui::effect_preview_tests::a_processed_chairman_seal_commits_the_preview_png`
   - 期待：`use_committed`
   - 実際：`use_cancelled`。`使用履歴を記録できないため、押印しませんでした。保存した理事長印のファイルを読み直して確認できませんでした。`
   - 種類：`VaultError::Verify`。書いた直後の読み戻しが一致しない、または読めない

同時に走っていたのは、同じプロセスの `seal_flow` / `seal_store` / `seal_store_v2` / `stamps_ui` と、ディスクを使う他の UI 試験（結合グリッドのジャンクション、staging 保存など）。593 試験を `--test-threads` 既定（CPU 数）で並列実行していた。

### 切り分け（環境依存と決めつけない）

| 疑い | 結果 |
|---|---|
| 試験名不明 | **特定した**（上記 2 件） |
| 15 ms の時間 assertion（`a_briefly_held_file_is_retried_a_bounded_number_of_times`） | その回は成功。今回の失敗ではない |
| 117D の control 期限（RST） | その回は成功。別件で修正済み |
| `fail_writes_after` の漏れ | フックは vault ごと。失敗時の文言は `test: stopped` ではない。フック漏れではない |
| ロック待ち（Busy） | 文言は「ほかの SwiftPDF が…使用中です」ではない |
| 試験ディレクトリの共有 | 各試験は `SealDir`（`temp\swiftpdf-sealui-test-<pid>-<name>-<id>`）。通常の `%LOCALAPPDATA%\SwiftPDF` は使わない |
| 同じ `seals.bin.lock` | ロックはフォルダーごと。名前の衝突は 8 hex（約 32 bit）。今回の 2 件は `target` と `commit` で別フォルダー |
| データ競合で押印が通った | **通っていない**。製品は押印を拒否し、2 件目は `use_cancelled` を残した |
| 監査漏れ | 開いたままの use を残した証拠はない。失敗した側は記録できない旨を出して止まっている |
| 保存の不整合を製品が無視 | 読み戻し不一致は `Verify` で拒否する実装。その経路が動いた |

重点再実行：失敗した 2 件と、書き込み失敗を注入する近傍 2 件を 8 スレッドで 12/12 成功。4 件だけでは再現しない。失敗は **ライブラリ全体を並列にしたとき** にだけ観測されている。同じ系列は 40 回中 1 回（run 18）。117E で追加した全体実行は 3/3 成功。合計 **43 回中 1 回**。

製品側の書き込みは、共有違反とアクセス拒否を約 2 秒再試行する（ウイルス対策が直後のファイルを開く場合向け）。`Verify` の読み戻しには同じ再試行はない。今回、試験の意味を弱めずに直せる最小の修正は見つかっていない。再試行を足すと、本当の上書きを隠す恐れがある。assertion は弱めていない。

**停止条件：** データ競合・監査漏れ・保存不整合が「製品が壊れた結果を通した」と判明したわけではない。安全側の拒否が並列負荷で誤って発火した、という観測である。GUI 準備は止めない。ただし原因の最終特定はできていない。**未解決のまま残す。v1.0.0 を出せるとは判断しない。**

## 13. 未確認事項

- 利用者の実 GUI で確認したのは 0 章の 4 項目だけ。それ以外の実 GUI は未確認。
- E4（保存失敗での終了）と E5（要復旧での終了）は、安全な GUI スイッチがない。
- 使用履歴の一件ずつの一覧 UI はない。
- 並列全体実行は 46 回中 1 回が未解決（117F）。
- 48 MiB 超ストアの遅さ（117D 11 章）は今回の対象外。
- WARP 並列描画クラッシュは既知。描画試験は単一スレッド。今回は描画試験を再実行していない。
- Cursor はクリック承認のたびに前面に出るため、実窓のクリック操作はできない。

## 14. 人による確認が必要な項目

117G の重点は `TEST_CHECKLIST.md` の **G1〜G5**。最低限の流れ：

1. エクスプローラーで `D:\dev\SwiftPDF-seals-validate\step117e` を開く。
2. `START_SWIFTPDF_TEST.cmd` をダブルクリックする。`SwiftPDF.exe` は直接開かない。
3. 右上を「簡単」のままにする。
4. 「開く」で `samples\stamp-test.pdf` を開く。
5. 「スタンプ」→「理事長印」で `テスト印A` を選び、仮置きして承認し「確定」する。
6. Ctrl+Z / Ctrl+Y。一般スタンプを足して Undo/Redo する。
7. 「名前を付けて保存」で `output\117g-stamped-after-redo.pdf`。`samples` は上書きしない。開き直す。
8. `samples\unsaved-test.pdf` に日本語を書いて × →「キャンセル」→ もう一度 × →「保存しない」。
9. 編集なしで起動し直し、× で閉じる。
10. 結果は OK / NG / 未実施。NG ならそこで止める。

合成データ以外を開かない。実プリンターへ印刷しない。通常の SwiftPDF は起動しない。

## 15. 次の工程の判断基準

- 人がチェックリストを埋め、A〜D と E1〜E3 に NG がない → GUI 受け入れの結果を別報告に書いてよい。
- NG がある → その項目を直すまで受け入れ完了としない。
- 12 章の並列失敗が、データ競合・監査漏れ・保存不整合だと後から判明した → 受け入れとリリースを止める。
- 12 章が未解決のまま → **リリース可能とは言わない。** GUI 準備は済んでいても、v1.0.0 の判定は保留。
- E4 / E5 は未実施のまま残してよい（スイッチがない）。要復旧は自動試験で確認済み、と明記する。
- コミット・push・main へのマージ・タグ・GitHub Release・正式 MSI は、人が GUI を確認し、並列失敗の扱いを決めたあと。
