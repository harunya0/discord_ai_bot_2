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
