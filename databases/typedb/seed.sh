#!/bin/sh
set -eu

# Drop any existing bench database so re-runs reseed cleanly; the failure on
# a fresh server (nothing to delete) is expected.
typedb console "$@" --script=/reset.tqls || true

typedb console "$@" --script=/load.tqls
