function toggleSidebar() {
  const sidebar = document.getElementById('sidebar');
  const overlay = document.getElementById('overlay');
  if (!sidebar || !overlay) return;

  sidebar.classList.toggle('open');
  overlay.classList.toggle('active');
}

// スマホ操作時：メニュー内の項目(セッション切り替え等)を押したら自動でメニューを閉じる
function initSidebarAutoClose() {
  const sidebar = document.getElementById('sidebar');
  if (!sidebar) return;

  sidebar.addEventListener('click', (e) => {
    if (window.innerWidth <= 768) {
      // ボタンやリストの項目(li)がクリックされたら閉じる
      if (e.target.tagName === 'BUTTON' || e.target.closest('li')) {
        // 少し遅延させて処理結果を見せてから閉じる
        setTimeout(() => {
          sidebar.classList.remove('open');
          const overlay = document.getElementById('overlay');
          if (overlay) overlay.classList.remove('active');
        }, 150);
      }
    }
  });
}
