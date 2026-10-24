# SwiftPDF v1.0 理事長印ストア（SWSEAL2）実装報告（STEP 117B）

状態: ストア層の実装と検証は完了した。GUI・承認画面・押印操作への接続は未着手（STEP 117C）。コミットと push は行っていない。

| 日付 | 変更 |
|---|---|
| 2026-10-23 | 初版（STEP 117B） |

正とする設計は `docs/SWIFTPDF_V1_SEAL_STORAGE_SPEC.md`（STEP 117A）である。この文書は、実装した内容と、設計書との相違点（すべて明記する）を記録する。

## 1. 実装したモジュール・API

### 1.1 変更したファイル

| ファイル | 内容 |
|---|---|
| `crates/platform/src/lock.rs`（新規） | `lock_with_timeout(path, timeout, poll) -> Result<FileLock, LockError>`。`File::try_lock` の再試行。`FileLock` を drop すると解放される |
| `crates/platform/src/lib.rs` | `pub mod lock;` の 1 行 |
| `crates/platform/src/dpapi.rs` | テストだけを追加（別の鍵の暗号文を拒否すること） |
| `crates/ui-egui/src/seal_store_v2.rs`（新規） | SWSEAL2 ストア本体。`#[cfg(not(target_arch = "wasm32"))] pub mod` |
| `crates/ui-egui/src/seal_store_v2_tests.rs`（新規） | 単体テスト 30 件（うち 1 件は `#[ignore]` の 64 MiB 実寸試験） |
| `crates/ui-egui/tests/seal_store_v2_processes.rs`（新規） | 実プロセス試験（Windows のみ） |
| `crates/ui-egui/src/lib.rs` | モジュール登録の 2 行 |
| `crates/ui-egui/src/seal_store.rs` | `validate_seal`、`decode_file`、`event_mark_ok` を `pub(crate)` にした（可視性だけ。処理は変更なし） |
| `crates/ui-egui/src/stamps_ui.rs` | `sha256_hex` を `pub(crate)` にした（可視性だけ） |

依存の追加・更新は無い（`getrandom` 0.4 は ui-egui の既存の直接依存）。`unsafe` は無い。既存の SWSEAL1 ストア（`seal_store.rs`）の動作、GUI、承認画面、押印操作は変更していない。

### 1.2 公開 API（`pdfcraft_ui_egui::seal_store_v2`）

`SealVault::new(dir)` / `SealVault::with_limits(dir, Limits)`。保存先フォルダは呼び出し側が明示的に渡す。既定のパスは持たない。

| メソッド | 書き込み | 内容 |
|---|---|---|
| `state()` | しない | 現在の状態（`Empty` / `NeedsMigration` / `Ready{snapshot, legacy, notices}`）。中断した移行や旧版の書き戻しは「報告」だけで、処理しない |
| `use_status(op_id)` | しない | 1 回の押印の試みの状態とイベント。結果が不明なときの再確認用 |
| `migrate()` | する | SWSEAL1 からの移行。**移行はこの呼び出しでだけ起きる** |
| `register(NewSeal)` | する | 新しい ID で登録。同名は競合 |
| `update(seal_id, base_revision, SealChange)` | する | 倍率または加工設定の変更。revision を照合 |
| `delete(seal_id, base_revision)` | する | 削除。ID は `deleted` に残り、再利用しない |
| `record_approval(&ApprovalRef)` | する | 画面で承認したことの記録（`use_approved`） |
| `prepare_use(&ApprovalRef, &UseDetails)` | する | 押印の直前（`use_prepared`）。承認の記録と最新の印影を照合 |
| `commit_use(op_id)` / `cancel_use(op_id)` | する | 終端イベント |
| `note_use_step(op_id, step_id, UseStep)` | する | 取り消し・やり直し・削除・削除の取り消し |

`ApprovalRef{op_id, seal_id, seal_revision, seal_sha256}` は `ApprovalRef::new(&seal)` で作る（op_id は毎回新規）。

戻り値:
- 成功: `Applied{snapshot, changed, created, notices}`。`changed == false` は「既に記録済みで、何も書いていない」。
- 失敗: `VaultError`（`Busy`、`NeedsMigration`、`V2Missing`、`Unreadable`、`UnknownLegacyFile`、`Conflict{reason, latest}`、`Full`、`Rejected`、`Io`、`Verify`）。
- 文言は固定の日本語で、印影名・ファイル名・復号内容を含まない。

## 2. SWSEAL2 形式

設計書 4 節のとおり。

- ファイルは `seals.v2.bin`。8 バイトの `SWSEAL2\n` に、平文 JSON 全体の DPAPI 暗号文（現在ユーザー、エントロピー無し）が続く。
- トップレベル: `schema`(=2)、`store_id`、`generation`、`migrated_from`、`seals`、`deleted`、`events`。
- 設計書から追加した欄:
  - イベントの `step_id`、`seal_revision`、`seal_sha256`。
  - `note` の `backup`、`source_sha256`（移行の記録用）。
- 全構造体に `deny_unknown_fields` を付けた（設計書 19 節の推奨）。
  - 例外は `effects`（既存の `SealEffects` 型を共用）で、この中の未知の欄は読み捨てる。

読み込みのたび、書き込みの前、書き込み後の読み直しで、同じ検証（`validate`）を行う。拒否する条件は次のとおり。

- ファイル全体:
  - マジック違い、DPAPI 失敗、80 MiB 超、JSON 不正、未知の欄。
  - `schema != 2`、`store_id` 不正、`generation == 0`。
- 印影（8 件を超える、ID・名前の重複、`revision == 0`）:
  - SHA-256 が PNG と不一致。
  - 既存の `validate_seal` で不合格。
- 削除済みの ID が `seals` にもある、またはその世代が `generation` より大きい。
- イベント:
  - ID の重複、`seq` が 0・減少・`generation` 超、未知の `kind`、欄の長さや形式の違反。
  - 同じ `op_id` の不正な状態遷移、同じ `step_id` の重複。

拒否したファイルは変更せず、空のストアとして扱わない（`Unreadable`）。

## 3. ロック方式

- 対象は `seals.bin.lock`。`create(true).truncate(false)` で開くだけで、内容は書かず、消さない。
- `File::try_lock` を 25 ms ごとに再試行し、待つのは最大 3 秒（`Limits` で変更可）。時間切れは `Busy` を返し、何も適用しない。
- 保持する範囲は、1 回の操作の「読み込み → 検証 → 適用 → 暗号化 → 書き込み → 読み直しの照合 → 墓標」の全体。`state()` と `use_status()` も、ロックの中で読む。
- フォルダは `canonicalize` してから扱う。
- 実証したこと:
  - ジャンクション経由、大文字表記、`/` 区切り、`.\..\real` 表記のどれでも同じロックで止まった（`spellings_and_a_junction_reach_the_same_lock`）。
  - ロックを持ったプロセスが `abort()` しても `exit(0)` しても、OS が解放し、1 秒以内に取得できた（`a_lock_held_by_a_process_that_aborts_or_exits_is_released`）。
- ロックが無いと何が起きるかを確かめた（11.3 節）。

## 4. 競合検出

- **登録**: ID は `seal-` と 128 bit の乱数。同名は `Conflict::DuplicateName`、9 件目は `TooManySeals`。同名の再登録は別の ID になる。
- **更新・削除**: `seal_id` と `base_revision` を照合する。
  - 違えば `SealChanged`、削除済みなら `SealDeleted`、無ければ `SealMissing`。
  - 成功すると `revision + 1` になる。
  - 削除した ID は `deleted` に残り、古いスナップショットから復活しない。
- **承認・準備**: 承認の記録と、最新の印影の `seal_id`・`revision`・`sha256` を、ロックの中で照合する。
  - 削除された場合も、同名で再登録された場合も（ID が違うので）、拒否する。
- 競合時の `Conflict{reason, latest}` は、最新のスナップショットを持つ。利用者の依頼内容（`NewSeal` など）は呼び出し側に残るので、画面で保持できる。
- 自動マージは、最新の内容に依頼を 1 件だけ適用する方式に限る。メモリ上のストア全体を書き戻す経路は無い。

## 5. 監査イベントと状態遷移

| 状態 | 許可する次のイベント |
|---|---|
| （無し） | `use_approved` |
| approved | `use_prepared` |
| prepared | `use_committed`、`use_cancelled` |
| committed（文書内） | `use_undone`、`use_removed` |
| committed（取り消し済み） | `use_redone` |
| committed（削除済み） | `use_removal_undone` |
| cancelled | なし |

- **冪等性**:
  - 同じ承認・準備・確定・取消の再試行は、書かずに `changed == false` を返す。
  - 同じ op_id で内容が違う依頼は `OpIdInUse`。
  - 取り消しなどの段階イベントは、呼び出し側が渡す `step_id` で重複を判定する。
- **禁止する遷移**:
  - cancelled → committed、committed → cancelled、承認なしの準備、準備なしの確定・取消は、すべて `Conflict::UseState` になる。
  - 同じ規則を読み込み時の検証でも使うので、ファイル上で禁止された並びも拒否する。
- 他の op_id の行を消す API は存在しない。全操作は追記だけである。
- 印影の管理イベントは `seal_registered`、`seal_updated`、`seal_deleted`。
- `seq` は、そのイベントを書いた時の `generation`。1 回の書き込みで 1 件なので、通常は連続する。
- **成功の返し方**: 書き込みの後に、ファイルを読み直す。バイト単位で一致し、復号した内容も一致した場合だけ `Ok` を返す。一致しなければ `Verify` を返す。
- **承認を迂回しない設計**:
  - ストアは、自分から押印を許可しない。
  - `prepare_use` は、`record_approval` で記録した承認と内容が一致し、かつ印影が承認時のままである場合にだけ通る。
  - 画面の承認チェック（`approval_matches` と PNG の SHA-256）を置き換えるものではなく、その後にもう一度照合する層である。

## 6. 容量上限の扱い

`Limits` の既定値:

| 値 | 既定 | 意味 |
|---|---|---|
| `max_file_bytes` | 64 MiB | ヘッダーと暗号文を含めた `seals.v2.bin` の実サイズの上限 |
| `reserve_per_open_use` | 8 KiB | 準備済みの押印 1 件ごとに、終端イベント用に確保する領域 |
| `step_headroom` | 1 MiB | 登録・更新・承認・準備には使わせない、取り消しなどの行の余地 |
| `warn_events` | 10,000 | 超えたら `Notice::ManyEvents` |
| `warn_file_bytes` | 48 MiB | 超えたら `Notice::NearFull` |

書き込み可能な上限は次のとおり。

- 登録・更新・承認・準備: `max − step_headroom − reserve × 準備中の件数（書き込み後）`
- 確定・取消・削除・段階イベント: `max − reserve × 準備中の件数（書き込み後）`

**終端イベントが必ず入る理由**:
- 準備のイベントは JSON で 2 KiB 以下に制限している。終端イベントはその写しなので、ほぼ同じ大きさになる。
- 準備のときに 8 KiB を確保する。確定または取消で、その確保分が解放される。
- したがって、準備が通った押印は、上限の近くでも必ず終端を記録できる。

上限に達したら、新しい準備（押印）を `Full` で拒否する。過去の履歴は消さない。400 件の上限は使わない。

## 7. SWSEAL1 からの移行

明示的な `migrate()` だけが移行する。`state()`、`register()` などは `NeedsMigration` を返すだけで、旧ファイルに触れない。

手順（すべてロックの中）:
1. v2 があれば、移行済みとして扱う（中断していれば完了させる）。
2. `seals.bin` を読み、src_sha を計算する。
   - 墓標なら `V2Missing`、不明な形式なら `UnknownLegacyFile` で止める。
3. 既存の `decode_file` で復号・検証する。失敗したら何も書かない。
4. v2 の本体を作る。
   - 印影に新しい ID と revision 1 を与える。
   - 旧行は `legacy:<event>`（seq 1）にし、`migrated` 行を 1 つ加える。
5. バックアップ名 `seals.bin.swseal1-<sha16>.bak` が既にあれば、**何も書かずに中止する**。
6. v2 を新規作成（置き換えない方法、11 節）し、読み直して照合する。
7. `seals.bin` をバックアップ名へ rename する（既存名なら中止）。
8. バックアップを読み、SHA-256 を照合する。
   - 違えば、手順 2〜7 の間に旧版が書いたことになる。移したファイルから作り直して v2 を書き直し、バックアップは新しい sha の名前へ移す。
9. 墓標 `seals.bin` を `create_new` で作る。

再開の判定（STEP 117A の方式を維持）:

| v2 | `seals.bin` | 判定 |
|---|---|---|
| あり（generation 1） | SWSEAL1、sha == `source_sha256`、バックアップ無し | 移行が中断した。手順 7 から完了させる |
| あり | 無し | 墓標を作る |
| あり | 墓標 | 正常 |
| あり | SWSEAL1（上記以外） | 旧版の書き戻し（8 節） |
| 無し | 墓標 | `V2Missing`（作り直さない） |
| 無し | SWSEAL1 | `NeedsMigration`（移行するまで何もしない） |

新版が既存の `seals.bin` を新しい暗号文で上書きすることは無い。使う操作は、rename による移動、`create_new`、v2 の新規作成だけである。既存のファイルは無条件には消さない。

## 8. 墓標と旧版書き戻し

- 墓標は `SWSEAL-MOVED\n` で始まる平文の 2 行で、秘密を含まない。
  - 旧版は `SWSEAL1\n` が無いので復号を拒否し、ロック状態になる（`SealStore::load(墓標).locked()` を試験で確認した）。
- 墓標だけがあって v2 が無い場合は `V2Missing` を返す。空のストアを作らず、移行の成功扱いにもしない。
- 移行後に旧版が `seals.bin` を書いた場合（STEP 117B の指示に従う）:
  - 次の変更操作のとき、ロックの中で `seals.bin.legacy-g<世代>-<sha16>.bak` へ rename する。
    - 名前が使われていれば `-2`、`-3`… を付け、既存のバックアップを上書きしない。
  - 移したファイルを読み、マジックと SHA-256 を確かめる。
  - 墓標を作り直し、`Notice::LegacyWriteSetAside{backup, sha256}` を返す。
  - **v2 は変更しない**。旧版の行を v2 へ取り込まない。
- 設計書との相違: 設計書 10 節の `legacy_write_detected` イベントは、v2 に書かない。
  - STEP 117B の指示「v2は変更しない」を優先したためである。
  - 検出の記録は、保全ファイルそのものと、利用者への通知になる。
  - 監査上の扱いは 14 節に要判断として挙げた。
- 復号した内容はどこにも出力しない。モジュールには `log`、`println` などの出力が無い。

## 9. 障害復旧

| 障害 | 結果 | 試験 |
|---|---|---|
| DPAPI 復号失敗、改ざん、別の鍵 | `Unreadable`。ファイルはそのまま、空にしない | `damaged_or_foreign_ciphertext_is_refused_and_kept`、platform `ciphertext_bound_to_another_key_is_refused` |
| ロックの時間切れ | `Busy`。何も適用しない | `a_held_lock_times_out_…` |
| 権限不足・置き換え失敗（読み取り専用の v2） | 約 2 秒の再試行の後に `Io`。元のファイルはそのままで、一時ファイルは残らない | `a_failed_replace_keeps_the_previous_file` |
| ほかのプログラムが一時的に開いている（ウイルス対策の検査など） | rename と置き換えを、アクセス拒否（5）と共有違反（32、33）に限って、約 2 秒まで再試行する。それ以外の誤りは再試行しない | `a_briefly_held_file_is_retried_a_bounded_number_of_times`（9.1 節） |
| 保存後の照合失敗 | `Verify`。成功を返さない。壊れたファイルは初期化しない | `a_write_that_does_not_read_back_…` |
| 書いた後に応答を失った | `use_status(op_id)` で確定済みと分かり、再試行は二重に記録しない | `an_unknown_outcome_is_rechecked_…` |
| 移行の中断（v2 書き込みの前、直後、rename の直後、墓標の直前） | 次の変更か `migrate()` で完了する。バックアップは元のバイトのまま、`migrated` は 1 件 | `migration_stopped_…`（4 件） |
| 墓標だけ | `V2Missing` | `a_tombstone_without_v2_…` |
| 旧版の遅延書き戻し（繰り返しを含む） | 保全し、v2 は不変 | `a_late_format_1_write_…` |
| 重複した操作 ID・不正な遷移 | `Conflict`。読み込み時も拒否 | `use_lines_follow_…`、`a_bad_file_…` |
| 容量不足（ディスク） | 一時ファイルへの書き込みが失敗すれば `Io` で、元は残る | **未実施**（ディスクを満杯にする試験は行っていない。置き換え失敗と同じ経路） |
| 一時ファイルの作成・書き込み失敗 | 既存の `staging::write_atomically` が一時ファイルを消して `Io` を返す | 置き換え失敗の試験で間接的に確認。単独の注入は**未実施** |

### 9.1 検証中に見つけた問題（修正済み）

**症状**: 単体テストを単一スレッドで連続実行すると、`seals.v2.bin` の置き換えが、ときどき `Io` で失敗した（6 回中 4 回の実行で、1〜3 件の試験が失敗した）。

**原因の調べ方と結果**:
- 一時的に OS のエラーコードを出力して調べた。結果は、すべて Windows のエラー 5（アクセス拒否）で、直前に書いたファイルを置き換える時に起きていた。
- 直前に書いたファイルを、ほかのプログラム（リアルタイムのウイルス検査と考えられる。特定はしていない）が、削除共有なしで開いている間に、rename で置き換えようとした場合の挙動である。
- データの損失は無かった。置き換えが失敗した場合、元のファイルはそのまま残り、操作は適用されない。

**影響**: 実際の利用でも、押印の確定や取消が一時的に失敗しうる。その場合、117C では文書の押印を戻すことになる。

**修正**:
- `with_retry` を追加した。rename と置き換えを、アクセス拒否と共有違反に限って、20〜500 ms の間隔で最大 8 回（合計約 2.1 秒）再試行する。それ以外の誤りは、すぐに返す。
- 再試行の間も、ロックは持ったままである。

**修正後の確認**:
- 同じ単一スレッドの実行を 6 回くり返し、6 回とも全件合格した。
- 再試行の回数の上限と、ほかの誤りを再試行しないことは、`a_briefly_held_file_is_retried_a_bounded_number_of_times` で確かめた。
- 既存の SWSEAL1 ストアや、ほかの保存処理（`staging::write_atomically` を使う箇所）にも同じ現象がありうるが、今回は変更していない（14 節）。

## 10. データ保護の検証

- 利用中の `%LOCALAPPDATA%\SwiftPDF\seals.bin` と同じフォルダのファイルは、存在確認を含めて一切参照していない。
- テストの保存先:
  - 単体テストは OS の一時フォルダの下の `swiftpdf-sealv2-test-*`。
  - 実プロセス試験は `D:\dev\SwiftPDF-seals-validate\step117b\swiftpdf-sealv2-test-*`（環境変数で指定）。
  - どちらも、作成の前と子プロセスを起動する前に、次を assert で確かめている。
    - 名前の接頭辞が `swiftpdf-sealv2-test-` であること。
    - `%LOCALAPPDATA%\SwiftPDF` の下でないこと。
  - 子プロセスも同じ検査を行い、外れていれば何もしない。
- 印影は合成 PNG（4×4）と、64 MiB 試験用の合成バイト列である。名前は「合成…」、文書名も合成で、実際の印影や業務 PDF は使っていない。
- ディスク上の v2 に、印影名（UTF-8）と PNG の `IHDR` が平文で現れないことを確かめた。
- GUI は起動していない。既存ストアの自動移行は試験していない（移行は明示的な `migrate()` だけ）。

## 11. テスト件数と結果

### 11.1 新規

| 範囲 | 件数 | 結果 |
|---|---|---|
| A スキーマ（往復、16 種の不正ファイル、未知の欄、暗号文の破損 5 種、SWSEAL1 の読み込み） | 4 | 合格 |
| B 競合（2 インスタンスの登録、更新・削除の競合、承認後の変更と削除、同名再登録と古い承認、承認の必須、ロックの時間切れと解放、ジャンクションと別表記） | 7 | 合格 |
| C 監査（400・401・10,000 件、状態遷移と冪等性、取消が他を消さない、上限直前の準備の拒否、結果不明の再確認、照合失敗、置き換え失敗、一時的なアクセス拒否の再試行） | 8 | 合格 |
| C 64 MiB 実寸（`#[ignore]`。`--include-ignored` で実行） | 1 | 合格（下記） |
| D 移行（正常、延期、対象無し・復号不能・不明形式、バックアップ名の衝突、中断 4 点、v2 欠落、旧版書き戻しの繰り返し、移行中の旧版の書き込み） | 10 | 合格 |
| platform（ロック 3、DPAPI の別鍵 1） | 4 | 合格 |
| E 実プロセス（20 試行、ロックの異常終了） | 2（＋子プロセス入口 1） | 合格 |

**64 MiB 実寸試験**: 66,046,462 バイト（約 63.0 MiB）のファイルを作った（8 印影、各 2 MiB、残りは監査行）。
- 承認は通った（`NearFull` の通知付き）。
- 次の準備は `Full` で拒否され、ファイルはバイト単位で変わらなかった。
- 準備済みの押印の確定は、確保した領域に収まった。
- 確定の後、解放された領域で次の準備と確定ができた。
- 全段階でファイルは 64 MiB 以下で、行は 1 件も欠けなかった。
- 1 操作あたり約 10 秒かかった（debug ビルド。14 節）。

**E 実プロセス試験**（`D:\dev\SwiftPDF-seals-validate\step117b\process-trials.txt`）:
- 20 試行のすべてで、次の値になった。
  - events=41（登録 1 件と、各プロセス 20 件の承認イベント）、承認 40 件、`generation` 41。
  - イベント ID 41 件はすべて異なり、op_id 40 件もすべて異なった。
  - `seq` は 1〜41 で連続した。
  - フォルダに残ったのは `seals.bin`（墓標）、`seals.bin.lock`、`seals.v2.bin` だけで、一時ファイルは無かった。
- 子プロセスの PID は、毎回 2 つが異なり、親とも異なった（例: 試行 1 は子 3892 と 7268、親 21048）。
- 1 試行は約 2.5〜3.5 秒。`Busy` による再試行は 0 回だった（待ちはロックの内側で吸収された）。

### 11.2 試験が欠陥を検出できることの確認（意図的な改変）

次の 5 つを同時に壊してビルドし、試験が失敗することを確かめてから元に戻した。

| 改変 | 失敗した試験 |
|---|---|
| `apply` でロックを取らない | 実プロセス試験（試行 1 で「every event is kept」が失敗、つまり行が失われた）、ロックの時間切れ、ジャンクション、置き換え失敗 |
| 準備中の押印の確保領域を 0 にする | 上限直前の試験 |
| 承認時の revision・sha の照合を外す | 承認後の変更・削除 |
| 移行後の SWSEAL1 を無視する | 旧版書き戻し、v2 直後の中断の再開、移行中の旧版の書き込み |
| 書き込み後の照合を外す | 照合失敗の試験 |

結果は 9 件の単体テストと実プロセス試験の失敗で、改変を戻した後はすべて合格した。

### 11.3 既存の試験

すべて単一スレッド（`--test-threads=1`）で実行した。

| 対象 | 結果 |
|---|---|
| `pdfcraft-engine`（単体） | 325 合格、1 無視 |
| `pdfcraft-engine`（結合） | 全件合格 |
| `pdfcraft-ui-egui`（単体、新規の 29 件を含む） | 568 合格、2 無視（64 MiB 実寸と再試行の計時試験） |
| `pdfcraft-ui-egui`（結合 48 対象、`--no-fail-fast`） | 失敗 2 件。ほかはすべて合格（`seal_store_v2_processes` の 3 件を含む） |
| `pdfcraft-platform` | 全件合格 |

失敗した 2 件は、どちらも v0.7 から続く既知の失敗で、原因も同じである（`docs/SWIFTPDF_V0.8_TEST_FAILURE_ANALYSIS.md`、`docs/SWIFTPDF_V0.9_XREF_SAFETY_FIX.md`）。
- `combine_grid::arrow_keys_move_by_card_and_by_row_and_shift_extends`（`f8.pdf` のカードが見つからない）
- `fonts::chinese_ui_uses_an_installed_chinese_face`

今回の変更は UI の動作を変えておらず、**新規の失敗は無い**。

## 12. 既存機能への影響

- 既存の SWSEAL1 ストア、`stamps_ui`、`editing`、`main.rs` の `set_seal_store` は、動作を変更していない。
  - 変更は関数の可視性（`pub(crate)`）だけである。
- 製品の起動経路は、新しいモジュールを呼ばない。現時点の SwiftPDF は、従来どおり `seals.bin`（SWSEAL1）を使う。
- 新しいモジュールは wasm では組み込まない（`getrandom` が非 wasm の依存のため）。wasm のビルド確認は、ターゲットが無いため**未実施**である。

## 13. UI 接続に必要な変更（STEP 117C）

1. **起動**:
   - `SealVault::new(dir)` を持つ。
   - `state()` が `NeedsMigration` なら、ダイアログ（設計書 11.1 の文言）で同意を得てから `migrate()` を呼ぶ。拒否されたら延期し、理事長印だけを使えない状態にする。
2. **表示**:
   - 一覧は `Snapshot.seals` の順で表示する。選択は添字ではなく `seal.id` で持つ。
   - 各操作の戻り値の `snapshot` で、画面を置き換える。
3. **登録・倍率・加工・削除**:
   - `register` / `update(seal_id, revision, …)` / `delete(seal_id, revision)` に置き換える。
   - `Conflict` のときは入力を保持したまま最新を表示し、文言を出す。
4. **承認**:
   - 承認ボタンで `ApprovalRef::new(&seal)` を作り、`record_approval` を呼ぶ。成功したときだけ `SealApproval` に保持する。
   - `SealApproval` に `op_id`、`seal_id`、`seal_revision`、`seal_sha256` を足す。
5. **押印**:
   - 既存の照合（`approval_matches` と PNG の SHA-256）の後に、`prepare_use` を呼ぶ。
   - `prepare_use` が `Ok` の場合だけ `apply_edit(AddCustomStamp)` を行う。
   - 成功したら `commit_use`、失敗したら `cancel_use` を呼ぶ。
   - `commit_use` が失敗したら、文書の押印を Undo で戻して `cancel_use` を試みる。それも失敗したら「未完了」と表示し、op_id を保持して後で `use_status` で確かめる。
   - 現在の `append_event` と `pop_event` の組は廃止する（`pop_event` は他の行を消しうる）。
6. **取り消し・やり直し・削除**:
   - `editing.rs` と `note_seal_history_step` を `note_use_step(op_id, step_id, step)` に置き換える。
   - `SealUse` に `op_id` を持たせる。`step_id` は、Undo スタックの項目ごとに 1 回作る。
7. **通知**: `Notice` を表示する（`ManyEvents`、`NearFull`、`LegacyWriteSetAside`、`MigrationFinished`、`TombstoneMissing`）。
8. **エラー表示**: `Busy`、`Full`、`V2Missing`、`Unreadable`、`Verify` の文言と、利用者の次の行動（待つ、旧版を閉じる、バックアップを確認する）を表示する。
9. **未完了の押印**: `Snapshot::open_uses()` に残っている押印を、利用者が確認して `cancel_use` で閉じる手段が要る（14 節）。
10. **automation ツール**: AGENTS.md §3 により、ストア API に対応するヘッドレスのツールが要る。今回は**未実施**。

## 14. 未解決事項・リスク

1. **大きなストアの速度（要判断）**:
   - debug ビルドでは、約 63 MiB で 1 操作に約 10 秒かかった。
   - release でも、STEP 117A の測定（23 MB で 2.5〜3.8 秒）から見て、数秒以上かかる見込みである。
   - その間、ほかのプロセスは 3 秒で `Busy` になる（データは安全で、再試行できる）。
   - 64 MiB の上限そのものを下げるか、`Busy` の扱いを UI でどうするかは判断が要る。
   - 年 480 行の想定なら、到達するまで十分な年数がある（設計書 9 節）。
2. **`legacy_write_detected` を v2 に記録しない（要判断）**:
   - STEP 117B の指示に従った。旧版が書いた行は保全ファイルに残るが、v2 の監査履歴には、検出したことが残らない。
   - 通知を画面に出すことは 117C で必須にする。
3. **準備したまま放置された押印**:
   - 異常終了などで残った `use_prepared` は、8 KiB の確保を持ち続ける。
   - 自動では取り消さない（文書に入ったかどうかをストアは知らない）。117C で、利用者が確認して閉じる手段が要る。
4. **段階イベントの上限**:
   - 取り消しなどは `step_headroom`（1 MiB）を使える。それも使い切ると `Full` になる。
   - 117C では、記録できない Undo/Redo をどう扱うか（文書の操作を止めるか）を決める必要がある。
5. **ハードリンクを作れないボリューム**（exFAT の USB など）:
   - v2 の新規作成は `create_new` での直接書き込みに切り替わる。
   - 途中で失敗した部分ファイルは消す。それでも、電源断のような異常終了では、壊れた v2 が残りうる。
   - その場合はロック状態になり、移行元の `seals.bin` はそのまま残るので、データは失われない。
6. **SWSEAL1 側の不整合**:
   - 旧版の API では起きないが、同名の印影が 2 つある SWSEAL1 ファイルは、移行時に拒否する（何も書かない）。
7. **`state()` はロックファイルを作る**: 0 バイトの `seals.bin.lock` を作ることがある。ストアのファイルがまだ一つも無いフォルダでは作らない。
8. **新規ストアにも墓標を作る**:
   - 移行元が無くても、最初の書き込みで墓標を作る。旧版がそのフォルダで新しい SWSEAL1 を作ることを防ぐためである。
   - その結果、旧版の画面はロック状態になる（リリースノートでの説明が要る）。
9. **`SealEffects` の中の未知の欄**: 既存の型を共用しているため、拒否せずに読み捨てる。
10. **ハードリンクされた `seals.v2.bin`**: 設計書 16 節のとおり、保証の対象外である。
11. **一時的なアクセス拒否**（9.1 節）:
    - v2 ストアでは、約 2 秒の再試行で吸収した。それより長く開かれていれば `Io` になる（データは安全）。
    - 既存の SWSEAL1 ストアと、ほかの `write_atomically` の利用箇所は変更していないので、同じ一時的な失敗が起こりうる（今回の範囲外。要判断）。
12. **未実施の確認**:
    - wasm のビルド確認。
    - ディスクを満杯にする試験。
    - 一時ファイル作成失敗の単独注入。
    - 別の Windows ユーザーでの復号（別鍵の暗号文で代替した）。
    - release ビルドでの 64 MiB の速度測定。

## 15. STEP 117C へ進めるかの評価

**ストア層としては、117C の UI 接続へ進められると評価する。** 理由:
- 停止条件に当たる事象は起きなかった。
  - 実データへのアクセスは無い。
  - 旧 SWSEAL1 が想定外に変わることは無かった。
  - 他プロセスの登録・履歴の消失、イベントの不整合、64 MiB 付近での欠落は無かった。
  - 既存のテストに新規の失敗は無い（11.3 節。失敗は既知の 2 件だけ）。
- 各保証は、意図的な改変で試験が失敗することを確かめた。

**ただし、117C の着手前に、次の 3 点の判断が要る。**
1. 14 節 1: 大きなストアの速度と、64 MiB の上限値。
2. 14 節 2: 旧版の書き戻しの検出を v2 に記録しない扱い。
3. 14 節 3・4: 放置された準備と、段階イベントが上限に達したときの UI。
