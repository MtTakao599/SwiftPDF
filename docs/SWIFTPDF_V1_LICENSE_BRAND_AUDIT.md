# SwiftPDF v1.0.0 商標・ライセンス・NOTICE・帰属表示の監査（STEP 120）

状態：**監査と、根拠が明確な最小修正まで。commit / push / タグ / Release / 正式 MSI／ZIP は作っていない。法的適合は断定していない。**

通常の `%LOCALAPPDATA%\SwiftPDF` には触れていない。

## 1. 調査対象と Git 状態

| 項目 | 値 |
|---|---|
| リポジトリ | `D:\dev\SwiftPDF` |
| ブランチ | `feature/seal-data-safety-v1.0.0` |
| 開始 HEAD | `4eb07ee3f71a2a2c75366098510193518a226036`（製品コード未コミット） |
| 開始時の未追跡 | `docs/SWIFTPDF_V1_RELEASE_GAP_ANALYSIS.md`（維持） |
| origin | `https://github.com/MtTakao599/SwiftPDF.git` |

読んだ原典：`LICENSE-MIT`、`LICENSE-APACHE`、`NOTICE`、`ATTRIBUTION.md`、`ATTRIBUTION.toml`、`docs/brand/LICENSE-brand.txt`、`README.md`、`Cargo.toml`、`packaging/windows/package.ps1`、`pdfcraft.wxs`、`portable-readme.md`、`deny.toml`、`.github/workflows/release.yml`、フォント／アイコンの LICENSE。

## 2. 元プロジェクトのライセンス

| 区分 | 内容 | 断定 |
|---|---|---|
| A. ソフトウェア | PdfCraft 由来コードは **MIT OR Apache-2.0**。`LICENSE-MIT` の著作権行は `Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors`。MIT は「上記の著作権表示と許諾表示をすべての複製に含める」と定める | 表示は確認した。法的効果の解釈は要確認としないが、**この行を消さない** |
| A. Apache-2.0 | NOTICE の維持（§4(d)）が配布条件になり得る | NOTICE は残し、本文は推測で変えていない |
| B. ブランド | `docs/brand/LICENSE-brand.txt`：ArtCraft Marks は商標でありオープンソースではない。改変版・派生の配布・公開では **Marks を除去**し、ArtCraft 製品として示さない。プレーンテキストで「PdfCraft by the ArtCraft team を基にしている」とは言ってよい | 条項は明確。製品バイナリからの除去はこの STEP で実施 |
| C. 資産 | Lucide ISC、Inter / JetBrains Mono / Dancing Script OFL-1.1、craft-fonts OFL-1.1、Adobe CMap は NOTICE 上 `adobe_data` の非視覚データ | 個別ライセンス文はリポジトリにある |

`LICENSE-MIT` / `LICENSE-APACHE` の本文は変更していない。

## 3. NOTICE／ATTRIBUTION の関係

- `ATTRIBUTION.md` は**存在する**（`ATTRIBUTION.toml` から生成。手編集禁止）。
- `NOTICE` は「一覧は ATTRIBUTION.md」と書く。
- 修正前の `package.ps1` は ZIP／MSI に `LICENSE-MIT`、`LICENSE-APACHE`、`NOTICE` だけを同梱し、`ATTRIBUTION.md` は入らなかった。
- `NOTICE` 本文は推測で直していない（文書アイコン経路が `pdfcraft-document.svg` のまま、など。不正確の疑いは 13 節）。
- 今回、配布スクリプトが `ATTRIBUTION.md` を Stage → ZIP／MSI にコピーするよう足した。

## 4. ArtCraft ブランド素材一覧

| ファイル | 種類 | 修正前の参照 |
|---|---|---|
| `docs/brand/artcraft-logo.svg` | ワードマーク | `widgets::artcraft_logo` の `include_bytes!`、README ヘッダー |
| `docs/brand/artcraft-logo-white.svg` | ダーク用ワードマーク | 同上 |
| `docs/brand/artcraft-mark.svg` | マーク | `widgets::artcraft_mark`、home、README フッター |
| `docs/brand/artcraft-mark-black.svg` | 黒マーク | ソースのみ（埋め込みなし） |
| 上記の `.png` 4 点 | ラスタ | ATTRIBUTION 登録。製品コードは未参照 |
| `docs/brand/LICENSE-brand.txt` | 商標条件 | NOTICE が指す。今回パッケージへコピー |

Windows アプリアイコン／タスクバー／MSI ショートカットは `assets/app-icon/swiftpdf.ico`（SwiftPDF 用）。PdfCraft の `pdfcraft.ico` は同梱しない。

## 5. 使用中・埋め込み・ソースのみの分類

| 素材 | 分類 | 今 |
|---|---|---|
| ArtCraft SVG 3 点（logo / logo-white / mark） | 修正前は **バイナリに埋め込み**。閲覧専用では描かない。通常モードの About／Home では描いていた | **ソースのみ**。`include_bytes!` を削除 |
| ArtCraft PNG 4 点、mark-black.svg | **ソースのみ** | 変更なし（リポジトリに残る） |
| About の Copyright 行 | **帰属表示** | 残し、LICENSE-MIT に合わせて年と ArtCraft Team を補った |
| `links.rs` の getartcraft.com / Discord URL | 閲覧専用では Help から隠す。通常モードのボタンは残る | **製品機能は変えていない**。商標画像ではない |
| README 先頭・末尾のロゴ画像 | 修正前は **製品で使用中**（公開 README） | 画像参照を外し、テキストの由来説明に置換 |
| LICENSE-MIT / NOTICE の「ArtCraft Team」 | **帰属表示** | 削除していない |

単純に実行ファイルへ `ArtCraft` という文字列があることだけでは失敗としない。帰属行は残る。

## 6. 修正したファイル

| ファイル | 内容 |
|---|---|
| `crates/ui-egui/src/widgets.rs` | `artcraft_logo` / `artcraft_mark`（`include_bytes!`）を削除 |
| `crates/ui-egui/src/dialogs.rs` | About からマーク描画を除去。閲覧専用でも通常でもプレーンテキストの由来行 |
| `crates/ui-egui/src/home.rs` | Home のマーク描画を除去。コミュニティ文面は残す（閲覧専用ではカード自体が出ない） |
| `crates/ui-egui/tests/ui_group/links.rs` | マーク alt ではなく「Based on PdfCraft」を確認 |
| `packaging/windows/package.ps1` | `ATTRIBUTION.md`、Lucide / リポジトリ OFL、`LICENSE-brand.txt` を同梱。exe に ArtCraft SVG 断片が無いことを検査 |
| `packaging/windows/pdfcraft.wxs` | 上記ファイルの MSI コンポーネント（新規 GUID） |
| `packaging/windows/test-msi.ps1` | 同梱名の検査を追加 |
| `packaging/windows/portable-readme.md` | 同梱文書の案内 |
| `README.md` | SwiftPDF であることの明示、由来、LICENSE/NOTICE 参照。本家機能一覧は書き換えず「upstream」と注記 |

## 7. 修正しなかったファイルと理由

| 対象 | 理由 |
|---|---|
| `LICENSE-MIT` / `LICENSE-APACHE` | ライセンス本文の書き換え禁止 |
| `NOTICE` | 不正確の疑い（アイコン経路名など）は推測で直さない |
| `ATTRIBUTION.toml` / `ATTRIBUTION.md` | 生成物。ブランドファイルをリポジトリから消す判断が残る |
| `docs/brand/*` の画像 | 参照を先に切った。削除は ATTRIBUTION 再生成が要る |
| `crates/engine/src/links.rs` | URL 表。閲覧専用では出さない。機能変更になる |
| `apps/pdfcraft-cli` の Discord 行 | CLI は配布しない |
| 内部 crate 名 `pdfcraft-*` | 影響が大きい |
| 設定フォルダー名 `SwiftPDF` | 既存データを壊す |
| `ROADMAP.md` | 指示どおり触らない |
| `SWIFTPDF_V1_RELEASE_GAP_ANALYSIS.md` | 維持 |

## 8. ZIP／MSI 同梱候補（作成はしていない）

ZIP と MSI で揃える。予定パスはいずれもインストール先／展開先のルート（OCR モデルだけ `models\`）。

| ファイル | 要否 | 予定名 |
|---|---|---|
| `LICENSE-MIT` | 要 | `LICENSE-MIT` |
| `LICENSE-APACHE` | 要 | `LICENSE-APACHE` |
| `NOTICE` | 要 | `NOTICE` |
| `ATTRIBUTION.md` | 要（今回追加） | `ATTRIBUTION.md` |
| 文書アイコン LICENSE | 要 | `LICENSE-document.txt` |
| Lucide ISC | 要（今回追加） | `LICENSE-lucide.txt` |
| ArtCraft 商標条件（画像は入れない） | 要（NOTICE が指す。今回追加） | `LICENSE-brand.txt` |
| リポジトリ埋め込みフォント OFL | 要（今回追加） | `OFL-Inter.txt` 等 |
| craft-fonts OFL | 要（既存） | `OFL-biz-ud-pgothic.txt` 等 |
| ポータブル説明 | ZIP のみ | `README.md`（`portable-readme.md`） |
| OCR モデル帰属 | 要（既存） | `models/ATTRIBUTION.txt` と `*.LICENCE.txt` |
| `docs/brand/*.svg` / `*.png` | **入れない** | — |
| `pdfcraft-cli.exe` | **入れない** | — |

ブランド画像が Stage にコピーされる経路は無い。exe 検査で SVG 断片を拒否する。

## 9. Rust 依存ライセンス

既存の方法は `deny.toml` と `cargo xtask deny`（CI の cargo-deny）。許可は MIT / Apache-2.0 / BSD / ISC 等。例外は `epaint_default_fonts`（OFL / Ubuntu Font）と `webpki-root-certs`（ビルド専用）。GPL/AGPL はリンクしない方針。

今回 `cargo xtask deny` は再実行していない。依存を増やしていない。不明な新規 crate は無い。

## 10. craft-fonts のライセンス・pin

| 項目 | 値 |
|---|---|
| 取得元 | 任意のビルド入力 `CRAFT_FONTS_DIR`。https://github.com/storytold/craft-fonts |
| pin | `3d9ab3a132ed0eff457e424226a435a901da30d3`（`release.yml`、`ci.yml` Linux、`freebsd.yml`） |
| ライセンス | 各ファミリーの `OFL.txt`。パッケージでは `OFL-<family>.txt` |
| 配布範囲 | BIZ UDPGothic / BIZ UDMincho / Shippori Mincho / Noto Sans Arabic / Noto Sans Thai。**Noto Sans CJK は埋め込まない**（`package.ps1` が拒否） |
| ZIP と MSI | 同じ Stage から同じ OFL をコピー |

## 11. README の状況

全面書き換えはしていない。

今回の範囲：

- 製品名を SwiftPDF と明記した。
- PdfCraft からの派生であること、LICENSE-MIT の著作権行を残した。
- LICENSE / NOTICE / ATTRIBUTION への参照を先頭に置いた。
- 本家の機能一覧・Downloads は「upstream / 未公開」と注記し、getartcraft.com を SwiftPDF の配布先として案内しない。
- 先頭と末尾の ArtCraft 画像参照を外した。

残る：Highlights 以降は本家 PdfCraft の文面。SwiftPDF の実機能（閲覧専用、理事長印など）とは一致しない。配布用 README の分離は次工程。

## 12. 品質確認

| 確認 | 結果 |
|---|---|
| `cargo fmt --check` | 成功 |
| `cargo check -p pdfcraft -j 2` | 成功 |
| `ui_group`：`about_dialog_*` | 2 件成功 |
| `ui_group`：`home_screen_links` | 成功 |
| 検証用 debug ビルド | `D:\dev\SwiftPDF-seals-validate\step117a\target\debug\SwiftPDF.exe`（隔離 target。正式配布ではない） |
| exe 内の `bytes://artcraft-logo` / `artcraft-mark` / ロゴ path `M104.28,49.49L81.55` / `artcraft-*.svg` | **なし** |
| exe 内の `ArtCraft Team` / `Based on PdfCraft` | **あり**（帰属。失敗としない） |
| 正式 MSI／ZIP | 未作成 |
| 実ユーザーデータ | 未アクセス |

release ビルドは時間がかかるため、debug で埋め込み切断を確認した。`package.ps1` の同じ検査は今後の release パッケージでも走る。

## 13. 未解決の権利・配布問題

1. **リポジトリに ArtCraft 画像が残る。** GitHub 上は公開物。バイナリからは外した。削除するなら `ATTRIBUTION.toml` の更新と `cargo xtask assets --write` が要る。判断を待つ。
2. **`NOTICE` の一部記述。** 文書アイコンを `pdfcraft-document.svg` と書く。SwiftPDF の exe アイコンは `swiftpdf.ico`。推測で直していない。
3. **README 本文が本家のまま。** 機能の混同が残る。
4. **`links.rs` の ArtCraft URL。** 閲覧専用では出さない。通常ビルド経路には残る。
5. **版表示は 0.1.0。** この STEP の対象外。
6. **DPAPI／再インストールの利用者説明**は `portable-readme` に未記載（STEP 119 の P0-C）。
7. **コード署名・発行者名。** WiX の Manufacturer は `SwiftPDF`。法人名は作っていない。
8. **cargo-deny の再実行なし。**

権利が不明な素材の新規導入はしていない。

## 14. P0 の解消状況（STEP 119 対照）

| ID | 内容 | この STEP の後 |
|---|---|---|
| P0-D | NOTICE が指す ATTRIBUTION が配布に無い。README が本家 | **部分解決**。同梱設定は足した。NOTICE 本文と README 本文は残る |
| P0-E | ArtCraft がバイナリに残る | **部分解決**。製品コードの埋め込みは切った。リポジトリの画像は残る |
| P0-A | 版 0.1.0 | 未着手 |
| P0-B | MSI/ZIP 未作成 | 未着手（設定だけ更新） |
| P0-C | DPAPI 説明 | 未着手 |
| P0-F / P0-G | 配布 exe の人確認、全体テスト | 未着手 |

## 15. 次工程の推奨

1. リポジトリから ArtCraft 画像を消すかどうかを利用者が決める。消すなら ATTRIBUTION 再生成まで一括。
2. `NOTICE` のアイコン経路を、実ファイルを見てから直すか決める。
3. 版 1.0.0 と `--version`／ログ（旧 STEP 案 120 の識別部分）。
4. 検証フォルダーで ZIP／MSI を作り、同梱リストと exe 検査を確認する（公開しない）。
5. 配布用 README を本家本文から分離するか決める。

## 16. `git diff --stat`

```
 README.md                              | 28 ++++++++++++++++------------
 crates/ui-egui/src/dialogs.rs          | 28 ++++++++++------------------
 crates/ui-egui/src/home.rs             |  9 +++------
 crates/ui-egui/src/widgets.rs          | 20 --------------------
 crates/ui-egui/tests/ui_group/links.rs |  3 ++-
 packaging/windows/package.ps1          | 27 +++++++++++++++++++++++----
 packaging/windows/pdfcraft.wxs         | 18 ++++++++++++++++++
 packaging/windows/portable-readme.md   |  4 ++--
 packaging/windows/test-msi.ps1         |  6 ++++++
 9 files changed, 80 insertions(+), 63 deletions(-)
```

（この報告書ファイルは未追跡のため `--stat` に含まれない。）

## 17. `git status`

開始 HEAD は `4eb07ee3` のまま。変更は未コミット。未追跡は STEP 119 のギャップ分析と本報告書。

## 改訂

- 2026-10-24：STEP 120。埋め込み切断と同梱追加。commit なし。
