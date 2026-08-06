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
RELATION_ATTRS = {"ordering", "stoichiometry"}


def value_types() -> dict[str, str]:
    """Attribute label -> TypeQL value type, read from the schema source."""
    out = {}
    for m in re.finditer(r"attribute ([\w-]+), value (\w+);", _bs.ATTRIBUTES):
        out[m.group(1).replace("-", "_")] = m.group(2)
    return out


def role_player_types() -> dict[str, str]:
    """Role label -> the entity type a pass should match its player at.

    TypeDB checks role compatibility when the query compiles, so matching a
    player at a type that does not play the role rejects the whole pass. The
    schema declares each role exactly once (see hoist_roles), and that type is
    the one to match at.
    """
    parent = _bs.read_hierarchy()
    out = {}
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
            # rel__<type>[__optional-suffix]
            name = stem[len("rel__"):]
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
