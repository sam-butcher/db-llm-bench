# The model doesn't know your query language — and that's fine

*Benchmarking LLM query generation across SQL, Cypher and TypeQL*

If you ask an LLM to write SQL, it draws on decades of SQL in its training data. Ask it to
write TypeQL — a language whose current version is younger than most models' training cutoffs —
and it has almost nothing to draw on. Common sense says that should settle the question of
which database to put behind an LLM. We built a benchmark to check, and the answer turns out
to be more interesting: with a modest amount of in-context help, the ranking flips. The
best-scoring configuration in our entire run grid is a TypeDB one — and the properties that
get it there say as much about query languages as they do about models.

## The benchmark

We ask the same 42 natural-language questions of the [Reactome](https://reactome.org)
biological pathway curation database, loaded identically into MySQL, Neo4j and TypeDB, and
score each model's SQL, Cypher and TypeQL against a known answer. Reactome is real,
large-schema data — the MySQL DDL alone runs to ~87k characters, so navigating a schema too
large to hold comfortably in working memory is part of the task. The MySQL and Neo4j databases
are restored from the dumps Reactome publishes with each release; the TypeDB database is our
own, built from the Neo4j graph.

Each run is simple: the model gets the schema and the question, writes one query, we execute
it, and we compare the *result* to the expected answer. The questions range from simple
lookups to expert-level queries stressing a variety of constructs — recursion, aggregation,
polymorphism, and more.

What we varied, and why, is the interesting part. The full grid for this run:

- **Models**: Claude Sonnet 5 and DeepSeek V4 Pro.
- **Skills on/off**: whether each database's official (or best available vendored)
  query-writing skill — a markdown document teaching the language — is included in the prompt.
- **Few-shot examples**: 0 or 5 worked question-and-query examples.
- **Retry budget**: 0, 2 or 4. A retry fires only on a failure the harness can *see*, such as
  a syntax error, which is fed back to the model; a query that runs and returns a wrong answer
  is terminal.
- **3 repetitions** of everything, because models are non-deterministic.

That's 3,024 top-level runs and roughly 68 million tokens.

## The headline numbers

Averaged over all variations at the full retry budget:

| model | MySQL | Neo4j | TypeDB |
|---|---|---|---|
| Claude Sonnet 5 | 77% | 75% | 75% |
| DeepSeek V4 Pro | 83% | 81% | 65% |

At first glance this reads as "SQL wins, TypeQL trails". But the average hides the real story,
which is how differently the three languages respond to help. Everything below is a cut of the
same grid.

## Finding 1: in-context resources flip the ranking

Here is accuracy pooled over both models at the full retry budget, split by what the prompt
contained:

| config | MySQL | Neo4j | TypeDB |
|---|---|---|---|
| no skill, no examples | 72% | 65% | **29%** |
| skill only | 77% | 71% | 70% |
| examples only | 81% | 83% | 82% |
| skill + examples | 84% | 86% | **89%** |

Bare, TypeDB is by far the worst of the three — DeepSeek with no skill, no examples and no
retries scores just **7%** on TypeQL. No surprise: the models have seen decades of SQL and
almost no TypeQL 3.x.

But look at what each language gains from the same help. The TypeQL skill — about 33KB of
markdown documentation — is worth **+41 points** on its own (29% → 70%). The identical
intervention buys SQL five points and Cypher six. Add five worked examples on top and TypeDB
overtakes both incumbents. The single best cell in the whole 72-row variation grid is
**Sonnet 5 writing TypeQL with skill, examples and retries: 93%**, ahead of the best Neo4j
configuration (90%, DeepSeek) and the best MySQL configuration (86%, DeepSeek).

The lesson we take from this: what looks like a language deficit is mostly a *training-data*
deficit, and a training-data deficit is cheap to fix. A skill file in the prompt substantially
erases it. For anyone deploying LLM-generated queries, "how well does the model already know
the language" matters much less than "how well can the language be taught in context" — and a
regular, composable language teaches well.

## Finding 2: TypeQL fails loudly, SQL fails silently

Accuracy is not the only thing that matters. When a generated query is wrong, it matters
enormously *how* it is wrong. Of the runs that failed on their first attempt (no retries,
answerable questions only):

| first-attempt failures | MySQL | Neo4j | TypeDB |
|---|---|---|---|
| failed with a visible error | 40% | 62% | **85%** |
| ran fine, returned the wrong answer | 60% | 38% | 15% |

A silent wrong answer is the worst outcome an application can get: nothing downstream can tell
it from a right one. When an LLM's SQL fails, it usually fails this way — the query joins the
wrong tables or aggregates over duplicated rows, executes without complaint, and hands back a
plausible number. When its TypeQL fails, it overwhelmingly fails with an error, because in
TypeDB the schema is part of the query semantics: a wrong guess about structure — a role that
doesn't exist, an attribute owned by the wrong type — is a type error at the server, not an
empty-ish result set.

It would be easy to read that 85% as nothing more than finding 1 wearing a different hat: the
models don't know TypeQL, so of course their TypeQL doesn't parse. The variation grid says
otherwise. Here is how TypeQL first attempts break down as the prompt gains resources (models
pooled, share of all runs):

| TypeQL first attempts | correct | visible error | silently wrong |
|---|---|---|---|
| no skill, no examples | 12% | 76% | 12% |
| skill + examples | 69% | 28% | 3% |

Teaching the model the language cuts the error rate by nearly two-thirds — and the reclaimed
runs land almost entirely in the *correct* column, with the silently-wrong rate falling from
12% to 3% alongside. In-context resources don't trade loud failures for quiet ones; they turn
them into right answers.

Classifying the errors themselves, by the TypeDB error code they carry, shows exactly which
kind of loudness the resources remove. Genuine *syntax* errors — the query doesn't parse — are
the not-knowing-TypeQL signal, and the skill all but eliminates them: they fall from 33% of
all first attempts bare to 2% with the skill in the prompt. But *semantic* errors — a type
label that doesn't exist, a variable used out of scope across pipeline stages, a recursion the
language doesn't permit — hold steady at around 6–7% of first attempts in every configuration,
resourced or not. Those are not the model failing to write TypeQL; they are the model making
an ordinary mistake about a large schema, and TypeDB's compiler catching it. The same class of
mistake in SQL — joining the wrong tables, aggregating over duplicated rows — executes without
complaint and hands back a plausible number. So loud failure is a property of the language,
not a symptom of the model's ignorance: the skill fixes the grammar, and the type system keeps
catching the rest. SQL is the mirror image — its error rate also falls with examples (19% to
6%), but its silently-wrong rate only drifts from 21% to 14%, because most wrong SQL was never
going to error in the first place.

The gap persists all the way through the pipeline. At the full retry budget, the share of
*all* runs ending in a silent wrong answer is 21% for MySQL and Neo4j against 15% for TypeDB —
and of the failures that remain, TypeDB's are still mostly loud (53% carry an error), while
MySQL's are almost entirely silent (97% return a plausible wrong answer).

Loud failure is also *why* retries help TypeDB so much. A retry loop can only act on failures
it can see. Sonnet writing TypeQL with skill and examples climbs 77% → 89% → 93% as the retry
budget grows from 0 to 2 to 4; SQL barely moves under the same budget, because its failures
don't announce themselves. Put differently: loud failure converts a fixed accuracy ceiling
into an engineering knob you can turn.

## Finding 3: polymorphism is a schema problem before it is a query problem

The questions where TypeDB most clearly beats the other two databases are the polymorphic
ones — questions that range over Reactome's deep class hierarchy through a supertype. In the
best-resourced configuration, pooled over both models:

| | MySQL | Neo4j | TypeDB |
|---|---|---|---|
| polymorphism accuracy | 55% | 65% | **82%** |

One question shows why: *"Ignoring case, how many database objects go by a name of some kind
that contains 'PIK3' although their display name does not?"*

In TypeQL, "a name of some kind" is one pattern, because `name` is an attribute *supertype*
covering gene names, systematic names, surnames — every kind of name in the schema:

```typeql
match
  $x has name $n;
  $n contains "PIK3";
  not { $x has display-name $d; $d contains "PIK3"; };
select $x;
distinct;
reduce $count = count;
```

The SQL reference query for the same question is a seventeen-branch `UNION ALL` over per-class
name tables. The Cypher one is a wall of `coalesce` over differently-typed properties. Neither
model *ever* produced a correct SQL answer to this question, even fully resourced (0/6);
TypeDB scored 4/6.

The point is not that the LLM is better at one syntax than another. It's that in one language
the query is *derivable from the schema*, and in the others it requires exhaustively
enumerating the schema — and the LLM inherits exactly the failure mode a human engineer has
here: forgetting one of the seventeen tables.

## Where TypeQL struggles

The results are not one-sided, and the weak spots are worth naming.

**Argmax is TypeQL's worst category.** On the question asking which people authored the
greatest number of qualifying literature references, TypeDB scored 2/6 in the best
configuration against 6/6 for both MySQL and Neo4j. The expected answer is a *tie* — two
people — and a tie-safe per-group extreme currently requires re-deriving the pipeline twice in
TypeQL (or a user-defined function). Models instead reach for `sort ... limit 1` and return
one of the two winners. SQL's window functions and Cypher's `collect` make the tie-safe
version natural. This is a real language gap, not a resourcing gap.

**Recursion needs the resources.** TypeQL expresses transitive closure through recursive
functions, which are exotic enough that bare models score 0% on the recursion questions. With
skill and examples, Sonnet recovers to 12/12 — the same in-context-learnability story as
above, just steeper.

**Loud failure isn't free.** TypeQL runs consumed roughly twice the output tokens of SQL runs,
between retry loops and a more verbose skill. The retries that buy TypeDB its accuracy show up
on the token bill.

## A note on the playing field

One thing to keep in mind when reading all of the numbers above: Reactome is a really
idealized domain. It is a professionally curated scientific database, and every label,
property and relationship in it carries a clear, descriptive, consistently applied name.
That is precisely the best case for less structured systems like Neo4j, where the names *are*
the model's only guide to structure — there is no schema to contradict a plausible-looking
property, so the model's success rests entirely on the data being named as helpfully as this
data is. Production databases are rarely so kind: cryptic column names, abbreviations that
made sense to someone in 2009, conventions that drifted across teams and years. A
schema-enforced database keeps its guarantees — the query either fits the schema or fails
loudly — regardless of naming discipline; a schema-light one leans on exactly the discipline
that messy real-world data lacks. On a less pristine dataset, we'd expect the gaps in
findings 1 and 2 to widen, not close.

## Takeaways

Three things we'd want a reader to leave with:

1. **Pre-training familiarity is not destiny.** The language the model knows worst ended up
   producing the best score in the grid, once ~33KB of documentation and five examples were in
   the prompt. In-context learnability beats corpus volume.
2. **Failure mode matters as much as accuracy.** A language whose wrong queries fail loudly
   gives an agentic loop something to react to; a language whose wrong queries return
   plausible numbers does not. TypeDB's type system turns most LLM mistakes into visible,
   retryable errors.
3. **Schema expressiveness shows up in query accuracy.** Where the schema can say "these are
   all names", one pattern covers them; where it can't, every query re-encodes that knowledge,
   and both humans and LLMs drop branches.

The benchmark, dataset builds, prompts, skills and analysis scripts are all in the
db-llm-bench repository, along with the full results file this post is drawn from.
<!-- TODO: link the repo at its published location -->.
