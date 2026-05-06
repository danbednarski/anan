# anan

A native macOS desktop app for Gramps family trees. It reads and writes the Gramps 6.x SQLite format directly, so there's no backend, no sync daemon, and no separate process to keep alive. The same crate ships a CLI binary so scripts and agents can do the same things from a shell.

## The desktop app

`cargo run --bin anan` opens a frameless window with a sidebar list of people, a canvas-based family tree in the main panel, and a right-click context menu for adding parents, children, siblings, or editing details. The tree aligns generations on horizontal rows and draws Bezier curves for parent-child connectors, with card borders colored by gender. The same panel toggles to a flat list view for keyboard-friendly browsing, and the sidebar has inline search.

All ten primary Gramps types (person, family, event, place, source, citation, media, note, repository, tag) load on open and write through one transactional path. Adding a person via the modal wires up birth and death events, an optional source citation, and the relevant family edges in a single commit. Every write first auto-snapshots the tree to `<file>.bak-<unix>` if the last snapshot is older than 5 minutes, keeping the 10 most recent.

The window uses macOS' transparent titlebar with the fullsize content view, so it fits the platform look without losing the traffic-light buttons.

## The CLI

`anan-cli` is the same library with a clap shell on top. Every read prints JSON to stdout and every write goes through the same auto-snapshot path. Point it at a tree with `--tree <path>` or set `ANAN_TREE` once per shell.

```
anan-cli stats
anan-cli person list --limit 10
anan-cli person get I0001
anan-cli person add --first Jane --surname Doe --gender female --birth 1990-05-15
anan-cli family add --father I0001 --mother I0002 --rel married
anan-cli family add-child F0007 I0048
anan-cli search "Smith"
anan-cli dump --table event > events.json
```

Identifiers can be either the 32-character handle or the Gramps ID (`I0001`, `F0007`, `E0011`, `P0003`). Pass `--compact` for one-line JSON. `LLMS.md` has the agent-facing reference covering how dates, event types, family relationship types, and place types are encoded.

## Building

Rust 1.80 or later. `cargo build --release` builds both binaries. The CLI works on any platform Rust supports, but the iced GUI has only been used on macOS (Apple Silicon, Darwin 25.x).

anan only opens Gramps 6.x trees (schema version 21). Older or newer versions are refused on open rather than risking silent data loss. If you need to read an older tree, open it in Gramps proper once and let it migrate.

## Data

`*.db`, `*.sqlite`, and `test-fixtures/` are gitignored so a tree never lands in the repo. The auto-snapshots are a safety net for bugs in this codebase, not a real backup strategy.
