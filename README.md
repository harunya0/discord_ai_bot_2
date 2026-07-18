これはdiscord用のAIbotです<br>
メンションの後にメッセージを入力すれば、AIから返答が帰ってきます<br>
/modelでAIのモデルが変更できます<br>
/sessionで会話履歴の切り替えができます（雑談用や、仕事用など）<br>
src/aiの中にGCPのAIのjson鍵を登録してください<br>
また、AIのAPI使うので、API料金は別途かかります<br>
あと、メッセージのアクセス権限が必要です<br>
web機能を使う場合、アドレスを取得してください（Duck DNSが一番手軽かと）<br>
- 使用API（必須）
  - Discord API（discord連携させるものです。discord developer portalを参照ください）
  - GCP AI API（geminiのAPIです）
  - brave API（検索用です）

- 使用API（任意）
  - Chatgpt AI API（chatgptのAPIです）
  - DiscordサーバーID（即時反映用ですが、なくても普通に動きます）
- webを使う場合
 - Caddy
 - ドメイン