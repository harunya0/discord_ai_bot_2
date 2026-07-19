async function runSearch() {
  const query = document.getElementById('searchQuery').value.trim();
  const count = parseInt(document.getElementById('searchCount').value) || 5;
  const useAi = document.getElementById('searchAi').checked;
  if (!query) return;

  const resultsEl = document.getElementById('searchResults');
  resultsEl.innerHTML = '<li class="mono" style="color:var(--text-muted); font-size:11px;">' +
    (useAi ? 'AIが調べています...' : '検索中...') + '</li>';

  try {
    const data = await api('/search', { method: 'POST', body: JSON.stringify({ query, count, ai: useAi }) });
    resultsEl.innerHTML = '';

    if (data.ai) {
      const div = document.createElement('div');
      div.className = 'search-result search-result-ai';
      const html = DOMPurify.sanitize(marked.parse(data.text || ''));
      div.innerHTML = '<div class="title">AI要約</div><div class="ai-desc">' + html + '</div>';
      resultsEl.appendChild(div);
      return;
    }

    const results = data.results || [];
    if (results.length === 0) {
      resultsEl.innerHTML = '<li class="mono" style="color:var(--text-muted); font-size:11px;">結果なし</li>';
      return;
    }
    results.forEach(r => {
      const div = document.createElement('div');
      div.className = 'search-result';
      div.innerHTML =
        '<div class="title">' + escapeHtml(r.title) + '</div>' +
        '<div class="url">' + escapeHtml(r.url) + '</div>' +
        '<div class="desc">' + escapeHtml(r.description) + '</div>';
      resultsEl.appendChild(div);
    });
  } catch (e) {
    resultsEl.innerHTML = '<li class="mono" style="color:var(--danger); font-size:11px;">検索失敗</li>';
  }
}

// AI要約モード時は件数指定が使われないため、入力欄を無効化して分かりやすくする
function onSearchAiToggle() {
  const useAi = document.getElementById('searchAi').checked;
  document.getElementById('searchCount').disabled = useAi;
}
