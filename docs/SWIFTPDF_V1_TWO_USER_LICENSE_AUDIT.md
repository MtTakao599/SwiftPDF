# SwiftPDF 1.0.0 を2台で使う場合のライセンス監査（STEP 127）

これは弁護士の意見ではない。リポジトリと STEP 126 の ZIP に実際にある条文と、`cargo metadata` が返した crate の `license` 欄だけを根拠にする。条文を読んでいないものは「未確認」とし、配布してよいとは書かない。

Status: audit only. No product change, no new package, no commit.

Revision history:
- 2026-10-26 — STEP 127, on `e503124b`.

## 1. 調査日時・対象コミット

| Item | Value |
|---|---|
| Date | 2026-10-26 |
| Branch | `feature/seal-data-safety-v1.0.0` |
| HEAD | `e503124bd9adf502d7f027f49a0b3b3eed4f3f69` |
| Start `git status` | `docs/SWIFTPDF_V1_PACKAGE_SAFETY_AUDIT.md` and `docs/images/SwiftPDFiconORG.png`, both untracked |
| Package reviewed | `D:\dev\SwiftPDF-package-validate\step126\SwiftPDF-1.0.0-windows-x64-portable.zip` (SHA-256 `FB4A8812…`, 22 entries). Not rebuilt |

`SwiftPDFiconORG.png` was not opened or changed.

## 2. 対象となる利用形態

Not for sale and not for a public GitHub release. The people are the author and Kato, both doing the same management work for Tokyo Terrace, on two separate Windows PCs. The work is placing the chairman seal, recording the approval, saving the PDF, and recovery.

Six situations are kept apart:

1. The author alone, on one PC.
2. Copying the EXE to another PC in the same workplace.
3. Giving Kato the EXE and the needed documents.
4. Installing it on PCs the organization administers.
5. Keeping the GitHub repository private.
6. A later public GitHub release.

"Same organization" does not, by itself, cancel a notice or share-alike duty. The management association's permission to place the chairman seal on a second PC is not a software licence and is not decided here.

## 3. 元プロジェクトのライセンス

The tree's own licence is in `LICENSE-MIT` and `LICENSE-APACHE`, and `Cargo.toml` says `license = "MIT OR Apache-2.0"`. Copyright line, in both `LICENSE-MIT` and `NOTICE`: "Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors." `NOTICE` says the choice is MIT or Apache-2.0.

What those texts allow, for this software:

- MIT (`LICENSE-MIT`): use, copy, modify, publish, distribute, sublicense, and sell, if the copyright line and the permission notice stay with the copy or a substantial portion of it. No duty to publish the source.
- Apache-2.0 (`LICENSE-APACHE`, section 4): give recipients a copy of the licence, keep copyright and attribution notices, and if a `NOTICE` file exists, keep its attribution notices in a place a reader can find. Modified files need a prominent statement that they were changed. Section 4 is how that is done for a binary: the statement can sit in `NOTICE` or the documentation that travels with the binary. There is no duty to publish the source.

The ZIP's `README.md` says SwiftPDF is based on PdfCraft and names `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, and `ATTRIBUTION.md`. Those four files are in the ZIP. That is the change notice a recipient of the ZIP can see. A recipient of `SwiftPDF.exe` alone cannot.

ArtCraft marks are not under that licence. `docs/brand/LICENSE-brand.txt` says the name, wordmark, and logos are trademarks, may be used only unchanged and only inside PdfCraft, and a modified version that is distributed must remove them and must not be called ArtCraft. Plain text "based on PdfCraft by the ArtCraft team" is allowed. `NOTICE` says this tree does not include the mark images. STEP 126's ZIP listing has `LICENSE-brand.txt` and no logo image, and `package.ps1` refuses an EXE that contains the ArtCraft mark byte strings. That check passed for the EXE inside the ZIP.

Rating: **A** for using and for giving someone the whole ZIP, including the licence files. **B** if only the EXE is copied, because the MIT notice and the Apache licence would not go with it. The marks themselves are **A** only while they stay out of the product, which is the current ZIP.

## 4. Rust 依存ライブラリのライセンス

`cargo-deny` is not installed, so `deny.toml` was read as policy and was not executed. The graph that was executed is `cargo metadata --locked` for package `pdfcraft`: 772 crates in that resolve, including crates used only on other operating systems. Every one of the 772 had a `license` field. None was blank.

Counts below are that resolve, not a count of object files inside `SwiftPDF.exe`.

| Expression | Crates |
|---|---|
| MIT OR Apache-2.0 | 369 |
| MIT | 132 |
| Apache-2.0 OR MIT | 109 |
| MIT/Apache-2.0 | 28 |
| Apache-2.0 | 23 |
| Unicode-3.0 | 18 |
| Unlicense OR MIT | 13 |
| Zlib OR Apache-2.0 OR MIT | 11 |
| BSD-3-Clause | 8 |
| other permissive `OR` combinations (0BSD, ISC, BSL-1.0, Zlib, BSD-2-Clause, CC0-1.0, MIT-0, BSD-1-Clause, LLVM-exception) | the rest, except the three rows below |
| Apache-2.0 OR GPL-2.0-only | 1 (`self_cell` 1.3.0) |
| MIT OR Apache-2.0 OR LGPL-2.1-or-later | 2 (`r-efi` 5.3.0 and 6.0.0) |
| CDLA-Permissive-2.0 | 1 (`webpki-root-certs` 1.0.9) |

No crate was GPL-only, LGPL-only, AGPL, or MPL. `deny.toml` forbids linking GPL, AGPL, and LGPL. For `self_cell` and `r-efi` the expression is a choice. Distributing under the MIT or Apache-2.0 side does not take the GPL or LGPL side, and does not create a source-publication duty for those crates. That is what the `license` field says. It is not a separate legal opinion about every file inside those crates.

`webpki-root-certs` is CDLA-Permissive-2.0. `deny.toml` says this crate is pulled in by winprint's build script and is not compiled into the application. This audit did not re-check that link. CDLA-Permissive-2.0 is a permissive data licence, not a copyleft. Rating for the known `license` fields: **A**, on the permissive side of each `OR`. The build-script claim for CDLA is **C** until `cargo-deny` is actually run. It does not, on the text in `deny.toml`, put a source duty on `SwiftPDF.exe`.

`epaint_default_fonts` 0.36.2 is `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`. The font conditions are in §6. They are not a code copyleft.

## 5. OCR モデルの条件

The ZIP contains `models\text-detection.rten`, `models\text-recognition.rten`, each file's `LICENCE.txt`, and `models\ATTRIBUTION.txt`.

`ATTRIBUTION.toml` `[[fetched]]` says both files are CC-BY-SA-4.0, by Robert Knight (ocrs-models, trained on HierText), from <https://github.com/robertknight/ocrs-models>, model card <https://huggingface.co/robertknight/ocrs>. The licence URL recorded there is <https://creativecommons.org/licenses/by-sa/4.0/legalcode.txt>. The ZIP's `LICENCE.txt` starts with "Attribution-ShareAlike 4.0 International" and is the Creative Commons legal text. The files were not modified (`ATTRIBUTION.txt` says "unmodified").

CC-BY-SA-4.0, as that text is written, requires attribution and the licence to stay with the model when it is shared, and share-alike for adapted material. It does not require the surrounding application to be relicensed, and it does not require the application source to be published. Section 2(a)(5)(B) of that licence says the recipient must not be offered additional terms that restrict exercising the licence. A confidentiality clause that forbids Kato from sharing those two model files would conflict with that sentence. The application source can stay unpublished.

The HierText corpus is named in the author line. This audit did not read a separate HierText licence file. The model files themselves are distributed under the CC-BY-SA text that is in the ZIP.

Rating: **A** for giving Kato the unmodified model files together with `ATTRIBUTION.txt` and both `LICENCE.txt` files, if no extra restriction is added to those files. **D** is not met. **C** remains only for a confidentiality term that would cover the model files, and for the HierText corpus text, which was not read.

## 6. フォント・アイコン・ブランド素材の条件

Code licences and asset licences are not the same.

In the ZIP, with the full licence text beside the EXE:

| Asset | Licence in the tree | Text in the ZIP | Rating |
|---|---|---|---|
| Inter, JetBrains Mono, Dancing Script | OFL-1.1 (`assets/fonts/OFL-*.txt`) | `OFL-Inter.txt`, `OFL-JetBrainsMono.txt`, `OFL-DancingScript.txt` | **A**. OFL 1.1 allows bundling an unmodified font with software. The font is not sold alone. Reserved names are not an issue while the fonts are unmodified. |
| BIZ UDPGothic, BIZ UDMincho, Shippori Mincho, Noto Sans Arabic, Noto Sans Thai | OFL-1.1, build input `craft-fonts` (`ATTRIBUTION.toml` `[[build_input]]`) | matching `OFL-*.txt` | **A**, on the same OFL terms. Noto Sans CJK is not in the EXE (`package.ps1` rejects that string). |
| Lucide icons | ISC, plus MIT for the Feather-derived names (`assets/icons/LICENSE-lucide.txt`) | `LICENSE-lucide.txt` | **A**. The copyright and permission text is in the ZIP. |
| SwiftPDF icon | `ATTRIBUTION.toml` entries for `swiftpdf.ico` and `docs/images/SwiftPDFicon.png`: author "SwiftPDF project", `MIT OR Apache-2.0`, "Not derived from the PdfCraft icon" | the icon is inside the EXE; the MIT and Apache texts are in the ZIP | **A** on that record. This audit did not open `SwiftPDFiconORG.png` and did not re-prove the artwork's origin beyond `ATTRIBUTION.toml`. |
| ArtCraft marks | `docs/brand/LICENSE-brand.txt`, not open source | `LICENSE-brand.txt` only; no mark image in the 22-file list | **A** while the images stay out. |
| PdfCraft document icon | ISC AND (MIT OR Apache-2.0), `LICENSE-document.txt` | `LICENSE-document.txt` is in the ZIP. `ATTRIBUTION.toml` says the EXE's icon resource is `swiftpdf.ico`, not `pdfcraft-document` | **A**. The extra licence file does not put the PdfCraft icon into the EXE. |

Not a separate file in the 22-entry ZIP, though `NOTICE` names them:

| Asset | Licence | Gap | Rating |
|---|---|---|---|
| Ubuntu Light, inside `epaint_default_fonts` | Ubuntu Font Licence 1.0. `NOTICE` names it. No `Ubuntu-font` file in the ZIP or in this search of the tree | The Ubuntu licence's own terms require the copyright and the licence to travel with the font. A one-line mention is not the licence text | **B** |
| Hack, inside the same crate | MIT AND Bitstream-Vera. `NOTICE` names it. The Bitstream Vera text was not found as a file in the ZIP | Same: the notice text is not in the package | **B** |
| Noto Emoji, inside the same crate | OFL-1.1. `NOTICE` names it. The ZIP's OFL files are for other families and carry those families' copyright lines | Noto Emoji's own copyright line is not a file in the ZIP | **B** |
| emoji-icon-font | MIT (`ATTRIBUTION.toml`) | Covered if `LICENSE-MIT` is read as the permission text; the author line "John Slegers" is in `ATTRIBUTION.md`, which is in the ZIP | **A** |
| Adobe CMap data, via hayro-cmap | BSD-3-Clause. `NOTICE` gives the copyright, a URL, and the non-endorsement sentence | The three BSD clauses are not pasted in the ZIP | **B** |
| Foxit standard-14 fonts, via hayro-interpret | BSD-3-Clause. `NOTICE` points at `vendor/hayro-interpret/assets/LICENSE_FOXIT` | That path is in the source tree. It is not one of the 22 ZIP entries, so a recipient of the ZIP cannot open it | **B** |
| ICC profile `CGATS001Compat-v2-micro.icc` | CC0-1.0 (`ATTRIBUTION.toml`) | CC0 does not require a notice. `NOTICE` names it anyway | **A** |

OFL 1.1 does not require the application to be published, and it does not forbid giving the application to one other person.

## 7. 同梱ライセンス文書の十分性

The ZIP's 22 entries include `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `ATTRIBUTION.md`, `LICENSE-brand.txt`, `LICENSE-lucide.txt`, `LICENSE-document.txt`, the OFL files for the faces `package.ps1` copies, and the two OCR `LICENCE.txt` files plus `models\ATTRIBUTION.txt`. There is no root `LICENSE` file in the repository; the dual-licence pair is what the project ships.

That set is enough for the project's own MIT-or-Apache code, Lucide, the OFL faces that have their own file, and the OCR models.

It is not a complete copy of every notice `NOTICE` itself points at. The missing pieces are the Ubuntu Font Licence, the Bitstream Vera notice, Noto Emoji's copyright line, the Foxit `LICENSE_FOXIT` file, and the full BSD text for the CMap data. Those are fixable by adding the texts. They were not added in this step.

## 8. バイナリだけを渡す場合の条件

"Binary only" here means no application source. It does not mean the EXE with no documents.

MIT still requires the copyright and permission notice to accompany the copy. Apache-2.0 section 4 still requires the licence and the `NOTICE` attributions. ISC, BSD, OFL, and CC-BY-SA have the same shape: the relevant notice goes with the thing being shared. None of those, on the texts read here, requires publishing the SwiftPDF source.

So Kato can receive `SwiftPDF.exe` without the Git repository. She cannot receive `SwiftPDF.exe` as the only file. The ZIP, or the same set of files copied beside the EXE, is the distribution that carries the notices.

## 9. 本人のみ使用する場合の評価

**ケース1: 使ってよい。** 区分 **A**。

自分の PC で動かすだけなら、複製を他人に渡していない。MIT や Apache の「受領者に表示を渡す」条件は、その渡し方が起きていない。ソースを公開する義務も、この構成からは出ない。印影を自分の Windows ユーザーで登録する技術上の手順は §12。管理組合が本人の PC への印影配置を認めているかは、この監査の外である。

## 10. 2台利用する場合の評価

**ケース2: ソフトウェアの条項上は、条件を満たせば渡せる。無条件には「配布可能」としない。**

| 渡し方 | 区分 | 理由 |
|---|---|---|
| 著者の PC だけ | A | §9 |
| 同じ職場の別 PC へ EXE だけ | B | ライセンス文が付かない。EXE だけのコピーは、読んだ条文の条件を満たさない |
| 加藤さんへ STEP 126 の ZIP 一式 | A、ただし §7 の不足文は添える | プロジェクト自身、Lucide、同梱 OFL、OCR モデルの条文は ZIP にある。Ubuntu / Bitstream Vera / Noto Emoji / Foxit / CMap の全文は ZIP に無い |
| 組織の管理する PC へ同じ ZIP を置く | A、ソフトウェア条項としては ZIP の場合と同じ | 「社内だから表示が不要」とは読まない。印影を2台に置く許可は別問題で、未確認 (**C**) |
| GitHub を非公開のまま使う | A | 非公開であること自体を禁じる条文は、読んだライセンスには無い。MIT / Apache はソース公開を要求しない |

加藤さんへ渡すときに必要なもの:

- ZIP のまま渡す。EXE だけを渡さない。
- §7 の足りない許諾文（Ubuntu Font Licence、Bitstream Vera、Noto Emoji の著作権行、`LICENSE_FOXIT`、CMap の BSD 全文）を同じフォルダーに足す。この STEP では足していない。
- OCR の2ファイルについて、CC-BY-SA で再共有できないという追加の秘密保持を掛けない。アプリのソースや印影データに秘密保持を掛けることとは別である。
- ArtCraft のロゴや名称を、この製品の名前として使わない。現状の ZIP はその状態である。
- 加藤さんの Windows ユーザーで、印影をその PC 上で登録する。ストアのコピーは移行にならない（§12）。

## 11. 一般公開する場合の評価

**ケース3: 今の ZIP のまま公開するのはまだ足りない。公開作業はしていない。**

公開しても、この依存関係の `license` 欄からは GPL 系のソース公開義務は出ない。`self_cell` と `r-efi` は、MIT または Apache-2.0 の側を選べる。公開するなら、受領者が不特定になるだけで、§8 の「表示を同梱する」義務は同じである。加えて次が残る。

- §7 の許諾文を ZIP に入れる。公開では「社内の補足メール」では足りない。
- CC-BY-SA のモデルは、受領者がその2ファイルを BY-SA の条件で再配布できる。アプリ全体を BY-SA にする必要は、読んだ条文からは出ない。
- ArtCraft の標章を公開物に入れない。`LICENSE-brand.txt` の「改変版は標章を外す」は、公開時に特にそのまま適用される。
- `.github/workflows/release.yml` はまだ `app-title: PdfCraft` を渡し、`windows-arm64.yml` は `app-dir: PdfCraft` を渡す。公開パイプラインとしては未確認であり、この名前のまま使わない。ワークフローの中身は実行していない。
- 販売は MIT と Apache と OFL（ソフトウェアに同梱する限り）と CC-BY-SA が禁じてはいない。今回の予定には販売が無い。フォント単体の販売は OFL が禁じる。ZIP からフォントだけを商品にしない。

## 12. 技術的な制約（DPAPI・監査履歴）

ライセンスとは別である。コードと STEP 126 の実測による。

- `seals.v2.bin` は、その Windows ユーザーの DPAPI で暗号化される（`seal_store_v2.rs` の `decode`）。加藤さんの PC、または加藤さんの Windows ユーザーでは、著者のストアをコピーしても復号できるとは限らない。README も同じことを書いている。
- 復号できないファイルは `Unreadable` になり、空のストアとしては書き直さない。コピーが失敗しても、ファイルを消す処理にはなっていない。
- 加藤さん側の登録は、加藤さんの PC で、加藤さんの Windows ユーザーとして行う。著者の `Data` や `%LOCALAPPDATA%\SwiftPDF` を渡す方法は、移行手順にならない。
- 監査履歴は各ストアの中にある。2台の履歴を合わせる機能は、`crates/ui-egui/src` の seal 関連に `merge` も `sync` も無い。`store_id` はストアを作ったときに決まる。同じ印影画像を両 PC で登録しても、履歴は別々のままである。
- 押印記録を後から見る場所は、その PC のストアである。もう一方の PC の履歴は、そこには出ない。
- ポータブル版は EXE の隣の `Data`。MSI 版は `%LOCALAPPDATA%\SwiftPDF`。どちらも、その Windows ユーザーに結び付く。PC の紛失、Windows プロファイルの削除、担当の交代では、残ったファイルが復号できるとは限らない。

## 13. 管理組合・勤務先の承認事項

ソフトウェアの許諾では決まらない。未確認であり、区分 **C**。この監査は承認を与えない。

- 理事長印の印影データを、2台の PC にそれぞれ登録してよいか。
- 押してよい人は著者と加藤さんの二人か。代理や共有アカウントを使わないか。
- PC の紛失、退職、担当交代のとき、その PC のストアと Windows アカウントを誰が無効にするか。
- 2本に分かれた監査履歴の、どちらを業務上の記録とするか。アプリは統合しない。
- 印影画像や業務 PDF を、ソフトウェアのライセンス文書と一緒に第三者へ渡さない。ライセンス文の同梱は、印影の再配布許可ではない。

## 14. 不明点と追加確認先

| 事項 | 状態 | 確認先 |
|---|---|---|
| `cargo-deny` の実行 | 未実施。方針ファイルは読んだ | この PC に `cargo-deny` は無い。入れると `deny.toml` を実行できる |
| `webpki-root-certs` が EXE に入るか | `deny.toml` はビルドスクリプトだけと書く。依存辺は取り直していない | `cargo tree -p pdfcraft -i webpki-root-certs` |
| HierText コーパスの条文 | モデルの作者行にあるだけ。原文は未読 | <https://github.com/robertknight/ocrs-models> と HierText の配布元 |
| Ubuntu Font Licence の全文が NOTICE の1行で足りるか | 足りない、とこの監査は扱う | <https://ubuntu.com/legal/font-licence> の全文を同梱する |
| Foxit BSD の全文 | ZIP に無い | ソースの `vendor/hayro-interpret/assets/LICENSE_FOXIT` |
| 印影を2台に置いてよいか | 未確認 | 東京テラスの管理側。ソフトウェアの作者ではない |
| SwiftPDF アイコンの出自 | `ATTRIBUTION.toml` はプロジェクト作成と書く。画像の再鑑定はしていない | 既存の帰属記録。`SwiftPDFiconORG.png` は開けていない |

## 15. 必要な最小限の対応

コードもパッケージも、この STEP では変えていない。渡す前に足すなら、次で足りる。

1. 加藤さんには ZIP 一式を渡す。EXE 単体にしない。
2. ZIP の隣に、Ubuntu Font Licence の全文、Bitstream Vera の表示、Noto Emoji の著作権行と OFL、`LICENSE_FOXIT`、CMap の BSD-3-Clause 全文を置く。
3. OCR モデルの2ファイルだけは、再共有を禁じる契約で縛らない。
4. 印影は各 PC で登録する。ストアはコピーしない。
5. 2台に印影を置く許可を、管理側に取る。許可が無い間は、2台目へ印影を登録しない。
6. 一般公開と販売は、この監査の結論には含めない。

## 16. 最終的な推奨方針

本人の PC だけで使う分には、今の構成で止める理由はライセンス上は見つかっていない。

加藤さんの PC へは、ZIP と §15 の足りない許諾文をセットで渡し、印影は向こうで登録し、モデルファイルに追加の再共有禁止を掛けない、という条件なら、読んだソフトウェア・フォント・モデルの条文はそれを禁じていない。管理側が印影の2台配置を認めるまでは、2台目への印影登録は進めない。

一般公開は、同じ表示義務が不特定の受領者に向く。足りない許諾文と、release ワークフローに残る PdfCraft という名前を片付ける前には公開しない。ソース公開を強制する GPL 単独の依存は、この `pdfcraft` 解決結果には無かった。

## 17. `git status`

この報告書を加えたあと。コミットはしていない。

```
?? docs/SWIFTPDF_V1_PACKAGE_SAFETY_AUDIT.md
?? docs/SWIFTPDF_V1_TWO_USER_LICENSE_AUDIT.md
?? docs/images/SwiftPDFiconORG.png
```
