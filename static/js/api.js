function getToken() {
  return localStorage.getItem('apiToken') || '';
}

function saveToken() {
  localStorage.setItem('apiToken', document.getElementById('tokenInput').value);
  logSystem('トークンを保存しました');
  refreshStatus();
}

// /api 配下への共通fetchラッパー。トークンとJSONヘッダを自動付与する
async function api(path, options = {}) {
  const res = await fetch('/api' + path, {
    ...options,
    headers: {
      ...options.headers,
      'x-api-token': getToken(),
      'Content-Type': 'application/json'
    }
  });
  if (!res.ok) throw new Error('リクエスト失敗 (' + res.status + ')');
  const ct = res.headers.get('content-type') || '';
  return ct.includes('json') ? res.json() : null;
}
