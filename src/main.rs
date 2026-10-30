//! Policy advisor API: takes the flood model results the frontend already has and answers
//! which of the team's policies the mayor should take, in which order (Claude or a local model,
//! with a rule-based fallback).

mod facts;
mod llm;
mod policies;
mod rules;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncWriteExt, sync::{Mutex, Semaphore}};
use tower_http::cors::{Any, CorsLayer};

const MODEL_KEYS: &[&str] = &["flood", "evacuation", "timeline", "region", "impact", "models"];

struct App {
    llm: llm::Llm,
    out: PathBuf,
    slots: Semaphore,
    cache: Mutex<HashMap<u64, Value>>,
    jobs: Mutex<HashMap<u64, Job>>,
    next_job: AtomicU64,
}

enum Job {
    Running(Instant),
    Done(StatusCode, Value),
}

#[derive(Deserialize)]
struct AdviseReq {
    #[serde(default)]
    area: String,
    #[serde(default)]
    question: String,
    #[serde(default)]
    language: String,
    /// "claude" | "local" | "rules"; empty = server default. The Claude key, if the frontend
    /// supplies one, comes in the `x-anthropic-key` header (never in the body, never logged).
    #[serde(default)]
    provider: String,
    /// flood_api responses keyed by endpoint: flood, evacuation, timeline, region, impact, models.
    /// They may also be sent at the top level.
    #[serde(default)]
    models: Map<String, Value>,
    #[serde(flatten)]
    rest: Map<String, Value>,
}

#[derive(Deserialize)]
struct AdviseQuery {
    llm: Option<u8>,
    #[serde(rename = "async")]
    is_async: Option<u8>,
}

fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(json!({"error": msg.into()}))).into_response()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let addr = env("POLICY_ADDR").unwrap_or_else(|| "127.0.0.1:8082".into());
    let out = PathBuf::from(env("POLICY_OUT").unwrap_or_else(|| "out".into()));
    tokio::fs::create_dir_all(&out).await?;
    let app = Arc::new(App {
        llm: llm::Llm {
            http: reqwest::Client::builder().timeout(Duration::from_secs(env("POLICY_TIMEOUT_S").and_then(|v| v.parse().ok()).unwrap_or(900))).build()?,
            key: env("ANTHROPIC_API_KEY"),
            token: env("ANTHROPIC_AUTH_TOKEN"),
            model: env("POLICY_MODEL").unwrap_or_else(|| "claude-opus-5-5".into()),
            effort: env("POLICY_EFFORT").unwrap_or_else(|| "medium".into()),
            local_url: env("POLICY_LOCAL_URL").unwrap_or_else(|| "http://127.0.0.1:11434".into()),
            local_model: env("POLICY_LOCAL_MODEL"),
            local_threads: env("POLICY_LOCAL_THREADS").and_then(|v| v.parse().ok()).unwrap_or(6),
        },
        out,
        slots: Semaphore::new(2),
        cache: Mutex::new(HashMap::new()),
        jobs: Mutex::new(HashMap::new()),
        next_job: AtomicU64::new(1),
    });
    let cors = match env("POLICY_CORS_ORIGINS") {
        Some(list) => CorsLayer::new()
            .allow_origin(list.split(',').filter_map(|o| HeaderValue::from_str(o.trim()).ok()).collect::<Vec<_>>())
            .allow_methods(Any)
            .allow_headers(Any),
        None => CorsLayer::permissive(),
    };
    let router = Router::new()
        .route("/health", get(health))
        // git commit of this build (deploy sets GIT_SHA), checked by CI after a deploy
        .route("/version", get(|| async { option_env!("GIT_SHA").unwrap_or("dev") }))
        .route("/policies", get(|| async { Json(json!({"policies": policies::POLICIES})) }))
        .route("/advise", post(advise))
        .route("/jobs/:id", get(job))
        .layer(cors)
        .with_state(app.clone());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!(
        "policy_api on http://{addr} (claude key on server: {}, local: {})",
        app.llm.server_claude(),
        app.llm.local().unwrap_or("none")
    );
    axum::serve(listener, router).await?;
    Ok(())
}

async fn health(State(app): State<Arc<App>>) -> Json<Value> {
    let default = match app.llm.pick("", None) {
        Ok(Some(p)) => app.llm.label(&p),
        _ => "rules".into(),
    };
    Json(json!({
        "ok": true,
        "providers": {
            "claude": {"model": app.llm.model, "server_key": app.llm.server_claude(), "user_key_header": "x-anthropic-key"},
            "local": app.llm.local(),
            "rules": true,
        },
        "default": default,
        "policies": policies::POLICIES.len(),
    }))
}

async fn advise(State(app): State<Arc<App>>, Query(q): Query<AdviseQuery>, headers: HeaderMap, Json(req): Json<AdviseReq>) -> Response {
    let user_key = headers
        .get("x-anthropic-key")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let want = if q.llm == Some(0) { "rules".to_string() } else { req.provider.trim().to_lowercase() };
    let provider = match app.llm.pick(&want, user_key) {
        Ok(p) => p,
        Err(e) => return err(StatusCode::BAD_REQUEST, e),
    };
    if q.is_async != Some(1) {
        let (code, body) = run_advice(&app, req, provider).await;
        return (code, Json(body)).into_response();
    }
    let id = app.next_job.fetch_add(1, Ordering::Relaxed);
    {
        let mut jobs = app.jobs.lock().await;
        // drop finished jobs beyond the last 100
        if jobs.len() > 100 {
            let mut done: Vec<u64> = jobs.iter().filter(|(_, j)| matches!(j, Job::Done(..))).map(|(k, _)| *k).collect();
            done.sort();
            for k in done.iter().take(done.len().saturating_sub(50)) {
                jobs.remove(k);
            }
        }
        jobs.insert(id, Job::Running(Instant::now()));
    }
    let a = app.clone();
    tokio::spawn(async move {
        let (code, body) = run_advice(&a, req, provider).await;
        a.jobs.lock().await.insert(id, Job::Done(code, body));
    });
    (StatusCode::ACCEPTED, Json(json!({"job": id, "status": "running", "poll": format!("/jobs/{id}")}))).into_response()
}

async fn job(State(app): State<Arc<App>>, Path(id): Path<u64>) -> Response {
    match app.jobs.lock().await.get(&id) {
        None => err(StatusCode::NOT_FOUND, "no such job"),
        Some(Job::Running(t)) => (StatusCode::ACCEPTED, Json(json!({"job": id, "status": "running", "seconds": t.elapsed().as_secs()}))).into_response(),
        Some(Job::Done(code, body)) => (*code, Json(body.clone())).into_response(),
    }
}

async fn run_advice(app: &App, req: AdviseReq, provider: Option<llm::Provider>) -> (StatusCode, Value) {
    let mut models = req.models;
    for k in MODEL_KEYS {
        if let Some(v) = req.rest.get(*k) {
            models.entry(k.to_string()).or_insert_with(|| v.clone());
        }
    }
    models.retain(|k, v| MODEL_KEYS.contains(&k.as_str()) && v.is_object());
    if models.is_empty() {
        return (StatusCode::UNPROCESSABLE_ENTITY, json!({"error": format!("send at least one model result under `models`: {}", MODEL_KEYS.join(", "))}));
    }
    let language = if req.language.is_empty() { "English".to_string() } else { req.language };

    let facts = facts::extract(&models);
    let triggers = rules::run(&facts);
    let caveats = facts::caveats(&models);
    if facts.0.is_empty() {
        return (StatusCode::UNPROCESSABLE_ENTITY, json!({"error": "no usable numbers found in the model results (expected flood_api response shapes)"}));
    }

    let label = provider.as_ref().map(|p| app.llm.label(p));
    let user = llm::prompt(&facts, &triggers, &caveats, &req.area, &req.question, &language);
    let key = {
        let mut h = DefaultHasher::new();
        (&label, &user).hash(&mut h);
        h.finish()
    };
    if let Some(v) = app.cache.lock().await.get(&key) {
        let mut v = v.clone();
        v["cached"] = true.into();
        return (StatusCode::OK, v);
    }

    let mut ai_rejected: Option<Value> = None;
    let mut body = None;
    if let (Some(p), Some(label)) = (&provider, &label) {
        let _slot = app.slots.acquire().await;
        let t0 = Instant::now();
        let res = app.llm.ask(p, &user).await;
        let secs = t0.elapsed().as_secs_f64();
        let (log, b) = match res {
            Ok((mut advice, raw, usage)) => {
                let problems = llm::validate(&mut advice, &facts);
                let log = json!({"raw": raw, "usage": usage, "problems": problems, "seconds": secs});
                if problems.is_empty() {
                    (log, Some(from_llm(&advice, &facts, label)))
                } else {
                    ai_rejected = Some(json!({"reason": "validation failed", "problems": problems}));
                    (log, None)
                }
            }
            Err(e) => {
                ai_rejected = Some(json!({"reason": format!("{e:#}")}));
                (json!({"error": format!("{e:#}"), "seconds": secs}), None)
            }
        };
        body = b;
        write_log(app, json!({"area": req.area, "model": label, "prompt": user, "result": log})).await;
    }
    let mut body = body.unwrap_or_else(|| from_rules(&triggers, &facts));
    body["ai_rejected"] = ai_rejected.unwrap_or(Value::Null);
    body["facts"] = json!(facts.0);
    body["rules_triggered"] = json!(triggers);
    body["model_caveats"] = json!(caveats);
    body["inputs"] = json!(models.keys().collect::<Vec<_>>());
    body["disclaimer"] = "Decision support only. The models rank risk; they do not give flood probabilities. The mayor stays accountable for each decision.".into();
    if body["ai_rejected"].is_null() {
        let mut c = app.cache.lock().await;
        if c.len() > 200 {
            c.clear();
        }
        c.insert(key, body.clone());
    }
    (StatusCode::OK, body)
}

fn rec_json(rank: usize, id: &str, priority: &str, why: &str, evidence: &[String], facts: &facts::Facts) -> Value {
    let p = policies::get(id).expect("validated policy id");
    let ev: Vec<Value> = evidence
        .iter()
        .filter_map(|e| facts.0.iter().find(|f| &f.id == e))
        .map(|f| json!({"id": f.id, "text": f.text, "value": f.value}))
        .collect();
    json!({
        "rank": rank, "policy_id": id, "title": p.title, "horizon": p.horizon, "pillar": p.pillar,
        "priority": priority, "why": why, "evidence": ev, "what": p.what, "lesson_2021": p.lesson_2021, "source": p.source,
    })
}

fn from_llm(a: &llm::LlmAdvice, facts: &facts::Facts, model: &str) -> Value {
    json!({
        "source": if model.starts_with("local:") { "local" } else { "claude" },
        "model": model,
        "summary": a.summary,
        "recommendations": a.recommendations.iter().enumerate()
            .map(|(i, r)| rec_json(i + 1, &r.policy_id, &r.priority, &r.why, &r.evidence, facts)).collect::<Vec<_>>(),
        "unknowns": a.unknowns,
    })
}

fn from_rules(t: &[rules::Trigger], facts: &facts::Facts) -> Value {
    let recs: Vec<Value> = t
        .iter()
        .take(6)
        .enumerate()
        .map(|(i, x)| {
            let p = policies::get(x.policy_id).expect("rules only emit catalogue ids");
            let priority = if p.horizon == "mid_long" { "later" } else if x.score >= 3.0 { "now" } else { "soon" };
            let why = format!("{}.", x.reasons.join("; "));
            rec_json(i + 1, x.policy_id, priority, &why, &x.evidence, facts)
        })
        .collect();
    let summary = match t.first().and_then(|x| policies::get(x.policy_id)) {
        Some(p) => format!("Rule-based advice. First decision: {} ({}).", p.title, t[0].reasons[0]),
        None => "The model results show no exposure that triggers a policy rule.".into(),
    };
    json!({"source": "rules", "model": null, "summary": summary, "recommendations": recs, "unknowns": []})
}

async fn write_log(app: &App, mut entry: Value) {
    entry["ts"] = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).into();
    let path = app.out.join("ai_log.jsonl");
    if let Ok(mut f) = tokio::fs::OpenOptions::new().create(true).append(true).open(&path).await {
        let _ = f.write_all(format!("{entry}\n").as_bytes()).await;
    }
}
