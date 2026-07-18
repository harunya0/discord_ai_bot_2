document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('tokenInput').value = getToken();

  document.getElementById('messageInput').addEventListener('keydown', function (e) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  });

  initDragAndDrop();
  initSidebarAutoClose();

  refreshStatus();
  setInterval(refreshStatus, 30000);
});
