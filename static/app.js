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

  let displayMsg = text;
  if (selectedFiles.length > 0) {
    const names = selectedFiles.map(f => `[📎 ${f.name}]`).join(' ');
    displayMsg = displayMsg ? `${displayMsg}\n${names}` : names;
  }

  appendMsg('user', displayMsg);
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

async function refreshStatus() {
  try {
    const status = await api('/status');
    document.getElementById('sbModel').textContent = status.current_model;
    document.getElementById('sbSession').textContent = status.current_session;
    document.getElementById('sbUptime').textContent = formatUptime(status.uptime_seconds);
    document.getElementById('modelSelect').value = status.current_model;

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
  } catch (e) {
    logSystem('ステータス取得失敗。トークンを確認してください。');
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

document.getElementById('tokenInput').value = getToken();
refreshStatus();
setInterval(refreshStatus, 30000);