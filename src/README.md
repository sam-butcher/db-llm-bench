# Benchmarking framework

This contains the framework for running LLM benchmarks against various DBs

Prior steps:
- Launch DBs
- Load data

Inputs (taken in through config file - see config.yml for a suggested format):
- List of DBs to hit, each of which contains:
  - ID of DB (e.g. sql/neo4j/typedb)
  - Path to the prompt folder for that DB (contained in a text file)
    - The template file is `prompt.txt`; examples are `example-1.txt`, `example-2.txt`, ... (contiguous from 1)
    - Prompt is expected to be a template
    - Contains template slot for the question
    - Contains template slot for the schema
    - Contains template slot for any examples
    - Contains template slot for skills (filled with the loaded `.md` skill files, empty when running
      with skills off)
    - Slot syntax is `{{question}}`, `{{schema}}`, `{{examples}}`, and `{{skills}}`
    - The examples slot fills with an `Examples:` heading followed by the examples - or with nothing at
      all when running with zero examples, so the heading never dangles. Templates should not add their
      own heading
    - Must instruct the model to respond with exactly one fenced code block containing only the query,
      or the literal token `UNANSWERABLE` alone on its own line if it believes the question cannot be
      answered against the schema (the own-line rule stops prose that merely mentions the token from
      reading as a decline)
  - URL of DB
  - Database name, for DBs that namespace by database (e.g. TypeDB, Neo4j)
  - Whatever auth info is required for the DB
  - Optional skills path - any `.md` files in this folder will be loaded as skills when generating queries
  - Schema path to the schema description of the data for that DB
- List of models to use
  - ID of model (e.g. claude/llama/chatgpt)
    - Determines how it's queried
    - The same provider may appear multiple times (e.g. several `claude` entries benchmarking
      different models or settings); each entry's records are identified by its label, which
      defaults to the model name and must be unique across entries
  - Whatever info is required to query it
    - Will vary per provider
    - e.g. a local model might just require a url
    - claude will require auth info, model name, and thinking level
    - the `openai-compatible` provider covers any endpoint speaking the OpenAI chat-completions
      API (OpenAI itself, Groq, OpenRouter, local Ollama/vLLM): it takes a `base_url`, an optional
      `api_key_env` naming the environment variable holding the key (omitted entirely for
      unauthenticated local servers), and a `max_tokens_field` knob for OpenAI's newer models,
      which require `max_completion_tokens` instead of the widely cloned `max_tokens`
- Questions path
  - Path to the list of questions
  - List of questions will be a JSON file containing a list of questions
  - each question also has a difficulty level, correct query in each language (under a `queries` map,
    keyed by language), and expected result
  - a question may set `"ordered": true` when the order of a top-level list result is part of
    correctness (i.e. the question demands an ordering); the default is unordered, comparing rows as a
    bag. Nested lists always compare ordered, as tuples
  - when `expected` is an object (or a list of row objects), its keys define the answer's field names:
    the runner appends a standardized naming instruction to the prompt ("Name the output fields
    exactly: ...") so field naming is a uniform, explicit part of the task, with `expected` as the
    single source of truth. The Neo4j package strips plain property-access prefixes (`c.brand` ->
    `brand`) so unaliased Cypher isn't unfairly penalised; TypeDB may answer via row variables or
    `fetch` documents — both produce keyed objects
  - We may augment each question with its expected return type (e.g. numeric vs object), so the framework knows how to compare results for that question
- Example counts
  - List of numbers indicating what level of examples we should test with
  - We expect the prompt folder to optionally contain a list of example queries (as separate files, e.g. example-1.txt, example-2.txt)
  - We'd re-run the tests for each example count, inserting that many of the examples into the prompt template
- maxRetryCounts
  - List of numbers indicating what level of max retry counts we should test with
  - This indicates how many times we allow the query to outright error, with the error being returned to the LLM for iteration, before marking a failure
  - An error is anything that can't possibly be correct - incorrect result shape, syntax error, timeout - but not empty results
  - Infrastructure errors (DB connection failures, provider rate limits) are not the model's fault and
    don't count as retries - the harness retries those itself with backoff, without involving the model
  - On retry, the LLM receives the prior conversation plus the error message
  - Runs only execute at the highest configured retry count; results for the lower counts are derived from the attempt trace rather than re-run

Before any benchmarking begins, the full configuration is validated up-front, so a bad combination
fails immediately rather than partway through a run: every provider and DB client is built, all
prompt assets load, each DB has at least as many example files as the highest example count,
prompt templates contain the required slots ({{question}} and {{schema}} always; {{examples}} when
examples are configured; {{skills}} when a skills folder is), skills folders are non-empty, and
every question has a ground-truth query for each configured DB's language.

With these inputs the program will do the following for each DB/example count/skills on-off/model combination
(max retry count is not part of the combination - see below)
- Iterate through the questions, running each 3 times (repetitions) to account for LLM non-determinism
- Send the question, wrapped in the prompt (template with the appropriate number of examples), to the chosen model
- Extract the query from the response (the last fenced code block; a block wins over an `UNANSWERABLE`
  token when both appear, and an unterminated trailing block still counts, so truncated responses yield
  their partial query and a real execution error rather than "no query found"), and use it to query the
  database
  - An explicit `UNANSWERABLE` response is terminal (never retried) and scored as a failure for answerable
    questions - and as correct if we later add deliberately-unanswerable questions
  - A response with neither a code block nor the `UNANSWERABLE` marker is malformed, and counts as a
    retryable error ("no query found in response") - safe because declining has an explicit channel,
    so retrying doesn't pressure the model into hallucinating a query
- On error, return the prior conversation plus the error message to the model and retry, up to the highest
  configured max retry count; results for lower retry counts are derived from the attempt trace
- Compare the result to the expected result from the question
  - Comparison depends on the question - e.g. "how many cars are there" compares a number, others may compare full objects
  - List results compare as bags unless the question sets `ordered: true`; floats compare exactly (no
    tolerance, revisit if cross-DB aggregation disagrees)
  - Individual DB packages handle result type coercion on a per-DB basis

At the end, it will produce a file containing the list of questions along with their generated queries,
and whether each query failed. Results are keyed by DB ID (not query language, so two DBs sharing a
language don't collide). Each result record includes the model used, the number of retries actually
used, and the full attempt trace - with per-attempt token usage and latency, so lower retry levels can
be derived by cutting the trace. Latency covers model + DB work only; harness backoff waits are
excluded. If the run aborts on an unrecoverable infrastructure or provider failure, everything
completed up to that point is still written.

```json
{
  "questions": [
    {
      "question": "How many cars are there?",
      "difficulty": "easy",
      "expected": 3,
      "dbs": {
        "typedb": {
          "language": "typeql",
          "correct": "match $x isa car; reduce $count = count;",
          "results": [
            {
              "model": "claude-opus-4.8",
              "maxRetries": 0,
              "retriesUsed": 0,
              "examples": 0,
              "skills": false,
              "repetition": 1,
              "generated": "match $x isa car; reduce $count = count",
              "attempts": [
                {
                  "query": "match $x isa car; count",
                  "tokens": { "input": 1200, "output": 34 },
                  "latencyMs": 900,
                  "error": "syntax error: ..."
                }
              ],
              "tokens": { "input": 1200, "output": 34 },
              "latencyMs": 900,
              "result": "error",
              "accurate": false
            }
          ]
        },
        "sql": {
          "language": "sql",
          "correct": "SELECT COUNT(*) FROM Cars;",
          "results": [
            {
              "model": "claude-opus-4.8",
              "maxRetries": 0,
              "retriesUsed": 0,
              "examples": 0,
              "skills": false,
              "repetition": 1,
              "generated": "SELECT COUNT(*) FROM Cars;",
              "attempts": [
                {
                  "query": "SELECT COUNT(*) FROM Cars;",
                  "tokens": { "input": 1150, "output": 30 },
                  "latencyMs": 850,
                  "error": null
                }
              ],
              "tokens": { "input": 1150, "output": 30 },
              "latencyMs": 850,
              "result": 3,
              "accurate": true
            }
          ]
        }
      }
    }
  ]
}
```

Framework language: Rust
- The result coercion/comparison layer is the deciding factor: a concrete canonical value enum with
  exhaustive matching means every driver-type mapping and cross-type equality rule is an explicit,
  compiler-enforced decision - once the comparison rules are settled, we can actually be confident in them
  (in Python, wrong coercions like `3 == 3.0 == True` pass silently)
- The official `typedb-driver` crate is the source driver that other language drivers wrap; SQL is well
  served by `sqlx`/`rusqlite`; Neo4j uses the community `neo4rs` crate
- LLM provider clients will be small hand-rolled `reqwest` clients (no official Anthropic/OpenAI Rust
  SDKs) - the needed surface is one POST endpoint per provider, which also keeps the
  local-model-behind-a-URL case uniform
- The per-DB and per-provider packages become workspace crates behind shared traits
  (`sendQuery`/`sendPrompt`)

Expected domain separations:
- Config file processor
- DB packages
  - Should be extensible for adding other DBs later -> each valid ID gets its own package to make it easy to add new packages
  - Each package should expose some unified endpoint like `sendQuery(query: string)` that returns some consistent value type
  - Each DB ID maps to the query language that DB uses (typedb -> typeql, neo4j -> cypher, sql -> sql;
    "sql" is used generically since the specific engine doesn't matter)
  - Each package handles result type coercion for its DB; we could expose typed endpoints
    (e.g. `runNumericQuery`) keyed off a question's return type
  - Each package is responsible for query safety: read-only transactions and query timeouts where viable
  - This way extension is localised
- Model provider packages
  - Should be extensible for adding other models later -> each valid ID gets its own package to make it easy to add new packages
  - Ultimate exposes a `sendPrompt(prompt: string)` endpoint
- Actual benchmark runner
  - Build one benchmark runner per setup
  - So it takes the DB + model provider + max retry/example counts and runs the questions
- Output marshalling

Assumptions & non-goals (for now):
- The framework assumes the provided questions, correct queries, and expected results are valid - no
  upfront ground-truth validation
- The contents of the example queries are a separate concern to the framework itself
- No rate limiting, parallelism, or crash resumability yet - we'll be doing our best to keep the
  combinatorial space small
