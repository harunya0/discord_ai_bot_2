function getToken() { return localStorage.getItem('apiToken') || ''; }

function saveToken() {
  localStorage.setItem('apiToken', document.getElementById('tokenInput').value);
  logSystem('トークンを保存しました');
  refreshStatus();
}

async function api(path, options = {}) {
  const res = await fetch('/api' + path, {
    ...options,
    headers: { ...options.headers, 'x-api-token': getToken(), 'Content-Type': 'application/json' }
  });
  if (!res.ok) throw new Error('リクエスト失敗 (' + res.status + ')');
  const ct = res.headers.get('content-type') || '';
  return ct.includes('json') ? res.json() : null;
}

function logSystem(text) {
  const log = document.getElementById('log');
  const div = document.createElement('div');
  div.className = 'msg system';
  div.innerHTML = '<div class="content">' + text + '</div>';
  log.appendChild(div);
  log.scrollTop = log.scrollHeight;
}

function appendMsg(role, text) {
  const log = document.getElementById('log');
  const div = document.createElement('div');
  div.className = 'msg ' + role;
  const label = role === 'user' ? 'you' : 'albot';
  div.innerHTML = '<div class="role">' + label + '</div><div class="content"></div>';
  div.querySelector('.content').textContent = text;
  log.appendChild(div);
  log.scrollTop = log.scrollHeight;
}

async function sendMessage() {
  const input = document.getElementById('messageInput');
  const text = input.value.trim();
  if (!text && selectedFiles.length === 0) return;

  // チャットログにテキストとファイルのカードを描画
  const filesToDisplay = [...selectedFiles];
  appendMsg('user', text, filesToDisplay);
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
  log.innerHTML = ''; // 画面を一度クリア

  try {
    const history = await api('/history');
    if (!history || history.length === 0) {
      logSystem('このチャンネル・セッションの過去の会話記録はありません');
      return;
    }
    history.forEach(item => {
      appendMsg(item.role, item.text);
    });
    logSystem('過去の会話履歴を同期しました');
  } catch (e) {
    logSystem('履歴の読み込み失敗: ' + (e.message || e));
  }
}

async function refreshStatus() {
  try {
    const status = await api('/status');
    document.getElementById('sbModel').textContent = status.current_model;
    document.getElementById('sbSession').textContent = status.current_session;
    document.getElementById('sbUptime').textContent = formatUptime(status.uptime_seconds);
    document.getElementById('modelSelect').value = status.current_model;
    document.getElementById('sbChannel').textContent = status.current_channel_id === "0" ? "0 (Web単独)" : status.current_channel_id;
    if (document.getElementById('channelInput').value === "") {
      document.getElementById('channelInput').value = status.current_channel_id === "0" ? "" : status.current_channel_id;
    }
    if (currentLoadedChannel !== status.current_channel_id || currentLoadedSession !== status.current_session) {
      currentLoadedChannel = status.current_channel_id;
      currentLoadedSession = status.current_session;
      await loadHistory();
    }

    const sessions = await api('/sessions');
    const list = document.getElementById('sessionList');
    list.innerHTML = '';
    sessions.forEach(s => {
      const li = document.createElement('li');
      if (s === status.current_session) li.classList.add('active');
      li.innerHTML = '<span>' + s + '</span><span class="del">削除</span>';
      li.onclick = (e) => {
        if (e.target.classList.contains('del')) {
          if (confirm('セッション「' + s + '」を削除しますか?')) {
            api('/sessions/' + encodeURIComponent(s), { method: 'DELETE' }).then(refreshStatus);
          }
        } else {
          document.getElementById('newSession').value = s;
          switchSession();
        }
      };
      list.appendChild(li);
    });
    document.getElementById('sbChannel').textContent = status.current_channel_id === "0" ? "0 (Web単独)" : status.current_channel_id;
    if (document.getElementById('channelInput').value === "") {
      document.getElementById('channelInput').value = status.current_channel_id === "0" ? "" : status.current_channel_id;
    }
  } catch (e) {
    logSystem('ステータス取得失敗。トークンを確認してください。');
    console.error('詳細エラー:', e);
    logSystem('ステータス取得失敗: ' + (e.message || e));
  }
}

function formatUptime(sec) {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  return h + 'h ' + m + 'm';
}

async function switchModel() {
  const name = document.getElementById('modelSelect').value;
  await api('/model', { method: 'POST', body: JSON.stringify({ name }) });
  logSystem('モデルを ' + name + ' に切り替えました');
  refreshStatus();
}

async function switchSession() {
  const name = document.getElementById('newSession').value.trim();
  if (!name) return;
  await api('/sessions/switch', { method: 'POST', body: JSON.stringify({ name }) });
  logSystem('セッションを「' + name + '」に切り替えました');
  document.getElementById('newSession').value = '';
  refreshStatus();
}

async function runSearch() {
  const query = document.getElementById('searchQuery').value.trim();
  const count = parseInt(document.getElementById('searchCount').value) || 5;
  if (!query) return;
  const resultsEl = document.getElementById('searchResults');
  resultsEl.innerHTML = '<li class="mono" style="color:var(--text-muted); font-size:11px;">検索中...</li>';
  try {
    const results = await api('/search', { method: 'POST', body: JSON.stringify({ query, count }) });
    resultsEl.innerHTML = '';
    if (results.length === 0) {
      resultsEl.innerHTML = '<li class="mono" style="color:var(--text-muted); font-size:11px;">結果なし</li>';
      return;
    }
    results.forEach(r => {
      const li = document.createElement('div');
      li.className = 'search-result';
      li.innerHTML =
        '<div class="title">' + escapeHtml(r.title) + '</div>' +
        '<div class="url">' + escapeHtml(r.url) + '</div>' +
        '<div class="desc">' + escapeHtml(r.description) + '</div>';
      resultsEl.appendChild(li);
    });
  } catch (e) {
    resultsEl.innerHTML = '<li class="mono" style="color:var(--danger); font-size:11px;">検索失敗</li>';
  }
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
let selectedFiles = [];

function handleFileSelect(event) {
  const files = Array.from(event.target.files);
  selectedFiles = selectedFiles.concat(files);
  renderFilePreview();
  event.target.value = '';
}

function renderFilePreview() {
  const container = document.getElementById('filePreview');
  if (!container) return;
  container.innerHTML = '';
  selectedFiles.forEach((file, index) => {
    const chip = document.createElement('div');
    chip.className = 'file-chip';
    chip.innerHTML = `<span>📎 ${escapeHtml(file.name)}</span><span class="remove" onclick="removeFile(${index})">×</span>`;
    container.appendChild(chip);
  });
}

function removeFile(index) {
  selectedFiles.splice(index, 1);
  renderFilePreview();
}

function fileToBase64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const base64Data = reader.result.split(',')[1];
      resolve({
        name: file.name,
        mime: file.type || 'text/plain',
        data: base64Data
      });
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}

// クリップボードからの画像ペースト（コピペ）に対応
document.addEventListener('paste', (event) => {
  const items = (event.clipboardData || event.originalEvent.clipboardData).items;
  let imageAdded = false;

  for (let i = 0; i < items.length; i++) {
    if (items[i].type.indexOf('image') !== -1) {
      const file = items[i].getAsFile();
      if (file) {
        // ペーストされた画像（スクショ等）に分かりやすいファイル名を自動付与
        const ext = file.type.split('/')[1] || 'png';
        const pastedFile = new File([file], `pasted_${Date.now()}.${ext}`, {
          type: file.type
        });
        selectedFiles.push(pastedFile);
        imageAdded = true;
      }
    }
  }

  // 画像がペーストされた場合はプレビューを更新
  if (imageAdded) {
    renderFilePreview();
  }
});

// テキスト拡張子のリスト（ボット側と統一）
const TEXT_EXTENSIONS = [
  'txt', 'md', 'rs', 'py', 'js', 'ts', 'json', 'toml', 'yaml', 'yml',
  'csv', 'html', 'css', 'c', 'cpp'
];

// プレビュー表示関数をアップグレード
async function renderFilePreview() {
  const container = document.getElementById('filePreview');
  if (!container) return;
  container.innerHTML = '';

  for (let i = 0; i < selectedFiles.length; i++) {
    const file = selectedFiles[i];
    const ext = file.name.split('.').pop().toLowerCase();
    const isImage = file.type.startsWith('image/');
    const isText = TEXT_EXTENSIONS.includes(ext) || file.type.startsWith('text/');

    const card = document.createElement('div');
    card.className = 'file-card';
    card.innerHTML = `<div class="remove" onclick="removeFile(${i})">×</div>`;

    if (isImage) {
      // 画像ならサムネイルを生成して正方形に収める
      const url = URL.createObjectURL(file);
      card.classList.add('image-card');
      card.innerHTML += `<img src="${url}" alt="${escapeHtml(file.name)}">`;
    } else if (isText) {
      // テキストなら最初の5行を取得して表示
      card.classList.add('text-card');
      try {
        const text = await file.text();
        const lines = text.split(/\r?\n/).slice(0, 5).join('\n');
        card.innerHTML += `
          <div class="file-name" title="${escapeHtml(file.name)}">${escapeHtml(file.name)}</div>
          <div class="text-preview">${escapeHtml(lines)}</div>
        `;
      } catch (e) {
        card.innerHTML += `<div class="file-name">${escapeHtml(file.name)}</div><div class="text-preview">(読み込み失敗)</div>`;
      }
    } else {
      // その他ファイル
      card.classList.add('text-card');
      card.innerHTML += `<div class="file-name">${escapeHtml(file.name)}</div><div class="text-preview">📎 ファイル</div>`;
    }

    container.appendChild(card);
  }
}

async function appendMsg(role, text, files = []) {
  const log = document.getElementById('log');
  const div = document.createElement('div');
  div.className = 'msg ' + role;
  const label = role === 'user' ? 'you' : 'albot';
  div.innerHTML = '<div class="role">' + label + '</div><div class="content"></div>';
  div.querySelector('.content').textContent = text;
  
  // ファイルがある場合はチャットログ内にカードを描画
  if (files.length > 0) {
    const attachDiv = document.createElement('div');
    attachDiv.className = 'chat-attachments';
    
    for (const file of files) {
      const card = document.createElement('div');
      card.className = 'file-card chat-file-card';
      const ext = file.name.split('.').pop().toLowerCase();
      const isImage = file.type.startsWith('image/');
      const isText = TEXT_EXTENSIONS.includes(ext) || file.type.startsWith('text/');
      
      if (isImage) {
        const url = URL.createObjectURL(file);
        card.classList.add('image-card');
        card.innerHTML = `<img src="${url}" alt="${escapeHtml(file.name)}">`;
      } else if (isText) {
        card.classList.add('text-card');
        try {
          const content = await file.text();
          const lines = content.split(/\r?\n/).slice(0, 5).join('\n');
          card.innerHTML = `
            <div class="file-name" title="${escapeHtml(file.name)}">${escapeHtml(file.name)}</div>
            <div class="text-preview">${escapeHtml(lines)}</div>
          `;
        } catch (e) {
          card.innerHTML = `<div class="file-name">${escapeHtml(file.name)}</div><div class="text-preview">(読み込み失敗)</div>`;
        }
      } else {
        card.classList.add('text-card');
        card.innerHTML = `<div class="file-name">${escapeHtml(file.name)}</div><div class="text-preview">📎 ファイル</div>`;
      }
      attachDiv.appendChild(card);
    }
    div.appendChild(attachDiv);
  }

  log.appendChild(div);
  log.scrollTop = log.scrollHeight;
}
// --- ドラッグ＆ドロップでファイルを添付 ---
const dropZone = document.getElementById('main');

if (dropZone) {
  ['dragenter', 'dragover'].forEach(eventName => {
    dropZone.addEventListener(eventName, (e) => {
      e.preventDefault();
      e.stopPropagation();
      dropZone.classList.add('drag-over');
    }, false);
  });

  ['dragleave', 'drop'].forEach(eventName => {
    dropZone.addEventListener(eventName, (e) => {
      e.preventDefault();
      e.stopPropagation();
      dropZone.classList.remove('drag-over');
    }, false);
  });

  dropZone.addEventListener('drop', (e) => {
    const dt = e.dataTransfer;
    const files = Array.from(dt.files);
    if (files.length > 0) {
      selectedFiles = selectedFiles.concat(files);
      renderFilePreview();
    }
  }, false);
}

let currentLoadedSession = null;
let currentLoadedChannel = null;

async function loadHistory() {
  const log = document.getElementById('log');
  if (!log) return;
  log.innerHTML = '';

  try {
    const history = await api('/history');
    if (!history || !Array.isArray(history)) return;

    history.forEach(item => {
      appendMsg(item.role, item.text);
    });
  } catch (e) {
    logSystem('履歴の読み込みに失敗しました: ' + e.message);
  }
}

async function switchChannel() {
  const channel_id = document.getElementById('channelInput').value.trim() || "0";
  try {
    await api('/channel', { method: 'POST', body: JSON.stringify({ channel_id }) });
    logSystem('同期チャンネルを「' + (channel_id === "0" ? "Web単独 (0)" : channel_id) + '」に切り替えました');
    currentLoadedChannel = null;
    currentLoadedSession = null;
    await refreshStatus();
  } catch (e) {
    logSystem('チャンネル切り替え失敗: サーバー側の更新や再起動が完了しているか確認してください (' + e.message + ')');
  }
}

document.getElementById('tokenInput').value = getToken();
refreshStatus();
setInterval(refreshStatus, 30000);