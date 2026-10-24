# SwiftPDF v1.0.0 STEP 117I — 押印確定後の再照合（設計監査）

状態：**設計監査で停止。製品コードは変更していない。** `use_status` では成功判定に必要な識別情報が揃わない。現在の API で確定済みと扱わない。最小 API 拡張は提案のみ。コミット・push はしない。

改訂：2026-10-24（117I 監査）。`ROADMAP.md` は変更していない。

---

## 1. STEP 117H 時点の問題

`commit_use` の原子的置換が成功したあと、読み戻しが `Verify` または `Io` になると、呼び出しは失敗を見る。窓は PDF を Undo し、`Job::Abandon` が `use_undone` を 1 行足す。ストアと PDF は `Committed(Undone)` / 空ページで一致する。二重の commit は起きない。

この経路は安全側だが、保存済みの確定を取り消す。117I の目標は、一致が証明できるときだけ印を残し、証明できないときは要復旧にすること。

現行は次のとおりである。

| 段階 | 動作 |
|---|---|
| `commit_use` が `Ok` | 印を残し、`seal_uses` に載せる |
| `Verify` / `Io` | 直ちに `take_back_seal` → abandon。再照合しない |
| `Job::Abandon` が `Committed(InDocument)` | `cancel_use` ではなく `use_undone` |

---

## 2. `use_status` の実際の返却情報

```
pub struct UseStatus {
    pub state: UseState,
    pub events: Vec<Event>,
    pub generation: u64,
}
```

`use_status` は内部で `state()` を呼び、同一ロック下の `Snapshot` から上だけを返す。`Snapshot.store_id` はここで捨てている。

`StoreState::Empty` のときは `state = Unknown`、`events = []`、`generation = 0`。ストア ID はない。`Unreadable` / `V2Missing` / `NeedsMigration` / `Busy` は `Err`。

`Event` にあるもの：`op_id`、`seal_id`、`seal_revision`、`seal_sha256`、`image_sha256`、`file`、`page`、`note`（加工・角度など）、`seq`。ないもの：PDF のオブジェクト番号、矩形、文書 ID、`store_id`。

`Job::Commit` がワーカーに渡すのは `op_id` だけである。

---

## 3. 再照合の照合項目

依頼書が成功に求める項目と、現状の充足。

| # | 必要な項目 | `use_status` | 備考 |
|---|---|---|---|
| 1 | ストア識別子 | **不足** | `Snapshot.store_id` はあるが返していない |
| 2 | `op_id` | イベント内 | 呼び出し側も持つ |
| 3 | 印影 ID | `Event.seal_id` | 窓は `StampOp.seal` / `ApprovalRef` を持つ |
| 4 | revision | `Event.seal_revision` | 同上 |
| 5 | 画像 SHA-256 | `seal_sha256` と `image_sha256` | 登録 PNG と押印 PNG |
| 6 | 押印操作の識別 | ファイル名とページのみ | 矩形・文書 ID・PDF `ObjRef` はストアにない |
| 7 | `Committed(InDocument)` | `state` | ある |

成功判定は 1 と 6 が足りない。依頼書どおり、**現在の API では確定済みとしない。**

窓側の `StampOp` は `doc`、`op_id`、`seal`（id / revision / sha256）、`place`（page / rect / preview_gen）、`bytes`、`file_name` を持つ。期待する `store_id` は持たない。PDF 側は追加直後の `ObjRef` だけを一意の手がかりにする。エンジンは押印画像の SHA-256 を annot から返す API を持たない。117C のとおり、Undo のあとオブジェクト番号は再利用される。

---

## 4. 状態遷移（実装していない設計）

`commit_use` が `Ok` なら現行どおり。

`Verify` または `Io` のときだけ、次を意図する。分類情報が足りないため、置換前失敗と置換後失敗はどちらも再照合対象にする。

```
commit_use Ok                 → 成功（現行）
commit_use Verify | Io
    → ワーカーで use_status（拡張後）
        → 同一 store_id かつ Committed(InDocument)
           かつ印影属性が StampOp と一致
           かつ PDF 上の当該印を一意に確認  → A 確定維持
        → 同一 store_id かつ読み取り健全
           かつ状態が Prepared（commit 行なし） → B 未適用
        → それ以外（Busy・期限・復号失敗・属性不一致・
           PDF を一意に確認できない）         → C 要復旧
```

A に必要な照合が今の API では揃わないので、この遷移は実装していない。

---

## 5. commit 済みの扱い（A）

拡張後に証明できたときだけ：

- PDF の印を残す
- 監査を増やさない（`commit_use` の再送は `Unchanged`）
- 成功として `seal_uses` に載せ、完了を通知する

今は A を選ばない。

---

## 6. 未適用の扱い（B）

同一ストアを読め、状態が `Prepared` のときだけ。`Unknown` を未適用とはしない（空ストアや別ストアと区別できない）。

その場合は現行に近い take-back + cancel。cancel の書き込み結果も確認する。失敗したら成功にしない（C へ）。

今は B も実装していない。現行は Verify/Io をすべて take-back + undone にする。

---

## 7. 状態不明の扱い（C）

再照合 I/O、Busy、期限切れ、復号失敗、`store_id` 不一致、印影属性の不一致、PDF 印を一意に判定できない、未適用判定後の取消失敗。

期待：成功にしない。当該文書だけ `Session::hold_output`。保存・印刷・書き出し・編集・Undo/Redo はエンジンが拒否する。警告バーは閉じるまで残る。解除は保存せずに閉じるだけ。**不明を Undo 成功としない。**

`hold_output` 自体は既存で、対象文書に限定され、自動保存も通らない。ここは足りている。足りないのは、その前の「確定済みと証明できる」判定である。

---

## 8. 非同期ワーカー

再照合は GUI スレッドでは行わない。既存ワーカーは同時に 1 ジョブ。`Flow::blocking` 中は `PromptLayer::SealWork` が入力を取る。タブ切替・編集・保存・印刷・Undo・二重操作はそこで止まる。

足すなら `Job::ReconcileCommit { op_id }` のように、同じ worker で `use_status` する。`Job::Commit` が `Verify`/`Io` のとき、GUI で take-back せず、同じフローを Reconcile に進める。待ち上限は既存のロック約 3 秒に合わせ、Busy と期限は C。無限リトライはしない。

`Job::Commit` は今 `op_id` しか持たない。照合する印影属性は `StampOp` が GUI 側に残っている。ワーカーが属性まで見るなら、ジョブに `ApprovalRef` と `UseDetails` を足すか、GUI が返ってきた `UseStatus` を照合する。後者の方がストア API は小さい。

---

## 9. 要復旧状態との連携

`Session::hold_output` / `check_held` を再利用する。対象文書だけが制限される。他文書は巻き込まれない。UI のボタン無効化だけに依存しない。これは 117D の試験で確認済み。117I では新しい hold 経路を足していない。

---

## 10. Undo／Redo への影響

実装していないため、117C のオブジェクト番号再利用対策と、一般スタンプが理事長印の行を書かないことは、コード上そのままでる。A で印を残したあとの Undo は、現行どおり `note_use_step` → `use_undone` になる想定である。実装しないので、再照合後の Undo 試験は回していない。

---

## 11. 故障注入テスト結果

実装していない。新しい注入は追加していない。117H の現行試験（着地済み確定を Undo + `use_undone`）はそのまま残してある。

---

## 12. 既存機能の回帰確認

製品コードを変えていない。閲覧専用、署名 PDF の回転拒否、原本上書き禁止、一般スタンプ、日本語、別名保存、印刷プレビューは手を付けていない。fmt / check / clippy の再実行は、変更がないため 117I では行っていない。

---

## 13. 残存リスク

1. 現行の Class C 窓経路（着地済み確定を取り消す）は残る
2. 並列 I/O 1/46 は未解決
3. E4 / E5 の実 GUI は未実施
4. `WRITE_ERROR` / `Verify` の OS 番号は不明
5. ストアに PDF `ObjRef` も画像の再ハッシュもないため、拡張後も PDF 側の一致は「追加した参照がまだ所有されている」までしか言えない

---

## 14. コミット・push の可否

**しない。** 製品変更なし。この報告書と `step117i` の説明だけを足した。

---

## 15. 次工程の推奨

承認を得てから、次だけを足す。大規模な `VaultError` 再分類はしない。

### 最小 API

`UseStatus` に、同じロック読みの値を足す。

```
pub struct UseStatus {
    pub state: UseState,
    pub events: Vec<Event>,
    pub generation: u64,
    pub store_id: String,   // 追加。Empty は空文字
}
```

影響：`use_status` の 2 箇所の構築、既存の `UseStatus` を書く試験。ファイル形式は変わらない。`op_id` 再送の冪等性は現行のまま。

窓は準備完了時の `snapshot.store_id` を `StampOp` に残し、再照合結果と比較する。

### PDF 側

エンジンに押印バイトの SHA-256 を足さないなら、A の PDF 確認は次に限定する。

- `finish_seal_commit` が受け取った `ObjRef` が `present_owned_stamps` に残る
- ページが `StampOp.place.page` と一致する
- 再照合中は `SealWork` で編集が入っていない

これが「一意」として足りないと判断するなら、A は選ばず C（要復旧）にする。無理に成功へしない。

### ワーカー

`Verify` / `Io` のあと take-back しない。`Job::ReconcileCommit`（または Commit ジョブ内の `use_status`）を 1 回。GUI は返ってきた `UseStatus` と `StampOp` を照合する。A / B / C は §4。

Busy・期限は C。Verify の無条件リトライはしない。

### 試験方針（実装時）

117I 依頼の表（通常完了、再照合で維持、未適用で取消、印影 ID / revision / ハッシュ不一致、op 衝突、I/O、Busy、終了拒否、タブ拒否、PDF 印なし、取消記録失敗、Undo/Redo）を、合成ストアと一時フォルダーだけで足す。
