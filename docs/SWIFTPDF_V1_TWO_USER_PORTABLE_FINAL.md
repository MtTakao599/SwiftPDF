# SwiftPDF 1.0.0 二人用ポータブル版（STEP 128）

これは弁護士の意見ではない。同梱した許諾文は、配布元のファイルとバイト列を照合したものだけを書く。推測で条文は作っていない。加藤さんへは渡していない。

Status: portable ZIP rebuilt with the five missing notices and a corrected `portable.txt`. No product-code change, no commit, no distribution.

Revision history:
- 2026-10-27 — STEP 128 resumed and closed, on `e503124b`. The ZIP is `D:\dev\SwiftPDF-package-validate\step128\SwiftPDF-1.0.0-windows-x64-portable.zip`.

## 1. 再開時の Git 状態

中断前の確認（ライセンス原文のコピーより前）:

| Item | Value |
|---|---|
| Branch | `feature/seal-data-safety-v1.0.0` |
| HEAD | `e503124bd9adf502d7f027f49a0b3b3eed4f3f69` |
| `git diff --check` | 問題なし |
| 未追跡 | `docs/SWIFTPDF_V1_PACKAGE_SAFETY_AUDIT.md`, `docs/SWIFTPDF_V1_TWO_USER_LICENSE_AUDIT.md`, `docs/images/SwiftPDFiconORG.png` |

想定外の変更はなかった。HEAD は作業後も同じである。`SwiftPDFiconORG.png` は開いていない。

## 2. STEP 128 の既存成果

STEP 126 は、既知の EXE（`551D6DE1…`、製品コミット `7db6ef69`）から ZIP と MSI を作り、インストールはしていない。`portable.txt` が「`%LOCALAPPDATA%` に何も書かない」「フォルダーごとコピーすれば設定ごと移る」と書いており、実測（ソケットは `%LOCALAPPDATA%\SwiftPDF`、印影は現在ユーザーの DPAPI）と合っていなかった。

STEP 127 は、その ZIP を2人に渡すときの許諾を監査し、次の5つが ZIP に無いと評価した。一般公開は対象外。

| 不足 | STEP 127 の評価 | 再開前 |
|---|---|---|
| Ubuntu Font Licence 1.0 の全文 | B | 原文は `epaint_default_fonts` 0.36.2 の `fonts/UFL.txt`。著作権行は Ubuntu Light の name ID 0。ファイルはまだ ZIP に入っていなかった |
| Hack に含まれる Bitstream Vera の全文 | B | 原文は同 crate の `fonts/Hack-Regular.txt`（MIT と Bitstream Vera が1ファイル）。ZIP に無かった |
| Noto Emoji の著作権表示 | B | OFL 本文は同 crate の `fonts/OFL.txt`。著作権行は name ID 0。ZIP の他 OFL は別書体のもの |
| Foxit の BSD 全文 | B | 原文は `vendor/hayro-interpret/assets/LICENSE_FOXIT`。ZIP に無かった |
| CMap の BSD 全文 | B | 原文は `hayro-cmap` 0.1.0 の `assets/LICENSE.txt`。`NOTICE` の著作権行とは一致。全文は ZIP に無かった |

再開前に、上記5ファイルは `packaging/windows/third-party/` へ原本どおり置いてあった。`portable.txt` と `portable-readme.md` の文面も直してあった。残っていたのは、ZIP 生成へつなぐこと、再生成、照合、短い起動確認、この報告書である。

名前表の読み取りに使ったスクリプトは、利用者が `D:\dev\SwiftPDF-package-validate\step128\work\_name_dump.py` へ移している。リポジトリには戻していない。この再開では再実行していない。再開後に照合したのは、ライセンス本文のバイト列である。

## 3. 不足ライセンス文書の補完結果

5ファイルとも ZIP に入っている。本文は配布元と一致する。著作権者名は変えていない。第三者の表示は削っていない。

| ZIP 内のファイル | バイト | 照合 |
|---|---|---|
| `Ubuntu-Light-UFL.txt` | 4,813 | 末尾 4,673 バイトが `fonts/UFL.txt` と一致。先頭は name ID 0 と name ID 7 の2行 |
| `Hack-Regular-LICENSE.txt` | 3,734 | `fonts/Hack-Regular.txt` と全バイト一致。`BITSTREAM VERA LICENSE` を含む |
| `OFL-NotoEmoji.txt` | 4,350 | 末尾 4,301 バイトが `fonts/OFL.txt` と一致。先頭は name ID 0 の著作権行 |
| `LICENSE-FOXIT.txt` | 1,553 | `LICENSE_FOXIT` と全バイト一致 |
| `LICENSE-CMAP.txt` | 1,453 | `hayro-cmap` 0.1.0 `assets/LICENSE.txt` と全バイト一致 |

`THIRD-PARTY-SOURCES.txt` は出所のメモであり、許諾文ではない。許諾ファイルの中には説明を書き足していない。

`epaint_default_fonts` の `emoji-icon-font-mit-license.txt`（Copyright 2014 John Slegers）は STEP 127 で A だった。今回の5件には含めず、足していない。

## 4. ライセンス原文の出典

crate の実体は、ローカルの Cargo レジストリ `index.crates.io-1949cf8c6b5b557f` にある。

- Ubuntu Font Licence 1.0 の本文: `epaint_default_fonts-0.36.2\fonts\UFL.txt`。ファイル自体に著作権行は無い。条件が求める著作権表示は、同じ crate の `fonts\Ubuntu-Light.ttf` name ID 0（`Copyright 2011 Canonical Ltd. Licensed under the Ubuntu Font Licence 1.0`）。商標の行は name ID 7（`Ubuntu and Canonical are registered trademarks of Canonical Ltd.`）。SwiftPDF は egui 経由でこの書体を内包する。フォントファイル自体は ZIP に別ファイルとしては入っていない。
- Hack / Bitstream Vera: `epaint_default_fonts-0.36.2\fonts\Hack-Regular.txt`。Hack は Copyright 2018 Source Foundry Authors の MIT。Bitstream Vera Sans Mono は Copyright 2003 Bitstream, Inc.、予約名は Bitstream と Vera。
- Noto Emoji: 本文は `epaint_default_fonts-0.36.2\fonts\OFL.txt`（SIL OFL 1.1、先頭に著作権行は無い）。著作権行は `fonts\NotoEmoji-Regular.ttf` name ID 0（`Copyright 2013 Google Inc. All Rights Reserved.`）。
- Foxit: リポジトリの `vendor/hayro-interpret/assets/LICENSE_FOXIT`。BSD-3-Clause。先頭は `Copyright 2014 PDFium Authors. All rights reserved.`。非推奨条項の名前は Google Inc.。
- CMap: `hayro-cmap-0.1.0\assets\LICENSE.txt`。BSD-3-Clause。先頭は `Copyright 1990-2023 Adobe. All rights reserved.`。`NOTICE` と同じ行である。上流の場所は NOTICE が指す cmap-resources である。

OCR モデルの CC-BY-SA-4.0 全文は、従来どおり `models\text-detection.rten.LICENCE.txt` と `models\text-recognition.rten.LICENCE.txt`（各 20,138 バイト）にある。作者の行は変えていない。

## 5. `portable.txt` の変更内容

マーカーの有無だけがポータブルモードを決める。中身はアプリから読まれない。読まれるのは人なので、STEP 126 の実測に合わせた。

いまの文面:

- 設定、ログ、復旧用の自動保存、新しいデジタル ID、理事長印データは、原則として EXE の隣の `Data` に置く。
- `Data` に書けないときはポータブルをやめ、`%LOCALAPPDATA%\SwiftPDF` を使う。About は実際の保存先を示し、ポータブルの `Data` を使っていないときはその旨を出す。
- 単一インスタンス用ソケットは、ポータブル版でも `%LOCALAPPDATA%\SwiftPDF` に作る。印影ではなく、`Data` の中でもない。
- `Data\seals.v2.bin` は現在の Windows ユーザーの DPAPI で暗号化する。別 PC・別ユーザーへの `Data` コピーでは復号に失敗することがあり、ファイルはそのまま残る。フォルダーコピーを印影の移行方法としては書いていない。
- 各 PC の印影ストアと監査履歴は独立し、同期も統合もしない。

`README.md`（生成元は `portable-readme.md`）も同じ内容にした。以前の「データは `%LOCALAPPDATA%` ではなく `Data`」という断定は外した。ソケット、書けないときのフォールバック、DPAPI、2台では履歴が分かれたままであること、印影画像や `seals.v2.bin` を不用意に渡さないことを書いた。ZIP に無い `install-notes.md` へのリンクは外し、この配布はポータブル版だけであること、MSI を使う場合の同じファイル名は `%LOCALAPPDATA%\SwiftPDF` にあること、だけを残した。

`crates/ui-egui/src/portable.rs` のコメントは、ポータブル版が `%LOCALAPPDATA%` に何も書かない、という古い説明のままである。製品コードは今回変えていない。配布物の `portable.txt` と README が実測と一致する。

## 6. OCR モデル帰属表示の扱い

`models\ATTRIBUTION.txt` の先頭1文は、`xtask` が付けるこちらの説明である。第三者の著作権表示ではない。

ZIP 内ではその1文だけを次に変えた。

`SwiftPDF's text recognition (Scan & OCR) uses these models, unmodified.`

続く行はそのままである。Robert Knight、ocrs-models、HierText、CC-BY-SA-4.0、ソース URL、各 `.LICENCE.txt` への参照を確認した。ライセンス全文は変えていない。

変更は `package.ps1` の中だけである。`cargo xtask models` が書いた先頭が、想定した PdfCraft の1文でなければスクリプトは止める。`xtask` のソースは変えていない。`cargo xtask models` を単体で実行すると、ステージ上のファイルは再び PdfCraft の文で上書きされる。ZIP に入るのは `package.ps1` が書き換えた後のファイルである。

## 7. ZIP の全内容物

ルートは `SwiftPDF-1.0.0-windows-x64-portable\`。29 エントリ。

| Bytes | Path |
|---|---|
| 97,062,912 | `SwiftPDF.exe` |
| 1,236 | `portable.txt` |
| 7,654 | `README.md` |
| 4,441 | `NOTICE` |
| 85,921 | `ATTRIBUTION.md` |
| 1,100 | `LICENSE-MIT` |
| 10,174 | `LICENSE-APACHE` |
| 2,256 | `LICENSE-brand.txt` |
| 1,580 | `LICENSE-document.txt` |
| 3,208 | `LICENSE-lucide.txt` |
| 4,380 | `OFL-Inter.txt` |
| 4,399 | `OFL-JetBrainsMono.txt` |
| 4,443 | `OFL-DancingScript.txt` |
| 4,501 | `OFL-biz-ud-pgothic.txt` |
| 4,501 | `OFL-biz-ud-mincho.txt` |
| 4,492 | `OFL-shippori-mincho.txt` |
| 4,475 | `OFL-noto-sans-arabic.txt` |
| 4,473 | `OFL-noto-sans-thai.txt` |
| 4,813 | `Ubuntu-Light-UFL.txt` |
| 3,734 | `Hack-Regular-LICENSE.txt` |
| 4,350 | `OFL-NotoEmoji.txt` |
| 1,553 | `LICENSE-FOXIT.txt` |
| 1,453 | `LICENSE-CMAP.txt` |
| 1,526 | `THIRD-PARTY-SOURCES.txt` |
| 872 | `models\ATTRIBUTION.txt` |
| 2,510,284 | `models\text-detection.rten` |
| 20,138 | `models\text-detection.rten.LICENCE.txt` |
| 9,716,568 | `models\text-recognition.rten` |
| 20,138 | `models\text-recognition.rten.LICENCE.txt` |

`pdfcraft.exe`、`pdfcraft-cli.exe`、`seals.v2.bin`、`seals.bin`、PDF、`.pfx`、秘密鍵、git メタデータは無い。ArtCraft のロゴ画像は無い。`LICENSE-brand.txt` は商標条件のテキストである。

## 8. ZIP と EXE の SHA-256

| File | Bytes | SHA-256 |
|---|---|---|
| `D:\dev\SwiftPDF-package-validate\step128\SwiftPDF-1.0.0-windows-x64-portable.zip` | 55,321,170 | `C1DD154C0776539118173D34EA71F8AFF0D5989F5B81543DAF7D7244B2949DFE` |
| ZIP 内および生成に使った `SwiftPDF.exe` | 97,062,912 | `551D6DE1DFF75FFB525CDCC21C61D45200B8955399727D01B2404C4E1452E318` |

EXE は再ビルドしていない。ファイルバージョンは 1.0.0、ProductName は SwiftPDF。

同じスクリプトが隣に MSI を書いた。配布物ではない。インストールしていない。署名していない。

| File | Bytes | SHA-256 |
|---|---|---|
| `D:\dev\SwiftPDF-package-validate\step128\SwiftPDF-1.0.0-windows-x64.msi` | 46,960,640 | `45226614BC78AFCA6A471EAE0544981F144B8E443520E71D1B138238BA041F3E` |

インストーラデータベースのファイル名に `Ubuntu-Light-UFL`、`LICENSE-FOXIT`、`LICENSE-CMAP` は無い。5つの許諾文は `pdfcraft.wxs` に足していない。MSI のコンポーネント GUID は変えていない。この MSI は渡さない。

STEP 126 は上書きしていない。

| File | SHA-256 |
|---|---|
| `step126\SwiftPDF-1.0.0-windows-x64-portable.zip` | `FB4A88121F76B787633BC10851F8DC39BA5F62FB110B94A3F7D4AAE55F5063D3` |
| `step126\SwiftPDF-1.0.0-windows-x64.msi` | `9663D6AB4E1007AF1D103D7E8924EBDA565DB1C6935CF2C8A498917DD8F7B31A` |

ローカルの `target\x86_64-pc-windows-msvc\release\SwiftPDF.exe` は、スクリプトのあいだだけ既知の EXE と入れ替え、終了後に `EA1488BE75940E05957405CC5478B5236B1A00036A08E43A34BA581153B41C08` へ戻した。既知の EXE `D:\dev\SwiftPDF-repro-step123\target-clean\release\SwiftPDF.exe` は `551D6DE1…` のままである。

## 9. パッケージ検証結果

`package.ps1 -Arch x64 -SkipBuild` を `powershell.exe` で実行した。`DIST` は `D:\dev\SwiftPDF-package-validate\step128`。`CRAFT_FONTS_DIR` は `D:\dev\craft-fonts`。終了コード 0。署名秘密は未設定で、スクリプトは未署名のまま警告して終わった。`test-msi.ps1` はデータベースを開いただけで、インストールはしていない。

確認したこと:

- 5つの許諾文と `THIRD-PARTY-SOURCES.txt` が ZIP にある。
- 本文のバイト列は §3 のとおり出典と一致する。
- `portable.txt` は §5 の文面である。
- `models\ATTRIBUTION.txt` の先頭は SwiftPDF の1文で、作者と CC-BY-SA-4.0 の行は残っている。
- ZIP 内 EXE の SHA-256 は `551D6DE1…`。
- 印影ストア、PDF、監査ログ、秘密情報は ZIP に無い。

## 10. 実施した動作確認

自動確認と、人による確認は分けて書く。

### この STEP の自動確認

ZIP を `step128\expand` へ展開し、その EXE を起動した。子プロセスの `LOCALAPPDATA`、`APPDATA`、`TEMP`、`TMP` だけを `step128\iso` 配下へ向けた。実フォルダー `%LOCALAPPDATA%\SwiftPDF` の更新時刻は、起動の前後とも `2026-10-23T22:18:55.4903354Z` だった。中身は見ていない。

| 項目 | 結果 |
|---|---|
| バージョン | ファイル情報は 1.0.0、ProductName は SwiftPDF |
| データ保存先 | ログ: `portable mode: settings are kept in …\step128\expand\SwiftPDF-1.0.0-windows-x64-portable\Data` |
| ソケット | 向け先の `local\SwiftPDF\portable-instance-Console.lock` と `.sock`。`Data` の中には無い |
| 再起動 | 2回目も同じ portable のログ。`Data\app.ron` は残った（6,272 バイトのあと 6,265 バイト） |
| 印影 | この起動では登録していない。`seals.v2.bin` はできていない |

About の画面は開いていない。保存先はログで確認した。

### 同じ EXE バイト列について、以前に人が確認済み

STEP 124 の B1–B8 は、利用者が `D:\dev\SwiftPDF-seals-validate\step124\portable\SwiftPDF.exe`（SHA-256 `551D6DE1…`）で実施し、STEP 125 の報告書に記録されている。この ZIP の EXE と同じバイト列である。この STEP ではやり直していない。

| ID | 内容 | 誰が |
|---|---|---|
| B1 | バージョン 1.0.0 | 利用者 |
| B2 | 合成 PDF が表示される | 利用者 |
| B3 | 一般印を置いて取り消す | 利用者 |
| B4 | 合成の理事長印を確定する | 利用者 |
| B5 | Undo / Redo | 利用者 |
| B6 | 別名保存して開き直し、印と追加文字が残る | 利用者 |
| B7 | About が `step124\portable\Data` を示す | 利用者 |
| B8 | 印を置いたタブを閉じる前に確認する | 利用者 |

STEP 122A の A1–A10 は別の EXE（`F9B3E2DD…`）である。この ZIP の確認には数えない。

## 11. 未確認事項

- この ZIP を、人が About まで開いて見ていない。
- この ZIP の展開先で、合成 PDF、合成印影、別名保存はやり直していない。根拠は同じ EXE バイト列に対する STEP 124 の人の確認である。
- 別の Windows ユーザー、別 PC での DPAPI 復号失敗は、この機械では試していない。コード上は復号失敗時にファイルを書き換えない、という STEP 126 の読みを維持する。
- `cargo-deny` は実行していない。依存関係は更新していない。STEP 127 の crate 一覧（`self_cell` の Apache-2.0 OR GPL-2.0-only、`r-efi` の MIT OR Apache-2.0 OR LGPL-2.1-or-later は許諾側を選ぶ）は再計測していない。
- MSI の中の `ATTRIBUTION.txt` は CAB を展開して見ていない。ファイル表に5つの許諾文が無いことだけを確認した。
- 中国語 UI のプレビュー、SWSEAL2 の断続的な I/O 失敗、バイト単位の再ビルド、この PC の bugcheck 0x1A は、今回の対象外のままである。
- 署名はしていない。

## 12. 2台利用の技術的制約

- ポータブル版の設定と印影は、書けるときは EXE の隣の `Data` に入る。書けないときは、その Windows ユーザーの `%LOCALAPPDATA%\SwiftPDF` に落ちる。
- ソケットは、ポータブルでも `%LOCALAPPDATA%\SwiftPDF` にできる。`Data` をコピーしてもソケットは付いてこない。
- `seals.v2.bin` は現在ユーザーの DPAPI である。同じユーザー・同じ PC で鍵が残っていれば再び開けることがある。別ユーザーや別 PC へフォルダーをコピーしても、印影の移行にはならない。復号できなければファイルはそのまま残る。
- エクスポート／インポートは無い。承認された印影は、その PC で登録する。
- 監査履歴は PC ごとに残り、自動では1つにならない。
- アプリは2台のストアを同期しない。

## 13. 管理側の承認が必要な事項

ソフトウェアの許諾文は、理事長印を2台目に載せる承認ではない。

- 加藤さんの PC で理事長印を登録する前に、管理側の承認が要る。この STEP では登録していない。
- 印影画像、`seals.v2.bin`、監査ログを、ライセンス文書と一緒に渡してはならない。
- OCR モデルは CC-BY-SA-4.0 のままである。追加の再配布禁止は付けていない。付けるとモデルの条件と衝突する。

## 14. 加藤さんへの配布可否

STEP 127 が穴と書いた5つの許諾文は、この ZIP の中で出典と一致している。その範囲では、文書の不足は塞いである。

配布はしていない。利用者の承認がまだ無いので、この ZIP を加藤さんへ渡す判断はしていない。渡す場合も、隣の MSI ではなく ZIP だけである。理事長印を加藤さんの PC に登録するかどうかは、§13 の管理側の承認が別である。

## 15. 変更ファイル一覧

製品コード、SWSEAL2、押印、監査、UI、バージョン、依存関係は変えていない。EXE は再ビルドしていない。

追跡ファイル:

- `packaging/windows/package.ps1` — ポータブル ZIP へ5つの許諾文と出所メモをコピーする。ZIP にそれらと `SwiftPDF.exe` と `portable.txt` が無ければ失敗する。印影ストア、PDF、秘密鍵のファイル名があれば失敗する。OCR の先頭1文だけを SwiftPDF に書き換え、想定文でなければ失敗する。コメントから「`%LOCALAPPDATA%` には書かない」を外した。
- `packaging/windows/portable.txt`
- `packaging/windows/portable-readme.md`

未追跡（今回追加）:

- `packaging/windows/third-party/Ubuntu-Light-UFL.txt`
- `packaging/windows/third-party/Hack-Regular-LICENSE.txt`
- `packaging/windows/third-party/OFL-NotoEmoji.txt`
- `packaging/windows/third-party/LICENSE-FOXIT.txt`
- `packaging/windows/third-party/LICENSE-CMAP.txt`
- `packaging/windows/third-party/THIRD-PARTY-SOURCES.txt`
- `docs/SWIFTPDF_V1_TWO_USER_PORTABLE_FINAL.md`（このファイル）

以前からの未追跡（保持）:

- `docs/SWIFTPDF_V1_PACKAGE_SAFETY_AUDIT.md`
- `docs/SWIFTPDF_V1_TWO_USER_LICENSE_AUDIT.md`
- `docs/images/SwiftPDFiconORG.png`

## 16. `git diff --stat`

未追跡は含まない。

```
 packaging/windows/package.ps1        | 36 +++++++++++++++++++++++++++++++-----
 packaging/windows/portable-readme.md | 27 ++++++++++++++++++++-------
 packaging/windows/portable.txt       | 24 +++++++++++++++++++-----
 3 files changed, 70 insertions(+), 17 deletions(-)
```

`git diff --check` は問題なし。

## 17. `git status`

HEAD は `e503124bd9adf502d7f027f49a0b3b3eed4f3f69`。ブランチは `feature/seal-data-safety-v1.0.0`。

```
 M packaging/windows/package.ps1
 M packaging/windows/portable-readme.md
 M packaging/windows/portable.txt
?? docs/SWIFTPDF_V1_PACKAGE_SAFETY_AUDIT.md
?? docs/SWIFTPDF_V1_TWO_USER_LICENSE_AUDIT.md
?? docs/SWIFTPDF_V1_TWO_USER_PORTABLE_FINAL.md
?? docs/images/SwiftPDFiconORG.png
?? packaging/windows/third-party/
```

コミット、push、マージ、タグ、Release、公開、MSI のインストール、加藤さんへの配布はしていない。
