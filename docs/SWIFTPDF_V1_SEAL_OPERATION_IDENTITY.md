# SwiftPDF v1.0.0 STEP 117J — 押印操作の一意性

状態：**案Aで最小実装した。** エンジンは注釈追加時に ISO 32000 の `/NM` を既に付ける。再照合中は、この `/NM` と `StampOp` と `UseStatus.store_id` で同一操作を識別する。PDF への新しいメタデータ形式は足していない。コミット・push はしない。

改訂：2026-10-24（117J）。`ROADMAP.md` は変更していない。

---

## 1. 調査したエンジン API

| 役割 | 位置 |
|---|---|
| スタンプ追加 | `crates/engine/src/lib.rs` `Edit::AddCustomStamp` → `run_edit`（約 2508 行）→ `pdfcraft_annot::add_annotation` |
| 追加直後の `ObjRef` | 同ファイル `Session::apply`（約 4287 行）。undo 前後の `stamp_obj_refs` の差。UI は `present_owned_stamps` の差（`seal_flow.rs` 約 528 行） |
| `/NM` の付与 | `crates/engine/src/lib.rs` `EditCtx::meta`（約 2434 行）。UUID。`crates/annot/src/lib.rs` `base_dict`（約 957 行）が `/NM` に書く |
| `/NM` の読み取り | `Document.info.annotations[].name`（`crates/render/src/inspect.rs` `Annotation`）。`owned_stamp_ref(doc, page, index)` で `ObjRef` と対応 |
| Undo／Redo | `Document::can_undo` / `undo` / `redo`。ラベルは `"Add stamp"`。履歴は COS スナップショット。操作 ID は持たない |
| セッション所有 | `Editor.owned_stamps: Vec<ObjRef>`。Undo しても id は残る。再 Redo は同じ番号を指す |
| 出力保留 | `Session::hold_output` / `check_held`（約 5316 行） |
| ストア再読 | `SealVault::use_status`（`seal_store_v2.rs`） |
| 窓の操作 | `StampOp`（`seal_service.rs`）。ワーカーは `Job` / `Flow` |

---

## 2. 押印追加・削除の経路

1. 承認 → `Job::Prepare`（`use_approved` + `use_prepared`）
2. `Edit::AddCustomStamp`。`/NM` が付く。`owned_stamps` に新しい `ObjRef`
3. `Job::Commit`（`use_committed`）
4. 成功なら `seal_uses` に `(op_id, doc, num, generation)`
5. Undo は文書が先。`note_use_step` が `use_undone`。Redo は `use_redone`
6. 削除は `Delete comment` → `use_removed`

一般スタンプも同じ `AddCustomStamp` と `/NM` を使う。理事長印の監査行は `seal_uses` に載った操作だけが書く。

---

## 3. `ObjRef` の再利用条件

Undo で印がページから消えても、オブジェクト番号は空きになる。次の `AddCustomStamp` が同じ `ObjRef` を使うことがある（`a_number_freed_by_an_undone_seal_is_not_mistaken_for_it`）。

新しい印は **新しい `/NM`** を持つ。`owned_stamps` は古い番号を残したまま新しい印も持つ。UI は 117C の `seal_orphans` で、取り消し済みの番号を Redo で理事長印と誤認しない。

`ObjRef` 単独は合格にしない。

---

## 4. Undo／Redo 履歴の識別能力

エンジンの undo はラベルと COS 全体だけである。論理 `op_id` はない。`"Add stamp"` が最上段であることと、`present_owned_stamps` に当該 `ObjRef` があることまでは分かる。

再照合中は `PromptLayer::SealWork` が編集・Undo・保存を止めるので、その間に番号再利用は起きない。照合そのものは `/NM` と `op_id` で行う。

---

## 5. 文書セッションの識別能力

`StampOp.doc` は `DocId`。`Flow::Commit` / `Reconcile` がその文書と `ObjRef` を保持する。別タブの結果は `op.doc` 以外へ適用しない。`SealWork` 中はタブ切替も止まる。

---

## 6. ストア側の識別情報

`UseStatus` は今、同じロック読みの `store_id` を返す。`Empty` は `store_id` 空・`Unknown`。空を既存ストアとはしない。自動作成はしない。

イベントには `op_id`、`seal_id`、`seal_revision`、`seal_sha256`、`image_sha256` がある。`StampOp` は準備時の `store_id`・画像 SHA-256 と、追加後の `/NM` を持つ。

PDF 画像バイトを annot から再ハッシュする API はない。照合は **準備時 PNG の SHA-256 とイベント** である。

---

## 7. 案A・B・Cの比較

| 案 | 内容 | 判定 |
|---|---|---|
| **A** | 既存の `/NM`、`StampOp`、`DocId`、`SealWork`、`hold_output` | **採用。** 新形式なし。セッション中の再照合に足りる |
| B | エンジンの Edit に内部操作 ID を足す | 不要。`/NM` が既にある。undo 基盤に触れる |
| C | PDF へ独自 ID を新設、ページ構造や undo の再設計 | 対象外 |

`/NM` は ISO 32000 の注釈名である。他ビューアは XFDF などで既に使う。新しい埋め込みではない。

---

## 8. 最小実装の可否

許可条件は満たした。

- 同一スナップショットの `store_id`
- 監査の `op_id` と `StampOp.op_id`
- 印影 ID / revision / 画像 SHA-256
- PDF 側は `/NM`（`ObjRef` 再利用後も新しい印は別 `/NM`）
- 再照合中は `SealWork`
- 失敗は `hold_output`
- Undo／Redo 監査は成功後の既存経路
- 変更は `UseStatus` と押印確定フローと試験

---

## 9. 実装した変更

- `UseStatus.store_id`
- `Job::Reconcile` / `Flow::Reconcile`。キャプション「押印の記録を確認しています…」
- `commit_use` の `Verify` / `Io` のあと、take-back せずワーカーで 1 回再照合
- **A** 同一 `store_id`・属性・`/NM`・`Committed(InDocument)` → 印を残す
- **B** 同一ストアで `Prepared` → PDF 取消 + cancel。PDF を戻せないときだけ要復旧。cancel の書き込みだけ失敗し PDF が空なら、未完了の準備として残し要復旧にはしない
- **C** それ以外 → 要復旧。自動の `commit_use` 再送なし
- `StampOp` に `store_id` / `image_sha256` / `annot_nm`

---

## 10. テスト結果

`seal_flow` + `seal_store_v2`：59 合格、0 失敗、3 無視。`seal_store_v2_processes`：3 合格。fmt / check / clippy は合格。追加：

- `a_landed_commit_reported_as_error_is_kept_after_reconciliation`（維持 + その後の Undo 監査）
- `a_store_that_vanishes_after_commit_holds_the_document`
- `use_status` が `store_id` を返すこと

既存の置換前失敗（`a_failed_commit_takes_the_stamp_back_and_redo_cannot_restore_it`）と要復旧は残す。

---

## 11. 現行方式継続の安全性

117H までの方式（不明なら Undo + `use_undone`）は、取消と追記が成功すれば監査と PDF が一致する。取消失敗時は既に `hold_output` がある。

今回は、証明できた確定を残す。証明できないときは 117H より undo を減らし、要復旧へ寄せた。

---

## 12. 残存リスク

1. annot から画像 SHA-256 は再計算していない
2. `/NM` が取れない追加は A にせず要復旧
3. 並列 I/O 1/46 は未解決
4. E4 / E5 の実 GUI は未実施
5. ロック外の外部改変は Verify / 要復旧

---

## 13. v1.0.0 への推奨

この最小再照合を 117B〜J の一部として保持する。公開は 1/46 と E4/E5 が残る間は承認しない。再照合を v1.1 へ戻す必要はない。現行 Undo 方式へ戻すと、保存済み確定が再び消える。

---

## 14. コミット・push

**しない。**

---

## 15. 次工程

品質ゲートと関連試験の確認。問題なければ、117B〜J のコミット方針を利用者が決める。`ROADMAP.md` はまだ変えない。
