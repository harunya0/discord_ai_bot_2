function logSystem(text) {
  const log = document.getElementById('log');
  const div = document.createElement('div');
  div.className = 'msg system';
  div.innerHTML = '<div class="content">' + text + '</div>';
  log.appendChild(div);
  log.scrollTop = log.scrollHeight;
}

// role: 'user' | 'bot'。text はMarkdownとして描画され、files は添付カードとして表示される
async function appendMsg(role, text, files = []) {
  const log = document.getElementById('log');
  const div = document.createElement('div');
  div.className = 'msg ' + role;
  const label = role === 'user' ? 'you' : 'albot';

  // マークダウンをHTMLに変換し、DOMPurifyでサニタイズしてから挿入
  const htmlContent = DOMPurify.sanitize(marked.parse(text || ''));
  div.innerHTML = `<div class="role">${label}</div><div class="content">${htmlContent}</div>`;

  if (files.length > 0) {
    const attachDiv = document.createElement('div');
    attachDiv.className = 'chat-attachments';
    for (const file of files) {
      const card = await buildFileCard(file, { removable: false });
      attachDiv.appendChild(card);
    }
    div.appendChild(attachDiv);
  }

  log.appendChild(div);
  log.scrollTop = log.scrollHeight;
}

async function sendMessage() {
  const input = document.getElementById('messageInput');
  const text = input.value.trim();
  if (!text && selectedFiles.length === 0) return;

  // チャットログにテキストとファイルのカードを描画
  const filesToDisplay = [...selectedFiles];
  await appendMsg('user', text, filesToDisplay);
  input.value = '';

  const filePayloads = await Promise.all(selectedFiles.map(fileToBase64));
  selectedFiles = [];
  renderFilePreview();

  try {
    const data = await api('/chat', {
      method: 'POST',
      body: JSON.stringify({
        message: text,
        files: filePayloads
      })
    });
    appendMsg('bot', data.reply);
  } catch (e) {
    logSystem('エラー: ' + e.message);
  }
}

async function loadHistory() {
  const log = document.getElementById('log');
  if (!log) return;
  log.innerHTML = '';

  try {
    const history = await api('/history');
    if (!history || !Array.isArray(history) || history.length === 0) {
      logSystem('このチャンネル・セッションの過去の会話記録はありません');
      return;
    }
    for (const item of history) {
      await appendMsg(item.role, item.text);
    }
    logSystem('過去の会話履歴を同期しました');
  } catch (e) {
    logSystem('履歴の読み込みに失敗しました: ' + (e.message || e));
  }
}
