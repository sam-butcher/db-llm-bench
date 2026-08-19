# Skills

Vendored LLM query-writing skills, one folder per DB. Each is loaded (all `.md`
files in the folder) into the prompt's `{{skills}}` slot when a DB's config sets
`skills: data/skills/<db>`, so the benchmark can measure skill-on vs skill-off.

These are third-party skills, downloaded and included under their upstream
licenses. Retrieved 2026-07 from HEAD of each repo.

| DB | Skill | Source | License |
| --- | --- | --- | --- |
| `typedb` | TypeQL (TypeDB 3.8+) | [typedb/typedb-skills](https://github.com/typedb/typedb-skills) `typeql.md` (official) | first-party (no explicit license file) |
| `neo4j` | Cypher 25 | [neo4j-contrib/neo4j-skills](https://github.com/neo4j-contrib/neo4j-skills) `neo4j-cypher-skill/SKILL.md` | MIT |
| `sql` | PostgreSQL best practices | [wimolivier/postgresql-best-practices](https://github.com/wimolivier/postgresql-best-practices) `SKILL.md` | MIT |

Notes:

- Only the top-level `SKILL.md` of each upstream skill is vendored (`load_skills`
  reads a skill folder non-recursively). The Neo4j and PostgreSQL upstreams also
  ship a `references/` directory of deeper material not included here.
- The Neo4j and TypeQL skills are query-writing-focused. A comparable "write
  correct SELECT queries" skill barely exists for SQL (LLMs already write SQL
  well), so the PostgreSQL skill leans toward schema-design and best-practices.
- The Neo4j skill targets Cypher 25 (Neo4j 2025.x+) and tells the model to open
  every query with `CYPHER 25`; the benchmark server is Neo4j 5.26, which
  rejects that preamble. No comparably sourced, licensed Cypher-5 skill exists
  (the closest, tomasonjo/blogs `neo4j-cypher-guide`, has no license), so the
  skill is kept as-is and `data/prompts/neo4j.txt` names the server version
  instead, as the TypeDB prompt does for TypeQL 3.x.
