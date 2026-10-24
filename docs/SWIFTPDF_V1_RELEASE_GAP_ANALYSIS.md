# SwiftPDF v1.0.0 残課題の総点検（STEP 119）

状態：**調査と設計のみ。製品コードは変えていない。commit / push / タグ / Release / MSI / ZIP は作っていない。v1.0.0 の一般公開は未承認。**

通常の `%LOCALAPPDATA%\SwiftPDF` と利用中の印影データには触れていない。

## 1. 調査時点の Git 情報

| 項目 | 値 |
|---|---|
| リポジトリ | `D:\dev\SwiftPDF` |
| ブランチ | `feature/seal-data-safety-v1.0.0` |
| HEAD | `4eb07ee3f71a2a2c75366098510193518a226036` |
| 作業ツリー | クリーン（本ファイル追加前） |
| origin | `https://github.com/MtTakao599/SwiftPDF.git` |
| 直前 | STEP 118 で SWSEAL2 を 2 コミット push 済み（`0623b414` 製品、`4eb07ee3` 文書） |

参照：`docs/SWIFTPDF_V1_RELEASE_READINESS.md`（STEP 115）。過去の報告書だけで「解決済み」とはしない。

## 2. STEP 115 の P0 再評価

| # | 当時の課題 | 今の分類 | 根拠 |
|---|---|---|---|
| P0-1 | 複数プロセスで印影・履歴が消える | **部分解決** | `crates/platform/src/lock.rs` の `LockFileEx`、`SealVault::lock`、書き込み前の読み直し。試験：`seal_store_v2_processes`（2 プロセス 40 イベント保持）、`two_windows_keep_each_others_seals_and_a_stale_change_is_refused`。残：並列 46 回中 1 回の `WRITE_ERROR`/`Verify`（3 節） |
| P0-2 | SWSEAL1 の 400 行で古い履歴が消える | **部分解決** | SWSEAL2 は追記専用。上限はファイル 64MiB。警告は 48MiB と 1 万件（`Limits`、`the_panel_shows_size_warnings_and_open_preparations`）。移行前の SWSEAL1 は 400 行のまま。移行延期中は理事長印が使えない |
| P0-3 | 修正後のワークスペース全体テスト | **未解決** | STEP 117K/118 は platform / engine / ui-egui の scoped 試験。`--workspace` 全件は未実施 |
| P0-4 | 最終バイナリの Windows 実 GUI | **部分解決** | 117K 検証 exe（SHA-256 `18A227FC…8392`）で K1〜K7 は利用者 OK。E4／E5 未実施。配布用 MSI/ZIP の exe では未確認 |
| P0-5 | 版を 1.0.0 にし製品識別を整える | **未解決** | `[workspace.package] version = "0.1.0"`。About と `--version` はこれを表示。`--version` は `pdfcraft 0.1.0`。ログは `PdfCraft 0.1.0`。`portable-readme.md` 見出しは「SwiftPDF 0.1.0」 |
| P0-6 | MSI と ZIP を検証環境で作り、導入試験する | **未解決** | `packaging/windows/package.ps1` と `pdfcraft.wxs` はある。このブランチで成果物を作った記録は無い |
| P0-7 | 印影の移動・保持の利用者説明 | **未解決** | 移行ダイアログは「以前の SwiftPDF では使えません」とある。`portable-readme.md` は Data ごとコピーとだけ書き、別 PC・別ユーザー・再インストールでは復号できないことを書いていない |
| P0-8 | ライセンス表記の整合 | **未解決** | 7 節。`NOTICE` は `ATTRIBUTION.md` を参照するが、ZIP/MSI の同梱は `LICENSE-*` と `NOTICE` のみ。README は本家 PdfCraft のまま |
| P0-9 | ArtCraft ブランド画像 | **未解決（要判断）** | `widgets.rs` が SVG を `include_bytes!`。閲覧専用 About ではロゴを出さない（`dialogs.rs`）。バイナリには残る。`docs/brand/LICENSE-brand.txt` は改変版・フォークでの削除を求める |

品質基準（当時の必須 1〜10）の今：

| 必須 | 判定 |
|---|---|
| 1–2 原本・既存先を上書きしない | 概ね満たす。ジャンクションは未検証（P1） |
| 3 失敗時に入力を失わない | 確定済み編集は保護。仮置きスタンプは終了確認に入らない（P1） |
| 4 署名・権限ガード | 構造判定の範囲で維持 |
| 5 承認・監査を破損しない | 主な消失経路は塞いだ。不定期 I/O は安全側拒否。原因未確定 |
| 6 再オープン | 自動試験と 117K の K4/K5 で確認。配布 exe では未確認 |
| 7 実ウィンドウ | 部分。配布物では未確認 |
| 8 新規失敗 0 | 未達。全体テスト未実施。既知 2 件は残る |
| 9 秘密・実印影を含めない | リポジトリは確認済み。パッケージ未作成 |
| 10 ライセンス整合 | 未達 |

## 3. SWSEAL2 の未解決 I/O リスク

観測（解決済みにしない）：

- 全体並列 46 回中 1 回：`WRITE_ERROR` と `Verify`。OS コード不明
- STEP 118 の `seal_` suite 内で、効果プレビュー試験が 1 回 Verify 相当で失敗。単独再実行と次の suite は成功
- 失敗時は押印を拒否または hold。直近の障害注入では既存データの消失は再現していない

### 3.1 コード上の経路

1. 原子置換：`write_atomically`（`crates/platform/src/staging.rs`）。隣に `create_new` の一時ファイル（`.seals.v2.bin.pdfcraft-<16hex>.tmp`）、`write_all`、`sync_all`、閉じてから `rename` で上書き。Windows では閉じたあとでないと rename できない
2. 共有モード：明示していない。標準の `OpenOptions`。既存の `seals.v2.bin` を他者が開いていると rename が `ERROR_SHARING_VIOLATION`（32）や `ERROR_ACCESS_DENIED`（5）になり得る
3. 一時名：`RandomState::hash_one`。16 回まで。予測して埋められる名ではない
4. ロック：`seals.bin.lock` を `LockFileEx` 相当（`File::try_lock`）で保持しているあいだに書く。ロックは空ファイル。データファイル自体はロックしない（rename でハンドルが変わるため）
5. 置換と Verify：書いた直後に読み直し、バイト比較と decode。不一致は `VaultError::Verify`。書いた内容は採用しない
6. DPAPI：平文 JSON を現在ユーザーで暗号化してから書く。復号失敗はファイルを置き換えない
7. Defender 等：新しい一時ファイルと rename 直後の読み直しを走査すると、欠落や共有違反になり得る。証拠は無い
8. 同一プロセス：試験の `io_note` はプロセス全体。フックは vault ごと。製品の一時名はファイルごと
9. エラー分類：製品の `VaultError::Io` は固定文言 `WRITE_ERROR`。`raw_os_error` は `cfg(test)` の `io_note` にしか残らない。本番ログには Win32 番号が残らない
10. 再照合：読み取りだけ。WRITE_ERROR の原因にはならない。Verify/Io のあとに状態を確定する

`with_retry` は `PermissionDenied` と Win32 32/33 だけを約 2 秒再試行する。回数を今回増やさない。

### 3.2 追加調査の設計（実施しない）

目的：次に失敗したとき、OS コードと段階（create / write / sync / rename / reread）が残ること。

- 試験または診断ログに、`raw_os_error` と段階名を 1 件分残す（製品の常設スイッチは付けない）
- 再現は「同じフォルダーへの連続書き込み」を上限付きで 1 セット。無制限の全体並列はしない
- Verify 不一致を成功にしない。履歴の等式を緩めない

## 4. データ保存・移行・DPAPI の制約

| 項目 | 現状 | 利用者への説明 |
|---|---|---|
| 64MiB 上限 | 超過の登録・準備は拒否。開いている押印の確定用に余裕を残す | パネル警告（48MiB） |
| 1 万件超 | `Notice::ManyEvents`。追記は続ける | パネル表示 |
| 処理中 | `SealWork` モーダル。編集・保存・終了を止める | 「理事長印の処理が終わるまで…」 |
| Busy 3 秒 | ロック待ち切れ。入力は保持 | 「ほかの SwiftPDF が…使用中」 |
| 未完了準備 | `use_prepared` を一覧表示。自動では閉じない | パネル |
| 移行 | ダイアログからのみ。延期は原本を変えない。失敗は原本保持 | 一部あり |
| 旧版の遅延書き戻し | 取り込みず退避し、記録する | トースト |
| 破損ストア | 自動初期化しない | 使えず、ファイルは残る |
| 別ユーザー・別 PC | 現在ユーザー DPAPI。復号できない。登録も拒否 | **不足**。`portable-readme.md` 未記載 |
| アンインストール | MSI は `%LOCALAPPDATA%\SwiftPDF` を消さない | **不足** |

暗号化の大規模改修はしない。説明と、必要ならバックアップ案内が先。

## 5. 未保存編集と終了

コード：`editing.rs` の `has_unsaved_work` / `request_quit` / `guard_quit`、`seal_flow.rs`。

| 動作 | 自動試験 | 実 GUI |
|---|---|---|
| dirty な PDF の終了確認 | あり | 117K の × は OK。E の周辺は未実施 |
| SealWork 中の終了拒否 | `the_close_button_waits_for_seal_work` | 未確認 |
| 保存失敗で終了しない | あり | 未確認 |
| `hold_output` 中の保存・印刷・書き出し拒否 | `a_stamp_that_can_neither_be_recorded_nor_taken_back_holds_the_document` | 未確認 |
| 保存せずに閉じる | 同上（残る操作） | 117K K6 |
| 仮置きスタンプ | **`has_unsaved_work` に含まれない** | 警告なしに消える可能性（P1） |
| クラッシュ | 作業コピー／復旧は別経路。確定前の押印は残らない | 未確認 |
| 自動保存 | hold 中は対象外 | 未確認 |
| 複数タブ | dirty を順に尋ねる | 未確認 |
| 印刷・エクスポート | 閲覧専用では印刷ダイアログと新規印刷 PDF。hold は拒否 | E4 未実施 |

クラッシュで消える未確定編集は、終了確認では防げない。説明事項。

## 6. 名称・バージョンの監査

利用者に見えるもの（内部 crate 名 `pdfcraft-*` は変えない）：

| 場所 | 今 | v1.0.0 で必要か |
|---|---|---|
| ウィンドウタイトル、About 見出し | SwiftPDF（`identity::APP_NAME`） | 維持 |
| About の版 | `0.1.0`（`CARGO_PKG_VERSION`） | **1.0.0 に更新** |
| `--version` | `pdfcraft 0.1.0` | SwiftPDF 1.0.0 |
| ログ先頭 | `PdfCraft 0.1.0, log file …` | SwiftPDF |
| exe | `SwiftPDF.exe` | 維持 |
| Windows 資源 | ProductName / FileDescription は SwiftPDF。著作権は PdfCraft contributors。CompanyName 無し | 発行者は判断 |
| 設定フォルダー | `%LOCALAPPDATA%\SwiftPDF` / ポータブル `Data` | **変えない** |
| `portable-readme.md` | 「SwiftPDF 0.1.0」 | 1.0.0 と DPAPI 注意 |
| MSI / ZIP 名 | ワークスペース版から生成 | 版更新後 |
| README.md | 本家「PdfCraft」 | SwiftPDF 向けに分けるか、配布物だけ差し替える |
| 内部型 `PdfCraftApp` | コード識別 | 変更しない |

設定フォルダー名を変えると既存データが読めなくなる。版だけを上げる。

## 7. LICENSE / NOTICE / 依存

現在の表示：

- リポジトリ：`LICENSE-MIT`、`LICENSE-APACHE`、`NOTICE`（「PdfCraft / ArtCraft Team and the PdfCraft contributors」）
- About（閲覧専用）：「PdfCraft を基にしています。Copyright (c) the PdfCraft contributors。」
- ZIP/MSI：`LICENSE-MIT`、`LICENSE-APACHE`、`NOTICE`、文書アイコン用 `LICENSE-document.txt`、craft-fonts の OFL。**`ATTRIBUTION.md` は同梱しない**
- `NOTICE` は「一覧は ATTRIBUTION.md」と書く
- フォント：Inter / JetBrains Mono / Dancing Script（リポジトリ）、BIZ UDPGothic 等（craft-fonts、リリース必須）
- アイコン：Lucide ISC。SwiftPDF アイコンは ATTRIBUTION 済み
- 依存：hayro、egui、lopdf 等は NOTICE に要約。`cargo-deny` は資産を見ない（NOTICE 自身がそう書く）
- ArtCraft：商標。公式 PdfCraft 内でのみ使用可。フォークは削除

不一致の疑い（断定しない）：

1. 配布物に `ATTRIBUTION.md` が無く、NOTICE の参照が空振りする
2. README が本家のまま（署名者・製品名・ArtCraft リンク）
3. バイナリに ArtCraft SVG が埋め込まれる（画面には出さない）
4. NOTICE の文書アイコン記述が `pdfcraft-document.svg` のまま
5. WiX ツール自体のライセンスは生成物に乗らない、という理解は未確認

公開前に必要な証拠：

- 同梱ファイルの実リスト（パッケージ作成後）
- ArtCraft を残す／外す判断の記録
- 発行者名と著作権行の方針
- craft-fonts の pin と OFL ファイル

不明な著作権表示は推測で消さない。ライセンス不明のまま公開するのは P0。

## 8. ZIP と MSI の比較

| 項目 | ZIP ポータブル | MSI |
|---|---|---|
| 初回 | 展開して exe。`portable.txt` → `Data` | Program Files。スタートメニュー |
| 管理者 | 不要（書き込める場所） | マシン単位なので通常は必要 |
| 削除 | フォルダーを消す。Data も消える | プログラムは消える。ユーザーデータは残る |
| 印影の場所 | `<exe>\Data\seals.v2.bin` | `%LOCALAPPDATA%\SwiftPDF` |
| 更新 | exe の置き換え。同じ場所へ再展開したときの Data は手順未確認 | `MajorUpgrade`。ユーザーデータは残る想定（未試験） |
| SmartScreen | 未署名なら警告しやすい | 同様 |
| 署名 | `sign.ps1` あり。証明書未入手 | 同じ |
| 向き | 検証、持ち運び、管理者なし | 通常の導入 |

推奨：両方出す。先に ZIP で導入手順と DPAPI 説明を固め、MSI はインストール／更新／アンインストールとデータ残存を検証してから。今回は作らない。

見込み：ZIP 半日、MSI と導入試験 1〜2 日（WiX v5、フォント、OCR モデル、署名は別）。

## 9. 自動テストと実 GUI の現状

実施済み（117K/118、scoped）：

- ui-egui `--lib` 602 / 0 / 4
- うち `seal_` 96 / 0 / 3
- プロセス 3 / 0
- platform 24、engine 325
- fmt / check / clippy（対象 crate）

未実施：`--workspace` 全件。配布 exe での受け入れ。E4 印刷プレビュー、E5 周辺。MSI 導入。xref 修復 PDF の人による拒否確認（自動はある）。

既知の失敗：

| 試験 | 原因の見立て | 製品影響 | 扱い |
|---|---|---|---|
| `combine_grid::arrow_keys_move_by_card_and_by_row_and_shift_extends` | 760×1200 で 9 枚目が画面外。アクセシビリティ木に出ない | SwiftPDF は結合グリッドを閲覧専用で出さない | 公開可の根拠にはしない。直すか「環境・無効機能」と文書化するまで P1 |
| `fonts::chinese_ui_uses_an_installed_chinese_face` | 中国語 UI で Inter がスタックに無い、または system fallback の順 | 日本語 UI が主。中国語面 | 同上。`PDFCRAFT_SYSTEM_FONTS` とインストール済みフォントに依存 |

「以前から落ちている」だけでは公開可にしない。

## 10. P0 / P1 / P2

### P0（これがあると正式公開できない）

| ID | 現象 | 影響 | 原因／未確定 | 方針 | 試験 | 規模 | 延期 |
|---|---|---|---|---|---|---|---|
| P0-A | 表示版が 0.1.0 | 配布物と更新の識別が誤る | ワークスペース版 | `Cargo.toml` を 1.0.0。`--version` / ログ / portable-readme | `version_resource` | 小 | 不可 |
| P0-B | MSI/ZIP 未作成・未導入試験 | 導入でデータが消えるか未確認 | 未実施 | 検証フォルダーで `package.ps1`。インストール／更新／削除。CLI と秘密が無いこと | `test-msi.ps1`、目視 | 中 | 不可 |
| P0-C | DPAPI・残データの説明不足 | 別 PC へコピーして印影が使えなくなる。アンインストール後のデータが残ると誤解 | 文書不足 | portable と MSI の説明。推測で暗号は変えない | 文書レビュー | 小 | 不可 |
| P0-D | ライセンス同梱と README の食い違い | 配布条件が追えない | NOTICE が ATTRIBUTION を指す。README が本家 | 同梱リストを決める。README を配布用に分ける | パッケージ検査 | 小〜中 | 不可 |
| P0-E | ArtCraft 商標がバイナリに残る | ブランド規約。公開後に取り消せない | `include_bytes!` | 残すなら権利者の明示許可。無ければ埋め込みを外す | `xtask assets`、バイナリ文字列 | 中 | 判断後は不可 |
| P0-F | 配布 exe での人の受け入れが無い | 必須 7 | 117K は検証フォルダーの exe | パッケージした exe で主要操作。E4/E5 を含めるか判断 | チェックリスト | 中 | 不可（範囲は判断） |
| P0-G | ワークスペース全体テスト未実施 | 必須 8 | scoped のみ | 1 回。描画は単一スレッド。既知 2 件は別枠で記録 | `--workspace` | 中 | 不可 |

P0-1（複数プロセス消失）は **部分解決** とし、残る I/O は下の P1-A。消失そのものを未解決の P0 には戻さない。

### P1（公開前に望ましい。回避策があれば条件付き延期可）

| ID | 現象 | 影響 | 方針 | 規模 | 延期 |
|---|---|---|---|---|---|
| P1-A | 不定期 `WRITE_ERROR`/`Verify` | その回の押印が拒否される。既存データは残る想定 | 診断（OS コード）。再試行は増やさない | 小〜中 | 安全側拒否を説明すれば延期可。原因確定は公開条件にしない |
| P1-B | 既知 2 試験 | CI の「新規 0」が人依存 | 直すか、無効機能／環境依存と文書化 | 小 | 方針決定は公開前。修正自体は延期可 |
| P1-C | 仮置きスタンプが終了確認に入らない | 警告なしに消える | `has_unsaved_work` に入れる | 小 | 説明すれば延期可 |
| P1-D | ジャンクション等の原本保護が未検証 | 原本上書きの可能性（未確認） | `save_path` 試験 | 小 | 検証延期は要判断 |
| P1-E | `--version` / ログ / README の PdfCraft 表記 | 識別の混乱 | SwiftPDF 化（フォルダー名は維持） | 小 | P0-A と同時がよい |
| P1-F | コード署名なし | SmartScreen | 証明書の判断 | 中（調達） | 警告を書いて延期可 |
| P1-G | CLI/MCP がパッケージに入らないことの再確認 | 自動化面 | `package.ps1` の既存拒否を確認 | 小 | P0-B に含める |

### P2（次期）

WARP 並列、本家 #3362、`/AS` PrintState、暗号学的署名検証、ページ削除など、翻訳、Store、`docs/development.md`。安全に直結しない。

## 11. 残作業の優先順位

1. 版 1.0.0 と、利用者が見る識別（`--version`、ログ、portable-readme）（P0-A、P1-E）
2. ArtCraft とライセンス同梱の判断と実施（P0-D、P0-E）
3. 印影の移動・残存の説明（P0-C）
4. 検証用 ZIP（続いて MSI）の作成と導入試験（P0-B、P1-G）
5. ワークスペース試験 1 回と既知 2 件の扱い決定（P0-G、P1-B）
6. 配布 exe での人の受け入れ（P0-F）。E4/E5 を必須にするか決める
7. I/O 診断の最小実装は、公開と並行してよい（P1-A）
8. 仮置きの終了確認（P1-C）

おおよそ：識別と文書 1〜2 日、ブランド／NOTICE 0.5〜1 日、ZIP+MSI+導入 1〜2 日、全体テスト半日、実 GUI 半日。署名と証明書は別。

## 12. 推奨するリリース手順

1. この文書で P0 の実施順を利用者が承認する
2. 識別・説明・ライセンスを直す（別 STEP。今回はしない）
3. 検証フォルダーで ZIP を作り、隔離起動する。通常の `%LOCALAPPDATA%\SwiftPDF` は使わない
4. MSI を同じ検証方針で入れる（別 Windows ユーザーまたは VM）
5. 全体テスト 1 回。既知 2 件以外の新規失敗で止める
6. 配布 exe で人がチェックリストを埋める
7. 利用者が公開を承認してから、タグ・Release・署名

今回の STEP 番号（115 の旧 118〜120）とはずらす。SWSEAL2 の commit は済んでいる。

## 13. 公開可否

**不可。** 版表示、配布物未作成、ライセンス／ブランド、配布 exe の人確認、全体テストが残っている。SWSEAL2 の主な消失経路は塞いだが、I/O 原因未確定と E4/E5 未実施は残る。

## 14. 次の STEP 案

| STEP | 内容 | 製品コード |
|---|---|---|
| 120 | 版 1.0.0、`--version`／ログ、portable / MSI の印影説明 | 識別と文書。機能追加なし |
| 121 | NOTICE 同梱、ArtCraft の可否、README／配布説明 | 判断後の最小変更 |
| 122 | 検証用 ZIP/MSI 作成と導入試験（公開しない） | なし（スクリプト実行） |
| 123 | ワークスペース試験と既知 2 件の扱い | 試験か文書のみが望ましい |
| 124 | 配布 exe の実 GUI（K 相当 + E4/E5 の要否） | なし |
| 125 | 公開判断。タグと Release は明示指示のあと | なし |

I/O 診断は 120 と並行して設計してよい。再試行の増加はしない。

## 改訂

- 2026-10-24：STEP 119 初版。調査のみ。
