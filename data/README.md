# Data

- `prompts/<db>.txt` - the prompt template for each DB. Templates are dataset-independent, so every
  dataset shares them.
- `candidates/` - the primary dataset. `data.csv.gz` (gzip-compressed to stay under GitHub's file-size
  limit; `clean.py` reads it directly) plus the cleaner that produces it (`clean.py`), and
  the per-DB schema and load files.
- `sample/` - a small car dataset used to exercise the framework end-to-end. Carries its own
  `questions.json`, per-DB schema and data files, and the `example-N.txt` few-shot examples.
  `sample/dummy/` holds the fixtures for the dummy DB and model packages; no real database involved.

A dataset folder holds, per DB, the schema and data files needed to load it, and optionally the
`example-1.txt`, `example-2.txt`, ... examples fed into the prompt's examples slot.
