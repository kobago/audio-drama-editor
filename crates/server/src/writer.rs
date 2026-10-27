//! LLM でセリフと演技を書く: プロンプトの組み立てと、応答 (JSON) の読み取り。
//!
//! 「計画 → 執筆」の 2 段階で書く。
//! 1. 計画: 場面全体を見て、次の数行の話者・意図・長さ区分を決める (1 回)。
//! 2. 執筆: 1 行ずつ、話者の話し方 (システムプロンプト) と思い出した記憶を渡して、
//!    セリフと演技を書かせる。長さ区分の字数から外れたら書き直させる。

use std::collections::HashMap;

use llm::{ChatClient, Message, SamplingParams};
use serde_json::{Map, Value, json};

use crate::recall::{MemoryDoc, memory_text};
use crate::settings::{LlmSettings, Provider};

/// 長さ区分ごとの字数 (空白と改行を除く)。画面の LENGTH_RANGES と同じ。
pub fn length_range(class: &str) -> Option<(usize, usize)> {
    match class {
        "S" => Some((5, 15)),
        "M" => Some((20, 50)),
        "L" => Some((60, 150)),
        _ => None,
    }
}

pub fn count_chars(text: &str) -> usize {
    text.chars().filter(|c| !c.is_whitespace()).count()
}

/// 執筆を書き直させる回数の上限 (字数が外れたとき)
pub const MAX_RETRIES: usize = 2;

#[derive(Debug, Clone)]
pub struct SceneInfo {
    pub project_title: String,
    pub synopsis: String,
    pub scene_title: String,
    pub setting: String,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct CastMember {
    pub id: i64,
    pub name: String,
    pub profile: String,
    pub speech_style: String,
    /// (長さ区分, セリフ)
    pub samples: Vec<(String, String)>,
    pub lora: Option<(i64, f64)>,
}

/// 台本の 1 行。ト書きは話者なし。
#[derive(Debug, Clone)]
pub struct ScriptLine {
    pub speaker: Option<String>,
    pub text: String,
}

fn script_text(lines: &[ScriptLine]) -> String {
    lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .map(|l| match &l.speaker {
            Some(who) => format!("{who}「{}」", l.text.trim()),
            None => format!("（{}）", l.text.trim()),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scene_block(scene: &SceneInfo) -> String {
    let mut out = format!("# 作品\n{}", scene.project_title);
    if !scene.synopsis.trim().is_empty() {
        out.push_str(&format!("\nあらすじ: {}", scene.synopsis.trim()));
    }
    out.push_str(&format!("\n\n# シーン\n{}", scene.scene_title));
    if !scene.setting.trim().is_empty() {
        out.push_str(&format!("\n場面: {}", scene.setting.trim()));
    }
    if !scene.summary.trim().is_empty() {
        out.push_str(&format!(
            "\nこのシーンで起きること: {}",
            scene.summary.trim()
        ));
    }
    out
}

fn script_block(before: &[ScriptLine], after: &[ScriptLine]) -> String {
    let mut out = String::from("# ここまでの台本\n");
    let text = script_text(before);
    out.push_str(if text.is_empty() {
        "(まだない。シーンの最初から書く)"
    } else {
        &text
    });
    let text = script_text(after);
    if !text.is_empty() {
        out.push_str(&format!(
            "\n\n# この後に続く台本 (ここにつながるように書く)\n{text}"
        ));
    }
    out
}

const LENGTH_GUIDE: &str = "長さ区分: S は 5〜15 字 (相づち・短い反応)、M は 20〜50 字 (ふつうの発話)、L は 60〜150 字 (まとまった語り)。";

/// 計画: 次の `count` 行の話者・意図・長さ区分。
pub fn plan_messages(
    scene: &SceneInfo,
    cast: &[CastMember],
    before: &[ScriptLine],
    after: &[ScriptLine],
    count: usize,
    direction: &str,
) -> Vec<Message> {
    let system = "あなたはラジオドラマ・ドラマ CD の脚本家。音声だけで伝わる台本を書く。\n\
        聞き手に見えるのは声と音だけなので、見た目の説明に頼らず、セリフと効果音で場面を伝える。\n\
        出力は指定された形の JSON オブジェクトだけにする。";
    let cast_text = cast
        .iter()
        .map(|c| {
            let mut s = format!("- {}", c.name);
            if !c.profile.trim().is_empty() {
                s.push_str(&format!(": {}", truncate(c.profile.trim(), 200)));
            }
            s
        })
        .collect::<Vec<_>>()
        .join("\n");
    let direction = if direction.trim().is_empty() {
        "特になし。流れに沿って自然に進める。".to_owned()
    } else {
        direction.trim().to_owned()
    };
    let user = format!(
        "{scene}\n\n# 登場人物\n{cast_text}\n\n{script}\n\n# 次に起きること (作者の指示)\n{direction}\n\n\
        # 課題\n次に続く {count} 行の計画を立てる。セリフの本文はまだ書かない。\n\
        - セリフは kind を \"dialogue\" にし、speaker に登場人物の名前、intent にその行で何をどう言うか \
        (誰に・何を・どんな気持ちで。1 文) を書く。\n\
        - 効果音や間などのト書きは kind を \"direction\" にし、text に短く書く。音で表せるものだけにし、多用しない。\n\
        - {LENGTH_GUIDE}会話のテンポを考え、S と M を中心に、L はここぞというときだけ使う。\n\
        - 同じ人が続けて話すのは、間を置いて言い足すときだけにする。\n\n\
        # 出力の形\n\
        {{\"lines\": [{{\"kind\": \"dialogue\", \"speaker\": \"名前\", \"intent\": \"…\", \"length\": \"S\"}}, \
        {{\"kind\": \"direction\", \"text\": \"…\"}}]}}\n\
        lines はちょうど {count} 個にする。",
        scene = scene_block(scene),
        script = script_block(before, after),
    );
    vec![Message::system(system), Message::user(user)]
}

#[derive(Debug, Clone, PartialEq)]
pub enum Planned {
    Dialogue {
        character_id: i64,
        intent: String,
        length: String,
    },
    Direction {
        text: String,
    },
}

pub fn parse_plan(
    value: &Value,
    cast: &[CastMember],
    count: usize,
) -> anyhow::Result<Vec<Planned>> {
    let items = value["lines"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("計画に lines がありません"))?;
    let by_name: HashMap<&str, i64> = cast.iter().map(|c| (c.name.trim(), c.id)).collect();
    let mut out = Vec::new();
    for item in items.iter().take(count) {
        let str_of = |k: &str| item[k].as_str().map(str::trim).unwrap_or("");
        match str_of("kind") {
            "direction" => {
                if !str_of("text").is_empty() {
                    out.push(Planned::Direction {
                        text: str_of("text").to_owned(),
                    });
                }
            }
            _ => {
                // 知らない話者は飛ばす (登場人物にいない人を出させない)
                let Some(&character_id) = by_name.get(str_of("speaker")) else {
                    continue;
                };
                let length = match str_of("length") {
                    l @ ("S" | "M" | "L") => l.to_owned(),
                    _ => "M".to_owned(),
                };
                out.push(Planned::Dialogue {
                    character_id,
                    intent: str_of("intent").to_owned(),
                    length,
                });
            }
        }
    }
    if !out.iter().any(|p| matches!(p, Planned::Dialogue { .. })) {
        anyhow::bail!("計画にセリフがありません");
    }
    Ok(out)
}

const PERFORMANCE_GUIDE: &str = "performance は声優への演技指導:\n\
    - emotion: 感情 (短い語。例: 不満、照れ、動揺)\n\
    - intensity: 感情の強さ 0.0〜1.0\n\
    - tempo: \"ゆっくりした\" か \"速い\"。ふつうなら null\n\
    - volume: \"小さい\" か \"大きい\"。ふつうなら null\n\
    - distance: 相手との距離 \"近い\" \"普通\" \"遠い\" のどれか\n\
    - voice_quality: 声の質 (例: 震えた、囁く、かすれた)。特になければ null\n\
    - note: 声の出し方の指示を 1 文で (例: 語尾は消え入るように、ため息まじりに)。音声合成の指示に使うので、\
    視線・表情・しぐさ・立ち位置など見た目のことは書かない";

/// 執筆: 1 行のセリフと演技。
#[allow(clippy::too_many_arguments)]
pub fn write_messages(
    scene: &SceneInfo,
    speaker: &CastMember,
    others: &[CastMember],
    before: &[ScriptLine],
    after: &[ScriptLine],
    intent: &str,
    length: &str,
    memories: &[MemoryDoc],
    feedback: Option<&str>,
) -> Vec<Message> {
    // どう話すか → システムプロンプト (persona-lora の方針)
    let mut system = format!(
        "あなたはラジオドラマの脚本家で、いまは「{name}」のセリフだけを書く。\n\
        {name}の人物像と話し方を守り、音声で聞いて自然な話し言葉にする。出力は JSON オブジェクトだけにする。\n\n# {name}",
        name = speaker.name
    );
    if !speaker.profile.trim().is_empty() {
        system.push_str(&format!("\n{}", speaker.profile.trim()));
    }
    if !speaker.speech_style.trim().is_empty() {
        system.push_str(&format!("\n\n# 話し方\n{}", speaker.speech_style.trim()));
    }
    // 同じ長さ区分の例を先に並べる
    let mut samples: Vec<&(String, String)> = speaker.samples.iter().collect();
    samples.sort_by_key(|(l, _)| l != length);
    if !samples.is_empty() {
        let lines: Vec<String> = samples
            .iter()
            .take(6)
            .map(|(l, t)| format!("- ({l}) {t}"))
            .collect();
        system.push_str(&format!(
            "\n\n# {}のセリフの例\n{}",
            speaker.name,
            lines.join("\n")
        ));
    }

    let (min, max) = length_range(length).unwrap_or((20, 50));
    let others_text = others
        .iter()
        .filter(|c| c.id != speaker.id)
        .map(|c| format!("- {}: {}", c.name, truncate(c.profile.trim(), 120)))
        .collect::<Vec<_>>()
        .join("\n");
    let mut user = scene_block(scene);
    if !others_text.is_empty() {
        user.push_str(&format!("\n\n# ほかの登場人物\n{others_text}"));
    }
    user.push_str(&format!("\n\n{}", script_block(before, after)));
    if !memories.is_empty() {
        let lines: Vec<String> = memories
            .iter()
            .map(|m| format!("- {}", memory_text(m)))
            .collect();
        user.push_str(&format!(
            "\n\n# {}がこの場面で思い出していること\n{}\n(記憶は踏まえるだけでよい。説明的に並べない)",
            speaker.name,
            lines.join("\n")
        ));
    }
    user.push_str(&format!(
        "\n\n# 書くセリフ\n話者: {}\n意図: {}\n長さ: {length} ({min}〜{max} 字。空白を除いて数える)",
        speaker.name,
        if intent.trim().is_empty() { "流れに沿って自然に続ける" } else { intent.trim() },
    ));
    if let Some(f) = feedback {
        user.push_str(&format!("\n\n# 前の案への指摘\n{f}"));
    }
    user.push_str(&format!(
        "\n\n# 出力の形\n\
        {{\"text\": \"セリフ\", \"performance\": {{\"emotion\": \"…\", \"intensity\": 0.5, \"tempo\": null, \
        \"volume\": null, \"distance\": \"普通\", \"voice_quality\": null, \"note\": \"…\"}}}}\n\
        text にはセリフだけを書く (話者名・かぎかっこ・ト書きは入れない)。\n{PERFORMANCE_GUIDE}"
    ));
    vec![Message::system(system), Message::user(user)]
}

/// 書いたセリフ: (本文, 演技)。
pub fn parse_written(value: &Value, speaker: &str) -> anyhow::Result<(String, Value)> {
    let text = value["text"]
        .as_str()
        .map(|t| clean_line(t, speaker))
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow::anyhow!("セリフが空でした"))?;
    Ok((text, normalize_performance(&value["performance"])))
}

/// 字数が外れていれば、書き直させる指摘。
pub fn length_feedback(text: &str, length: &str) -> Option<String> {
    let (min, max) = length_range(length)?;
    let n = count_chars(text);
    if n < min {
        Some(format!(
            "「{text}」は {n} 字で短すぎる。{min}〜{max} 字にする。"
        ))
    } else if n > max {
        Some(format!(
            "「{text}」は {n} 字で長すぎる。{min}〜{max} 字に収める。"
        ))
    } else {
        None
    }
}

/// 字数の外れ具合 (0 なら範囲内)。書き直しても外れたときに、いちばん近い案を選ぶのに使う。
pub fn length_miss(text: &str, length: &str) -> usize {
    let Some((min, max)) = length_range(length) else {
        return 0;
    };
    let n = count_chars(text);
    min.saturating_sub(n) + n.saturating_sub(max)
}

/// 演技だけを付ける: 既にあるセリフに。
pub fn direct_messages(
    scene: &SceneInfo,
    cast: &[CastMember],
    script: &[(i64, ScriptLine)],
    targets: &[i64],
) -> Vec<Message> {
    let system = "あなたはラジオドラマの音響監督。台本の各セリフに、声優への演技指導を付ける。\n\
        出力は指定された形の JSON オブジェクトだけにする。";
    let cast_text = cast
        .iter()
        .map(|c| format!("- {}: {}", c.name, truncate(c.profile.trim(), 160)))
        .collect::<Vec<_>>()
        .join("\n");
    let lines = script
        .iter()
        .filter(|(_, l)| !l.text.trim().is_empty())
        .map(|(id, l)| match &l.speaker {
            Some(who) if targets.contains(id) => format!("[{id}] {who}「{}」", l.text.trim()),
            Some(who) => format!("{who}「{}」", l.text.trim()),
            None => format!("（{}）", l.text.trim()),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "{scene}\n\n# 登場人物\n{cast_text}\n\n# 台本\n{lines}\n\n\
        # 課題\n[番号] の付いたセリフそれぞれに演技指導を付ける。前後の流れから気持ちを読み取る。\n\
        {PERFORMANCE_GUIDE}\n\n# 出力の形\n\
        {{\"lines\": [{{\"id\": 番号, \"performance\": {{\"emotion\": \"…\", \"intensity\": 0.5, \"tempo\": null, \
        \"volume\": null, \"distance\": \"普通\", \"voice_quality\": null, \"note\": \"…\"}}}}]}}",
        scene = scene_block(scene),
    );
    vec![Message::system(system), Message::user(user)]
}

pub fn parse_direction(value: &Value) -> HashMap<i64, Value> {
    value["lines"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| Some((i["id"].as_i64()?, normalize_performance(&i["performance"]))))
                .collect()
        })
        .unwrap_or_default()
}

/// 画面の選択肢にない値を落とし、強さを 0〜1 に収める。
pub fn normalize_performance(v: &Value) -> Value {
    let mut out = Map::new();
    let text = |k: &str| {
        v[k].as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "null" && *s != "なし")
            .map(str::to_owned)
    };
    let one_of = |k: &str, allowed: &[&str]| text(k).filter(|s| allowed.contains(&s.as_str()));
    if let Some(e) = text("emotion") {
        out.insert("emotion".into(), e.into());
    }
    if let Some(i) = v["intensity"].as_f64() {
        out.insert(
            "intensity".into(),
            json!((i.clamp(0.0, 1.0) * 10.0).round() / 10.0),
        );
    }
    if let Some(t) = one_of("tempo", &["ゆっくりした", "速い"]) {
        out.insert("tempo".into(), t.into());
    }
    if let Some(t) = one_of("volume", &["小さい", "大きい"]) {
        out.insert("volume".into(), t.into());
    }
    if let Some(t) = one_of("distance", &["近い", "普通", "遠い"]) {
        out.insert("distance".into(), t.into());
    }
    for k in ["voice_quality", "note"] {
        if let Some(t) = text(k) {
            out.insert(k.into(), t.into());
        }
    }
    Value::Object(out)
}

/// 話者名やかぎかっこを付けて返してきたら外す。
fn clean_line(text: &str, speaker: &str) -> String {
    let mut t = text.trim();
    for sep in ["「", "：", ":"] {
        if let Some(rest) = t
            .strip_prefix(speaker)
            .and_then(|r| r.trim_start().strip_prefix(sep))
        {
            t = if sep == "「" {
                rest
            } else {
                rest.trim_start()
            };
            break;
        }
    }
    let t = t.trim();
    let t = t.strip_prefix('「').unwrap_or(t);
    let t = t.strip_suffix('」').unwrap_or(t);
    t.trim().to_owned()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        s.chars().take(n).chain(['…']).collect()
    } else {
        s.to_owned()
    }
}

/// 応答から JSON オブジェクトを取り出す (コードブロックや前置きが付いていても読む)。
pub fn extract_json(raw: &str) -> anyhow::Result<Value> {
    let start = raw
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("JSON がありません: {}", truncate(raw, 200)))?;
    let end = raw
        .rfind('}')
        .ok_or_else(|| anyhow::anyhow!("JSON が閉じていません"))?;
    serde_json::from_str(&raw[start..=end])
        .map_err(|e| anyhow::anyhow!("JSON を読めません ({e}): {}", truncate(raw, 200)))
}

/// 生成に使う LLM。
pub struct Writer {
    client: ChatClient,
    provider: Provider,
}

impl Writer {
    /// `session` は OpenCode Go のプロンプトキャッシュ用。同じシーンの生成で同じ値にする。
    pub fn new(llm: &LlmSettings, session: &str) -> anyhow::Result<Self> {
        let client = match llm.provider {
            Provider::Opencode => {
                let key = llm.opencode_api_key.clone().ok_or_else(|| {
                    anyhow::anyhow!("アプリ設定で OpenCode Go の API キーを保存してください")
                })?;
                if llm.opencode_model.trim().is_empty() {
                    anyhow::bail!("アプリ設定で OpenCode Go のモデルを選んでください");
                }
                ChatClient::new(&llm.opencode_base_url, Some(key), &llm.opencode_model)
                    .with_header("x-opencode-session", session)
            }
            Provider::LlamaServer => {
                ChatClient::new(format!("{}/v1", llm.llama_server_url), None, "")
            }
        };
        Ok(Self {
            client,
            provider: llm.provider,
        })
    }

    /// JSON で答えさせる。読めなければ 1 回だけ聞き直す。
    pub async fn json(
        &self,
        messages: &[Message],
        temperature: f32,
        lora: Option<(i64, f64)>,
    ) -> anyhow::Result<Value> {
        let mut extra = Map::new();
        extra.insert("response_format".into(), json!({ "type": "json_object" }));
        if let (Provider::LlamaServer, Some((id, scale))) = (self.provider, lora) {
            extra.insert("lora".into(), json!([{ "id": id, "scale": scale }]));
        }
        let params = SamplingParams {
            temperature: Some(temperature),
            ..Default::default()
        };
        let mut last = None;
        for _ in 0..2 {
            let raw = self.client.chat(messages, &params, extra.clone()).await?;
            match extract_json(&raw) {
                Ok(v) => return Ok(v),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cast() -> Vec<CastMember> {
        ["アリス", "ボブ"]
            .iter()
            .enumerate()
            .map(|(i, n)| CastMember {
                id: i as i64 + 1,
                name: n.to_string(),
                profile: String::new(),
                speech_style: String::new(),
                samples: vec![],
                lora: None,
            })
            .collect()
    }

    #[test]
    fn plan_keeps_known_speakers_and_directions() {
        let v = json!({"lines": [
            {"kind": "dialogue", "speaker": "アリス", "intent": "責める", "length": "S"},
            {"kind": "dialogue", "speaker": "知らない人", "intent": "x", "length": "M"},
            {"kind": "direction", "text": "風の音"},
            {"kind": "dialogue", "speaker": "ボブ", "intent": "謝る", "length": "XL"},
        ]});
        let plan = parse_plan(&v, &cast(), 10).unwrap();
        assert_eq!(
            plan,
            vec![
                Planned::Dialogue {
                    character_id: 1,
                    intent: "責める".into(),
                    length: "S".into()
                },
                Planned::Direction {
                    text: "風の音".into()
                },
                Planned::Dialogue {
                    character_id: 2,
                    intent: "謝る".into(),
                    length: "M".into()
                },
            ]
        );
        assert!(
            parse_plan(
                &json!({"lines": [{"kind": "direction", "text": "x"}]}),
                &cast(),
                4
            )
            .is_err()
        );
    }

    #[test]
    fn written_lines_are_cleaned_and_performance_normalized() {
        let v = json!({"text": "アリス「遅いよ、もう。」", "performance": {
            "emotion": "不満", "intensity": 1.7, "tempo": "普通", "volume": null, "distance": "近い",
            "voice_quality": "なし", "note": "語尾を上げる"}});
        let (text, perf) = parse_written(&v, "アリス").unwrap();
        assert_eq!(text, "遅いよ、もう。");
        assert_eq!(
            perf,
            json!({"emotion": "不満", "intensity": 1.0, "distance": "近い", "note": "語尾を上げる"})
        );
        assert_eq!(
            parse_written(&json!({"text": "ボブ: ごめん"}), "ボブ")
                .unwrap()
                .0,
            "ごめん"
        );
        assert!(parse_written(&json!({"text": "「」"}), "ボブ").is_err());
    }

    #[test]
    fn length_feedback_and_miss() {
        assert_eq!(length_feedback("遅いよ、もう。", "S"), None);
        assert!(length_feedback("遅い", "S").unwrap().contains("短すぎる"));
        assert_eq!(length_miss("遅い", "S"), 3);
        assert_eq!(length_miss("あ".repeat(20).as_str(), "S"), 5);
        assert_eq!(count_chars("遅い よ\n"), 3);
    }

    #[test]
    fn json_is_extracted_from_wrapped_answers() {
        let v = extract_json("```json\n{\"a\": 1}\n```").unwrap();
        assert_eq!(v, json!({"a": 1}));
        assert!(extract_json("no json").is_err());
    }

    #[test]
    fn script_marks_targets_for_direction() {
        let scene = SceneInfo {
            project_title: "t".into(),
            synopsis: String::new(),
            scene_title: "屋上".into(),
            setting: String::new(),
            summary: String::new(),
        };
        let script = vec![
            (
                1,
                ScriptLine {
                    speaker: None,
                    text: "ドアの音".into(),
                },
            ),
            (
                2,
                ScriptLine {
                    speaker: Some("ボブ".into()),
                    text: "ごめん".into(),
                },
            ),
            (
                3,
                ScriptLine {
                    speaker: Some("アリス".into()),
                    text: "遅い".into(),
                },
            ),
        ];
        let m = direct_messages(&scene, &cast(), &script, &[3]);
        assert!(
            m[1].content
                .contains("（ドアの音）\nボブ「ごめん」\n[3] アリス「遅い」")
        );
        assert_eq!(
            parse_direction(&json!({"lines": [{"id": 3, "performance": {"emotion": "x"}}]}))[&3],
            json!({"emotion": "x"})
        );
    }
}
