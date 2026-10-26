-- LLM によるセリフと演技の生成。
--
-- generations: 「続きを書く」1 回分。同じ条件でまとめて作り直すために条件を残す。
CREATE TABLE generations (
    id             INTEGER PRIMARY KEY,
    scene_id       INTEGER NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
    -- この行の後ろに入れた (NULL ならシーンの先頭)
    after_line_id  INTEGER REFERENCES lines(id) ON DELETE SET NULL,
    count          INTEGER NOT NULL,
    -- 次に起きることの指示 (空でよい)
    direction      TEXT NOT NULL DEFAULT '',
    created_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

-- intent: このセリフで何を言うか (生成の計画。記憶の検索にも使う)
-- gen_state: NULL | 'generating' | 'failed'
ALTER TABLE lines ADD COLUMN intent TEXT NOT NULL DEFAULT '';
ALTER TABLE lines ADD COLUMN gen_state TEXT;
ALTER TABLE lines ADD COLUMN gen_error TEXT;
ALTER TABLE lines ADD COLUMN generation_id INTEGER REFERENCES generations(id) ON DELETE SET NULL;

-- 書き直す前の案。いつでも戻せるように残す。
CREATE TABLE line_versions (
    id            INTEGER PRIMARY KEY,
    line_id       INTEGER NOT NULL REFERENCES lines(id) ON DELETE CASCADE,
    text          TEXT NOT NULL,
    intent        TEXT NOT NULL DEFAULT '',
    length_class  TEXT,
    performance   TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_line_versions_line ON line_versions(line_id);
CREATE INDEX idx_lines_generation ON lines(generation_id);
