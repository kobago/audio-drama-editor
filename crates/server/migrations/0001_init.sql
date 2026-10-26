CREATE TABLE projects (
    id          INTEGER PRIMARY KEY,
    title       TEXT NOT NULL,
    synopsis    TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- persona-lora の知見に沿って分ける:
--   speech_style / sample_lines → システムプロンプト (どう話すか)
--   character_facts             → RAG (何を知っているか)
--   lora_*                      → llama-server 使用時のみ、口調の土台
CREATE TABLE characters (
    id            INTEGER PRIMARY KEY,
    project_id    INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    reading       TEXT NOT NULL DEFAULT '',
    profile       TEXT NOT NULL DEFAULT '',
    speech_style  TEXT NOT NULL DEFAULT '',
    -- [{ "length": "S"|"M"|"L", "text": "..." }]
    sample_lines  TEXT NOT NULL DEFAULT '[]',
    lora_id       INTEGER,
    lora_scale    REAL NOT NULL DEFAULT 1.0,
    -- Irodori-TTS の声: 参照音声 + 基本 caption + seed
    voice_ref_path TEXT,
    voice_caption  TEXT NOT NULL DEFAULT '',
    voice_seed     INTEGER,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

-- RAG 用。1 行 1 事実の短文で持つ。
CREATE TABLE character_facts (
    id            INTEGER PRIMARY KEY,
    character_id  INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    text          TEXT NOT NULL,
    embedding     BLOB
);

CREATE TABLE character_portraits (
    id            INTEGER PRIMARY KEY,
    character_id  INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    label         TEXT NOT NULL DEFAULT '',
    path          TEXT NOT NULL
);

CREATE TABLE scenes (
    id               INTEGER PRIMARY KEY,
    project_id       INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title            TEXT NOT NULL DEFAULT '',
    setting          TEXT NOT NULL DEFAULT '',
    summary          TEXT NOT NULL DEFAULT '',
    background_path  TEXT,
    sort_order       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE lines (
    id               INTEGER PRIMARY KEY,
    scene_id         INTEGER NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
    -- 'dialogue' | 'direction' (ト書き / SE 指示)
    kind             TEXT NOT NULL DEFAULT 'dialogue',
    character_id     INTEGER REFERENCES characters(id) ON DELETE SET NULL,
    text             TEXT NOT NULL DEFAULT '',
    tts_text         TEXT,
    -- 'S' | 'M' | 'L'
    length_class     TEXT,
    -- { emotion, intensity, tempo, volume, distance, voice_quality, note }
    performance      TEXT NOT NULL DEFAULT '{}',
    caption_override TEXT,
    pause_before_ms  INTEGER NOT NULL DEFAULT 0,
    pause_after_ms   INTEGER NOT NULL DEFAULT 300,
    sort_order       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE takes (
    id          INTEGER PRIMARY KEY,
    line_id     INTEGER NOT NULL REFERENCES lines(id) ON DELETE CASCADE,
    path        TEXT NOT NULL,
    seed        INTEGER,
    caption     TEXT NOT NULL DEFAULT '',
    params      TEXT NOT NULL DEFAULT '{}',
    adopted     INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_characters_project ON characters(project_id);
CREATE INDEX idx_scenes_project ON scenes(project_id);
CREATE INDEX idx_lines_scene ON lines(scene_id);
CREATE INDEX idx_takes_line ON takes(line_id);
