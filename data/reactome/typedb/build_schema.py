#!/usr/bin/env python3
"""Build the TypeQL schema for Reactome (TypeDB 3.12).

The entity hierarchy is generated from the class hierarchy observed in the
Reactome graph release (data/reactome/neo4j/schema.txt), so it tracks the real
data rather than a hand-copied reading of the docs. The relations are designed
by hand, because that is where TypeQL differs from the other two models and a
mechanical translation would throw the difference away:

  * Catalysis is a ternary relation. Reactome reifies it as a CatalystActivity
    row/node carrying a physical entity and a GO molecular function, which the
    reaction then points at — two hops in SQL and Cypher. A single
    CatalystActivity is reused by up to 63 reactions in this release, so the
    honest decomposition is one ternary fact per (catalyst, activity, reaction).

  * Regulation is a relation hierarchy. positive-regulation and its subtypes —
    including requirement, whose name gives no hint — are subtypes of one
    abstract regulation, so "regulated positively" is a single type query
    instead of the class-table join SQL needs.

  * Complex components and set members share one abstract composition relation,
    so walking a complex's parts is one recursive traversal rather than a union
    over two link tables.

  * entity-functional-status is genuinely 4-ary (event, disease entity, normal
    entity, status) and is modelled as such rather than as a hub node.

Ordering that the relational model keeps in `<attr>_rank` columns, and the
graph keeps in an `order` relationship property, lives here as an `ordering`
attribute owned by the relation.

Usage: build_schema.py [out.tql]
"""

import pathlib
import re
import sys

HIERARCHY_SRC = pathlib.Path(__file__).resolve().parents[1] / "neo4j" / "schema.txt"
# Bookkeeping labels that are not part of the domain hierarchy.
MIXINS = {"Trackable", "Deletable"}


def kebab(name: str) -> str:
    """Reactome's CamelCase class names to TypeQL's kebab-case labels."""
    name = name.replace("_", "-")
    name = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "-", name)
    return name.lower()


def read_hierarchy() -> dict[str, str | None]:
    """Direct-parent per label, derived from the 'is a' lines of the graph schema."""
    ancestors: dict[str, list[str]] = {}
    for line in HIERARCHY_SRC.read_text(encoding="utf8").splitlines():
        m = re.match(r"^(\w+) is a (.+)$", line)
        if m:
            ancestors[m.group(1)] = [x.strip() for x in m.group(2).split(",")]
    labels = sorted(set(ancestors) | {a for v in ancestors.values() for a in v})
    depth = {l: len(ancestors.get(l, [])) for l in labels}
    parent: dict[str, str | None] = {}
    for label in labels:
        # Some label pairs are co-extensive in this release — every node
        # carrying one carries the other — so containment cannot say which is
        # the subtype (Interaction/UndirectedInteraction, ReactionType/
        # DrugActionType, ModifiedNucleotide/TranscriptionalModification).
        # Treat those as siblings under their nearest shared ancestor rather
        # than inventing a direction the data does not support.
        real = [
            a for a in ancestors.get(label, [])
            if a not in MIXINS and label not in ancestors.get(a, [])
        ]
        parent[label] = max(real, key=lambda a: depth.get(a, 0)) if real else None
    return parent


# Attributes, with the value type each carries.
ATTRIBUTES = """
attribute db-id, value integer;
attribute st-id, value string;
attribute old-st-id, value string;
attribute display-name, value string;
attribute schema-class, value string;
attribute entity-name, value string;
attribute definition, value string;
attribute accession, value string;
attribute identifier, value string;
attribute variant-identifier, value string;
attribute gene-name, value string;
attribute ec-number, value string;
attribute tax-id, value string;
attribute abbreviation, value string;
attribute first-name, value string;
attribute surname, value string;
attribute affiliation-name, value string;
attribute summary-text, value string;
attribute note, value string;
attribute release-number, value integer;
attribute release-date, value date;
attribute edited-on, value datetime;
attribute coordinate, value integer;
attribute sequence-length, value integer;
attribute approved, value boolean;
attribute withdrawn, value boolean;
attribute is-canonical, value boolean;
attribute is-chimeric, value boolean;
attribute therapeutic-type, value string;
attribute action-name, value string;
# Ordering and stoichiometry sit on the relation, not on either endpoint:
# they are facts about the participation, not about the participant.
attribute ordering, value integer;
attribute stoichiometry, value integer;
"""

# Which entity types own which attributes. Declared on the highest type that
# has them so every subtype inherits — the same polymorphism the queries use.
OWNERSHIP = {
    "database-object": ["db-id @key", "display-name", "schema-class",
                        "st-id @card(0..1)", "old-st-id @card(0..1)"],
    "event": ["entity-name @card(0..)", "definition @card(0..1)"],
    "pathway": ["is-canonical @card(0..1)"],
    "reaction-like-event": ["is-chimeric @card(0..1)"],
    "physical-entity": ["entity-name @card(0..)", "definition @card(0..1)"],
    "reference-entity": ["identifier @card(0..1)", "entity-name @card(0..)"],
    "reference-sequence": ["gene-name @card(0..)", "sequence-length @card(0..1)"],
    "reference-isoform": ["variant-identifier @card(0..1)"],
    "reference-therapeutic": ["approved @card(0..1)", "withdrawn @card(0..1)",
                              "therapeutic-type @card(0..1)"],
    "go-term": ["accession @card(0..1)", "definition @card(0..1)"],
    "go-molecular-function": ["ec-number @card(0..)"],
    "external-ontology": ["identifier @card(0..1)", "definition @card(0..1)"],
    "taxon": ["tax-id @card(0..1)", "abbreviation @card(0..1)"],
    "person": ["first-name @card(0..1)", "surname @card(0..1)"],
    "affiliation": ["affiliation-name @card(0..)"],
    "instance-edit": ["edited-on @card(0..1)", "note @card(0..1)"],
    "summation": ["summary-text @card(0..1)"],
    "abstract-modified-residue": ["coordinate @card(0..1)"],
    "update-tracker": ["action-name @card(0..)"],
    "release-": ["release-number @card(0..1)", "release-date @card(0..1)"],
}

RELATIONS = """
# ---------------------------------------------------------------------------
# Event structure
# ---------------------------------------------------------------------------

# A pathway contains events in a curated order; `ordering` is the rank the
# relational model keeps in Pathway_2_hasEvent.hasEvent_rank.
relation event-containment,
  relates containing-pathway,
  relates contained-event,
  owns ordering @card(0..1);

# Ordering between events, and the curated statement that one event does NOT
# precede another (Reactome records the reason for the exclusion).
relation event-precedence,
  relates preceding-event,
  relates following-event;

relation negative-precedence,
  relates excluded-preceding-event,
  relates following-event,
  relates exclusion-reason @card(0..1);

# Reaction participants. The abstract supertype lets a query ask for any
# participation without naming the direction.
relation reaction-participation @abstract,
  relates reaction,
  relates participant,
  owns ordering @card(0..1),
  owns stoichiometry @card(0..1);

relation reaction-input sub reaction-participation,
  relates consumed-entity as participant;

relation reaction-output sub reaction-participation,
  relates produced-entity as participant;

relation required-input-component sub reaction-participation,
  relates required-component as participant;

# ---------------------------------------------------------------------------
# Catalysis — ternary
# ---------------------------------------------------------------------------
# Reactome reifies this as a CatalystActivity holding a physical entity and a
# GO molecular function, which reactions then reference. Because one such
# activity is shared by many reactions, the fact being stated is really
# three-way: this entity, performing this molecular function, catalyses this
# reaction. An active unit narrows which part of the catalyst is responsible.
relation catalysis,
  relates catalysed-reaction,
  relates catalyst,
  relates catalytic-activity @card(0..1),
  relates active-unit @card(0..);

# ---------------------------------------------------------------------------
# Regulation — a relation hierarchy
# ---------------------------------------------------------------------------
# Subtyping carries what the relational model can only express by joining the
# PositiveRegulation table: `requirement` is a positive regulation, and its
# name says nothing about that.
relation regulation @abstract,
  relates regulated-event,
  relates regulator,
  relates regulatory-activity @card(0..1),
  relates regulation-active-unit @card(0..);

relation positive-regulation sub regulation;
relation requirement sub positive-regulation;
relation positive-gene-expression-regulation sub positive-regulation;
relation negative-regulation sub regulation;
relation negative-gene-expression-regulation sub negative-regulation;

# ---------------------------------------------------------------------------
# Composition — one abstract relation over complexes, sets and polymers
# ---------------------------------------------------------------------------
# SQL needs Complex_2_hasComponent UNION EntitySet_2_hasMember to walk a
# structure; here both are `composition`, so one traversal covers them.
relation composition @abstract,
  relates whole,
  relates part,
  owns ordering @card(0..1),
  owns stoichiometry @card(0..1);

relation complex-composition sub composition,
  relates containing-complex as whole,
  relates component as part;

relation set-membership sub composition,
  relates containing-set as whole,
  relates member as part;

relation candidate-membership sub set-membership,
  relates candidate as member;

relation polymer-repetition sub composition,
  relates containing-polymer as whole,
  relates repeated-unit as part;

# ---------------------------------------------------------------------------
# Annotation
# ---------------------------------------------------------------------------
relation species-assignment,
  relates classified-thing,
  relates species;

relation related-species-assignment,
  relates classified-thing,
  relates related-species;

relation compartment-assignment,
  relates localised-thing,
  relates compartment;

relation included-location,
  relates localised-thing,
  relates location;

relation disease-annotation,
  relates diseased-thing,
  relates disease;

relation go-annotation,
  relates annotated-event,
  relates biological-process;

# A protein/molecule instance and the reference entity it is an instance of.
relation reference-assignment,
  relates instance-entity,
  relates reference;

relation modified-residue-assignment,
  relates modified-entity,
  relates residue;

relation cross-reference,
  relates referring-thing,
  relates external-identifier;

relation database-of,
  relates external-thing,
  relates reference-database;

# Ontology parenthood, used by GO and the external ontologies; recursive.
relation ontology-parenthood,
  relates ontology-child,
  relates ontology-parent;

relation taxonomy-parenthood,
  relates sub-taxon,
  relates super-taxon;

# ---------------------------------------------------------------------------
# Inference between species
# ---------------------------------------------------------------------------
relation event-inference,
  relates inferred-event,
  relates source-event;

relation entity-inference,
  relates inferred-entity,
  relates source-entity;

# ---------------------------------------------------------------------------
# Disease variants — 4-ary
# ---------------------------------------------------------------------------
# The fact ties an event to the diseased form of an entity, the normal form it
# replaces, and the functional consequence. Splitting it into binaries loses
# which normal entity the disease entity stands in for.
relation entity-functional-status,
  relates affected-event,
  relates disease-entity,
  relates normal-entity @card(0..1),
  relates functional-status @card(1..);

relation functional-status-typing,
  relates typed-status,
  relates status-type;

# ---------------------------------------------------------------------------
# Literature and curation
# ---------------------------------------------------------------------------
relation literature-citation,
  relates citing-thing,
  relates cited-publication;

relation summarisation,
  relates summarised-thing,
  relates summation;

relation publication-authorship,
  relates publication,
  relates publication-author,
  owns ordering @card(0..1);

relation person-affiliation,
  relates affiliated-person,
  relates affiliation;

# Every curation act is the same shape — an object and the edit that touched
# it — so the subtypes differ only in what the edit means. Asking "was this
# touched at all" is one query over the supertype.
relation curation @abstract,
  relates curated-object,
  relates edit;

relation creation sub curation;
relation modification sub curation;
relation authoring sub curation;
relation review sub curation;
relation internal-review sub curation;
relation revision sub curation;

relation edit-authorship,
  relates authored-edit,
  relates edit-author,
  owns ordering @card(0..1);

relation release-record,
  relates tracked-object,
  relates tracking-release;
"""

# Role players, declared on the most general type that can play the role so
# that every subtype inherits the capability.
PLAYS = {
    "pathway": ["event-containment:containing-pathway"],
    "event": [
        "event-containment:contained-event",
        "event-precedence:preceding-event", "event-precedence:following-event",
        "negative-precedence:excluded-preceding-event", "negative-precedence:following-event",
        "event-inference:inferred-event", "event-inference:source-event",
        "species-assignment:classified-thing", "related-species-assignment:classified-thing",
        "compartment-assignment:localised-thing", "disease-annotation:diseased-thing",
        "go-annotation:annotated-event", "literature-citation:citing-thing",
        "summarisation:summarised-thing", "cross-reference:referring-thing",
    ],
    "reaction-like-event": [
        "reaction-participation:reaction", "catalysis:catalysed-reaction",
        "regulation:regulated-event", "entity-functional-status:affected-event",
    ],
    "physical-entity": [
        "reaction-participation:participant", "catalysis:catalyst",
        "catalysis:active-unit", "regulation:regulator",
        "regulation:regulation-active-unit", "composition:part",
        "species-assignment:classified-thing", "related-species-assignment:classified-thing",
        "compartment-assignment:localised-thing", "disease-annotation:diseased-thing",
        "entity-inference:inferred-entity", "entity-inference:source-entity",
        "entity-functional-status:disease-entity", "entity-functional-status:normal-entity",
        "literature-citation:citing-thing", "summarisation:summarised-thing",
        "cross-reference:referring-thing", "reference-assignment:instance-entity",
    ],
    "complex": ["complex-composition:containing-complex", "included-location:localised-thing"],
    "entity-set": ["set-membership:containing-set", "included-location:localised-thing"],
    "polymer": ["polymer-repetition:containing-polymer"],
    # A specialised role is not covered by `plays` on the role it specialises:
    # declaring `plays composition:part` does not let a physical entity play
    # `candidate-membership:candidate`, so each specialisation is declared too.
    "physical-entity+specialised": [
        "complex-composition:component", "set-membership:member",
        "candidate-membership:candidate", "polymer-repetition:repeated-unit",
        "reaction-input:consumed-entity", "reaction-output:produced-entity",
        "required-input-component:required-component",
    ],
    "entity-with-accessioned-sequence": ["modified-residue-assignment:modified-entity"],
    "abstract-modified-residue": ["modified-residue-assignment:residue"],
    "catalyst-activity": [],
    "go-molecular-function": ["catalysis:catalytic-activity", "regulation:regulatory-activity"],
    "go-biological-process": ["go-annotation:biological-process"],
    "go-cellular-component": ["compartment-assignment:compartment", "included-location:location",
                              "ontology-parenthood:ontology-child", "ontology-parenthood:ontology-parent"],
    "external-ontology": ["ontology-parenthood:ontology-child", "ontology-parenthood:ontology-parent",
                          "disease-annotation:disease"],
    "taxon": ["taxonomy-parenthood:sub-taxon", "taxonomy-parenthood:super-taxon"],
    "species": ["species-assignment:species", "related-species-assignment:related-species"],
    "reference-entity": ["reference-assignment:reference", "cross-reference:referring-thing",
                         "species-assignment:classified-thing", "database-of:external-thing"],
    "reference-database": ["database-of:reference-database"],
    "database-identifier": ["cross-reference:external-identifier", "database-of:external-thing"],
    "publication": ["literature-citation:cited-publication", "publication-authorship:publication"],
    "person": ["publication-authorship:publication-author", "edit-authorship:edit-author",
               "person-affiliation:affiliated-person"],
    "affiliation": ["person-affiliation:affiliation"],
    "instance-edit": ["curation:edit", "edit-authorship:authored-edit"],
    "database-object": ["curation:curated-object", "release-record:tracked-object"],
    "summation": ["summarisation:summation", "literature-citation:citing-thing"],
    "functional-status": ["entity-functional-status:functional-status",
                          "functional-status-typing:typed-status"],
    "functional-status-type": ["functional-status-typing:status-type"],
    "negative-preceding-event-reason": ["negative-precedence:exclusion-reason"],
    "release-": ["release-record:tracking-release"],
}

# Reactome reifies these as classes because neither the relational model nor a
# property graph can state an n-ary fact directly. Here they ARE the relation,
# so they must not also become entities.
AS_RELATIONS = {
    "Regulation", "PositiveRegulation", "NegativeRegulation", "Requirement",
    "PositiveGeneExpressionRegulation", "NegativeGeneExpressionRegulation",
    "CatalystActivity", "EntityFunctionalStatus", "NegativePrecedingEvent",
}

# Where two labels are co-extensive the data cannot tell them apart, so only
# one is kept as a type; keeping both would let a node belong to two types.
COEXTENSIVE_LOSERS = {"UndirectedInteraction", "DrugActionType", "TranscriptionalModification"}

# Reactome class names that collide with a TypeQL keyword or a role label.
RENAMES = {"Release": "release-"}


def _merge_specialised(plays: dict[str, list[str]]) -> dict[str, list[str]]:
    extra = plays.pop("physical-entity+specialised", [])
    plays["physical-entity"] = sorted(set(plays.get("physical-entity", []) + extra))
    return plays


def hoist_roles(plays: dict[str, list[str]], parent: dict[str, str | None]) -> dict[str, list[str]]:
    """Declare each role once, on the nearest common ancestor of its players.

    TypeDB type-checks role compatibility when a query is compiled, not when a
    row is inserted: a loader pass that matches its player at a type which does
    not itself play the role is rejected wholesale, however correct the data.
    Declaring `species-assignment:classified-thing` separately on event,
    physical-entity and reference-entity therefore leaves no single type a pass
    can match at, so the role moves up to the type that covers all three.
    """
    def label_of(name: str) -> str:
        return RENAMES.get(name, kebab(name))

    chain: dict[str, list[str]] = {}
    for name in parent:
        lbl, path, cur = label_of(name), [], name
        while cur:
            path.append(label_of(cur))
            cur = parent.get(cur)
        chain[lbl] = path                     # self first, root last

    owners: dict[str, list[str]] = {}
    for entity, roles in plays.items():
        for role in roles:
            owners.setdefault(role, []).append(entity)

    hoisted: dict[str, list[str]] = {}
    for role, ents in owners.items():
        known = [e for e in ents if e in chain]
        if not known:
            continue
        common = [t for t in chain[known[0]] if all(t in chain[e] for e in known)]
        target = common[0] if common else "database-object"
        hoisted.setdefault(target, []).append(role)
    return {k: sorted(v) for k, v in hoisted.items()}


def main() -> None:
    dst = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).with_name("schema.tql")
    parent = read_hierarchy()

    def label(name: str) -> str:
        return RENAMES.get(name, kebab(name))

    # Reactome's own root is DatabaseObject; everything hangs off it.
    order: list[str] = []
    seen: set[str] = set()

    def emit(name: str) -> None:
        if name in seen:
            return
        p = parent.get(name)
        while p in AS_RELATIONS:          # skip over reified classes
            p = parent.get(p)
        if p:
            emit(p)
        seen.add(name)
        order.append(name)

    for name in sorted(parent):
        if name in AS_RELATIONS or name in MIXINS or name in COEXTENSIVE_LOSERS:
            continue
        emit(name)

    lines = [
        "# Reactome schema for TypeDB 3.12.",
        "#",
        "# The entity hierarchy mirrors Reactome's own class hierarchy, so a query",
        "# against a supertype (physical-entity, regulation, composition) also matches",
        "# every subtype. Relations are n-ary where the fact is n-ary: catalysis ties a",
        "# catalyst, a molecular function and a reaction in one relation, and",
        "# entity-functional-status ties an event, a diseased entity, the normal entity",
        "# it replaces and the functional consequence.",
        "",
        "define",
        "",
        "# --- attributes ---",
    ]
    lines += [l for l in ATTRIBUTES.strip().splitlines()]
    lines += ["", "# --- entity hierarchy (mirrors Reactome's classes) ---", ""]

    plays_map = hoist_roles(_merge_specialised(dict(PLAYS)), parent)
    for name in order:
        lbl = label(name)
        p = parent.get(name)
        while p in AS_RELATIONS:
            p = parent.get(p)
        head = f"entity {lbl}" if not p else f"entity {lbl} sub {label(p)}"
        owns = OWNERSHIP.get(lbl, [])
        plays = plays_map.get(lbl, [])
        parts = [head]
        if name == "DatabaseObject":
            parts[0] += " @abstract"
        for o in owns:
            parts.append(f"  owns {o}")
        for pl in plays:
            parts.append(f"  plays {pl}")
        lines.append(",\n".join(parts) + ";")

    lines += ["", "# --- relations ---"]
    lines += RELATIONS.strip().splitlines()

    text = "\n".join(lines) + "\n"
    dst.write_text(text, encoding="utf8")
    print(f"wrote {dst} — {len(order)} entity types, {text.count('relation ')} relation defs, "
          f"{len(text)} chars, ~{len(text)//4} est. tokens")


if __name__ == "__main__":
    main()
