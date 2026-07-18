// 選択中（未送信）の添付ファイル一覧
let selectedFiles = [];

// 直近に読み込んだチャンネル / セッション（重複した履歴読み込みを防ぐため）
let currentLoadedChannel = null;
let currentLoadedSession = null;

// プレビューでテキストとして扱う拡張子（サーバー側の判定と統一）
const TEXT_EXTENSIONS = [
  'txt', 'md', 'rs', 'py', 'js', 'ts', 'json', 'toml', 'yaml', 'yml',
  'csv', 'html', 'css', 'c', 'cpp'
];
