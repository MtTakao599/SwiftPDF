# SwiftPDF 0.1.0

SwiftPDF is a view-only PDF reader for Windows. This folder is the portable build.

## ポータブル版

`SwiftPDF.exe` の隣にある `portable.txt` により、設定・ログ・クラッシュ復旧データは `%LOCALAPPDATA%` ではなく、実行ファイルの隣の `Data` フォルダに保存されます。フォルダを移すときは `Data` ごとコピーしてください。`portable.txt` を消すと、通常のユーザーフォルダ（`%LOCALAPPDATA%\SwiftPDF`）を使います。

この ZIP はレジストリ、ファイルの関連付け、PDF の既定アプリを変更しません。`pdfcraft-cli.exe` は含まれません。更新の確認やサポート用のウェブサイトはありません。

## Portable mode

`portable.txt` next to `SwiftPDF.exe` stores settings, logs and crash recovery in a `Data` folder beside the executable, not in `%LOCALAPPDATA%`. Copy the whole folder, including `Data`, when you move it. Delete `portable.txt` to use `%LOCALAPPDATA%\SwiftPDF` instead.

This zip does not change the registry, file associations, or which application opens PDF files. It does not include `pdfcraft-cli.exe`. There is no update check and no support website.

## Licence

SwiftPDF is based on PdfCraft.

Copyright (c) the PdfCraft contributors.

Licensed under MIT OR Apache-2.0. See `LICENSE-MIT`, `LICENSE-APACHE` and `NOTICE` in this folder. Those notices, including third-party licences, stay with the program.

Japanese interface text uses BIZ UDPGothic (OFL-1.1). Its licence is `OFL-biz-ud-pgothic.txt`, next to the licences of the other embedded faces. Noto Sans CJK is not included.
