async function refreshStatus() {
  try {
    const status = await api('/status');
    document.getElementById('sbModel').textContent = status.current_model;
    document.getElementById('sbSession').textContent = status.current_session;
    document.getElementById('sbUptime').textContent = formatUptime(status.uptime_seconds);
    document.getElementById('modelSelect').value = status.current_model;

    const channelLabel = status.current_channel_id === "0" ? "0 (Web単独)" : status.current_channel_id;
    document.getElementById('sbChannel').textContent = channelLabel;

    // モバイル用の折りたたみ詳細行にも同じ値を反映
    document.getElementById('sbModelM').textContent = status.current_model;
    document.getElementById('sbSessionM').textContent = status.current_session;
    document.getElementById('sbUptimeM').textContent = formatUptime(status.uptime_seconds);
    document.getElementById('sbChannelM').textContent = channelLabel;
    if (document.getElementById('channelInput').value === "") {
      document.getElementById('channelInput').value = status.current_channel_id === "0" ? "" : status.current_channel_id;
    }

    // チャンネル/セッションが変わっていたら履歴を再読み込み
    if (currentLoadedChannel !== status.current_channel_id || currentLoadedSession !== status.current_session) {
      currentLoadedChannel = status.current_channel_id;
      currentLoadedSession = status.current_session;
      await loadHistory();
    }

    await refreshSessionList(status.current_session);
  } catch (e) {
    logSystem('ステータス取得失敗: ' + (e.message || e));
    console.error('詳細エラー:', e);
  }
}

async function refreshSessionList(currentSession) {
  const sessions = await api('/sessions');
  const list = document.getElementById('sessionList');
  list.innerHTML = '';
  sessions.forEach(s => {
    const li = document.createElement('li');
    if (s === currentSession) li.classList.add('active');
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
