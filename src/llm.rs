//! One LLM call (Claude, or a local ollama model when no key is set): facts + rule candidates in,
//! structured policy advice out, then validated.

use crate::facts::Facts;
use crate::policies::{self, POLICIES};
use crate::rules::Trigger;
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

pub struct Llm {
    pub http: reqwest::Client,
    pub key: Option<String>,   // x-api-key
    pub token: Option<String>, // Authorization: Bearer (OAuth)
    pub model: String,
    pub effort: String,
    pub local_url: String,           // ollama
    pub local_model: Option<String>, // e.g. qwen3:8b; used when no Anthropic credentials
}

#[derive(Deserialize, Clone)]
pub struct LlmRec {
    pub policy_id: String,
    pub priority: String,
    pub why: String,
    pub evidence: Vec<String>,
}

#[derive(Deserialize, Clone)]
pub struct LlmAdvice {
    pub summary: String,
    pub recommendations: Vec<LlmRec>,
    pub unknowns: Vec<String>,
}

const SYSTEM: &str = "You advise a mayor in the cross-border Meuse/Vesdre region (BE/NL/DE) on which flood policies to apply now.
Input: facts computed by the team's flood models for one area, rule-based policy candidates, and the team's closed policy catalogue (from their Policy Brief and their review 'Policies since the flood 2021').

Rules:
- Recommend 3 to 6 policies, most urgent first, only ids from the catalogue.
- Every recommendation cites the fact ids it rests on in `evidence` (ids exactly as given). A catalogue policy with no supporting fact may only be recommended if you say so in `why`.
- Use only numbers that appear in the facts or in the catalogue. Do not compute new numbers (no sums, ratios or percentages of your own). Round only by writing 'about' before the given number.
- Model risk is a ranking, not a probability. Never say an area 'will flood'.
- priority: 'now' = act before or during this event, 'soon' = this season, 'later' = mid/long term.
- `why` is 1-3 plain sentences a mayor understands: the fact, then why this policy answers it, linked to the 2021 lesson when relevant.
- `summary` is 2-3 sentences: the main risk and the first decision to take.
- `unknowns`: what the models do not tell (missing inputs, weak model fit, caveats). Keep it short.
- Write in the language requested.";

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "recommendations", "unknowns"],
        "properties": {
            "summary": {"type": "string"},
            "recommendations": {"type": "array", "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["policy_id", "priority", "why", "evidence"],
                "properties": {
                    "policy_id": {"type": "string", "enum": policies::ids()},
                    "priority": {"type": "string", "enum": ["now", "soon", "later"]},
                    "why": {"type": "string"},
                    "evidence": {"type": "array", "items": {"type": "string"}}
                }
            }},
            "unknowns": {"type": "array", "items": {"type": "string"}}
        }
    })
}

pub fn prompt(facts: &Facts, triggers: &[Trigger], caveats: &[String], area: &str, question: &str, language: &str) -> String {
    json!({
        "area": area,
        "question": if question.is_empty() { "Which policies should the mayor take, in which order?" } else { question },
        "language": language,
        "facts": facts.0,
        "rule_candidates": triggers,
        "model_caveats": caveats,
        "policy_catalogue": POLICIES,
    })
    .to_string()
}

impl Llm {
    fn claude(&self) -> bool {
        self.key.is_some() || self.token.is_some()
    }

    pub fn enabled(&self) -> bool {
        self.claude() || self.local_model.is_some()
    }

    /// Model that answers: Claude when a key is set, else the local model.
    pub fn name(&self) -> Option<String> {
        if self.claude() {
            Some(self.model.clone())
        } else {
            self.local_model.as_ref().map(|m| format!("local:{m}"))
        }
    }

    /// Returns (parsed advice, raw text, usage).
    pub async fn ask(&self, user: &str) -> Result<(LlmAdvice, String, Value)> {
        match (&self.local_model, self.claude()) {
            (Some(m), false) => self.ask_local(m, user).await,
            _ => self.ask_claude(user).await,
        }
    }

    async fn ask_local(&self, model: &str, user: &str) -> Result<(LlmAdvice, String, Value)> {
        let body = json!({
            "model": model,
            "stream": false,
            "think": false,
            "format": schema(),
            "options": {"temperature": 0.2, "num_ctx": 16384},
            "messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}],
        });
        let url = format!("{}/api/chat", self.local_url.trim_end_matches('/'));
        let r = self.http.post(&url).json(&body).send().await.context("local model unreachable")?;
        let status = r.status();
        let v: Value = r.json().await.context("local model: bad JSON")?;
        if !status.is_success() {
            bail!("local model {status}: {}", v["error"]);
        }
        let text = v.pointer("/message/content").and_then(Value::as_str).unwrap_or("").to_string();
        let advice: LlmAdvice = serde_json::from_str(&text).context("local model output does not match the schema")?;
        let usage = json!({
            "input_tokens": v["prompt_eval_count"], "output_tokens": v["eval_count"],
            "seconds": v["total_duration"].as_f64().map(|ns| ns / 1e9),
        });
        Ok((advice, text, usage))
    }

    async fn ask_claude(&self, user: &str) -> Result<(LlmAdvice, String, Value)> {
        let body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "fallbacks": "default",
            "output_config": {"effort": self.effort, "format": {"type": "json_schema", "schema": schema()}},
            "system": SYSTEM,
            "messages": [{"role": "user", "content": user}],
        });
        let mut req = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("anthropic-version", "2026-10-21")
            .header("anthropic-beta", "server-side-fallback-2026-07-01")
            .json(&body);
        req = match (&self.key, &self.token) {
            (Some(k), _) => req.header("x-api-key", k),
            (None, Some(t)) => req.bearer_auth(t),
            _ => bail!("no Anthropic credentials"),
        };
        let r = req.send().await.context("Claude API unreachable")?;
        let status = r.status();
        let v: Value = r.json().await.context("Claude API: bad JSON")?;
        if !status.is_success() {
            let msg = v.pointer("/error/message").and_then(Value::as_str).unwrap_or("");
            bail!("Claude API {status}: {msg}");
        }
        match v["stop_reason"].as_str() {
            Some("refusal") => bail!("Claude declined: {}", v["stop_details"]),
            Some("max_tokens") => bail!("Claude output cut off (max_tokens)"),
            _ => {}
        }
        let text: String = v["content"]
            .as_array()
            .ok_or_else(|| anyhow!("no content"))?
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect();
        let advice: LlmAdvice = serde_json::from_str(&text).context("Claude output does not match the schema")?;
        Ok((advice, text, v["usage"].clone()))
    }
}

/// Numbers written in `s` ("20,458", "3.1", "58%").
fn numbers(s: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<f64>| {
        let t = cur.trim_end_matches(['.', ',']).replace(',', "");
        if let Ok(v) = t.parse::<f64>() {
            out.push(v);
        }
        cur.clear();
    };
    let cs: Vec<char> = s.chars().collect();
    for (i, &c) in cs.iter().enumerate() {
        let next_digit = cs.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        if c.is_ascii_digit() || ((c == '.' || c == ',') && !cur.is_empty() && next_digit) {
            cur.push(c);
        } else if !cur.is_empty() {
            flush(&mut cur, &mut out);
        }
    }
    if !cur.is_empty() {
        flush(&mut cur, &mut out);
    }
    out
}

/// Check the LLM output: evidence ids exist, numbers come from facts or the catalogue.
/// Returns the problems found (empty = OK). Unknown evidence ids are dropped, not fatal.
pub fn validate(a: &mut LlmAdvice, facts: &Facts) -> Vec<String> {
    let mut allowed: Vec<f64> = facts.0.iter().filter_map(|f| f.value.as_f64()).collect();
    for f in &facts.0 {
        if let Some(s) = f.value.as_str() {
            allowed.extend(numbers(s));
        }
        allowed.extend(numbers(&f.text)); // e.g. "1-in-100-year", "≥ 0.3 m"
    }
    for p in POLICIES {
        for s in [p.title, p.what, p.lesson_2021] {
            allowed.extend(numbers(s));
        }
    }
    let ok = |n: f64| n <= 10.0 || allowed.iter().any(|v| (n - v).abs() <= (0.05 * v.abs()).max(0.5));

    let mut problems = Vec::new();
    if a.recommendations.is_empty() {
        problems.push("no recommendations".into());
    }
    let mut seen = Vec::new();
    for r in &mut a.recommendations {
        if seen.contains(&r.policy_id) {
            problems.push(format!("{} recommended twice", r.policy_id));
        }
        seen.push(r.policy_id.clone());
        r.evidence.retain(|e| facts.0.iter().any(|f| &f.id == e));
        for n in numbers(&r.why).into_iter().filter(|n| !ok(*n)) {
            problems.push(format!("{}: number {n} is not in the facts", r.policy_id));
        }
    }
    for n in numbers(&a.summary).into_iter().filter(|n| !ok(*n)) {
        problems.push(format!("summary: number {n} is not in the facts"));
    }
    let lower = format!("{} {}", a.summary, a.recommendations.iter().map(|r| r.why.as_str()).collect::<Vec<_>>().join(" ")).to_lowercase();
    if lower.contains("will flood") {
        problems.push("says an area 'will flood'".into());
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::Fact;

    fn facts() -> Facts {
        Facts(vec![Fact { id: "flood.rp100.people_leave_now".into(), text: "people who must leave now".into(), value: 9041.0.into() }])
    }
    fn advice(why: &str, ev: &str) -> LlmAdvice {
        LlmAdvice {
            summary: "About 9,041 people must leave.".into(),
            recommendations: vec![LlmRec { policy_id: "ST-ALERT".into(), priority: "now".into(), why: why.into(), evidence: vec![ev.into()] }],
            unknowns: vec![],
        }
    }

    #[test]
    fn parses_numbers() {
        assert_eq!(numbers("20,458 people, 3.1 h, 58%. Then 2021."), vec![20458.0, 3.1, 58.0, 2021.0]);
    }

    #[test]
    fn accepts_fact_and_catalogue_numbers() {
        let mut a = advice("9,041 people must leave; in 2021 only 17.5% got a warning.", "flood.rp100.people_leave_now");
        assert!(validate(&mut a, &facts()).is_empty());
    }

    #[test]
    fn rejects_invented_numbers_and_drops_bad_evidence() {
        let mut a = advice("About 30,000 people are at risk.", "made.up");
        let p = validate(&mut a, &facts());
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(a.recommendations[0].evidence.is_empty());
    }
}
