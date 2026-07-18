// HTMLエスケープ（ユーザー由来の文字列をそのままinnerHTMLに入れないため）
function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

// 秒数を "1h 23m" 形式に整形
function formatUptime(sec) {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  return h + 'h ' + m + 'm';
}
