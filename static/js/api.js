// 接続先バックエンドのURL（例: https://api.example.com）。
// 空文字列の場合は相対パス扱いになり、フロントとバックエンドが同一オリジンの時だけ動く。
function getApiBase() {
  return localStorage.getItem('apiBase') || '';
}

function getToken() {
  return localStorage.getItem('apiToken') || '';
}

// 「接続」欄の入力(APIのURL・トークン)をまとめて保存する
function saveConnection() {
  const base = document.getElementById('apiBaseInput').value.trim().replace(/\/+$/, '');
  localStorage.setItem('apiBase', base);
  localStorage.setItem('apiToken', document.getElementById('tokenInput').value);
  logSystem('接続先を保存しました');
  refreshStatus();
}

// /api 配下への共通fetchラッパー。バックエンドURL・トークン・JSONヘッダを自動付与する
async function api(path, options = {}) {
  const res = await fetch(getApiBase() + '/api' + path, {
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
