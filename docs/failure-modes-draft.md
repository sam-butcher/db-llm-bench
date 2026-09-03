# TypeQL fails loudly, SQL fails silently

*How LLM-generated queries fail in SQL, Cypher and TypeQL — and why it matters more than
accuracy*

When an LLM writes a query and gets it wrong, one of two things happens. Either the database
rejects it — a syntax error, a type error, something the application can catch and act on —
or the query runs, and a wrong answer comes back looking exactly like a right one. The second
kind never shows up in a demo and never triggers a retry. It just flows downstream.

We recently benchmarked LLM query generation across MySQL, Neo4j and TypeDB, asking Claude
Sonnet 5 and DeepSeek V4 Pro the same 42 natural-language questions against the Reactome
pathway database in each store, and scoring the generated SQL, Cypher and TypeQL by executing
it and checking the result. The full methodology and results are in the accompanying report;
this piece is about one finding. The three languages don't just differ in how often they
fail — they differ in *how* they fail, and the difference is structural.
<!-- TODO: link the report at its published location -->

## When it fails, how does it fail?

Every percentage below is a share of all runs on answerable questions. First attempts, before
any retries:

| first attempts                      | MySQL     | Neo4j | TypeDB    |
|-------------------------------------|-----------|-------|-----------|
| correct                             | 70.5%     | 62.3% | 49.4%     |
| failed with a visible error         | 11.9%     | 23.5% | **43.2%** |
| ran fine, returned the wrong answer | **17.6%** | 14.2% | 7.5%      |

On this measure SQL and TypeQL are near mirror images. When SQL goes wrong it usually goes
wrong silently — three failed attempts in five join the wrong tables or aggregate over
duplicated rows, execute without complaint, and hand back a plausible number. When TypeQL
goes wrong, six failures in seven are visible errors, because in TypeDB the schema is part of
the query semantics: a wrong guess about structure — a role that doesn't exist, an attribute
owned by the wrong type — is a type error at the server rather than an empty result.

Cypher sits in between. In Neo4j, a property the model invents doesn't error — it matches
nothing — so a wrong structural guess becomes a silently empty or wrong result. What keeps
Cypher's numbers respectable here is that Reactome is an idealized domain: professionally
curated, with every label and property clearly and consistently named. Names are all a
schema-light system gives the model to steer by, and many databases — cryptic column names,
conventions that drift across teams and years — aren't named nearly as well. A schema-enforced
database fails loudly regardless of naming discipline; on messier data we'd expect these gaps
to widen.

## Loud failure isn't ignorance

An obvious objection: the models barely know TypeQL — its current version postdates most of
their training data — so of course their TypeQL doesn't parse. If that were the whole story,
teaching the model the language would make the errors disappear. It doesn't. TypeQL first
attempts, models pooled, as a share of all runs:

| TypeQL first attempts | correct | visible error | silently wrong |
|-----------------------|---------|---------------|----------------|
| no skill, no examples | 12.4%   | 75.6%         | 12.0%          |
| skill + examples      | 68.8%   | 28.2%         | 3.0%           |

Putting a ~33KB TypeQL skill and five worked examples in the prompt cuts the error rate by
nearly two-thirds, and the reclaimed runs land almost entirely in the correct column — the
silently-wrong rate falls too, from 12% to 3%. So the extra help genuinely fixes the failures
rather than just making them quieter.

Classifying the errors by their TypeDB error codes shows what kind of error the help removes.
Syntax errors — the query doesn't parse — are the sign of a model that doesn't know TypeQL,
and the skill all but eliminates them: from 33% of all first attempts down to 2%. Semantic
errors — a type label that doesn't exist, a variable used out of scope across pipeline
stages, a recursion the language doesn't permit — hold steady at 4–7% in every configuration.
Anyone writing queries against a schema this large makes mistakes like these; the difference
is that TypeDB's compiler catches them. The same mistakes in SQL execute without complaint.
Loud failure is a property of the language, not a symptom of the model's ignorance: the skill
fixes the grammar, and the type system keeps catching the rest. SQL's error rate also falls
with skill and examples (19% to 6%), but its silently-wrong rate only drifts from 21% to
14% — most wrong SQL was never going to error in the first place.

## Visible failures are fixable failures

Our benchmark's retry loop works the way any production one would: a failure the harness can
see — an error, a timeout, a malformed response — is fed back to the model for another
attempt. A query that runs and returns a wrong answer is terminal, because nothing knows it's
wrong. That means a language's retry ceiling is set by its failure visibility.

The effect is large. With skill and examples in the prompt, TypeDB climbs from 68.8% to 88.9%
accuracy as the retry budget grows from 0 to 4; MySQL moves from 79.9% to 83.8% on the same
budget, because most of its failures never announce themselves. Where each database ends up
once retries have done what they can:

| after retries                       | MySQL     | Neo4j | TypeDB    |
|-------------------------------------|-----------|-------|-----------|
| correct                             | 78.3%     | 76.0% | 67.4%     |
| failed with a visible error         | 0.6%      | 3.0%  | 17.2%     |
| ran fine, returned the wrong answer | **21.0%** | 21.0% | **15.4%** |

Retries have burned MySQL's visible errors down to almost nothing, but its silent failures
are untouched: 21% of runs still end in a wrong answer nothing can detect. TypeDB ends with
fewer silent failures, and more than half of what it gets wrong is still flagged as an error.

## What this means for agentic systems

For a human analyst, a query error and a wrong answer are both just failures. For an agentic
system they are opposites. An error is input: the loop reads it, revises the query, and tries
again — with TypeQL, a bigger retry budget simply buys more accuracy. A silent wrong answer
is invisible by definition; no amount of retry budget, self-review or orchestration can react
to a failure that doesn't present itself.

That reframes the question of which database to put behind an LLM. Raw first-shot accuracy
favours the language with the most training data. But the number an application actually
experiences is the silent-failure rate — how often a wrong answer arrives looking like a
right one — and on that number, the database that type-checks queries against its schema
comes out ahead, before counting the accuracy that retries can buy back. And unlike helpful
naming, a guarantee that wrong queries fail visibly survives messy real-world data.
