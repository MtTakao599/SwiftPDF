# SwiftPDF v1.0.0 ArtCraft ブランド画像の除去（STEP 120B）

状態：**作業ツリー上で画像を削除し、ATTRIBUTION を再生成した。commit / push はしていない。Git 履歴は書き換えていない。**

通常の `%LOCALAPPDATA%\SwiftPDF` には触れていない。

## 1. 開始時の Git 状態

| 項目 | 値 |
|---|---|
| ブランチ | `feature/seal-data-safety-v1.0.0` |
| HEAD | `4eb07ee3f71a2a2c75366098510193518a226036` |
| 開始時 | STEP 119/120 の未コミット変更と `SWIFTPDF_V1_RELEASE_GAP_ANALYSIS.md` / `SWIFTPDF_V1_LICENSE_BRAND_AUDIT.md` を保持 |

`reset` / `clean` / `stash` / `rebase` は使っていない。

## 2. 削除対象の一覧

削除したのは次の **8 ファイル**だけである。いずれも `docs/brand/` の ArtCraft Marks。

| 相対パス | 形式 |
|---|---|
| `docs/brand/artcraft-logo.svg` | SVG ワードマーク |
| `docs/brand/artcraft-logo.png` | PNG ワードマーク |
| `docs/brand/artcraft-logo-white.svg` | SVG 白ワードマーク |
| `docs/brand/artcraft-logo-white.png` | PNG 白ワードマーク |
| `docs/brand/artcraft-mark.svg` | SVG マーク |
| `docs/brand/artcraft-mark.png` | PNG マーク |
| `docs/brand/artcraft-mark-black.svg` | SVG 黒マーク |
| `docs/brand/artcraft-mark-black.png` | PNG 黒マーク |

`logo-white.svg` という独立ファイルは無い。実体は `artcraft-logo-white.svg`。

## 3. ブランド画像と判断した根拠

1. ファイルが `docs/brand/` にあり、名前が ArtCraft ワードマーク／マークである。
2. `ATTRIBUTION.toml` が `licence = "LicenseRef-Storyteller-Trademark"`、`kind = "logo"`、著者 `ArtCraft Team` と書いていた。
3. `docs/brand/LICENSE-brand.txt` が、これらのファイルを ArtCraft Marks（商標、非オープンソース）と定める。
4. 改変版の配布では Marks を除け、と明記されている。

次はブランド画像として削除していない。

| 対象 | 理由 |
|---|---|
| Lucide UI アイコン | ISC。一般 UI |
| `pdfcraft-document.svg` / `.ico` | Lucide + 寄稿者の文書アイコン。商標ロゴではない |
| `swiftpdf.ico` / `SwiftPDFicon.png` | SwiftPDF 公式アイコン。PdfCraft アイコン由来ではない |
| `pdfcraft.svg` 一式 | PdfCraft アプリアイコン（MIT OR Apache-2.0）。獅子の絵。ArtCraft Marks ではない |
| `docs/images/pdfcraft-*.png` | 本家画面のスクリーンショット。ファイル自体は寄稿者著作 |

## 4. 削除前の参照関係

| 参照 | 取り込み | 配布 | 削除時に必要な修正 |
|---|---|---|---|
| `widgets.rs` の `include_bytes!` | STEP 120 で切断済み | バイナリへは入らない | 追加なし |
| About / Home の描画 | STEP 120 で切断済み | 表示なし | 追加なし |
| README 先頭・末尾の `<img src="docs/brand/…">` | STEP 120 で切断済み | GitHub 表示 | 「ファイルが残る」という文言を直す |
| `ATTRIBUTION.toml` 8 件 | ソース目録 | `ATTRIBUTION.md` 経由 | エントリ削除と再生成 |
| `package.ps1` / MSI | 画像は同梱していなかった。`LICENSE-brand.txt` のみ | 文書のみ | 画像コピーは無し。検査用文字列は残す |
| Windows リソース | `swiftpdf.ico` のみ | exe / ショートカット | 変更しない |

削除前の帰属（Git 履歴と STEP 120 報告書に残る）：著者 ArtCraft Team、ライセンス `LicenseRef-Storyteller-Trademark`、条件 `LICENSE-brand.txt`、SHA-256 は旧 `ATTRIBUTION.toml`。

## 5. 削除したファイル

上記 8 点。作業ツリー上は未ステージの `D`。`git add` していない。

## 6. 削除を見送ったファイル

| ファイル | 理由 |
|---|---|
| `docs/brand/LICENSE-brand.txt` | 商標条件。維持必須 |
| `LICENSE-MIT` / `LICENSE-APACHE` | 本文は触らない |
| `NOTICE` の著作権行 | 帰属表示 B |
| `pdfcraft-document.*` / `LICENSE-document.txt` | 第三者ライセンス C。リポジトリに残る |
| `swiftpdf.ico` / `SwiftPDFicon*.png` | SwiftPDF アイコン。変更禁止 |
| PdfCraft アプリアイコン一式 | 商標 Marks ではない。由来記録 |
| 本家スクリーンショット | ファイルはブランド素材ではない |
| `AGENTS.md` の例外条項 | 方針文。画像パスの記述は古くなるが、本文改変はしない |

## 7. ATTRIBUTION の生成方式

- 生成元：`ATTRIBUTION.toml`
- 生成物：`ATTRIBUTION.md`（`cargo xtask assets --write`。手編集禁止）
- 検査：`cargo xtask assets`（CI でも実行）

自動生成である。推測ではない。

## 8. ATTRIBUTION の変更と帰属維持

1. toml から 8 件の `[[asset]]` を外した。
2. `docs/images/pdfcraft-tools.png` の usage だけ、「docs/brand/ に画像がある」と読める箇所を直した。
3. `--write` の差分を見てから採用した。

結果：

- リポジトリ資産 275 → **267**（8 減のみ）
- bundled 6、fetched 22 は変わっていない
- PdfCraft contributors、翻訳カタログ、フォント、Lucide、`swiftpdf.ico`、`pdfcraft-document` は残っている
- LICENSE-MIT の著作権行、NOTICE 冒頭の Copyright、About の由来行は残している

生成結果で必要な著作権が消えていないことを確認して採用した。

## 9. NOTICE のアイコン記述の調査

| 項目 | 結果 |
|---|---|
| `pdfcraft-document.svg` | `assets/app-icon/pdfcraft-document.svg` に存在する |
| ライセンス | ISC AND (MIT OR Apache-2.0)。`LICENSE-document.txt` |
| 著者 | Lucide（輪郭）+ PdfCraft contributors（バッジ等） |
| SwiftPDF での利用 | **埋め込まない**。exe の resource 1/2 は `swiftpdf.ico` |

判定：**別素材に置き換わっている。** 由来は確定できる。

NOTICE の変更：

- Lucide / `pdfcraft-document` の帰属は残した（ファイルがリポジトリに残るため）。
- SwiftPDF.exe が使うのは `swiftpdf.ico` だと一文足した。
- 「docs/brand/ にロゴがある」は、削除後に虚偽になるので、画像は含まれないと直した。
- 著作権行と LICENSE 本文は変えていない。

見た目の統一だけのための書き換えではない。

## 10. SwiftPDF アイコンの由来

| ファイル | 由来 | ライセンス |
|---|---|---|
| `docs/images/SwiftPDFicon-source.png` | 公式アイコンとしてリポジトリに供給。PdfCraft アイコン由来ではない | MIT OR Apache-2.0 |
| `docs/images/SwiftPDFicon.png` | 上記の白マットを透明化。意匠は同じ | 同上 |
| `assets/app-icon/swiftpdf.ico` | 上記 PNG を 16–256 px に再サンプル | 同上 |

`apps/pdfcraft/src/main.rs` が PNG を窓アイコンに `include_bytes!`。`build.rs` が ico を resource 1 と 2 に入れる。MSI ショートカットと PDF 関連付けも `swiftpdf.ico`。

変更していない。

## 11. ZIP／MSI 設定の確認

画像を Stage にコピーする経路は無い。同梱予定は文書のみ。

- `LICENSE-MIT` / `LICENSE-APACHE` / `NOTICE` / `ATTRIBUTION.md`
- `LICENSE-brand.txt`（条件文。画像ではない）
- `LICENSE-lucide.txt` / `LICENSE-document.txt`
- `OFL-Inter.txt` 等と craft-fonts の `OFL-*.txt`
- ポータブル `README.md`
- `models\` の帰属

`package.ps1` は exe 内の ArtCraft SVG 断片を拒否する検査を残している。正式パッケージは作っていない。

## 12. 参照切れの確認

製品・配布・README から `docs/brand/artcraft-*.svg|.png` へのリンクは無い。

残している参照：

- `package.ps1` の埋め込み拒否文字列（回帰）
- `xtask` 試験の架空パス `docs/brand/example-mark.svg`
- STEP 119/120 の過去報告書
- `AGENTS.md` の方針文
- 帰属・商標の説明に必要な「ArtCraft」という語

`include_bytes!` でブランド画像を読む箇所は無い。

## 13. 品質ゲートの結果

| 確認 | 結果 |
|---|---|
| `cargo fmt --check` | 成功 |
| `cargo check -p pdfcraft -j 2` | 成功 |
| `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings` | 成功 |
| `about_dialog_*` | 2 件成功 |
| `home_screen_links` | 成功 |
| `xtask` `brand_licence_only_for_brand_logos` | 成功 |
| `cargo xtask assets` | ok（267 / 6 / 22） |
| 正式 MSI／ZIP | 未作成 |

## 14. 未解決の権利問題

1. **Git 履歴**には画像が残る。履歴改変はしていない。
2. **本家スクリーンショット**（`docs/images/pdfcraft-tools.png` 等）は、画面の一部に ArtCraft マークが写っている可能性がある。ファイル自体は寄稿者スクリーンショットとして残した。
3. **`AGENTS.md`** は「logos in `docs/brand/`」と書く。方針の例外規定であり、今回は本文を変えていない。
4. PdfCraft アイコンの由来文に「drawn … in ArtCraft」とある。これは描画ツール名であり、Marks ファイルではない。
5. 版番号、DPAPI 説明、配布 exe の人確認は対象外のまま。

## 15. 次工程への推奨

1. 本家スクリーンショットを README から外すか、SwiftPDF の画面に差し替えるか決める。
2. `AGENTS.md` のブランド例外を「このツリーには画像が無い」と追記するか決める。
3. 版 1.0.0 と検証用 ZIP／MSI（公開しない）。
4. その後に commit（指示があったとき）。

## 16. `git diff --stat`

```
 ATTRIBUTION.md                         |  12 +----
 ATTRIBUTION.toml                       |  90 +--------------------------------
 NOTICE                                 |  10 ++--
 README.md                              |  37 +++++++-------
 crates/ui-egui/src/dialogs.rs          |  28 ++++------
 crates/ui-egui/src/home.rs             |   9 ++--
 crates/ui-egui/src/widgets.rs          |  20 --------
 crates/ui-egui/tests/ui_group/links.rs |   3 +-
 docs/brand/artcraft-logo-white.png     | Bin 32161 -> 0 bytes
 docs/brand/artcraft-logo-white.svg     |   7 ---
 docs/brand/artcraft-logo.png           | Bin 31302 -> 0 bytes
 docs/brand/artcraft-logo.svg           |   7 ---
 docs/brand/artcraft-mark-black.png     | Bin 26793 -> 0 bytes
 docs/brand/artcraft-mark-black.svg     |   4 --
 docs/brand/artcraft-mark.png           | Bin 24715 -> 0 bytes
 docs/brand/artcraft-mark.svg           |   4 --
 packaging/windows/package.ps1          |  27 ++++++++--
 packaging/windows/pdfcraft.wxs         |  18 +++++++
 packaging/windows/portable-readme.md   |   4 +-
 packaging/windows/test-msi.ps1         |   6 +++
 xtask/src/assets.rs                    |   8 ++-
 xtask/src/screenshots.rs               |   7 +--
 22 files changed, 100 insertions(+), 201 deletions(-)
```

（本報告書は未追跡のため上に含まれない。）

## 17. `git status`

HEAD は `4eb07ee3` のまま。削除は未ステージ。未追跡は STEP 119/120 の報告書と本ファイル。

## 改訂

- 2026-10-24：STEP 120B。8 画像を削除。ATTRIBUTION 再生成。commit なし。
