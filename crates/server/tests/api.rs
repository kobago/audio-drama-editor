//! API のテスト。一時ディレクトリをデータの置き場所にし、TTS は読み込まない。

use audio_drama_editor::settings::EnvDefaults;
use audio_drama_editor::{AppState, Config, TtsConfig, app};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

struct TestApp {
    app: Router,
    home: tempfile::TempDir,
}

async fn start(env: EnvDefaults) -> TestApp {
    let home = tempfile::tempdir().unwrap();
    let config = Config {
        home: home.path().to_path_buf(),
        tts: TtsConfig {
            enabled: false,
            ..Default::default()
        },
        rag: false,
        env,
    };
    TestApp {
        app: app(AppState::open(config).await.unwrap()),
        home,
    }
}

impl TestApp {
    async fn call(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(v) => {
                req = req.header("content-type", "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let res = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

#[tokio::test]
async fn data_lives_under_the_home_directory() {
    let t = start(EnvDefaults::default()).await;
    assert!(t.home.path().join("audio-drama-editor.db").exists());
    assert!(t.home.path().join("media").is_dir());
}

#[tokio::test]
async fn settings_resolve_saved_then_env_then_default() {
    let t = start(EnvDefaults {
        opencode_base_url: Some("https://example.test/v1".into()),
        ..Default::default()
    })
    .await;

    let (status, s) = t.call(Method::GET, "/api/settings", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        s["llama_server_url"],
        json!({ "value": "http://127.0.0.1:8080", "source": "default" })
    );
    assert_eq!(
        s["opencode_base_url"],
        json!({ "value": "https://example.test/v1", "source": "env" })
    );
    assert_eq!(s["opencode_api_key"], json!({ "source": null }));

    let input = json!({
        "llama_server_url": "http://192.168.1.5:8080/",
        "opencode_base_url": "https://opencode.ai/zen/go/v1",
        "opencode_model": "some-model",
    });
    let (status, _) = t.call(Method::PUT, "/api/settings", Some(input)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, s) = t.call(Method::GET, "/api/settings", None).await;
    // 末尾のスラッシュは落として保存する
    assert_eq!(
        s["llama_server_url"],
        json!({ "value": "http://192.168.1.5:8080", "source": "saved" })
    );
    assert_eq!(s["opencode_base_url"]["source"], "saved");
    assert_eq!(
        s["opencode_model"],
        json!({ "value": "some-model", "source": "saved" })
    );
}

#[tokio::test]
async fn settings_reject_non_http_urls() {
    let t = start(EnvDefaults::default()).await;
    let input = json!({
        "llama_server_url": "file:///etc/passwd",
        "opencode_base_url": "https://opencode.ai/zen/go/v1",
        "opencode_model": "",
    });
    let (status, _) = t.call(Method::PUT, "/api/settings", Some(input)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_api_key_is_write_only() {
    let t = start(EnvDefaults {
        opencode_api_key: Some("env-key".into()),
        ..Default::default()
    })
    .await;
    let (_, s) = t.call(Method::GET, "/api/settings", None).await;
    assert_eq!(s["opencode_api_key"], json!({ "source": "env" }));

    let (status, _) = t
        .call(
            Method::PUT,
            "/api/settings/opencode-api-key",
            Some(json!({ "key": " sk-saved " })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, s) = t.call(Method::GET, "/api/settings", None).await;
    assert_eq!(s["opencode_api_key"], json!({ "source": "saved" }));
    assert!(
        !s.to_string().contains("sk-saved"),
        "the key must never be sent back"
    );

    let (status, _) = t
        .call(
            Method::PUT,
            "/api/settings/opencode-api-key",
            Some(json!({ "key": "a b" })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 空で保存したキーを消すと、環境変数に戻る
    t.call(
        Method::PUT,
        "/api/settings/opencode-api-key",
        Some(json!({ "key": "" })),
    )
    .await;
    let (_, s) = t.call(Method::GET, "/api/settings", None).await;
    assert_eq!(s["opencode_api_key"], json!({ "source": "env" }));
}

#[tokio::test]
async fn checking_opencode_without_a_key_explains_why() {
    let t = start(EnvDefaults::default()).await;
    let (status, r) = t
        .call(
            Method::POST,
            "/api/settings/check",
            Some(json!({ "provider": "opencode" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r["ok"], false);
    assert_eq!(r["error"], "API キーが未設定");
}

#[tokio::test]
async fn unknown_api_paths_are_json_404s() {
    let t = start(EnvDefaults::default()).await;
    let (status, body) = t.call(Method::GET, "/api/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");
}

#[tokio::test]
async fn lines_append_and_report_their_takes() {
    let t = start(EnvDefaults::default()).await;
    let (_, p) = t
        .call(Method::POST, "/api/projects", Some(json!({ "title": "t" })))
        .await;
    let (_, scene) = t
        .call(
            Method::POST,
            &format!("/api/projects/{}/scenes", p["id"]),
            Some(json!({ "title": "屋上" })),
        )
        .await;
    let lines = format!("/api/scenes/{}/lines", scene["id"]);
    let (status, a) = t
        .call(Method::POST, &lines, Some(json!({ "text": "一" })))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, b) = t
        .call(Method::POST, &lines, Some(json!({ "text": "二" })))
        .await;
    assert_eq!(a["sort_order"], 0);
    assert_eq!(b["sort_order"], 1);
    assert_eq!(b["take_count"], 0);
    assert_eq!(b["adopted_take_id"], Value::Null);

    // TTS を読み込んでいなければ音声生成は 503
    let (status, _) = t
        .call(
            Method::POST,
            "/api/tts/jobs",
            Some(json!({ "line_ids": [a["id"]] })),
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn memories_and_their_marks_on_a_line() {
    let t = start(EnvDefaults::default()).await;
    let (_, p) = t
        .call(Method::POST, "/api/projects", Some(json!({ "title": "t" })))
        .await;
    let (_, alice) = t
        .call(
            Method::POST,
            &format!("/api/projects/{}/characters", p["id"]),
            Some(json!({ "name": "アリス" })),
        )
        .await;
    let memories = format!("/api/characters/{}/memories", alice["id"]);

    let (status, m) = t
        .call(
            Method::POST,
            &memories,
            Some(json!({ "kind": "recollection", "period": " 小学4年の夏 ", "people": ["ボブ", " "], "text": "花火を見た。" })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(m["period"], "小学4年の夏");
    assert_eq!(m["people"], json!(["ボブ"]));
    assert_eq!(m["embedded"], false);

    let (status, _) = t
        .call(
            Method::POST,
            &memories,
            Some(json!({ "kind": "dream", "text": "x" })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = t
        .call(Method::POST, &memories, Some(json!({ "text": "  " })))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, edited) = t
        .call(
            Method::PUT,
            &format!("/api/memories/{}", m["id"]),
            Some(json!({ "kind": "experience", "text": "花火を見に行った。" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["kind"], "experience");
    assert_eq!(edited["people"], json!([]));

    // セリフに印を付ける。検索していなくても印の付いた記憶は並ぶ
    let (_, scene) = t
        .call(
            Method::POST,
            &format!("/api/projects/{}/scenes", p["id"]),
            Some(json!({ "title": "屋上" })),
        )
        .await;
    let (_, line) = t
        .call(
            Method::POST,
            &format!("/api/scenes/{}/lines", scene["id"]),
            Some(json!({ "character_id": alice["id"], "text": "遅い!" })),
        )
        .await;
    let recall = format!("/api/lines/{}/recall", line["id"]);
    let (status, v) = t.call(Method::GET, &recall, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v, json!({ "recall": null, "items": [] }));

    let mark = format!("/api/lines/{}/memory-marks/{}", line["id"], m["id"]);
    let (status, v) = t
        .call(Method::PUT, &mark, Some(json!({ "mark": "pin" })))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["items"][0]["mark"], "pin");
    assert_eq!(v["items"][0]["selected"], true);
    assert_eq!(v["items"][0]["candidate"], Value::Null);
    assert_eq!(v["items"][0]["memory"]["text"], "花火を見に行った。");

    let (_, v) = t
        .call(Method::PUT, &mark, Some(json!({ "mark": "exclude" })))
        .await;
    assert_eq!(v["items"][0]["selected"], false);
    let (_, v) = t
        .call(Method::PUT, &mark, Some(json!({ "mark": null })))
        .await;
    assert_eq!(v["items"], json!([]));

    // モデルを読み込んでいなければ検索は 503
    let (status, _) = t.call(Method::POST, &recall, Some(json!({}))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (_, status_body) = t.call(Method::GET, "/api/rag/status", None).await;
    assert_eq!(status_body["judge"]["state"], "disabled");

    let (status, _) = t
        .call(Method::DELETE, &format!("/api/memories/{}", m["id"]), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = t.call(Method::GET, &memories, None).await;
    assert_eq!(list, json!([]));
}
