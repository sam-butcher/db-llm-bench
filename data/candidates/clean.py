#!/usr/bin/env python3
"""Clean the Democracy Club candidates CSV for loading. Cleaning rules are
applied per column type; some normalisations are engine-specific (dialect),
since e.g. TypeDB wants `T`-separated, offset-free datetimes while Postgres
accepts the raw timestamptz. Default dialect: typedb.

Usage: clean.py [IN_CSV] [OUT_CSV] [--dialect typedb]

IN_CSV may be gzip-compressed (`.gz`); the committed source is `data.csv.gz`.
"""
import csv, gzip, re, sys

BOOL_COLS = ["election_current", "cancelled_poll", "by_election",
             "party_lists_in_use", "candidates_locked", "elected", "tied_vote_winner"]
DATE_COLS = ["election_date", "birth_date", "death_date"]
DATETIME_COLS = ["statement_last_updated", "person_last_updated"]
INT_COLS = ["person_id", "seats_contested", "party_list_position", "votes_cast",
            "rank", "turnout_reported", "spoilt_ballots", "total_electorate"]

stats = {}
def bump(key):
    stats[key] = stats.get(key, 0) + 1

def clean_bool(v):
    s = v.strip().lower()
    if s in ("t", "true"): return "true"
    if s in ("f", "false"): return "false"
    if s == "": return ""
    bump(f"bool:dropped:{v!r}")
    return ""

import datetime

def _valid(y, mo, d):
    # Assume UK order (day/month) first, then try swapped; on failure keep the
    # year (and month if plausible) rather than lose the row's date entirely.
    for yy, mm, dd in ((y, mo, d), (y, d, mo)):
        try:
            datetime.date(yy, mm, dd)
            return f"{yy:04d}-{mm:02d}-{dd:02d}"
        except ValueError:
            pass
    if 1 <= mo <= 12:
        bump("date:salvaged->month"); return f"{y:04d}-{mo:02d}-01"
    bump("date:salvaged->year"); return f"{y:04d}-01-01"

# (year, month, day) extractors for the formats seen in the data.
DATE_FORMS = [
    (re.compile(r"^(\d{4})-(\d{1,2})-(\d{1,2})$"), lambda m: (int(m[1]), int(m[2]), int(m[3]))),  # ISO-ish (incl. 00s)
    (re.compile(r"^(\d{1,2})/(\d{1,2})/(\d{4})$"), lambda m: (int(m[3]), int(m[2]), int(m[1]))),  # D/M/YYYY
    (re.compile(r"^(\d{1,2})-(\d{1,2})-(\d{4})$"), lambda m: (int(m[3]), int(m[2]), int(m[1]))),  # D-M-YYYY
    (re.compile(r"^(\d{4})-(\d{1,2})$"), lambda m: (int(m[1]), int(m[2]), 1)),                    # YYYY-MM
    (re.compile(r"^(\d{1,2})/(\d{4})$"), lambda m: (int(m[2]), int(m[1]), 1)),                    # MM/YYYY
    (re.compile(r"^(\d{4})$"), lambda m: (int(m[1]), 1, 1)),                                      # bare year
]

def clean_date(v):
    s = v.strip()
    if s == "": return ""
    for pat, extract in DATE_FORMS:
        m = pat.match(s)
        if m:
            return _valid(*extract(m))
    bump(f"date:dropped:{v!r}")
    return ""

TZ = re.compile(r"([+-]\d{2}(:?\d{2})?|Z)$")

def clean_datetime(v):
    s = v.strip()
    if s == "": return ""
    s = TZ.sub("", s).strip()               # drop timezone offset
    s = s.replace(" ", "T")                  # space -> T
    bump("datetime:normalised")
    return s

def clean_int(v):
    s = v.strip()
    if s == "": return ""
    if re.match(r"^-?\d+$", s): return s
    bump(f"int:dropped:{v!r}")
    return ""

def strip_quotes(v):
    # An export artifact double-quotes some text fields, so after CSV parsing
    # they arrive wrapped in literal quotes (e.g. nuts1 as `"Scotland"`). Remove
    # one matching leading+trailing pair. Values quoted only on one side, or
    # with the pair inside (e.g. `Christian Party "..."`), are left untouched.
    if v and len(v) >= 2 and v[0] == '"' and v[-1] == '"':
        bump("quotes:stripped")
        return v[1:-1]
    return v

def open_source(path):
    # The committed source is gzip-compressed; a plain .csv still works.
    if path.endswith(".gz"):
        return gzip.open(path, "rt", newline="")
    return open(path, newline="")

def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    src = args[0] if args else "data/candidates/data.csv.gz"
    dst = args[1] if len(args) > 1 else "data/candidates/data.cleaned.csv"
    with open_source(src) as fi, open(dst, "w", newline="") as fo:
        r = csv.DictReader(fi)
        w = csv.DictWriter(fo, fieldnames=r.fieldnames)
        w.writeheader()
        n = 0
        for row in r:
            for c in row: row[c] = strip_quotes(row[c])
            for c in BOOL_COLS: row[c] = clean_bool(row[c])
            for c in DATE_COLS: row[c] = clean_date(row[c])
            for c in DATETIME_COLS: row[c] = clean_datetime(row[c])
            for c in INT_COLS: row[c] = clean_int(row[c])
            w.writerow(row)
            n += 1
    print(f"cleaned {n} rows -> {dst}")
    for k in sorted(stats):
        print(f"  {k}: {stats[k]}")

if __name__ == "__main__":
    main()
