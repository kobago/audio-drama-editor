-- セリフで使う立ち絵 (表情差分)。立ち絵を消したら未指定に戻す。
ALTER TABLE lines ADD COLUMN portrait_id INTEGER REFERENCES character_portraits(id) ON DELETE SET NULL;
-- テイクの尺 (ミリ秒)。波形の長さと一覧の表示に使う。
ALTER TABLE takes ADD COLUMN duration_ms INTEGER NOT NULL DEFAULT 0;
