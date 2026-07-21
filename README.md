# discord_ai_bot

Discord上でAI（Gemini / GPT）と会話できるBotです。Discordのスラッシュコマンド・メンション経由の会話に加えて、同一プロセス内でWeb API（`/api/*`）も起動します。

> **構成メモ**: 以前はこのプロセスがWebコンソールの静的ファイルも配信していましたが、Webフロントエンドは別サービスとして分離しました。このリポジトリはDiscord Bot + Web APIのみを提供し、フロントエンドは別ドメイン/別サービスからこのAPIを`x-api-token`ヘッダー付きで呼び出します。API利用方法は下記「Web APIについて」を参照してください。

## 主な機能

- **AIとの会話**: Botにメンションすると応答します。Vertex AI経由のGeminiモデル、またはOpenAIのGPTモデルをチャンネルごとに切り替え可能です
- **画像・ファイル添付**: 画像はそのままAIに渡され、テキスト系ファイル（`.txt`, `.md`, `.py`, `.js`, `.json`など）は内容を読み込んでプロンプトに含めます
- **返信コンテキスト**: 誰かのメッセージに返信する形でBotにメンションすると、その元メッセージの内容も踏まえて応答します
- **会話履歴とRAG**: SQLiteに会話履歴とembeddingを保存し、過去の関連する会話を時間経過で重みを弱めながら（decay付き類似度検索）自動的に呼び出します
- **セッション管理**: チャンネルごとに複数の会話セッションを切り替え・削除できます（`/session`コマンド、Web APIでも操作可能）
- **Web検索**: Brave Search APIによる検索、またはGoogle検索連携済みのAIによる要約回答（`/search`コマンド）
- **Web API**: 別サービス(フロントエンド)から会話・設定変更ができるAPI（詳細は下記）

## スラッシュコマンド

| コマンド | 内容 |
|---|---|
| `/model name:<モデル名>` | そのチャンネルで使うAIモデルを切り替え |
| `/session name:<名前>` | セッションを切り替え（省略時は現在のセッションと一覧を表示） |
| `/session delete:<名前>` | 指定したセッションの履歴を削除 |
| `/search query:<内容> ai:<true/false> count:<件数>` | Web検索（`ai:true`でAIによる要約回答） |

## セットアップ

### 必要なもの

- Rust（stable）
- Discord Botのトークンとアプリケーション
- GCPプロジェクト + Vertex AI用サービスアカウント（Gemini / embedding用）
- （任意）OpenAI APIキー（GPTモデルを使う場合）
- （任意）Brave Search APIキー（Web検索を使う場合）

### 1. クローン

```bash
git clone https://github.com/あなたのID/discord_ai_bot.git
cd discord_ai_bot
```

### 2. 環境変数を設定

`.env.example`を`.env`にコピーして、値を埋めてください。

```bash
cp .env.example .env
```

| 変数名 | 必須 | 説明 |
|---|---|---|
| `DISCORD_TOKEN` | ○ | Discord Botのトークン |
| `DISCORD_GUILD_ID` | - | 指定するとそのサーバーにコマンドを即時反映（開発時向け。未指定でもグローバル登録される） |
| `GCP_CREDENTIALS_PATH` | ○ | Vertex AI用サービスアカウントjsonのパス（`./src/ai/AI_API_KEY.json`など） |
| `GCP_PROJECT_ID` | ○ | GCPのプロジェクトID |
| `GCP_LOCATION` | - | Vertex AIのロケーション（デフォルト: `global`） |
| `GCP_MODEL` | - | デフォルトのGeminiモデル（デフォルト: `gemini-3.1-flash-lite`） |
| `OPENAI_API_KEY` | - | GPTモデルを使う場合のOpenAI APIキー |
| `BRAVE_API_KEY` | - | Web検索を使う場合のBrave Search APIキー |
| `WEB_API_TOKEN` | ○ | Web APIへのアクセス用トークン（自分で好きな文字列を決める。`x-api-token`ヘッダーで送る） |
| `WEB_ORIGIN` | - | Web API(CORS)を許可するオリジン（別サービス化したフロントエンドのURL、例: `https://web.example.com`）。未設定の場合は全オリジン許可（認証はトークンで担保） |

サービスアカウントのjsonファイルは`GCP_CREDENTIALS_PATH`に指定したパスに置いてください。リポジトリには含まれていません（`.gitignore`済み）。

### 3. ビルド・起動

```bash
cargo build --release
./target/release/discord_ai_bot
```

起動時にDiscordのスラッシュコマンドが自動登録され、Webサーバーが`0.0.0.0:3000`（アプリ内で固定）で立ち上がります。

### 4. 動作確認

- **Discord**: サーバーにBotを招待し、メンションして話しかける
- **Web API**: `http://localhost:3000/api/status` に`x-api-token: <WEB_API_TOKENの値>`ヘッダーを付けてアクセスし、レスポンスが返ってくることを確認

APIは「どのDiscordチャンネル（セッション）を見る/操作するか」を`/api/channel`で切り替える設計なので、フロントエンド側は初回に対象のDiscordチャンネルIDを指定してください（未指定/`0`のままならWeb単独のセッションとして動きます）。

## Web APIについて

すべてのエンドポイントは`/api`配下で、リクエストヘッダーに`x-api-token: <WEB_API_TOKENの値>`が必須です。別サービス(フロントエンド)からブラウザ経由で叩く場合は、`WEB_ORIGIN`環境変数にそのフロントエンドのオリジンを設定してCORSを許可してください。

| メソッド | パス | 内容 |
|---|---|---|
| `POST` | `/api/chat` | メッセージ送信、AI応答を取得（`{ "message": "...", "files": [...] }`） |
| `GET` | `/api/history` | 現在のチャンネル/セッションの会話履歴を取得 |
| `GET` | `/api/sessions` | セッション一覧を取得 |
| `POST` | `/api/sessions/switch` | セッションを切り替え（`{ "name": "..." }`） |
| `DELETE` | `/api/sessions/:name` | 指定セッションを削除 |
| `POST` | `/api/model` | 使用モデルを切り替え（`{ "name": "..." }`） |
| `POST` | `/api/channel` | 操作対象のDiscordチャンネルIDを切り替え（`{ "channel_id": "..." }`） |
| `GET` | `/api/status` | 現在のチャンネル・モデル・セッション状態を取得 |
| `POST` | `/api/search` | Web検索（`{ "query": "...", "ai": false, "count": 5 }`） |

例:
```bash
curl -H "x-api-token: $WEB_API_TOKEN" http://localhost:3000/api/status
```

動作例は[こちら](https://github.com/harunya0/webFront)

## プロジェクト構成

```
src/
├── main.rs        … エントリポイント、Discordイベントループ、スラッシュコマンド登録
├── bot/           … メッセージ/インタラクションのハンドリング
├── ai/            … Vertex AI(Gemini) / OpenAI クライアント、embedding、フォーマット変換
├── rag/           … 類似度検索(コサイン類似度 + 時間減衰)
├── search/        … Brave Search クライアント
├── strage/        … SQLiteによる会話履歴の保存・検索
└── web/           … Web APIサーバー(axum、フロントエンドの静的配信は行わない)
data/              … 実行時に会話履歴DB(history.db)が作られる
```

フロントエンド(Webコンソール)は別サービスとして分離済みです。旧`static/`配下のHTML/CSS/JSはそちらのリポジトリに移動し、APIの呼び出し先を本サービスの公開URL（例: `https://api.example.com/api/...`）に向けてください。

## 本番環境へのデプロイ

さくらのクラウド・AWSなど、Rocky Linux系のVPSへのデプロイ手順（Caddyによる自動HTTPS化、fail2banでの防御、DuckDNSでのIP更新を含む）をAnsibleでコード化したものを`ansible-iac/`以下に同梱しています。使い方は[`ansible-iac`](https://github.com/harunya0/ansible-iac)を参照してください。

## ライセンス

[`LICENSE`](./LICENSE)を参照してください。
