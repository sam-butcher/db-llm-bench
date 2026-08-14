#!/usr/bin/env python3
"""Generate one loader pass (.tql) per exported CSV.

The loader binds a CSV column to a `$variable` of the same name, so each pass's
`given` block is derived from its CSV's header. Two shapes:

  entity__<type>.csv   ->  insert the entity, keyed on db-id, with `try` for
                           every optional attribute.
  rel__<type>[__…].csv ->  match each role player by db-id, then insert the
                           relation. Players are matched as `database-object`
                           because db-id is the key on that root type; the
                           schema's `plays` constraints reject a wrong player,
                           so a mis-mapped role shows up as a reject rather
                           than as silently wrong data.

Run export.py first. Usage: generate_passes.py [workdir] [passdir]
"""

import csv
import importlib.util
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("build_schema", HERE / "build_schema.py")
_bs = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_bs)

# Columns that are attributes of the relation rather than role players.
# Columns on a relation CSV that are attributes rather than role players.
# Anything not listed here is taken to be a role and matched by db-id, so a new
# owned attribute must be added or it will be read as a player and reject.
# db-id, display-name and schema-class appear on the relations Reactome reifies
# as nodes (catalysis, regulation, entity-functional-status,
# negative-precedence): the node carries them, so the relation that replaces it
# should too.
RELATION_ATTRS = {"ordering", "stoichiometry",
                  "db_id", "display_name", "schema_class", "st_id"}


def value_types() -> dict[str, str]:
    """Attribute label -> TypeQL value type, read from the schema source.

    Handles the three shapes the schema uses: a plain declaration, a subtype
    that states its own value type (`db-id sub id, value integer`), and a
    subtype that inherits one from an abstract parent (`display-name sub name`,
    where `name` carries the `value string`).

    Resolving inheritance is not optional. Callers fall back to `string` for an
    attribute they cannot find, which is silently correct for most of them and
    catastrophic for `db-id`: every entity pass inserts one, so a missed
    integer rejects the entire load one pass at a time.
    """
    declared: dict[str, tuple[str | None, str | None]] = {}
    abstract: set[str] = set()
    for line in _bs.ATTRIBUTES.splitlines():
        m = re.match(r"\s*attribute ([\w-]+)(.*);\s*$", line)
        if not m:
            continue
        label, rest = m.group(1), m.group(2)
        parent = re.search(r"\bsub ([\w-]+)", rest)
        value = re.search(r"\bvalue (\w+)", rest)
        declared[label] = (parent.group(1) if parent else None,
                           value.group(1) if value else None)
        if "@abstract" in rest:
            abstract.add(label)

    def resolve(label: str, seen: frozenset[str] = frozenset()) -> str | None:
        if label not in declared or label in seen:
            return None
        parent, value = declared[label]
        if value:
            return value
        return resolve(parent, seen | {label}) if parent else None

    resolved = {label: resolve(label) for label in declared}
    # An abstract attribute need not carry a value type; anything else that
    # fails to resolve would be silently defaulted to `string` by the callers,
    # which is how a subtyped `db-id` once rejected every entity in the load.
    missing = sorted(l for l, v in resolved.items() if not v and l not in abstract)
    if missing:
        raise SystemExit(
            "value_types: no value type resolved for " + ", ".join(missing) +
            " — the schema uses a declaration shape this parser does not read")
    return {label.replace("-", "_"): v for label, v in resolved.items() if v}


def role_player_types() -> dict[str, str]:
    """Role label -> the entity type a pass should match its player at.

    TypeDB checks role compatibility when the query compiles, so matching a
    player at a type that does not play the role rejects the whole pass. The
    schema declares each role exactly once (see hoist_roles), and that type is
    the one to match at.
    """
    parent = _bs.read_hierarchy()
    out = {}
    # A relation can itself play a role — the literature evidence for a
    # catalysis points at that catalysis, not at a stand-in for it — and those
    # are declared with `plays` inside the relation rather than in PLAYS, which
    # only covers entities. Without them the pass falls back to matching the
    # player as database-object, which no relation is, and type inference
    # rejects the whole pass rather than a row.
    current = None
    for line in _bs.RELATIONS.splitlines():
        m = re.match(r"\s*relation ([\w-]+)", line)
        if m:
            current = m.group(1)
            continue
        m = re.match(r"\s*plays ([\w-]+:[\w-]+)", line)
        if m and current:
            out[m.group(1)] = current
    for entity, roles in _bs.hoist_roles(_bs._merge_specialised(dict(_bs.PLAYS)), parent).items():
        for role in roles:
            # Keyed by the FULL relation:role. Role names repeat across
            # relations with different players — `localised-thing` belongs to
            # both compartment-assignment (any database-object) and
            # included-location (physical-entity only) — and keying on the bare
            # name let one silently overwrite the other, which does not reject:
            # the pass just matches nothing and inserts nothing.
            out[role] = entity
    # A specialised role (`relates candidate as member`) is played by whatever
    # plays the role it overrides, and `plays` is only ever declared on the
    # base. Resolve each override to its base, repeatedly, since the chains
    # nest (candidate -> member -> part).
    by_name = {}
    for full, entity in out.items():
        by_name.setdefault(full.split(":", 1)[1], entity)
    overrides = dict(re.findall(r"relates ([\w-]+) as ([\w-]+)", _bs.RELATIONS))
    for _ in range(len(overrides) + 1):
        for child, base in overrides.items():
            if child not in by_name and base in by_name:
                by_name[child] = by_name[base]
    missing = sorted(set(overrides) - set(by_name))
    if missing:
        raise SystemExit(f"no player type resolved for roles: {missing}")
    out.update({k: v for k, v in by_name.items() if k not in out})
    return out


def main() -> None:
    work = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "work"
    passes = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else HERE / "passes"
    passes.mkdir(parents=True, exist_ok=True)
    types = value_types()
    players = role_player_types()
    written = 0

    for csv_path in sorted(work.glob("*.csv")):
        with csv_path.open() as fh:
            header = next(csv.reader(fh), None)
        if not header:
            continue
        stem = csv_path.stem
        lines: list[str] = []

        if stem.startswith("attr__"):
            _, entity, attr = stem.split("__", 2)
            lines.append(f"given\n    ${header[0]}: integer,\n    ${header[1]}: "
                         f"{types.get(header[1], 'string')};")
            lines.append("match")
            lines.append(f"$x isa {entity}, has db-id == ${header[0]};")
            lines.append("insert")
            lines.append(f"$x has {attr} == ${header[1]};")
        elif stem.startswith("link__"):
            # link__<relation>__<role>. The base pass has already inserted the
            # relation with its first player, so this matches it on its db-id
            # key and adds one more player to a multi-valued role. Runs after
            # every rel__ pass for that reason.
            _, relation, role = stem.split("__", 2)
            col = header[1]
            lines.append(f"given\n    $db_id: integer,\n    ${col}: integer;")
            lines.append("match")
            lines.append(f"$x isa {relation}, has db-id == $db_id;")
            player = players.get(f"{relation}:{role}") or players.get(role, "database-object")
            lines.append(f"$p isa {player}, has db-id == ${col};")
            lines.append("insert")
            lines.append(f"$x links ({role}: $p);")
        elif stem.startswith("entity__"):
            entity = stem[len("entity__"):]
            given = [f"    ${header[0]}: {types.get(header[0], 'string')}"]
            given += [f"    ${c}: {types.get(c, 'string')}?" for c in header[1:]]
            lines.append("given\n" + ",\n".join(given) + ";")
            lines.append("insert")
            lines.append(f"$x isa {entity}, has db-id == $db_id;")
            for c in header[1:]:
                lines.append(f"try {{ $x has {c.replace('_', '-')} == ${c}; }};")
        else:
            # rel__<type>[__optional-suffix], or post__<type> for the relations
            # whose player is itself a relation, which load in a later phase.
            prefix = "post__" if stem.startswith("post__") else "rel__"
            name = stem[len(prefix):]
            relation = re.sub(r"__(none|[a-z_]+)$", "", name)
            roles = [c for c in header if c not in RELATION_ATTRS]
            attrs = [c for c in header if c in RELATION_ATTRS]
            given = [f"    ${c}: integer" for c in roles]
            given += [f"    ${c}: {types.get(c, 'string')}" for c in attrs]
            lines.append("given\n" + ",\n".join(given) + ";")
            lines.append("match")
            for i, role in enumerate(roles):
                role_label = role.replace("_", "-")
                player = players.get(f"{relation}:{role_label}") or players.get(role_label, "database-object")
                lines.append(f"$p{i} isa {player}, has db-id == ${role};")
            lines.append("insert")
            links = ", ".join(f"{r.replace('_', '-')}: $p{i}" for i, r in enumerate(roles))
            has = "".join(f", has {a.replace('_', '-')} == ${a}" for a in attrs)
            lines.append(f"$x isa {relation}, links ({links}){has};")

        (passes / f"{stem}.tql").write_text("\n".join(lines) + "\n", encoding="utf8")
        written += 1

    print(f"wrote {written} passes into {passes}")


if __name__ == "__main__":
    main()
