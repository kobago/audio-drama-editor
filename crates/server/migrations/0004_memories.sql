-- キャラクターの記憶 (RAG)。事実 (character_facts) を置き換える。
--   kind: experience (経験) | recollection (思い出) | knowledge (知識) | relation (関係) | habit (好み・習慣)
--   period: いつのことか (「小学4年の夏」など。空でよい)
--   people: 関わる人物の名前 (JSON 配列)
--   embedding: EmbeddingGemma 2 のベクトル (f32 リトルエンディアン)。検索の前に、足りないものと古いものを作り直す
CREATE TABLE character_memories (
    id              INTEGER PRIMARY KEY,
    character_id    INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL DEFAULT 'knowledge',
    period          TEXT NOT NULL DEFAULT '',
    people          TEXT NOT NULL DEFAULT '[]',
    text            TEXT NOT NULL,
    embedding       BLOB,
    -- 埋め込んだときのモデルと文書 ("モデル\n文書")。今の文書と違えば作り直す
    embedding_key   TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO character_memories (id, character_id, kind, text)
    SELECT id, character_id, 'knowledge', text FROM character_facts;
DROP TABLE character_facts;

CREATE INDEX idx_memories_character ON character_memories(character_id);

-- セリフごとの記憶の指定: pin は必ず使う、exclude は使わない。
CREATE TABLE line_memory_marks (
    line_id    INTEGER NOT NULL REFERENCES lines(id) ON DELETE CASCADE,
    memory_id  INTEGER NOT NULL REFERENCES character_memories(id) ON DELETE CASCADE,
    mark       TEXT NOT NULL CHECK (mark IN ('pin', 'exclude')),
    PRIMARY KEY (line_id, memory_id)
);

-- セリフごとの最後の記憶検索の結果 (候補と点数)。どれを使うかは読むときに印と合わせて決める。
CREATE TABLE line_recalls (
    line_id     INTEGER PRIMARY KEY REFERENCES lines(id) ON DELETE CASCADE,
    intent      TEXT NOT NULL,
    candidates  TEXT NOT NULL,
    timing      TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
