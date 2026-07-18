# discord_ai_bot

Discord上でAI（Gemini / GPT）と会話できるBotです。Discordのスラッシュコマンド・メンション経由の会話に加えて、ブラウザから使えるWebコンソールも同梱しています。

## 主な機能

- **AIとの会話**: Botにメンションすると応答します。Vertex AI経由のGeminiモデル、またはOpenAIのGPTモデルをチャンネルごとに切り替え可能です
- **画像・ファイル添付**: 画像はそのままAIに渡され、テキスト系ファイル（`.txt`, `.md`, `.py`, `.js`, `.json`など）は内容を読み込んでプロンプトに含めます
- **返信コンテキスト**: 誰かのメッセージに返信する形でBotにメンションすると、その元メッセージの内容も踏まえて応答します
- **会話履歴とRAG**: SQLiteに会話履歴とembeddingを保存し、過去の関連する会話を時間経過で重みを弱めながら（decay付き類似度検索）自動的に呼び出します
- **セッション管理**: チャンネルごとに複数の会話セッションを切り替え・削除できます（`/session`コマンド、Webコンソールでも操作可能）
- **Web検索**: Brave Search APIによる検索、またはGoogle検索連携済みのAIによる要約回答（`/search`コマンド）
- **Webコンソール**: Discordを介さずブラウザからBotと会話・設定変更ができる管理画面（`static/`以下）

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
| `WEB_API_TOKEN` | ○ | Webコンソールへのログイン用トークン（自分で好きな文字列を決める） |

サービスアカウントのjsonファイルは`GCP_CREDENTIALS_PATH`に指定したパスに置いてください。リポジトリには含まれていません（`.gitignore`済み）。

### 3. ビルド・起動

```bash
cargo build --release
./target/release/discord_ai_bot
```

起動時にDiscordのスラッシュコマンドが自動登録され、Webサーバーが`0.0.0.0:3000`（アプリ内で固定）で立ち上がります。

### 4. 動作確認

- **Discord**: サーバーにBotを招待し、メンションして話しかける
- **Webコンソール**: `http://localhost:3000` にアクセスし、`.env`の`WEB_API_TOKEN`と同じ値を入力して接続

Webコンソールは「どのDiscordチャンネル（セッション）を見る/操作するか」を切り替えられる管理画面なので、初回はサイドバーの「同期チャンネル」でDiscordのチャンネルIDを指定してください（`0`のままならWeb単独のセッションとして動きます）。

## プロジェクト構成

```
src/
├── main.rs        … エントリポイント、Discordイベントループ、スラッシュコマンド登録
├── bot/           … メッセージ/インタラクションのハンドリング
├── ai/            … Vertex AI(Gemini) / OpenAI クライアント、embedding、フォーマット変換
├── rag/           … 類似度検索(コサイン類似度 + 時間減衰)
├── search/        … Brave Search クライアント
├── strage/        … SQLiteによる会話履歴の保存・検索
└── web/           … Webコンソール用のAPIサーバー(axum)
static/            … Webコンソールのフロントエンド(HTML/CSS/JS)
data/              … 実行時に会話履歴DB(history.db)が作られる
```

## 本番環境へのデプロイ

さくらのクラウド・AWSなど、Rocky Linux系のVPSへのデプロイ手順（Caddyによる自動HTTPS化、fail2banでの防御、DuckDNSでのIP更新を含む）をAnsibleでコード化したものを`ansible-iac/`以下に同梱しています。使い方は[`ansible-iac`](https://github.com/harunya0/ansible-iac)を参照してください。

## ライセンス

[`LICENSE`](./LICENSE)を参照してください。
