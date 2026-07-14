# Pokedex pilot dataset

A pilot dataset for evaluating cross-language query generation, adapted from
the [Veekun Pokédex](https://veekun.github.io/pokedex/). Chosen because it has
a rich, relationship-heavy schema (including a genuine n-ary relationship) with
real data, but — unlike query benchmarks such as JOB/TPC or sample databases
such as Northwind/Sakila — no canonical query workload distributed across the
web, so no query language gets an unfair training-corpus head start.

## Entities

- `pokemon` (name, generation)
- `poketype` (name) — the elemental type; named `poketype` rather than `type`
  because `type` is reserved in TypeQL. Cypher uses `:PokeType`, SQL `poketype`.
- `ability` (name)
- `move` (name, power, damage class) — each belongs to one `poketype`
- `move_method` (name) — Level up, Machine, Tutor, Egg

## Relationships

- **typing** — a Pokémon has 1–2 types, ordered by `slot`
- **ability-slot** — a Pokémon has abilities, with `is_hidden` and `slot`
- **move-type** — a move belongs to a type
- **efficacy** — a *reflexive* relation: how effective one type's damage is
  against another (`factor`, a percentage). Same entity in two roles.
- **learning** — the n-ary showcase: a Pokémon learns a move via a method at a
  level. Genuinely relates three entities plus an attribute.

## The learning relationship across the three models

`learning` is the reason this pilot exists. The three data models express the
same four-way fact differently, and keeping them equivalent is the point:

- **SQL** — a junction table `pokemon_move(pokemon_id, move_id, method_id, level)`.
- **TypeQL** — a native ternary relation `learning(learner, learned, via)` that
  owns `level`.
- **Cypher** — a property graph has only binary edges, so `learning` is
  **reified** into a `:Learning` node linking the Pokémon, the move, and the
  method: `(:Pokemon)-[:LEARNS]->(:Learning {level})-[:OF_MOVE]->(:Move)` and
  `(:Learning)-[:VIA]->(:MoveMethod)`. This is a deliberate modeling decision:
  collapsing the method onto the edge as a property would make the Neo4j schema
  *less* expressive than the other two and break equivalence.

## Data

Deliberately sparse (19 Pokémon, 18 types, 18 moves, ...) so every expected
answer is hand-verifiable. Two simplifications versus upstream Veekun: display
names are materialized directly (no `_names`/`identifier` i18n tables), and a
single game version is assumed (so `learning` doesn't fan out across versions).

The three data files (`sql/data.sql`, `neo4j/data.cypher`, `typedb/data.tql`),
`questions.json`, and `src/pokedex-golden.yml` are all generated from one master
definition in [`generate.py`](generate.py) (`python3 data/pokedex/generate.py`),
so they can never drift apart. `generate.py` also independently computes each
expected answer, cross-checking the values in `questions.json`. The `schema.*`,
`prompt.txt`, and `example-*.txt` files are authored by hand.

## Running it

See `src/pokedex.yml` (benchmark against a model) and `src/pokedex-golden.yml`
(gold-equivalence check with no LLM). Both expect the pilot DB stack:

```sh
cd databases/pokedex && docker compose up -d --build   # run only this stack
```
