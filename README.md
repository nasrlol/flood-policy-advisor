# flood-policy-advisor

A small Rust API. It takes the flood model results that the frontend already has from
[`flood_api`](https://github.com/nasrlol/eu-ai-challange), such as `/flood`, `/evacuation`, `/timeline`, `/region`, `/impact` and `/models`.
It answers which of the team's policies the mayor should take, and in which order.

The policies are a closed catalogue (`src/policies.rs`). It is built from the team's **Policy Brief** and **Policies since the flood 2021**.

## How it answers
1. **Facts**: model responses are flattened into facts with ids (`src/facts.rs`), e.g. `flood.rp100.people_leave_now = 9041`.
2. **Rules**: thresholds on those facts trigger candidate policies, with reasons (`src/rules.rs`).
3. **LLM** picks 3–6 catalogue policies, orders them and explains them in plain words. Each one cites fact ids (`src/llm.rs`).
   - Default is Claude (`claude-opus-5-5`, structured JSON output).
   - Without a key it uses a local ollama model (`POLICY_LOCAL_MODEL`, JSON schema enforced by ollama).
4. **Validator**: policy ids come from an enum in the schema. Evidence ids must exist. Every number in the text must be a fact or appear in the catalogue. The text may not say an area "will flood".
   - If validation fails, or there is no key, the API returns the rule-based answer. `ai_rejected` gives the reason.
5. Each LLM call (prompt, raw output, validation) is logged to `out/ai_log.jsonl`. This log is for the AI sheet.

The LLM never makes up numbers or policies. It only selects, orders and phrases them.

## API
| Route | What |
|---|---|
| `GET /health` | `{ok, llm, model, policies}` |
| `GET /policies` | the catalogue |
| `POST /advise` | advice; `?async=1` returns `{job}` at once; `?llm=0` forces rules only |
| `GET /jobs/{id}` | 202 while running, then the advice |

```json
POST /advise
{
  "area": "study bbox 5.6,50.55,5.96,50.64",
  "question": "optional",
  "language": "English",
  "provider": "claude | local | rules (optional, default: claude if a key is known, else local, else rules)",
  "models": { "flood": {...}, "evacuation": {...}, "region": {...}, "impact": {...}, "models": {...} }
}
```
- The frontend may send a Claude key in the header `x-anthropic-key`. It is used for that request only and is never stored or logged. It overrides the server key.
- Send any subset of the flood_api responses, unchanged. They may also sit at the top level instead of under `models`.

Response fields:
- `source` (`claude` | `rules`), `summary`
- `recommendations[]` with `{rank, policy_id, title, priority: now|soon|later, why, evidence[], what, lesson_2021, source}`
- `unknowns`, `ai_rejected`, `facts`, `rules_triggered`, `model_caveats`, `disclaimer`

Identical requests are cached in memory. At most 2 LLM calls run at once.

## Run
```sh
cargo build --release
ANTHROPIC_API_KEY=... POLICY_ADDR=127.0.0.1:8082 ./target/release/policy_api
```

| Env | Default |
|---|---|
| `POLICY_ADDR` | `127.0.0.1:8082` |
| `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` | none: rules only |
| `POLICY_MODEL` | `claude-opus-5-5` |
| `POLICY_EFFORT` | `medium` |
| `POLICY_OUT` | `out` (LLM log) |
| `POLICY_LOCAL_MODEL` | none (e.g. `qwen3:8b` via ollama) |
| `POLICY_LOCAL_URL` | `http://127.0.0.1:11434` |
| `POLICY_LOCAL_THREADS` | 6 |
| `POLICY_TIMEOUT_S` | 900 (LLM call timeout) |
| `POLICY_CORS_ORIGINS` | permissive |

- Refused requests use the server-side fallback (`fallbacks: "default"`).
- The local model on a CPU-only 8-vCPU VM is slow: qwen3:8b reads the prompt at ~9 tok/s and writes at ~4 tok/s, so one answer takes over 15 min. Use Claude for interactive use.

## Hosting (team server, tailnet only)
- Port 8082, systemd `policy-api` (`deploy/policy-api.service`).
- `POLICY_ADDR` and the key go in `/etc/policy-api.env` (root, mode 600), never in git.
- Update: `cd ~/policy_api && git pull && cargo build --release && sudo systemctl restart policy-api`.
