# discord_ai_bot Web API 技術仕様書

## 1. 概要

discord_ai_bot は Discord Bot プロセス内で axum 製の HTTP API を同時に起動している。この API は本来 Bot に同梱していた Web コンソール用に作られたもので、現在はフロントエンドを別サービスとして分離し、この API を純粋なバックエンド API として外部（別ドメインの Web サービス）から利用する構成になっている。

- ベース URL: `http://127.0.0.1:3000/api`（本番運用時は Caddy 等のリバースプロキシで公開ドメインに紐付ける想定）
- フォーマット: JSON（`Content-Type: application/json`）
- 認証: 固定トークン方式（後述）
- CORS: `WEB_ORIGIN` 環境変数で許可オリジンを制御

実装は `src/web/mod.rs` 1ファイルにまとまっている。

## 2. 認証

すべての `/api/*` エンドポイントは axum の `route_layer` によるミドルウェア (`auth_middleware`) を通過する。

```
x-api-token: <WEB_API_TOKEN の値>
```

- ヘッダー `x-api-token` の値が環境変数 `WEB_API_TOKEN` と完全一致（大文字小文字含む単純な文字列比較）しない場合、**403 Forbidden** を返す（ボディなし）。
- ヘッダー自体が存在しない場合は空文字列として扱われ、同様に 403 になる。
- OAuth やセッションCookieのような仕組みは使っておらず、固定トークンをリクエストごとに送る単純な方式。トークンの値自体が漏れると誰でも操作できるため、HTTPS 経由でのみ運用することが前提。

## 3. CORS

`build_cors_layer()` が `Router` 全体に `CorsLayer` を適用する。

- 許可メソッド: `GET`, `POST`, `DELETE`
- 許可ヘッダー: `Content-Type`, `x-api-token`
- 許可オリジン:
  - 環境変数 `WEB_ORIGIN` が設定されていて、かつ正当な値としてパースできればそのオリジンのみ許可
  - 未設定、またはパース失敗時は `Any`（全オリジン許可）にフォールバックし、起動時に `eprintln!` で警告を出す
- **注意**: `Any` 許可時は `allow_credentials` は使っていない（Cookie を使わずヘッダートークンのみで認証しているため、ブラウザの credentials 制約には抵触しない）。ただし全オリジン許可のままだと、トークンさえ知っていればどこからでも叩けてしまうため、自分専用運用でも `WEB_ORIGIN` は明示的に設定することを推奨する。

## 4. 状態管理に関する重要な制約

API は Discord Bot 本体と同じプロセス・同じメモリ空間で動いている `AppState` を共有している。ここに以下の特性があり、API 設計上とても重要。

- `target_channel_id: Arc<RwLock<u64>>` — **プロセス全体で1つだけ**保持される「現在操作対象の Discord チャンネル ID」。`POST /api/channel` で切り替えると、以後すべての `/api/*` リクエストがそのチャンネルを対象に動作する。
  - つまりこの API は「リクエストごとに対象チャンネルを指定する」設計ではなく、**グローバルな現在地（カーソル）を切り替えてから操作する**設計になっている。複数クライアントが同時に別々のチャンネルを操作することはできない。
- `channel_models: Arc<RwLock<HashMap<u64, String>>>` と `channel_sessions: Arc<RwLock<HashMap<u64, String>>>` はオンメモリの `HashMap`。**プロセス再起動で内容は消える**（永続化されていない）。再起動後はモデルは `gemini-3.1-flash-lite`、セッションは `default` に戻る。
- 会話履歴・埋め込みベクトルは SQLite (`./data/history.db`) に永続化されているため、こちらは再起動しても消えない。

## 5. データモデル（内部の会話キー）

会話履歴 DB (`strage::history::HistoryStore`) は論理的な「チャンネル」を `channel_id` カラムに文字列で保持しているが、実際には以下の複合キー形式を使っている。

```
"{Discordチャンネルid}:{セッション名}"
```

例: `"123456789012345678:default"`

- Discord 側のメッセージ処理（`bot::handler`）も、Web API 側（`chat_handler` 等）も、両方ともこのキーで同じ `messages` テーブルを参照している。つまり **Discord 上の同じチャンネル・同じセッション名でやり取りした場合、Discord 経由でも Web API 経由でも会話履歴とRAG検索結果は共有される**。
- `GET /api/sessions` は `channel_id LIKE '{ch_id}:%'` で DISTINCT 検索し、`:` の後ろ（セッション名部分）だけを返す。

## 6. エンドポイント一覧

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| POST | `/api/chat` | 要 | メッセージを送信し、AI の応答を取得する |
| GET | `/api/history` | 要 | 現在のチャンネル/セッションの直近履歴を取得する |
| GET | `/api/sessions` | 要 | 現在のチャンネルのセッション一覧を取得する |
| POST | `/api/sessions/switch` | 要 | 操作対象のセッションを切り替える |
| DELETE | `/api/sessions/:name` | 要 | 指定セッションの履歴を全削除する |
| POST | `/api/model` | 要 | 現在のチャンネルで使う AI モデルを切り替える |
| POST | `/api/channel` | 要 | 操作対象の Discord チャンネル ID を切り替える |
| GET | `/api/status` | 要 | 現在のチャンネル・モデル・セッション状態を取得する |
| POST | `/api/search` | 要 | Web 検索を実行する |

以降、各エンドポイントの詳細。

---

### 6.1 `POST /api/chat`

メッセージを送信し、RAG＋直近履歴を踏まえた AI 応答を同期的に返す。

**リクエストボディ**

```json
{
  "message": "こんにちは",
  "files": [
    {
      "name": "example.png",
      "mime": "image/png",
      "data": "<base64文字列>"
    }
  ]
}
```

- `message`: 必須（空文字列も可。ただし空文字列かつ画像添付もない場合は事実上空の問い合わせになる）
- `files`: 任意。省略可。各要素:
  - `name`: ファイル名（拡張子から種別判定に使う）
  - `mime`: MIME タイプ
  - `data`: **base64 エンコード済み**のファイル内容

**ファイル種別の扱い**

- `mime` が `image/` で始まる → 画像として扱い、Gemini の `inlineData` パーツにそのまま変換される（base64 はデコードされずそのまま転送される）
- 拡張子が以下のいずれか、または `mime` が `text/` で始まる → テキストとして扱い、base64 デコード＋UTF-8 として読み込む
  ```
  txt, md, rs, py, js, ts, json, toml, yaml, yml, csv, html, css, c, cpp
  ```
  - 8000 文字を超える場合は先頭 8000 文字で切り捨て、`...(以降省略)` を付与
  - 該当しないファイルは無視される（画像でもテキスト拡張子でもないファイルはドロップされる）

**処理の流れ**

1. 現在の `target_channel_id` と `channel_sessions` から DB キー（`ch_id:session`）を決定
2. 現在の `channel_models` からモデル名を決定（未設定時は `gemini-3.1-flash-lite`）
3. 添付ファイルを画像パーツ／テキストパーツに分解
4. `embed_text`（埋め込み生成・RAG検索用のテキスト）を決定
   - `message` が空かつ画像がある場合は `"[画像添付]"` を使う
   - それ以外は `message` そのまま（テキスト添付ファイルの内容は embed_text には含まれない点に注意）
5. `embed_text` を Vertex AI Embedding API（`gemini-embedding-001`, task_type=`RETRIEVAL_DOCUMENT`）でベクトル化し、ユーザー発言として履歴 DB に保存（`author_id = "web_user"`, `role = "user"`）
6. 同じ `embed_text` を task_type=`RETRIEVAL_QUERY` で再度ベクトル化し、直近300件の候補からコサイン類似度＋時間減衰でトップ3件を検索（詳細は全体技術書のRAGセクション参照）
7. 直近10件の履歴を時系列順に取得
8. AI に渡す `contents` 配列を構築:
   - （関連する過去の会話があれば）合成の `user` メッセージとして先頭に追加
   - 直近履歴を時系列順に追加
   - 今回のメッセージ（`message` + テキストファイル内容を結合したもの、+ 画像パーツ）を最後に追加
9. モデル名が `gpt-` で始まれば `convert::to_openai_messages()` で OpenAI 形式に変換して `OpenAiClient::generate()` を呼ぶ。それ以外は `AiClient::generate_with_contents()`（Vertex AI Gemini、`url_context` ツール有効）を呼ぶ
10. 成功時はレスポンステキストを埋め込み生成してから履歴 DB に保存（`author_id = "bot"`, `role = "model"`）し、`{ "reply": "..." }` を返す
11. **失敗時もエラーステータスコードは返さない**。`unwrap_or_else` で `"エラーが発生しました"` という文字列に差し替えられ、常に `200 OK` で返る（後述「既知の制約」参照）

**レスポンス**

```json
{ "reply": "AIからの応答テキスト" }
```

常に `200 OK`。AI呼び出し自体が失敗しても `reply` に固定のエラーメッセージ文字列が入るだけで、HTTPステータスでは判別できない。

---

### 6.2 `GET /api/history`

現在のチャンネル/セッションの直近50件の会話履歴を、古い順で返す。

**レスポンス**

```json
[
  { "role": "user", "text": "こんにちは" },
  { "role": "bot", "text": "こんにちは！" }
]
```

- DB上の `role` が `model` または `bot` の場合は `"bot"` に、それ以外はすべて `"user"` に正規化される（Discord経由のユーザー発言もWeb経由のユーザー発言も同じ `"user"` として区別なく返る）

---

### 6.3 `GET /api/sessions`

現在の `target_channel_id` に紐づくセッション名の一覧を返す。

**レスポンス**

```json
["default", "work", "private"]
```

- 一度もメッセージが保存されていないセッションは含まれない（DBに実データがある組み合わせのみ列挙される）

---

### 6.4 `POST /api/sessions/switch`

**リクエストボディ**

```json
{ "name": "work" }
```

**レスポンス**: `200 OK`（ボディなし）

- 現在の `target_channel_id` に対して、オンメモリの `channel_sessions` マップを更新するだけ。DB上に新規レコードが作られるわけではなく、次にメッセージが送信された時点で初めてそのセッション名でレコードが作られる。
- プロセス再起動でこの切り替えは失われ、`default` に戻る。

---

### 6.5 `DELETE /api/sessions/:name`

**パスパラメータ**: `name` — 削除したいセッション名

**レスポンス**: `200 OK`（成功時）/ `500 INTERNAL_SERVER_ERROR`（DBエラー時）

- 現在の `target_channel_id` の、指定セッション名に一致する `messages` レコードを全削除する（`DELETE FROM messages WHERE channel_id = '{ch_id}:{name}'`）
- 埋め込みベクトルも含めて完全に削除される。取り消しはできない。

---

### 6.6 `POST /api/model`

**リクエストボディ**

```json
{ "name": "gemini-3.1-pro-preview" }
```

**レスポンス**: `200 OK`（ボディなし）

- 現在の `target_channel_id` に対して、オンメモリの `channel_models` マップを更新する。
- バリデーションは一切行われない。存在しないモデル名を渡しても200が返り、実際にAI呼び出し時にエラーになる。
- Discord側の `/model` スラッシュコマンドの選択肢は `gemini-3.1-flash-lite`, `gemini-3-flash-preview`, `gemini-3.1-pro-preview`, `gpt-4o-mini`, `gpt-4o` に限定されているが、この API 経由ではその制約は効かない。

---

### 6.7 `POST /api/channel`

**リクエストボディ**

```json
{ "channel_id": "123456789012345678" }
```

**レスポンス**: `200 OK`（ボディなし）

- `channel_id` は文字列で渡し、`u64` にパースして `target_channel_id` を更新する。パースに失敗した場合は例外にならず、黙って `0` にセットされる。
- Discord の実チャンネルIDと一致している必要はない（`0` のままなら「Web単独のチャンネル」として振る舞う）。

---

### 6.8 `GET /api/status`

**レスポンス**

```json
{
  "current_channel_id": "123456789012345678",
  "current_model": "gemini-3.1-flash-lite (デフォルト)",
  "current_session": "default",
  "session_count": 3,
  "uptime_seconds": 12345
}
```

- `current_model` はチャンネルに対してモデルが未設定の場合、`"gemini-3.1-flash-lite (デフォルト)"` という文字列（デフォルト値そのものではなく「(デフォルト)」という注記付き文字列である点に注意。実際の生成時には注記なしの `gemini-3.1-flash-lite` が使われる）。
- `uptime_seconds` はプロセス起動からの経過秒数。

---

### 6.9 `POST /api/search`

**リクエストボディ**

```json
{ "query": "検索したい内容", "count": 5, "ai": false }
```

- `query`: 必須
- `count`: 任意、省略時 5（Brave Search 側の上限は 20）
- `ai`: 任意、省略時 `false`

**`ai: false`（デフォルト）の場合**

Brave Search API を直接叩き、生の検索結果を返す。

```json
{
  "ai": false,
  "text": null,
  "results": [
    { "title": "...", "url": "...", "description": "..." }
  ]
}
```

**`ai: true` の場合**

Vertex AI Gemini（モデル固定で `gemini-3-flash-preview`、`google_search` ツール使用）に問い合わせ、要約済みテキストを返す。`count` は無視される。

```json
{ "ai": true, "text": "AIによる要約回答", "results": null }
```

- どちらのモードでも失敗時は例外を投げず、`ai:false` 時は空配列、`ai:true` 時は `"検索に失敗しました"` という文字列を返す（こちらもHTTPステータスでは失敗を判別できない）。

---

## 7. エラーハンドリングに関する既知の制約

このAPIを外部サービスから利用する際に踏まえておくべき点:

1. **ほとんどのハンドラは失敗してもHTTP的には成功（200）を返す。** 埋め込み生成の失敗は空ベクトル、AI生成の失敗は固定の日本語エラーメッセージ文字列、検索の失敗は空配列/エラー文字列として握りつぶされる。呼び出し側でエラーを検知したい場合は、レスポンス内の文字列（`"エラーが発生しました"` 等）をパターンマッチする必要がある。
2. **明示的な失敗ステータスを返すのは `DELETE /api/sessions/:name` の DB エラー時（500）と、認証失敗時（403）のみ。**
3. **バリデーションがほぼ無い。** モデル名、チャンネルIDのパース失敗、空メッセージなどはすべてサーバー側でエラーにならず、そのまま処理が進む（結果的にAI呼び出し時に失敗することはある）。
4. **リクエストのタイムアウト・リトライ機構は無い。** AI呼び出しが遅い/失敗する場合、クライアント側でタイムアウトを設定することを推奨する。
5. **レート制限は実装されていない。** 自分専用利用が前提のため、想定外に呼び出し頻度が上がるとAI APIやBrave Search APIの課金・クォータに直結する。

## 8. フロントエンド実装時の注意点まとめ

- 起動時にまず `POST /api/channel` で対象チャンネルを指定してから他のAPIを呼ぶこと（未指定だと `channel_id = 0` のWeb単独セッションとして動く）
- セッション・モデルの切り替えはプロセス再起動で失われるため、フロントエンド側で「現在の選択状態」を都度 `GET /api/status` で確認するか、自前で永続化するとよい
- `POST /api/chat` は同期API（ストリーミング非対応）。AI応答が返るまでリクエストがブロックされるため、UI側でローディング表示を用意すること
- 画像はbase64のまま送信でよいが、テキストファイルもbase64エンコードして送る必要がある（生テキストではなくbase64文字列を `data` に入れる）
