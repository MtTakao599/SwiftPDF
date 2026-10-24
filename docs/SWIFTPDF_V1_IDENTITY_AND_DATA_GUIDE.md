# SwiftPDF v1.0.0 製品識別とデータ保持説明（STEP 121）

状態：**表示・文書・ワークスペース版番号のみ。** PDF エンジン、SWSEAL2 の保存形式、監査フロー、既存データの保存先は変えていない。commit / push / タグ / Release / 正式 MSI／ZIP は作っていない。v1.0.0 の一般公開は未承認。

通常の `%LOCALAPPDATA%\SwiftPDF` と利用中の印影データには触れていない。

参照：`docs/SWIFTPDF_V1_RELEASE_GAP_ANALYSIS.md`（STEP 119）、`docs/SWIFTPDF_V1_LICENSE_BRAND_AUDIT.md`（STEP 120）、`docs/SWIFTPDF_V1_BRAND_ASSET_CLEANUP.md`（STEP 120B）。

## 1. 開始時の Git 状態

| 項目 | 値 |
|---|---|
| リポジトリ | `D:\dev\SwiftPDF` |
| ブランチ | `feature/seal-data-safety-v1.0.0` |
| HEAD | `4eb07ee3f71a2a2c75366098510193518a226036` |
| 作業ツリー |  dirty（STEP 119／120／120B の未コミットを保持したまま開始） |
| `git diff --check` | 空（空白事故なし） |
| 禁止操作 | `reset` / `clean` / `stash` / `rebase` は未使用 |

開始時に残っていた未コミット：AGENTS.md、ATTRIBUTION.*、NOTICE、ui-egui のブランド切り離し、`docs/brand/artcraft-*` の削除、package.ps1／wxs／portable-readme、xtask、STEP 119／120／120B の未追跡報告書。

## 2. 変更前の表示・バージョン一覧

分類は「利用者向け / 内部識別 / 互換性に関係する識別子」。一括置換はしていない。

| 箇所 | 変更前 | 分類 | 今回 |
|---|---|---|---|
| `[workspace.package] version` | `0.1.0` | 利用者向けの単一ソース | `1.0.0` |
| `workspace.dependencies` の `pdfcraft-*` path 版 | `0.1.0` | 内部（path crate は crate 版と一致必須） | `1.0.0`（一致のため） |
| `craft-ui` / `craft-quit` / `craft-print` | `0.1.0` | 外部 crate | 未変更 |
| About | `APP_NAME` + `build_info::version()` → SwiftPDF 0.1.0 | 利用者向け | ソース変更で 1.0.0 |
| `--version`（デスクトップ） | `SwiftPDF {version}`（本 STEP 前は `pdfcraft {}`） | 利用者向け | `SwiftPDF 1.0.0` |
| 起動ログ | `SwiftPDF {version}, log file …`（本 STEP 前は `PdfCraft {}`） | 利用者向け | `SwiftPDF 1.0.0, log file …` |
| パニック時 stderr | `SwiftPDF: {report}` | 利用者向け | 維持 |
| Windows VERSIONINFO | ProductName/FileDescription/InternalName = SwiftPDF。版は `CARGO_PKG_VERSION` | 利用者向け | 版だけ 1.0.0 |
| LegalCopyright | PdfCraft contributors | 帰属 | 未変更 |
| MSI ProductName / Manufacturer | SwiftPDF | 利用者向け | 未変更 |
| MSI `Version` | `package.ps1` がワークスペース版を読む | 利用者向け | 1.0.0 を読む |
| MSI UpgradeCode | `2BE82895-B377-4701-A51F-E03759ECEF56` | 互換性 | **未変更** |
| 実行ファイル名 | `SwiftPDF.exe` | 利用者向け | 未変更 |
| インストール先 | `Program Files\SwiftPDF` | 利用者向け | 未変更 |
| データフォルダー | `%LOCALAPPDATA%\SwiftPDF` / ポータブル `Data` | 互換性 | **未変更** |
| `DIR_NAME` / `APP_NAME` / `STORAGE_KEY` | SwiftPDF / SwiftPDF / swiftpdf | 互換性 | **未変更** |
| crate 名 `pdfcraft` / `pdfcraft-*` | 内部 | 内部 | **未変更** |
| ログファイル名 | `pdfcraft.log` | 内部 | **未変更** |
| `pdfcraft-cli --version` | `pdfcraft-cli {version}` | 内部（配布しない） | 表示文字列は未変更。版ソースは 1.0.0 |
| CLI VERSIONINFO ProductName | PdfCraft | 内部（配布しない） | 未変更 |
| README | 本家 PdfCraft | 利用者向け | SwiftPDF 専用に置換 |
| `portable-readme.md` 見出し | SwiftPDF 0.1.0 | 利用者向け | SwiftPDF 1.0.0 |
| `seals.v2.bin` / `seals.bin` / DPAPI | 現行形式 | 互換性 | **未変更** |

## 3. 変更後の製品名・バージョン

利用者が見る製品名は **SwiftPDF**、表示版は **1.0.0**。

単一ソースはルート `Cargo.toml` の `[workspace.package] version`。各 crate は `version.workspace = true`。`pdfcraft_engine::build_info::version()` は `CARGO_PKG_VERSION`。About、`--version`、起動ログ、winresource の FileVersion／ProductVersion、`package.ps1` の ZIP／MSI 名がこれを読む。

`PDFCRAFT_BUILD_LABEL` があるときだけ `1.0.0 (label)` になる。通常ビルドは `1.0.0`。

MSI の `ProductVersion` は数値 `major.minor.build`（プレリリースタグは捨てる）。`1.0.0` はそのまま使える。

## 4. 内部名を維持した理由

- crate / モジュール / 依存の `pdfcraft-*` を改名すると、ワークスペース全体と履歴が動く。停止条件に当たる。
- データフォルダー・`STORAGE_KEY`・ソケット名・DPAPI スコープは既に SwiftPDF。版番号を変えても保存先は動かない。
- `pdfcraft-cli` は配布物に入れない開発用。`--version` を SwiftPDF にすると、入っていない製品を名乗ることになる。
- ログ名 `pdfcraft.log` を変えると、既存のログ運用が切れる。利用者向け文書では「ログ」と書き、ファイル名だけ示す。

CLI の利用者向け表示を SwiftPDF にする案は、配布しないため採用していない。

## 5. Windows リソース

`apps/pdfcraft/build.rs`（`SwiftPDF.exe`）：

| 項目 | 値 |
|---|---|
| ProductName | SwiftPDF |
| FileDescription | SwiftPDF |
| OriginalFilename | SwiftPDF.exe |
| InternalName | SwiftPDF |
| FileVersion / ProductVersion | `CARGO_PKG_VERSION`（今回 1.0.0） |
| LegalCopyright | Copyright (c) the PdfCraft contributors. MIT OR Apache-2.0. |
| アイコン | `assets/app-icon/swiftpdf.ico`（リソース 1 と 2） |

UpgradeCode・コンポーネント GUID・インストール先は `pdfcraft.wxs` のまま。版を 1.0.0 にしただけでは製品識別は変わらない。

`pdfcraft-cli.exe` の VERSIONINFO は PdfCraft のまま。配布しない。

## 6. README の変更

`README.md` を SwiftPDF 専用に置き換えた。本家の機能一覧、getartcraft からの入手、本家スクリーンショットは載せない。

記載した項目：概要、Windows 10/11 x64、現行 UI で使える機能だけ、閲覧専用と許可している編集、署名／xref の制限、別名保存、日本語 UI、理事長印と SWSEAL2、保存先、ポータブルと MSI、ビルド／AGENTS.md、PdfCraft 帰属、LICENSE／NOTICE／ATTRIBUTION、既知の制約と未公開。

実装されていても現行 UI から使えない結合・分割・整理グリッド・フォーム設計・墨消し・デジタル署名作成などは「使えない」と書いた。未完成の MSI／ZIP をダウンロードできるようには書いていない。

## 7. 本家スクリーンショットの扱い

`docs/images/pdfcraft-*.png`（viewer、tools、dark など 13 枚）はディスクに残した。ATTRIBUTION.toml のエントリも残した。

- README からは外した。
- ファイル削除と Git 履歴改変はしていない（STEP 120B と同じ）。
- 画面の一部に ArtCraft マークが写っている可能性がある。公開用の製品紹介には使わない。
- `docs/images/SwiftPDFicon.png` と source は SwiftPDF のアイコンとして維持。

ArtCraft 商標画像 8 枚は STEP 120B で作業ツリーから削除済み（`D`）。`docs/brand/` に残るのは `LICENSE-brand.txt` だけ。

## 8. AGENTS.md の変更

§1.2 の ArtCraft 例外だけを直した。

- 商標であることは残す。
- `docs/brand/LICENSE-brand.txt` を残す。
- **このツリーに画像ファイルは無い。戻さない。**
- その他の安全規則、クリーンルーム、Never crash、MCP は未変更。

ロゴを再導入していない。

## 9. 通常版のデータ保存先

コード上：`identity::DIR_NAME` = `SwiftPDF`。Windows では `%LOCALAPPDATA%\SwiftPDF`。

`settings_dir()` がポータブルでなければこのフォルダー。`set_seal_dir(settings_dir())`。印影は `app.ron` に書かない。

| ファイル | 用途 |
|---|---|
| `seals.v2.bin` | SWSEAL2（印影と監査履歴。現在ユーザー向け DPAPI） |
| `seals.bin` | SWSEAL1 原本、または移行後の目印 |
| `seals.bin.lock` | 同時書き込み用ロック（空） |
| `seals.bin.swseal1-*.bak` | 移行時の旧暗号控え |
| `seals.bin.legacy-*` | 移行後に旧版が書いた内容の退避 |
| `app.ron` | アプリ設定 |
| `logs/pdfcraft.log` | ログ |
| `Recovery\` など | クラッシュ復旧（閲覧専用では使わない場合あり） |

単一インスタンス用ソケットも同じフォルダー（`instance-*`）。印影ではない。

## 10. ポータブル版のデータ保存先

コード：`portable.rs`。`SwiftPDF.exe` の隣に `portable.txt` または `SwiftPDF.portable`（ファイル）があると、`<exe ディレクトリ>/Data` を作って使う。中身は見ない。

- 初回起動で `Data` を作り、書き込み試験（`.write-test`）をする。
- 作れない／書けないときはポータブルを止め、`%LOCALAPPDATA%\SwiftPDF` に戻す。理由をログする。
- 印影・設定・ログ・復旧・新規デジタル ID は `Data`。
- **同時起動ソケットだけは** `%LOCALAPPDATA%\SwiftPDF\portable-<name>`。共有フォルダーへパスを書かないため。印影ではない。
- exe だけ移すと `Data` は付いてこない。`portable.txt` を消すと次の起動から通常フォルダーを使う。自動では移さない。

## 11. DPAPI の制約

`seals.v2.bin` は、今ログインしている Windows ユーザーの DPAPI で暗号化する。

- 同じ PC・同じユーザーで、プロファイルと DPAPI の鍵が残っていれば、再インストール後も読める**場合がある**。
- 別の Windows ユーザーでは、そのまま復号できない。
- 別 PC へ `Data` や `%LOCALAPPDATA%\SwiftPDF` をコピーしただけでは、通常はそのまま使えない。
- プロファイルや鍵を失うと、復号できなくなる**可能性がある**。

「コピーすれば移せる」「必ず復元できる」とは書いていない。エクスポート／インポート機能は無い（追加していない）。

## 12. バックアップ・復元

推奨（保証ではない）：

1. SwiftPDF を終了する。
2. 通常版は `%LOCALAPPDATA%\SwiftPDF`、ポータブルは exe 隣の `Data` を、書き込めない場所へコピーする。`seals.v2.bin`、`seals.bin`、`*.bak`、`legacy-*` を含める。
3. ストアを手で編集しない。旧形式の控えを安易に消さない。

限界：同じ環境への戻しと、別 PC／別ユーザーへの移行は別問題。バックアップがあっても鍵が無ければ読めない。復元できることは保証しない。

## 13. SWSEAL1 からの移行

- 利用者が画面で承認したあとだけ行う。
- 延期しても旧 `seals.bin` は変わらない。延期中は理事長印を使えない。
- 移行時に旧暗号データをバックアップする。
- 移行後に旧版が `seals.bin` へ書いた内容は `legacy-*` へ退避する。自動では SWSEAL2 に混ぜない。
- 壊れたストアを空の初期状態にはしない。

形式・移行コードは今回変更していない。説明だけを書いた。

## 14. 監査履歴の意味

履歴は、このアプリで行った押印操作の記録である。保存した PDF の状態を保証する証明ではない。

印影を PDF に確定したことと、その PDF をディスクへ保存したことは別である。

## 15. 容量・Busy・性能

利用者向け文書に次を書いた（実装は未変更）。

- 暗号化ストア上限 64MiB。超過の登録・準備は拒否。
- 48MiB 超で警告。
- 履歴 1 万件超で警告。自動削除・短縮はしない。
- 大きいストアでは保存に数秒以上かかることがある。
- ほかの SwiftPDF が同じストアを使っていると、約 3 秒で Busy。入力は残す。利用者がやり直す。
- 不定期の `WRITE_ERROR` / `Verify` は、その回を拒否する。成功扱いしない。

## 16. ZIP／MSI の説明

`packaging/windows/portable-readme.md` を SwiftPDF 1.0.0 向けに書き直し、ZIP 内 `README.md` としてコピーされる。`packaging/windows/install-notes.md` を新規。ルート README から両方へリンクする。

| | ZIP（予定） | MSI（予定） |
|---|---|---|
| 成果物名 | `SwiftPDF-1.0.0-windows-<arch>-portable.zip` | `SwiftPDF-1.0.0-windows-<arch>.msi` |
| exe | `SwiftPDF.exe` | 同じ |
| データ | exe 隣 `Data` | `%LOCALAPPDATA%\SwiftPDF` |
| 発行者表示 | （パッケージなし） | Manufacturer = SwiftPDF（法人名ではない） |
| UpgradeCode | なし | `2BE82895-B377-4701-A51F-E03759ECEF56`（未変更） |
| アンインストール | フォルダー削除。`Data` も消える | プログラムは消える。ユーザーデータ削除は **コード上「削除項目なし」の想定。実測前** |

コード上の想定：wxs に LOCALAPPDATA の `RemoveFolder` は無い。したがってアンインストール後もユーザーデータは残る**想定**。実インストール試験は未実施なので保証しない。再インストール後、同じ Windows ユーザーでプロファイルが残っていれば読める場合がある。

未署名のため SmartScreen が出ることがある、と書いた。正式 ZIP／MSI は作っていない。

`test-msi.ps1` の例パスを `SwiftPDF-1.0.0-windows-x64.msi` に合わせた。検査ロジックは版に依存しない。

## 17. 品質確認結果

環境：`CRAFT_FONTS_DIR=D:\dev\craft-fonts`、`CRAFT_FONTS_REQUIRED=1`、`CARGO_BUILD_JOBS=2`。

| 確認 | 結果 |
|---|---|
| `cargo fmt --check` | 成功 |
| `cargo check -p pdfcraft -j 2` | 成功（`pdfcraft v1.0.0`） |
| `cargo clippy -p pdfcraft-platform --all-targets -j 2 -- -D warnings` | 成功 |
| `cargo clippy -p pdfcraft-engine --all-targets -j 2 -- -D warnings` | 成功 |
| `cargo clippy -p pdfcraft-ui-egui --all-targets -j 2 -- -D warnings` | 成功 |
| `the_about_dialog_shows_the_running_version` | 成功（`Version` + `CARGO_PKG_VERSION`） |
| `about_dialog_shows_the_brand_and_links` | 成功（Based on PdfCraft。ArtCraft マークなし） |
| `version_resource`（ProductName／FileVersion 1.0.0／著作権） | 成功 |
| `view_only_launch`（`--control` 拒否） | 成功。通常の `%LOCALAPPDATA%\SwiftPDF` は使っていない |
| `pdfcraft-engine` `a_label_follows_the_version` | 成功 |
| `SwiftPDF --version` | 隔離 `LOCALAPPDATA` で `SwiftPDF 1.0.0`（exit 0）。ユーザーフォルダーにファイルを作っていない |
| 正式 MSI／ZIP | 未作成 |
| 実 GUI 受け入れ | 未実施（別工程） |

`Cargo.lock` はワークスペース crate の `0.1.0` → `1.0.0` だけ（36 パッケージ）。他 crate の取り込みは無い。`cargo update --workspace` は使っていない（`cargo check` が lock を合わせた）。

## 18. 残る P0／P1

STEP 119 の番号を引き継ぐ。今回「解決した」とはしない。

| # | 内容 | 今回の後 |
|---|---|---|
| P0-1 | 並列 I/O の不定期 `WRITE_ERROR` / `Verify` | 未解決 |
| P0-2 | SWSEAL1 の 400 行上限（移行前） | 部分のまま |
| P0-3 | ワークスペース全体テスト | 未実施 |
| P0-4 | 配布 exe の実 GUI（E4／E5 含む） | 未実施 |
| P0-5 | 版と製品表示 | **文書と表示は 1.0.0。正式公開は未承認** |
| P0-6 | 実 MSI／ZIP の導入・アンインストール試験 | 未実施 |
| P0-7 | 印影データの利用者説明 | **文書は書いた。実地確認は未了** |
| P0-8 | ライセンス同梱と表記 | STEP 120／120B で前進。正式パッケージ未作成 |
| P0-9 | ArtCraft 画像 | 作業ツリーから削除。Git 履歴と本家スクリーンショットは残る |
| P1 | ジャンクション、仮置きスタンプの終了確認、注釈画像の再ハッシュ、combine_grid／中国語フォント試験、未署名 SmartScreen | 未解決 |

閲覧専用ホームに、本家向け字幕「scriptable」が残る。ArtCraft コミュニティ欄は閲覧専用では出さない。i18n カタログの「PdfCraft is drawing without a graphics processor…」は、閲覧専用では使わない分岐に残る。

## 19. 次工程への提案

1. 隔離環境で ZIP／MSI を作り、導入・更新・アンインストールを測る。ユーザーデータが残るか消えるかを実測してから、install-notes の「想定」を「実測」に変える。
2. 不定期 I/O の診断（OS コードと段階）。再試行回数は増やさない。Verify を成功扱いしない。
3. E4／E5 の人による GUI。
4. 公開前に SmartScreen／署名方針を決める。
5. 必要なら閲覧専用ホームの「scriptable」字幕だけを直す。crate 改名はしない。
6. 正式公開は、上記と全体テストのあと、利用者の承認を待つ。

## 20. `git diff --stat`

報告書作成時点。STEP 119〜121 を含む。`ROADMAP.md` は未変更。

```
 AGENTS.md                               |   2 +-
 ATTRIBUTION.md                          |  12 +-
 ATTRIBUTION.toml                        |  90 +----
 Cargo.lock                              |  72 ++--
 Cargo.toml                              |  68 ++--
 NOTICE                                  |  10 +-
 README.md                               | 601 +++-----------------------------
 apps/pdfcraft/Cargo.toml                |   2 +-
 apps/pdfcraft/src/main.rs               |   6 +-
 apps/pdfcraft/tests/version_resource.rs |   2 +
 crates/ui-egui/src/dialogs.rs           |  28 +-
 crates/ui-egui/src/home.rs              |   9 +-
 crates/ui-egui/src/widgets.rs           |  20 --
 crates/ui-egui/tests/ui_group/links.rs  |   3 +-
 docs/brand/artcraft-logo-white.png      | Bin 32161 -> 0 bytes
 docs/brand/artcraft-logo-white.svg      |   7 -
 docs/brand/artcraft-logo.png            | Bin 31302 -> 0 bytes
 docs/brand/artcraft-logo.svg            |   7 -
 docs/brand/artcraft-mark-black.png      | Bin 26793 -> 0 bytes
 docs/brand/artcraft-mark-black.svg      |   4 -
 docs/brand/artcraft-mark.png            | Bin 24715 -> 0 bytes
 docs/brand/artcraft-mark.svg            |   4 -
 packaging/windows/package.ps1           |  27 +-
 packaging/windows/pdfcraft.wxs          |  18 +
 packaging/windows/portable-readme.md    |  86 ++++-
 packaging/windows/test-msi.ps1          |   8 +-
 xtask/src/assets.rs                     |   8 +-
 xtask/src/screenshots.rs                |   7 +-
 28 files changed, 286 insertions(+), 815 deletions(-)
```

未追跡は stat に出ない：`docs/SWIFTPDF_V1_*.md` 4 本と `packaging/windows/install-notes.md`。

## 21. `git status`

```
feature/seal-data-safety-v1.0.0
4eb07ee3f71a2a2c75366098510193518a226036
```

変更 28、削除 8（ArtCraft 画像）、未追跡 5（STEP 119／120／120B／121 の報告書と install-notes）。commit / push していない。
