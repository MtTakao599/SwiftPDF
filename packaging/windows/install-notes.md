# SwiftPDF 1.0.0（MSI）

マシン単位で `Program Files\SwiftPDF` に入れる構成です。スタートメニューと、任意のデスクトップショートカットの名前は SwiftPDF です。発行者表示はパッケージ上「SwiftPDF」です（法人名は付けていません）。

**正式な一般公開はまだ承認していません。** この文書は WiX 設定とコード上の想定です。実際のインストール／アンインストール試験は未実施です。ユーザーデータが残ること・消えることを、実測前に保証しません。

## 導入

- 管理者権限と `ALLUSERS=1` が必要です。ユーザー単位の導入は拒否します。
- UpgradeCode は `2BE82895-B377-4701-A51F-E03759ECEF56` です。版番号を 1.0.0 にしても、この識別子は変えていません。
- 製品バージョンはワークスペースの版（`1.0.0`）から数値部分を取ります。
- PDF の既定アプリは変えません。「プログラムから開く」と、任意の「SwiftPDF で開く」メニューだけを登録します。
- `pdfcraft-cli.exe` は入りません。

無人導入の例（管理者、デスクトップショートカットなし）：

```powershell
msiexec /i "SwiftPDF-1.0.0-windows-x64.msi" /qn /norestart INSTALLDESKTOPSHORTCUT=0
```

## データ（コード上の想定）

印影・設定・ログは `%LOCALAPPDATA%\SwiftPDF` です。MSI のコンポーネント一覧に、このフォルダーを削除する項目はありません。

したがって、**アンインストールしてもユーザーデータは残る想定**です。実機で確認するまでは断定しません。再インストール後、同じ Windows ユーザーでプロファイルが残っていれば、設定と印影を読める場合があります。

ポータブル版（exe 隣の `Data`）との違い、DPAPI、バックアップ、SWSEAL1 からの移行は [portable-readme.md](portable-readme.md) と同じ制約です。`%LOCALAPPDATA%\SwiftPDF` を別 PC へコピーしただけでは、通常は使えません。

## 更新

同じ UpgradeCode の新しい MSI が、古いインストールを置き換える想定です。ユーザーデータフォルダーは更新対象ではありません。実測前です。

## 未署名

コード署名が無いと、SmartScreen などが警告することがあります。

## ライセンス

同じインストール先に `LICENSE-MIT`、`LICENSE-APACHE`、`NOTICE`、`ATTRIBUTION.md` と各 OFL が付きます。帰属は PdfCraft / ArtCraft Team の著作権表示を含みます。ArtCraft のロゴ画像は入りません。
