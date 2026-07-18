function handleFileSelect(event) {
  const files = Array.from(event.target.files);
  selectedFiles = selectedFiles.concat(files);
  renderFilePreview();
  event.target.value = '';
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

// ファイル1件分のカードDOMを生成
// - 添付前プレビュー（削除ボタンあり）と、送信済みチャットログ内の表示（削除ボタンなし）の両方で共用
async function buildFileCard(file, { index = null, removable = false } = {}) {
  const ext = file.name.split('.').pop().toLowerCase();
  const isImage = file.type.startsWith('image/');
  const isText = TEXT_EXTENSIONS.includes(ext) || file.type.startsWith('text/');

  const card = document.createElement('div');
  card.className = 'file-card' + (removable ? '' : ' chat-file-card');

  if (removable) {
    card.innerHTML = `<div class="remove" onclick="removeFile(${index})">×</div>`;
  }

  if (isImage) {
    const url = URL.createObjectURL(file);
    card.classList.add('image-card');
    card.innerHTML += `<img src="${url}" alt="${escapeHtml(file.name)}">`;
  } else if (isText) {
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
    card.classList.add('text-card');
    card.innerHTML += `<div class="file-name">${escapeHtml(file.name)}</div><div class="text-preview">📎 ファイル</div>`;
  }

  return card;
}

async function renderFilePreview() {
  const container = document.getElementById('filePreview');
  if (!container) return;
  container.innerHTML = '';
  for (let i = 0; i < selectedFiles.length; i++) {
    const card = await buildFileCard(selectedFiles[i], { index: i, removable: true });
    container.appendChild(card);
  }
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

  if (imageAdded) {
    renderFilePreview();
  }
});

// --- ドラッグ＆ドロップでファイルを添付 ---
function initDragAndDrop() {
  const dropZone = document.getElementById('main');
  if (!dropZone) return;

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
