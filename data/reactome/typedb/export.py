#!/usr/bin/env python3
"""Export the Reactome graph from Neo4j into per-pass CSVs for the TypeDB load.

Neo4j is the source rather than MySQL because the TypeQL schema was derived
from the same class hierarchy the graph exposes, so labels map to entity types
and relationship types map to relations with no extra reconciliation.

Two things make this more than a dump:

  * Entities are written per *most specific* label. A Reaction node carries
    DatabaseObject/Event/ReactionLikeEvent/Reaction, but TypeDB needs the one
    concrete type, so the same hierarchy logic that built the schema picks it.

  * The n-ary relations are reassembled here, not in TypeQL. Reactome reifies
    catalysis, regulation and entity-functional-status as intermediate nodes;
    the export joins through them so each CSV row is one complete n-ary fact.
    That join is the whole point of the modelling and has to happen somewhere.

Usage: export.py [outdir]      (default: data/reactome/typedb/work)
"""

import csv
import importlib.util
import json
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
NEO4J_HTTP = os.environ.get("NEO4J_HTTP", "http://localhost:7474/db/neo4j/tx/commit")
NEO4J_USER = os.environ.get("NEO4J_USER", "neo4j")
NEO4J_PASS = os.environ.get("NEO4J_PASS", "password")

_spec = importlib.util.spec_from_file_location("build_schema", HERE / "build_schema.py")
_bs = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_bs)

# Per-entity attribute columns, keyed by TypeQL entity label. `db-id`,
# `display-name` and `schema-class` are on every row and added automatically.
ENTITY_ATTRS = {
    "pathway": [("is-canonical", "isCanonical")],
    "reaction-like-event": [("is-chimeric", "isChimeric")],
    "reference-sequence": [("sequence-length", "sequenceLength")],
    "reference-isoform": [("variant-identifier", "variantIdentifier")],
    "reference-therapeutic": [("approved", "approved"), ("withdrawn", "withdrawn"),
                              ("therapeutic-type", "type")],
    "go-cellular-component": [("accession", "accession")],
    "go-molecular-function": [("accession", "accession")],
    "go-biological-process": [("accession", "accession")],
    "taxon": [("tax-id", "taxId"), ("abbreviation", "abbreviation")],
    "species": [("tax-id", "taxId"), ("abbreviation", "abbreviation")],
    "person": [("first-name", "firstname"), ("surname", "surname")],
}

# Multi-valued attributes. A node property that is a list cannot share the
# entity's CSV — one cell cannot hold several values — so each gets its own
# (db-id, value) file and its own pass.
MULTI_ATTRS = [
    # (typeql entity, typeql attribute, Reactome label, node property)
    ("affiliation", "affiliation-name", "Affiliation", "name"),
]

# Binary relations: (typeql relation, role A, role B, cypher pattern, extra cols)
# The pattern binds $a and $b; direction is written as it exists in the graph.
BINARY = [
    ("event-containment", "containing-pathway", "contained-event",
     "(a:Pathway)-[r:hasEvent]->(b:Event)", [("ordering", "r.order")]),
    ("event-precedence", "preceding-event", "following-event",
     "(b:Event)-[r:precedingEvent]->(a:Event)", []),
    ("reaction-input", "reaction", "consumed-entity",
     "(a:ReactionLikeEvent)-[r:input]->(b:PhysicalEntity)",
     [("ordering", "r.order"), ("stoichiometry", "r.stoichiometry")]),
    ("reaction-output", "reaction", "produced-entity",
     "(a:ReactionLikeEvent)-[r:output]->(b:PhysicalEntity)",
     [("ordering", "r.order"), ("stoichiometry", "r.stoichiometry")]),
    ("required-input-component", "reaction", "required-component",
     "(a:ReactionLikeEvent)-[r:requiredInputComponent]->(b:PhysicalEntity)", []),
    ("complex-composition", "containing-complex", "component",
     "(a:Complex)-[r:hasComponent]->(b:PhysicalEntity)",
     [("ordering", "r.order"), ("stoichiometry", "r.stoichiometry")]),
    ("set-membership", "containing-set", "member",
     "(a:EntitySet)-[r:hasMember]->(b:PhysicalEntity)", [("ordering", "r.order")]),
    ("candidate-membership", "containing-set", "candidate",
     "(a:CandidateSet)-[r:hasCandidate]->(b:PhysicalEntity)", []),
    ("polymer-repetition", "containing-polymer", "repeated-unit",
     "(a:Polymer)-[r:repeatedUnit]->(b:PhysicalEntity)", []),
    ("species-assignment", "classified-thing", "species",
     "(a)-[r:species]->(b:Species)", []),
    ("related-species-assignment", "classified-thing", "related-species",
     "(a)-[r:relatedSpecies]->(b:Species)", []),
    ("compartment-assignment", "localised-thing", "compartment",
     "(a)-[r:compartment]->(b:Compartment)", []),
    ("included-location", "localised-thing", "location",
     "(a)-[r:includedLocation]->(b)", []),
    ("disease-annotation", "diseased-thing", "disease",
     "(a)-[r:disease]->(b)", []),
    ("go-annotation", "annotated-event", "biological-process",
     "(a:Event)-[r:goBiologicalProcess]->(b:GO_BiologicalProcess)", []),
    ("reference-assignment", "instance-entity", "reference",
     "(a:PhysicalEntity)-[r:referenceEntity]->(b:ReferenceEntity)", []),
    ("modified-residue-assignment", "modified-entity", "residue",
     "(a:EntityWithAccessionedSequence)-[r:hasModifiedResidue]->(b:AbstractModifiedResidue)", []),
    ("cross-reference", "referring-thing", "external-identifier",
     "(a)-[r:crossReference]->(b:DatabaseIdentifier)", []),
    ("database-of", "external-thing", "reference-database",
     "(a)-[r:referenceDatabase]->(b:ReferenceDatabase)", []),
    ("ontology-parenthood", "ontology-child", "ontology-parent",
     "(a)-[r:instanceOf]->(b)", []),
    ("taxonomy-parenthood", "sub-taxon", "super-taxon",
     "(a:Taxon)-[r:superTaxon]->(b:Taxon)", []),
    # inferredTo runs source -> inferred in the graph, the opposite of the
    # relational inferredFrom; the roles here restore the Reactome reading.
    ("event-inference", "inferred-event", "source-event",
     "(b:Event)-[r:inferredTo]->(a:Event)", []),
    ("entity-inference", "inferred-entity", "source-entity",
     "(b:PhysicalEntity)-[r:inferredTo]->(a:PhysicalEntity)", []),
    ("literature-citation", "citing-thing", "cited-publication",
     "(a)-[r:literatureReference]->(b:Publication)", []),
    ("summarisation", "summarised-thing", "summation",
     "(a)-[r:summation]->(b:Summation)", []),
    ("publication-authorship", "publication", "publication-author",
     "(b:Person)-[r:author]->(a:Publication)", [("ordering", "r.order")]),
    ("person-affiliation", "affiliated-person", "affiliation",
     "(a:Person)-[r:affiliation]->(b:Affiliation)", []),
    ("edit-authorship", "authored-edit", "edit-author",
     "(b:Person)-[r:author]->(a:InstanceEdit)", [("ordering", "r.order")]),
    ("functional-status-typing", "typed-status", "status-type",
     "(a:FunctionalStatus)-[r:functionalStatusType]->(b)", []),
    # Curation: the graph runs InstanceEdit -> object for most of these, but
    # internalReviewed runs the other way. Both are written to the same shape.
    ("creation", "curated-object", "edit",
     "(b:InstanceEdit)-[r:created]->(a)", []),
    ("modification", "curated-object", "edit",
     "(b:InstanceEdit)-[r:modified]->(a)", []),
    ("authoring", "curated-object", "edit",
     "(b:InstanceEdit)-[r:authored]->(a)", []),
    ("review", "curated-object", "edit",
     "(b:InstanceEdit)-[r:reviewed]->(a)", []),
    ("revision", "curated-object", "edit",
     "(b:InstanceEdit)-[r:revised]->(a)", []),
    ("internal-review", "curated-object", "edit",
     "(a)-[r:internalReviewed]->(b:InstanceEdit)", []),
]

# Roles that may be absent. A relation instance either has a role player or it
# does not, so rows are grouped by which optionals are present and each group
# gets its own pass — cleaner than trying to make one insert conditional.
OPTIONAL_COLS = {
    "catalysis": ["catalytic_activity"],
    "regulation": ["regulatory_activity"],
    "entity-functional-status": ["normal_entity"],
    "negative-precedence": ["exclusion_reason"],
    "event-containment": ["ordering"],
    "reaction-input": ["ordering", "stoichiometry"],
    "reaction-output": ["ordering", "stoichiometry"],
    "complex-composition": ["ordering", "stoichiometry"],
    "set-membership": ["ordering"],
    "publication-authorship": ["ordering"],
    "edit-authorship": ["ordering"],
}

# `regulation` rows carry the concrete subtype in a `subtype` column; each
# becomes its own pass, which is what makes requirement load as a
# positive-regulation without any extra statement.
SUBTYPE_COL = {"regulation": "subtype"}

# N-ary relations reassembled by joining through Reactome's reified nodes.
NARY = {
    "catalysis": """
MATCH (rle:ReactionLikeEvent)-[:catalystActivity]->(ca:CatalystActivity)
MATCH (ca)-[:physicalEntity]->(cat)
OPTIONAL MATCH (ca)-[:activity]->(act)
RETURN rle.dbId AS catalysed_reaction, cat.dbId AS catalyst,
       act.dbId AS catalytic_activity""",
    # The regulation subtype comes from the reified node's own label, which is
    # exactly the distinction SQL can only reach through the class table.
    "regulation": """
MATCH (rle:ReactionLikeEvent)-[:regulatedBy]->(reg:Regulation)
MATCH (reg)-[:regulator]->(who)
OPTIONAL MATCH (reg)-[:activity]->(act)
RETURN rle.dbId AS regulated_event, who.dbId AS regulator,
       act.dbId AS regulatory_activity,
       CASE
         WHEN reg:Requirement THEN 'Requirement'
         WHEN reg:PositiveGeneExpressionRegulation THEN 'PositiveGeneExpressionRegulation'
         WHEN reg:NegativeGeneExpressionRegulation THEN 'NegativeGeneExpressionRegulation'
         WHEN reg:PositiveRegulation THEN 'PositiveRegulation'
         WHEN reg:NegativeRegulation THEN 'NegativeRegulation'
       END AS subtype""",
    "entity-functional-status": """
MATCH (rle:ReactionLikeEvent)-[:entityFunctionalStatus]->(efs:EntityFunctionalStatus)
MATCH (efs)-[:diseaseEntity]->(de)
MATCH (efs)-[:functionalStatus]->(fs)
OPTIONAL MATCH (efs)-[:normalEntity]->(ne)
RETURN rle.dbId AS affected_event, de.dbId AS disease_entity,
       ne.dbId AS normal_entity, fs.dbId AS functional_status""",
    "negative-precedence": """
MATCH (ev:Event)-[:negativePrecedingEvent]->(npe:NegativePrecedingEvent)
MATCH (npe)-[:precedingEvent]->(prev)
OPTIONAL MATCH (npe)-[:reason]->(why)
RETURN prev.dbId AS excluded_preceding_event, ev.dbId AS following_event,
       why.dbId AS exclusion_reason""",
}


def var(label: str) -> str:
    """Attribute/role label to a loader variable name. Hyphens are valid in
    type labels but not in `$variables`, so the CSV header uses underscores."""
    return label.replace("-", "_")


def cypher(query: str) -> list[list[str]]:
    """Run a read query and return header + rows.

    Neo4j's HTTP endpoint is used rather than cypher-shell because
    `--format plain` does not quote values containing commas: two Taxon names
    ("dsDNA viruses, no RNA stage") split across columns and were rejected by
    the loader. JSON has no such ambiguity.
    """
    body = json.dumps({"statements": [{"statement": query}]})
    out = subprocess.run(
        ["curl", "-sS", "-u", f"{NEO4J_USER}:{NEO4J_PASS}",
         "-H", "Content-Type: application/json", "-d", body, NEO4J_HTTP],
        capture_output=True, text=True,
    )
    if out.returncode != 0:
        raise SystemExit(f"curl failed: {out.stderr}")
    payload = json.loads(out.stdout)
    if payload.get("errors"):
        raise SystemExit(f"cypher error: {payload['errors']}\nquery:\n{query}")
    result = payload["results"][0]
    rows = [result["columns"]]
    for entry in result["data"]:
        rows.append(["" if v is None else str(v) for v in entry["row"]])
    return rows


def concrete_entity_types() -> dict[str, str]:
    """TypeQL entity label -> Reactome label, for every type that can be a
    node's most specific label."""
    parent = _bs.read_hierarchy()
    out = {}
    for label in parent:
        if label in _bs.AS_RELATIONS or label in _bs.MIXINS or label in _bs.COEXTENSIVE_LOSERS:
            continue
        out[_bs.RENAMES.get(label, _bs.kebab(label))] = label
    return out


def export_entities(outdir: pathlib.Path, only: set[str] | None) -> None:
    """One CSV per concrete entity type, keyed on db-id.

    A node is written to the pass for its most specific label only. The
    hierarchy is expressed in the schema, so writing a Reaction into both
    `reaction` and `event` would insert it twice under two @key values.
    """
    types = concrete_entity_types()
    ancestors_of = _bs.read_hierarchy()
    for tql_label, reactome in sorted(types.items()):
        if only and tql_label not in only:
            continue
        extra = ENTITY_ATTRS.get(tql_label, [])
        # "most specific" = carries this label and no label that is a subtype of it
        subtypes = [l for l, p in ancestors_of.items() if p == reactome]
        guard = "".join(f" AND NOT n:{s}" for s in subtypes)
        cols = ["dbId AS db_id", "displayName AS display_name",
                "schemaClass AS schema_class", "stId AS st_id", "oldStId AS old_st_id"]
        cols += [f"n.{src} AS {var(dst)}" for dst, src in extra]
        q = (f"MATCH (n:{reactome}) WHERE true{guard} "
             f"RETURN " + ", ".join(("n." + c) if not c.startswith("n.") else c for c in cols))
        rows = cypher(q)
        path = outdir / f"entity__{tql_label}.csv"
        with path.open("w", newline="") as fh:
            csv.writer(fh).writerows(rows)
        print(f"  {tql_label:<38} {max(len(rows) - 1, 0):>9} rows")


def write_relation(outdir: pathlib.Path, name: str, rows: list[list[str]]) -> None:
    """Write one CSV per (subtype, present-optional-roles) group.

    Splitting here keeps every pass a plain unconditional insert: the loader
    has no way to omit a role player per row, so rows that differ in which
    roles they carry cannot share a pass.
    """
    if len(rows) < 2:
        print(f"  {name:<38} {'0':>9} rows")
        return
    header, data = rows[0], rows[1:]
    idx = {c: i for i, c in enumerate(header)}
    opt = [c for c in OPTIONAL_COLS.get(name, []) if c in idx]
    sub = SUBTYPE_COL.get(name)
    groups: dict[tuple, list[list[str]]] = {}
    for row in data:
        present = tuple(c for c in opt if row[idx[c]] != "")
        subtype = row[idx[sub]] if sub else None
        groups.setdefault((subtype, present), []).append(row)
    for (subtype, present), rs in sorted(groups.items(), key=lambda kv: str(kv[0])):
        cols = [c for c in header if c != sub and (c not in opt or c in present)]
        keep = [idx[c] for c in cols]
        stem = _bs.kebab(subtype) if subtype else name
        suffix = "" if len(groups) == 1 else "__" + ("none" if not present else "_".join(present))
        path = outdir / f"rel__{stem}{suffix}.csv"
        with path.open("w", newline="") as fh:
            w = csv.writer(fh)
            w.writerow(cols)
            w.writerows([[r[i] for i in keep] for r in rs])
        print(f"  {path.stem:<38} {len(rs):>9} rows")


def export_multi(outdir: pathlib.Path, only: set[str] | None) -> None:
    for entity, attr, label, prop in MULTI_ATTRS:
        if only and attr not in only and entity not in only:
            continue
        q = (f"MATCH (n:{label}) WHERE n.{prop} IS NOT NULL "
             f"UNWIND n.{prop} AS v RETURN n.dbId AS db_id, v AS {var(attr)}")
        rows = cypher(q)
        path = outdir / f"attr__{entity}__{attr}.csv"
        with path.open("w", newline="") as fh:
            csv.writer(fh).writerows(rows)
        print(f"  {path.stem:<38} {max(len(rows) - 1, 0):>9} rows")


def export_binary(outdir: pathlib.Path, only: set[str] | None) -> None:
    for name, role_a, role_b, pattern, extra in BINARY:
        if only and name not in only:
            continue
        cols = [f"a.dbId AS {var(role_a)}", f"b.dbId AS {var(role_b)}"]
        cols += [f"{src} AS {var(dst)}" for dst, src in extra]
        q = f"MATCH {pattern} RETURN " + ", ".join(cols)
        write_relation(outdir, name, cypher(q))


def export_nary(outdir: pathlib.Path, only: set[str] | None) -> None:
    for name, query in sorted(NARY.items()):
        if only and name not in only:
            continue
        write_relation(outdir, name, cypher(query.strip()))


def main() -> None:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    only = None
    for a in sys.argv[1:]:
        if a.startswith("--only="):
            only = set(a.split("=", 1)[1].split(","))
    outdir = pathlib.Path(args[0]) if args else HERE / "work"
    outdir.mkdir(parents=True, exist_ok=True)
    print(f"exporting into {outdir}" + (f" (only: {sorted(only)})" if only else ""))
    print("entities:")
    export_entities(outdir, only)
    print("multi-valued attributes:")
    export_multi(outdir, only)
    print("binary relations:")
    export_binary(outdir, only)
    print("n-ary relations:")
    export_nary(outdir, only)


if __name__ == "__main__":
    main()
