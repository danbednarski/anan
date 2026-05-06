# anan

Native macOS desktop genealogy app for Gramps trees. Two binaries:

- **`anan`** — the iced-based GUI. Open a Gramps SQLite file, see a canvas
  family tree, click around. This is the day-to-day surface.
- **`anan-cli`** — non-interactive CLI for scripts, agents, and LLMs. Every
  read prints JSON; every write goes through a transaction with an
  auto-snapshot of the tree first. See [`LLMS.md`](LLMS.md) for the
  agent-facing usage guide.

Both binaries link against the same library code in `src/db/` and
`src/gramps/`, so the GUI and CLI never disagree about how a record looks.

## Quick start

```bash
cargo run --bin anan                     # GUI
cargo run --bin anan-cli -- stats        # CLI, $ANAN_TREE must be set
ANAN_TREE=~/family.db cargo run --bin anan-cli -- stats
```

A short tour of the CLI:

```bash
anan-cli stats                           # row counts per primary type
anan-cli person list --limit 5
anan-cli person get I0001                # by gramps_id or 32-char handle
anan-cli search "Smith"                  # substring across persons + places
anan-cli person add --first Jane --surname Doe --gender female --birth 1990-05-15
anan-cli family add --father I0001 --mother I0002 --rel married
anan-cli family add-child F0007 I0048
anan-cli event add --type Marriage --date 2010-06-01 --place P0003
```

## Crate layout

```
src/
  lib.rs           re-exports db, gramps, app, theme, views
  main.rs          GUI binary (iced)
  bin/cli.rs       CLI binary (clap)
  db/
    database.rs    open + write_txn + auto-snapshot
    repo/          per-type CRUD (person, family, event, place, source, ...)
  gramps/          serde structs for the Gramps 6.x JSON object model
  app.rs / views/  GUI state machine and canvas tree (not used by the CLI)
```

## Schema

anan supports Gramps schema version **21** only (Gramps 6.0.x). Opening
any other version refuses with a loud error rather than risking silent
data loss. The Rust structs in `src/gramps/` were modeled against that
single version's JSON layout.

## Safety

Every write transaction first writes a `<tree>.bak-<unix>` snapshot if
the last snapshot is older than 5 minutes. The 10 most recent snapshots
are kept, older ones pruned. This is a guard against bugs in this
codebase, not a substitute for your real backup strategy.

## Data is yours

`*.db`, `*.sqlite`, and `test-fixtures/` are gitignored. **Never commit
your tree** — it contains real people's names, dates, and places.
