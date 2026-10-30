# flood-policy-advisor

A small Rust API. It takes the flood model results that the frontend already has from
[`flood_api`](https://github.com/nasrlol/eu-ai-challange), such as `/flood`, `/evacuation`, `/timeline`, `/region`, `/impact` and `/models`.
It answers which of the team's policies the mayor should take, and in which order.

The policies are a closed catalogue (`src/policies.rs`). It is built from the team's **Policy Brief** and **Policies since the flood 2021**.

## How it answers
1. **Facts**: model responses are flattened into facts with ids (`src/facts.rs`), e.g. `flood.rp100.people_leave_now = 9041`.
2. **Rules**: thresholds on those facts trigger candidate policies, with reasons (`src/rules.rs`).
3. **Claude** (`claude-opus-5-5`, structured JSON output) picks 3–6 catalogue policies, orders them and explains them in plain words. Each one cites fact ids (`src/llm.rs`).
4. **Validator**: policy ids come from an enum in the schema. Evidence ids must exist. Every number in the text must be a fact or appear in the catalogue. The text may not say an area "will flood".
   - If validation fails, or there is no key, the API returns the rule-based answer. `ai_rejected` gives the reason.
5. Each LLM call (prompt, raw output, validation) is logged to `out/ai_log.jsonl`. This log is for the AI sheet.

The LLM never makes up numbers or policies. It only selects, orders and phrases them.

## API
| Route | What |
|---|---|
| `GET /health` | `{ok, llm, model, policies}` |
| `GET /policies` | the catalogue |
| `POST /advise` | advice; `?llm=0` forces rules only |

```json
POST /advise
{
  "area": "study bbox 5.6,50.55,5.96,50.64",
  "question": "optional",
  "language": "English",
  "models": { "flood": {...}, "evacuation": {...}, "region": {...}, "impact": {...}, "models": {...} }
}
```
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
| `POLICY_CORS_ORIGINS` | permissive |

- Refused requests use the server-side fallback (`fallbacks: "default"`).

## Hosting (team server, tailnet only)
- Port 8082, systemd `policy-api` (`deploy/policy-api.service`).
- `POLICY_ADDR` and the key go in `/etc/policy-api.env` (root, mode 600), never in git.
- Update: `cd ~/policy_api && git pull && cargo build --release && sudo systemctl restart policy-api`.
