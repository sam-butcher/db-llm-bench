# The model doesn't know your query language — and that's fine

*Benchmarking LLM query generation across SQL, Cypher and TypeQL*

Ask an LLM to write SQL and it draws on decades of training data. Ask it to write TypeQL — a
language whose current version is younger than most models' training cutoffs — and it has
almost nothing to draw on. That would seem to settle which database to put behind an LLM. We
built a benchmark to check, and found otherwise: with a modest amount of in-context help, LLMs
generate correct TypeQL just as easily as they do SQL — and in some cases more easily.

## The benchmark

We ask the same 42 natural-language questions of the [Reactome](https://reactome.org)
biological pathway curation database, loaded identically into MySQL, Neo4j and TypeDB, and
score each model's SQL, Cypher and TypeQL against a known answer. Reactome is real data with a
large schema — the MySQL DDL alone runs to ~16k tokens. The MySQL and Neo4j databases are
restored from the dumps Reactome publishes with each release; the TypeDB database is our own,
built from the Neo4j graph.

Each run is simple: the model gets the schema and the question, writes one query, we execute
it, and we compare the result to the expected answer. The questions range from simple lookups
to expert-level queries involving recursion, aggregation and polymorphism.

The grid for this run:

- **Models**: Claude Sonnet 5 and DeepSeek V4 Pro.
- **Skills on/off**: whether each database's official (or best available vendored)
  query-writing skill — a markdown document teaching the language — is included in the prompt.
- **Few-shot examples**: 0 or 5 worked question-and-query examples written for the Reactome dataset.
- **Retry budget**: 0, 2 or 4. A retry fires only on a failure the harness can see, such as a
  syntax error, which is fed back to the model; a query that runs and returns a wrong answer
  is terminal.
- **3 repetitions** of everything, to cover non-determinism.

## The headline numbers

Averaged over all variations at the full retry budget:

| model           | MySQL | Neo4j | TypeDB |
|-----------------|-------|-------|--------|
| Claude Sonnet 5 | 77.0% | 74.8% | 74.6%  |
| DeepSeek V4 Pro | 82.7% | 80.6% | 64.9%  |

At first glance SQL wins and TypeQL trails, but the averages hide how differently the three
languages respond to help.

## Finding 1: in-context resources flip the ranking

Accuracy pooled over both models at the full retry budget, split by what the prompt contained
(answerable questions only, here and throughout the findings):

| config                | MySQL | Neo4j | TypeDB    |
|-----------------------|-------|-------|-----------|
| no skill, no examples | 71.8% | 64.5% | **29.1%** |
| skill only            | 76.9% | 71.4% | 69.7%     |
| examples only         | 80.8% | 82.5% | 82.1%     |
| skill + examples      | 83.8% | 85.5% | **88.9%** |

With no help at all, TypeDB is by far the worst of the three — DeepSeek with no skill, no
examples and no retries gets **every single question wrong** (0/117). No surprise: the models
have seen decades of SQL and almost no TypeQL 3.x.

The same help goes much further in some languages than others, though. The TypeQL skill —
about 33KB of markdown — is worth **+41 points** on its own (29% → 70%); the corresponding
skills gain SQL five points and Cypher seven. With examples added, TypeDB overtakes both.
The best configuration in the whole benchmark is **Sonnet 5 writing TypeQL with skill,
examples and retries: 93%**, ahead of the best Neo4j configuration (90%, with DeepSeek) and
the best MySQL configuration (86%, also DeepSeek).

What looks like a language deficit is mostly a training-data deficit, and a training-data
deficit is cheap to fix: a skill file in the prompt largely erases it. How well the model
already knows a language matters much less than how well the language can be taught in
context — and a regular, composable language is easy to teach.

## Finding 2: TypeQL fails loudly, SQL fails silently

When a generated query is wrong, it matters how it is wrong. Of the runs that failed on their
first attempt (no retries, answerable questions only):

| first-attempt failures              | MySQL | Neo4j | TypeDB    |
|-------------------------------------|-------|-------|-----------|
| failed with a visible error         | 40.2% | 62.3% | **85.2%** |
| ran fine, returned the wrong answer | 59.8% | 37.7% | 14.8%     |

A silent wrong answer is the worst outcome an application can get: nothing downstream can tell
it from a right one. Wrong SQL usually fails this way — it joins the wrong tables or
aggregates over duplicated rows, executes without complaint, and hands back a plausible
number. Wrong TypeQL usually fails with an error, because in TypeDB the schema is part of the
query semantics: a wrong guess about structure — a role that doesn't exist, an attribute owned
by the wrong type — is a type error at the server rather than an empty result.

Cypher sits in between. In Neo4j, a property the model invents doesn't error — it matches
nothing — so a wrong structural guess becomes a silently empty or wrong result. What keeps
Cypher's numbers respectable here is that Reactome is an idealized domain: professionally
curated, with every label and property clearly and consistently named. Names are all a
schema-light system gives the model to steer by, and many databases — cryptic column names,
conventions that drifted across teams and years — aren't named nearly as well. A schema-enforced
database fails loudly regardless of naming discipline; on messier data we'd expect these gaps
to widen.

TypeQL keeps this lead even as in-context resources improve the model's ability to write it.
TypeQL first attempts, models pooled, as a share of all runs:

| TypeQL first attempts | correct | visible error | silently wrong |
|-----------------------|---------|---------------|----------------|
| no skill, no examples | 12.4%   | 75.6%         | 12.0%          |
| skill + examples      | 68.8%   | 28.2%         | 3.0%           |

Teaching the model the language cuts the error rate by nearly two-thirds, and the reclaimed
runs land almost entirely in the correct column — the silently-wrong rate falls too, from 12%
to 3%. So the extra help genuinely fixes the failures rather than just making them quieter.

Classifying the errors by their TypeDB error codes shows what kind of error the resources
remove. Syntax errors — the query doesn't parse — are the sign of a model that doesn't know
TypeQL, and the skill all but eliminates them: from 33% of all first attempts down to 2%.
Semantic errors — a type label that doesn't exist, a variable used out of scope across
pipeline stages, a recursion the language doesn't permit — hold steady at 4–7% in every
configuration. Anyone writing queries against a schema this large makes mistakes like these;
the difference is that TypeDB's compiler catches them. The same mistakes in SQL execute without complaint. Loud failure is a
property of the language, not a symptom of the model's ignorance: the skill fixes the grammar,
and the type system keeps catching the rest. SQL's error rate also falls with skill and
examples (19% to 6%), but its silently-wrong rate only drifts from 21% to 14% — most wrong SQL
was never going to error in the first place.

The gap survives retries, too. At the full retry budget, 21% of MySQL and Neo4j runs still
end in a silent wrong answer against 15% for TypeDB — and TypeDB's remaining failures are
still mostly loud (53% carry an error), while MySQL's are almost entirely silent (97%).

Loud failure is also why retries help TypeDB so much: a retry loop can only act on failures it
can see. Sonnet writing TypeQL with skill and examples climbs 77% → 89% → 93% as the retry
budget grows from 0 to 2 to 4; SQL barely moves, because its failures don't announce
themselves. With TypeQL, a bigger retry budget simply buys more accuracy.

## Finding 3: polymorphism is a schema problem before it is a query problem

TypeDB's clearest wins are the polymorphic questions — those that range over Reactome's deep
class hierarchy through a supertype. In the best-resourced configuration, pooled over both
models:

|                       | MySQL | Neo4j | TypeDB    |
|-----------------------|-------|-------|-----------|
| polymorphism accuracy | 54.5% | 65.2% | **81.8%** |

One question shows why: *"Ignoring case, how many database objects go by a name of some kind
that contains 'PIK3' although their display name does not?"*

In TypeQL, "a name of some kind" is one pattern, because `name` is an attribute supertype
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

The SQL reference query is a seventeen-branch `UNION ALL` over per-class name tables; the
Cypher one, a wall of `coalesce` over differently-typed properties. Neither model ever
produced a correct SQL or Cypher answer to this question, even fully resourced (0/6 for both);
TypeDB scored 4/6.

The LLM isn't better at one syntax than another here. In one language the query can be derived
from the schema; in the others it means exhaustively enumerating the schema, and the LLM
inherits the same failure mode as a human engineer: forgetting one of the seventeen tables.

## Where TypeQL struggles

The results are not one-sided.

**Argmax is TypeQL's worst category.** On the question asking which people authored the
greatest number of qualifying literature references, TypeDB scored 2/6 in the best
configuration against 6/6 for both MySQL and Neo4j. The expected answer is a tie — two
people — and a tie-safe per-group extreme currently requires re-deriving the pipeline twice in
TypeQL (or a user-defined function). Models instead reach for `sort ... limit 1` and return
one of the two winners. SQL's window functions and Cypher's `collect` make the tie-safe
version natural.

**Recursion needs the resources.** TypeQL expresses transitive closure through recursive
functions, which are rare enough in the wild that models score 0% on the recursion questions
without help. With skill and examples, Sonnet recovers to 12/12 — the finding 1 story again,
just steeper.

**Loud failure isn't free.** TypeQL runs consumed roughly twice the output tokens of SQL runs,
between retry loops and a more verbose skill. The retries that buy TypeDB its accuracy show up
on the token bill.

## Takeaways

1. **Pre-training unfamiliarity is not insurmountable.** The language the model knows worst
   produced the best score in the benchmark, once ~33KB of documentation and five examples
   were in the prompt. In-context learnability beats corpus volume.
2. **Failure mode matters as much as accuracy.** A language whose wrong queries fail loudly
   gives an agentic loop something to react to; one whose wrong queries return plausible
   numbers does not. TypeDB's type system turns most LLM mistakes into visible, retryable
   errors — a guarantee that, unlike helpful naming, survives messy real-world data.
3. **Schema expressiveness shows up in query accuracy.** Where the schema can say "these are
   all names", one pattern covers them; where it can't, every query re-encodes that knowledge,
   and both humans and LLMs drop branches.

The benchmark, dataset builds, prompts, skills and analysis scripts are all in the
db-llm-bench repository, along with the full results file this post is drawn from.
<!-- TODO: link the repo at its published location -->
