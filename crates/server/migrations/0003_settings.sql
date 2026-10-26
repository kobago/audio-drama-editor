-- 画面から保存する設定。値の解決は「保存した値 → 環境変数 → 既定値」 (settings.rs)。
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
