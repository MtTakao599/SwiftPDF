# SwiftPDF

Windows 向けの閲覧中心 PDF アプリです。[PdfCraft](https://github.com/storytold/pdfcraft) から派生しています。PdfCraft ではありません。ArtCraft の製品でもありません。

**表示版は 1.0.0 です。正式な一般公開はまだ承認していません。** このリポジトリに、ダウンロード用の完成 MSI／ZIP はありません。

対象 OS は **Windows 10 / 11（64-bit）** です。macOS・Linux・ウェブ向けの SwiftPDF 配布は用意していません。

## 今できること

閲覧専用セッションのまま、次だけを許可しています。原本の上書き保存と、画像／Office などへの書き出しは拒否します。

- PDF を開いて読む（ズーム、連続表示、検索、テキストのコピー）
- **別名で保存**（新しいファイルへ。開いている原本は上書きしない）
- セッション中のテキスト追加
- 印刷と印刷用 PDF の作成
- このセッションで置いた画像スタンプ
- 表示中ページの回転（署名構造があると判断した文書では拒否）
- 理事長印（SWSEAL2）。承認・準備・確定と監査履歴

結合・分割・ページ削除・整理グリッド・フォーム設計・墨消し・デジタル署名の作成など、本家 README にある多くの編集機能は、現行の SwiftPDF UI では使えません。

## 開けない・保存できない文書

構造上、電子署名がある、または相互参照（xref）が壊れていると判断した PDF は、回転などの変更を拒否します。暗号学的な署名検証はしていません。拒否は安全側です。

ジャンクションや特殊なパスでの原本保護は、自動試験の範囲外です。

## 日本語 UI

日本語の画面文字は、ビルド時に `CRAFT_FONTS_DIR` で渡した [craft-fonts](https://github.com/storytold/craft-fonts) の BIZ UDPGothic を使います。これが無いビルドでは、日本語が欠けて見えることがあります。Noto Sans CJK は同梱しません。

## 理事長印とデータ

印影と監査履歴は、Windows の現在ユーザー向け DPAPI で暗号化した `seals.v2.bin`（SWSEAL2）に入ります。設定ファイル `app.ron` には書きません。

| 配布 | 保存先（コード上） |
|---|---|
| 通常（MSI など） | `%LOCALAPPDATA%\SwiftPDF` |
| ポータブル | `SwiftPDF.exe` と同じフォルダーの `Data`（隣に `portable.txt` があるとき） |

別の Windows ユーザーや別 PC へフォルダーをコピーしただけでは、通常は復号できません。エクスポート／インポート機能はありません。

詳細は [packaging/windows/portable-readme.md](packaging/windows/portable-readme.md) と [packaging/windows/install-notes.md](packaging/windows/install-notes.md) を見てください。

監査履歴は、アプリ内で行った押印操作の記録です。保存した PDF の内容を保証する証明ではありません。印影を PDF に確定したことと、その PDF をディスクへ保存したことは別です。

## 配布の形（未公開）

公開を承認したあとの予定です。いまダウンロードできる成果物はありません。

| | ZIP ポータブル | MSI |
|---|---|---|
| 実行ファイル | `SwiftPDF.exe` | 同じ |
| データ | exe 隣の `Data` | `%LOCALAPPDATA%\SwiftPDF` |
| 管理者 | 不要（書き込める場所） | マシン単位の導入では通常必要 |
| アンインストール | フォルダー削除。`Data` も一緒に消える | プログラムは消える。ユーザーデータの削除は **未実測** |

未署名の実行ファイルは、Windows の SmartScreen などで警告が出ることがあります。

## ビルド（開発者）

日本語フォントを埋め込むには craft-fonts が必要です。

```powershell
$env:CRAFT_FONTS_DIR="D:\dev\craft-fonts"
$env:CRAFT_FONTS_REQUIRED="1"
cargo run -p pdfcraft -- path\to\file.pdf
```

品質ゲートとレイヤー規則は [AGENTS.md](AGENTS.md) と [CLAUDE.md](CLAUDE.md) にあります。開発メモは [docs/development.md](docs/development.md) です。

`pdfcraft-cli` は開発用です。Windows 配布物には入れません。

## 帰属とライセンス

SwiftPDF は PdfCraft を基にしています。

**Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors.**

ソフトウェアは **MIT OR Apache-2.0** です。本文は [LICENSE-MIT](LICENSE-MIT) と [LICENSE-APACHE](LICENSE-APACHE)。必要な通知は [NOTICE](NOTICE)。素材の一覧は [ATTRIBUTION.md](ATTRIBUTION.md)。

ArtCraft の名称・ワードマーク・ロゴは ArtCraft Team の商標であり、オープンソースではありません。条件は [docs/brand/LICENSE-brand.txt](docs/brand/LICENSE-brand.txt) です。このツリーにそれらの画像ファイルはありません。

## 既知の制約（公開前）

次は未解決のままです。これらがあるあいだ、v1.0.0 の一般公開は承認しません。

- 印影保存の不定期な `WRITE_ERROR` / `Verify`（失敗時は安全側で拒否。原因未確定）
- 実 GUI での印刷プレビューと周辺確認（E4/E5）未実施
- 配布用 MSI／ZIP の導入・アンインストール試験未実施
- ワークスペース全体テスト未完了
- 未署名時の SmartScreen
- 既知の試験失敗（結合グリッドのキー操作、中国語 UI フォント）。SwiftPDF の現行 UI では結合グリッドは出しません

本家 PdfCraft の機能説明・スクリーンショット・getartcraft.com からの入手案内は、この README には載せていません。
