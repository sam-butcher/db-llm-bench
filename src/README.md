# Benchmarking framework

This contains the framework for running LLM benchmarks against various DBs

Prior steps:
- Launch DBs
- Load data

Inputs (taken in through config file - see config.yml for a suggested format):
- List of DBs to hit, each of which contains:
  - ID of DB (e.g. sql/neo4j/typedb)
  - Path to the prompt folder for that DB (contained in a text file)
    - Prompt is expected to be a template
    - Contains template slot for the question
    - Contains template slot for the schema
    - Contains template slot for any examples
    - Slot syntax is `{{question}}`, `{{schema}}`, and `{{examples}}`
    - Must instruct the model to respond with exactly one fenced code block containing only the query,
      or the literal token `UNANSWERABLE` if it believes the question cannot be answered against the schema
  - URL of DB
  - Whatever auth info is required for the DB
  - Optional skills path - any `.md` files in this folder will be loaded as skills when generating queries
  - Schema path to the schema description of the data for that DB
- List of models to use
  - ID of model (e.g. claude/llama/chatgpt)
    - Determines how it's queried
  - Whatever info is required to query it
    - Will vary per provider
    - e.g. a local model might just require a url
    - claude will require auth info, model name, and thinking level
- Questions path
  - Path to the list of questions
  - List of questions will be a JSON file containing a list of questions
  - each question also has a difficulty level, correct query in each language, and expected result
  - We may augment each question with its expected return type (e.g. numeric vs object), so the framework knows how to compare results for that question
- Example counts
  - List of numbers indicating what level of examples we should test with
  - We expect the prompt folder to optionally contain a list of example queries (as separate files, e.g. example-1.txt, example-2.txt)
  - We'd re-run the tests for each example count, inserting that many of the examples into the prompt template
- maxRetryCounts
  - List of numbers indicating what level of max retry counts we should test with
  - This indicates how many times we allow the query to outright error, with the error being returned to the LLM for iteration, before marking a failure
  - An error is anything that can't possibly be correct - incorrect result shape, syntax error, timeout - but not empty results
  - On retry, the LLM receives the prior conversation plus the error message
  - Runs only execute at the highest configured retry count; results for the lower counts are derived from the attempt trace rather than re-run

With these inputs the program will do the following for each DB/example count/skills on-off/model combination
(max retry count is not part of the combination - see below)
- Iterate through the questions, running each 3 times (repetitions) to account for LLM non-determinism
- Send the question, wrapped in the prompt (template with the appropriate number of examples), to the chosen model
- Extract the query from the response (the last fenced code block), and use it to query the database
  - An explicit `UNANSWERABLE` response is terminal (never retried) and scored as a failure for answerable
    questions - and as correct if we later add deliberately-unanswerable questions
  - A response with neither a code block nor the `UNANSWERABLE` marker is malformed, and counts as a
    retryable error ("no query found in response") - safe because declining has an explicit channel,
    so retrying doesn't pressure the model into hallucinating a query
- On error, return the prior conversation plus the error message to the model and retry, up to the highest
  configured max retry count; results for lower retry counts are derived from the attempt trace
- Compare the result to the expected result from the question
  - Comparison depends on the question - e.g. "how many cars are there" compares a number, others may compare full objects
  - Individual DB packages handle result type coercion on a per-DB basis

At the end, it will produce a file containing the list of questions along with their generated queries, 
and whether each query for a given language failed. Each result record includes the model used, the
number of retries actually used, token usage, and the attempt trace.

```json
{
  "questions": [
    {
      "question": "How many cars are there?",
      "difficulty": "easy",
      "expected": 3,
      "typeql": {
        "correct": "match $x isa car; count;",
        "results": [
          {
            "model": "claude-opus-4.8",
            "maxRetries": 0,
            "retriesUsed": 0,
            "examples": 0,
            "skills": false,
            "repetition": 1,
            "generated": "match $x isa car; count",
            "attempts": [
              { "query": "match $x isa car; count", "error": "syntax error: ..." }
            ],
            "tokens": 1234,
            "result": "error",
            "accurate": false
          }
        ]
      },
      "sql": {
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
            "attempts": [],
            "tokens": 1180,
            "result": 3,
            "accurate": true
          }
        ]
      },
      "cypher": {
        "correct": "MATCH (c:Car) RETURN count(*)",
        "results": [
          {
            "model": "claude-opus-4.8",
            "maxRetries": 0,
            "retriesUsed": 0,
            "examples": 0,
            "skills": false,
            "repetition": 1,
            "generated": "MATCH (c:Car) RETURN count(c.age)",
            "attempts": [],
            "tokens": 1305,
            "result": 2,
            "accurate": false
          }
        ]
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
