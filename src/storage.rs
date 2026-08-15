use rugst::{Rugst, SearchOptions, SearchResult};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct HistoryStore {
    rugst: Rugst,
    conn: Mutex<Connection>,
}

impl HistoryStore {
    pub fn new(db_path: &str) -> anyhow::Result<Self> {
        // 親ディレクトリが存在しない場合は自動作成
        if let Some(parent) = Path::new(db_path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let rugst = Rugst::new(db_path)?;
        let conn = Connection::open(db_path)?;
        Ok(Self {
            rugst,
            conn: Mutex::new(conn),
        })
    }

    /// メッセージを記憶(FastEmbedによるローカル埋め込みを自動生成・SQLiteへ保存)
    pub fn remember(
        &self,
        channel_id: &str,
        author_id: &str,
        role: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        self.rugst.remember(channel_id, author_id, role, content)
    }

    /// RAG検索(セマンティック類似度 + FTS5ハイブリッド + 時間減衰)
    pub fn search(
        &self,
        channel_id: &str,
        role: &str,
        query: &str,
        options: &SearchOptions,
    ) -> anyhow::Result<Vec<SearchResult>> {
        self.rugst.search(channel_id, role, query, options)
    }

    /// 直近の会話履歴を古い順(時系列順)で取得
    pub fn get_recent_history(
        &self,
        channel_id: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<(String, String)>> {
        self.rugst.get_recent_history(channel_id, limit)
    }

    /// セッション一覧を取得
    pub fn list_sessions(&self, channel_id: &str) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let pattern = format!("{}:%", channel_id);
        let mut stmt = conn.prepare(
            "SELECT DISTINCT channel_id FROM messages WHERE channel_id LIKE ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![pattern], |row| {
            row.get::<_, String>(0)
        })?;
        let sessions: Vec<String> = rows
            .filter_map(|r| r.ok())
            .filter_map(|full| full.split(':').nth(1).map(|s| s.to_string()))
            .collect();
        Ok(sessions)
    }

    /// セッションに紐づくメッセージを削除
    pub fn delete_session(&self, channel_id: &str, session: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let key = format!("{}:{}", channel_id, session);
        conn.execute("DELETE FROM messages WHERE channel_id = ?1", rusqlite::params![key])?;
        Ok(())
    }
}

/// RAG検索用のデフォルト設定(top_k: 3, 半減期: 14日, 最小スコア: 0.3, 候補窓: 300, FTSハイブリッド有効)
pub fn default_rag_search_options() -> SearchOptions {
    SearchOptions {
        top_k: 3,
        half_life_days: 14.0,
        min_score: 0.3,
        candidate_window: Some(300),
        enable_fts: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_store_remember_and_search() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_rugst_history_auto_dir/history.db");
        let db_str = db_path.to_str().unwrap();

        // 既存ファイルをクリーンアップ
        let _ = std::fs::remove_file(&db_path);

        let store = HistoryStore::new(db_str).expect("Failed to initialize HistoryStore");

        let ch_key = "12345:default";
        store
            .remember(ch_key, "user1", "user", "好きなプログラミング言語はRustです")
            .unwrap();
        store
            .remember(ch_key, "bot", "model", "Rustは素晴らしい言語ですね！")
            .unwrap();
        store
            .remember(ch_key, "user1", "user", "今日の晩ご飯はカレーライスでした")
            .unwrap();

        // 直近履歴の確認 (古い順)
        let recent = store.get_recent_history(ch_key, 10).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].1, "好きなプログラミング言語はRustです");

        // RAG検索の確認
        let search_opts = default_rag_search_options();
        let results = store.search(ch_key, "user", "プログラミング", &search_opts).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].text.contains("Rust"));

        // セッション管理の確認
        let sessions = store.list_sessions("12345").unwrap();
        assert_eq!(sessions, vec!["default".to_string()]);

        // セッション削除
        store.delete_session("12345", "default").unwrap();
        let recent_after_del = store.get_recent_history(ch_key, 10).unwrap();
        assert_eq!(recent_after_del.len(), 0);

        let _ = std::fs::remove_file(&db_path);
    }
}
