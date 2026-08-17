# Model selection

How the benchmark's models are chosen, and why the six below make up the
slate. Companion to the dataset-selection docs — a neutral reference for the
inputs to the choice, not a ranking.

Current as of August 2026. Model availability in this tier turns over fast, so
treat every name here as needing confirmation against provider docs.

## What we are selecting for

Not "the best models". The benchmark measures how much harder a question gets
across SQL, Cypher, and TypeQL, so a model is useful here when it lands
somewhere informative on that gradient.

- **Spread, not rank.** A model that scores 0/34 on TypeQL tells us nothing,
  and neither does one at 34/34. Opus already sits near the ceiling, which is
  what prompted looking at lower tiers.
- **Genuine agentic deployment.** The tier of interest is what teams actually
  run in agent pipelines, not what tops a leaderboard.
- **TypeQL is the discriminator.** SQL and Cypher are well represented in
  pretraining; TypeQL is not. Success there comes from reading the supplied
  schema rather than recalling idiom, so code-specialised models are not
  automatically the strongest candidates.
- **Format compliance is scored.** Models that ignore the fenced-output
  instruction produce a distinguishable failure mode (the harness tolerates
  unfenced queries, but the tolerance is not free).

## Constraints from the harness

| Constraint | Detail |
| --- | --- |
| Prompt size | ~37k tokens; the Neo4j schema alone is ~23k |
| Context served | Model windows are now 256K–1M, so the binding risk is a **provider or serving config that silently truncates** — not the model |
| Output budget | `max_tokens` defaults to 4096; reasoning-heavy models need far more or they never reach the query |
| Cost | A full sweep is tens of dollars. **Not a deciding factor** — choose on reproducibility |

The truncation risk is not hypothetical: an early local Qwen run returned
UNANSWERABLE for every question because Ollama capped the prompt at ~2050
tokens. That looked like a capability result and was a config bug. Every
endpoint needs a canary asking for something only visible at the start of the
schema.

## Landscape considered

| Candidate | Category | Note |
| --- | --- | --- |
| Claude Haiku 4.5 | closed, cheap | No new integration; ladders against existing Opus/Sonnet runs |
| Gemini Flash / Flash-Lite | closed, cheap | Cross-vendor variance |
| GPT-5-mini tier | closed, cheap | Naming least certain |
| GLM-5.2 | open, MIT | 753B MoE / ~40B active, 1M context; reportedly leading open-weights |
| Kimi K2.7-Code | open, mod. MIT | 1T / 32B active, 256K; thinking mode enforced |
| Kimi K3 | open | 2.8T MoE, 1M context, multimodal; frontier-tier, ~10x the price of the rest, reasoning not disableable — duplicates GLM-5.2's role |
| DeepSeek V4-Flash | open, MIT | 284B / 13B active, 1M context; agent-tuned GA build (`0731`) |
| Qwen3-Coder-Next | open | 80B / 3B active, 256K; coding-agent tuned |
| gpt-oss-120b | open, Apache-2.0 | Trivial to self-host |
| Devstral | open | Agentic SWE, narrower reasoning |
| Muse Glimmer 30B | open, Apache-2.0 | Dense 30B (all active), ~131k context, agentic; local-only, no hosted API |

Two findings changed the shape of the decision:

1. **Open-weights and non-frontier have come apart.** GLM-5.2, Kimi K2.7-Code
   and DeepSeek V4-Pro are frontier-adjacent. Picking an open model no longer
   implies picking a weaker one, so the models likely to produce a useful
   failure rate are the ones with small *active* parameter counts.
2. **Hosting removed Qwen's structural advantage.** Qwen was initially favoured
   because a local Ollama path already existed. Once the decision moved to
   hosted APIs, every open-weights candidate costs the same and needs the same
   zero integration work, so Qwen competes on merit alone.

## The slate

| Model | Role |
| --- | --- |
| Claude Haiku 4.5 | Closed cheap tier; free to wire, anchors against Opus/Sonnet |
| GLM-5.2 | Open flagship; expected upper anchor |
| DeepSeek V4-Flash | Mid-tier open, agent-tuned |
| Qwen3-Coder-Next | 3B active; expected lower anchor |
| Kimi K2.7-Code | Agentic generalist rather than code specialist |
| Muse Glimmer 30B | The only dense model; separates per-token compute from language coverage |

A model earns its place by landing at a genuinely different point on the
SQL → Cypher → TypeQL gradient. One that floors at zero carries no signal, and
one that clusters with another adds no spread. The roles above are
expectations to be tested by results, not conclusions.

## Operational notes

- **Labels carry the provider** (`glm-5.2@z.ai`). The same open weights served
  by two providers can differ in quantization and served context, so the model
  name alone does not identify a run.
- **`max_tokens` is set per model** — 32k for GLM-5.2 and Kimi (thinking
  enforced), 16k for DeepSeek and Muse Glimmer, 8k for Haiku and Qwen. At the 4096 default a
  reasoning model exhausts its budget mid-thought and never emits a query,
  which scores as a capability failure but is a configuration one.
- **Retries multiply cost unevenly.** They add calls on exactly the models
  that fail most, so a cost-capped comparison run is cheapest with a low
  `maxRetryCounts`; raise it for a full run.
- **The local entry is weaker evidence.** Muse Glimmer has no hosted API, so
  it runs quantized through Ollama — a different artifact from the FP8-ish
  hosted models, and exposed to silent prompt truncation unless
  `OLLAMA_CONTEXT_LENGTH` is raised well above the ~37k prompt. Read its
  numbers as indicative.
- **Avoid aggregator routing** (e.g. OpenRouter) for the real run: it routes
  across sub-providers with differing quantization and context caps, so runs
  are not reproducible unless the upstream is pinned.
