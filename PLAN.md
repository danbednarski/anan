# gramps-desktop — Rust Native Desktop UI

**Status:** Planning complete, ready to scaffold
**Target path:** `/Users/danielbednarski/gramps-monorepo/packages/desktop/`
**Planned:** 2026-04-11 (with fresh session context; safe to resume after `/clear`)

---

## North Star

Replace the web UI with a native macOS desktop Rust app that reads and writes
the Gramps SQLite family tree directly — no backend, no webapi, no Python at
runtime. Full CRUD parity with the web frontend and the desktop Gramps app,
built on `iced` for the UI layer.

## Locked scope decisions

| Decision | Choice | Why |
|---|---|---|
| Data access pattern | Option 3: direct SQLite read/write, no backend | User choice after discussion |
| UI framework | `iced` 0.13+ | Pure Rust, good for data-heavy apps, elm architecture fits CRUD |
| DB crate | `rusqlite` with `bundled` feature | Ships its own sqlite, no system dep |
| Serialization | `serde` + `serde_json` | Gramps 6.x writes JSON columns |
| Async | `tokio` only if/when needed for long ops; default single-threaded | Simplicity first |
| Target platform | macOS first, Linux/Windows later | User's environment |
| Concurrency model | Single-user, exclusive file lock while app is open | Avoid dual-writer corruption with webapi |
| Rust project layout | Standalone `packages/desktop/Cargo.toml`, not a Cargo workspace | No other Rust packages in monorepo yet |
| Gramps version targeted | 6.0.7 (schema used by current running tree) | Pinned; bump explicitly later |
| v1 media support | Read-only references, launch system viewer on click | Editing media out of scope for v1 |
| Tree source | Existing `gramps_gramps_db` Docker volume → `/root/.gramps/grampsdb/<uuid>/sqlite.db` | Same data the web app uses |

## Critical discovery (already done during planning)

Gramps 6.x uses **JSON columns**, not Python pickles. This was the load-bearing
uncertainty. It's confirmed.

- All 9 primary tables use `json_data TEXT NOT NULL` column:
  `person`, `family`, `event`, `place`, `source`, `citation`, `media`, `note`, `repository`
- Plus: `tag` (simple), `reference` (cross-ref denorm), `metadata` (DB-level),
  `name_group` (name grouping), `gender_stats`
- Objects are self-describing via `_class` field
- Nested objects carry `_class` tags too (`Name`, `Surname`, `Date`, `EventRef`, etc.)
- Tagged enums carry both `value: int` and `string: ""` — built-in enum labels
  live in Gramps core source; custom user types store their label in `string`
- Cross-references are denormalized in the `reference` table with indexes on
  both `obj_handle` and `ref_handle` — querying "who references X" is O(log n)
- Test fixture copied to `packages/desktop/test-fixtures/sample.db`
  (328K, 48 persons, 13 events, 18 families, 5 places)

**Implication:** no pickle parser, no Python runtime, no reimplementing the
Gramps object model from scratch. Rust structs derive `Deserialize`/`Serialize`
with serde, read/write JSON directly.

## Architecture

```
┌─────────────────────────────────────────────────┐
│  iced UI (src/app.rs, src/views/*.rs)           │
│  - Elm architecture: Message → State → View     │
│  - Keyboard-driven where possible               │
└────────────┬────────────────────────────────────┘
             │ dispatches
             ▼
┌─────────────────────────────────────────────────┐
│  Domain commands (src/commands/*.rs)            │
│  - create_person, update_person, delete_person  │
│  - Transactional: one command = one txn         │
│  - Returns Result<Outcome, DomainError>         │
└────────────┬────────────────────────────────────┘
             │ calls
             ▼
┌─────────────────────────────────────────────────┐
│  Repository layer (src/db/repo/*.rs)            │
│  - One repo per primary type                    │
│  - Handles JSON ser/de, cross-ref maintenance   │
│  - Knows the denormalized secondary columns     │
└────────────┬────────────────────────────────────┘
             │ prepared statements
             ▼
┌─────────────────────────────────────────────────┐
│  rusqlite Connection                            │
│  - Single connection wrapped in Mutex           │
│  - File lock on open, released on drop          │
│  - Backup-before-write policy                   │
└─────────────────────────────────────────────────┘
```

```
src/
├── main.rs                 # iced runtime entry
├── app.rs                  # Top-level iced::Application
├── views/
│   ├── mod.rs
│   ├── person_list.rs
│   ├── person_detail.rs
│   ├── family_detail.rs
│   ├── event_detail.rs
│   ├── place_tree.rs
│   ├── source_list.rs
│   ├── media_browser.rs
│   ├── note_editor.rs
│   └── search.rs
├── gramps/                 # JSON object model (serde structs)
│   ├── mod.rs
│   ├── person.rs           # Person, Name, Surname, Address, Attribute
│   ├── family.rs           # Family, ChildRef
│   ├── event.rs            # Event, EventRef, EventType, EventRoleType
│   ├── place.rs            # Place, PlaceRef, PlaceName, PlaceType
│   ├── source.rs           # Source, SourceMediaType
│   ├── citation.rs         # Citation, CitationRef
│   ├── media.rs            # Media, MediaRef
│   ├── note.rs             # Note, NoteType, StyledText
│   ├── repository.rs       # Repository, RepositoryType
│   ├── tag.rs              # Tag
│   ├── date.rs             # Date (complex — calendar, modifier, range, quality)
│   ├── url.rs              # Url, UrlType
│   └── enums.rs            # value→label for all built-in tagged enums
├── db/
│   ├── mod.rs              # Database struct, open/close, backup, lock
│   ├── schema.rs           # Table names, column names, known indexes
│   ├── txn.rs              # Transaction wrapper + backup-before-write
│   └── repo/
│       ├── mod.rs
│       ├── person.rs       # PersonRepo: get, list, create, update, delete
│       ├── family.rs
│       ├── event.rs
│       ├── place.rs
│       ├── source.rs
│       ├── citation.rs
│       ├── media.rs
│       ├── note.rs
│       ├── repository.rs
│       ├── tag.rs
│       └── refs.rs         # ReferenceRepo: cross-ref maintenance
├── commands/               # Domain operations (transactional)
│   ├── mod.rs
│   └── *.rs                # One file per object type
├── search.rs               # Full-text + indexed search
├── preferences.rs          # User prefs, recent files
├── errors.rs               # DomainError, DbError
└── lib.rs                  # re-exports (if library split ever needed)
examples/
├── dump_db.rs              # Phase 1 smoke test: read sample DB, print tree summary
└── round_trip.rs           # Phase 2 smoke: read → modify → write → read back
test-fixtures/
└── sample.db               # 328K, committed — from the running container
benches/                    # (later)
tests/                      # integration tests
Cargo.toml
README.md
```

## Phased roadmap

### Phase 1 — Research spike + data model (1 session)

**Goal:** Prove the entire object model deserializes from real Gramps JSON
into typed Rust structs with round-trip fidelity.

**Work order:**

1. `cargo init --lib` in `packages/desktop/`; add dependencies
2. Confirm sample DB at `test-fixtures/sample.db`
3. Write `src/gramps/person.rs` with Person, Name, Surname, Address,
   Attribute, PersonRef, EventRef structs; derive Serialize + Deserialize
4. Write `src/gramps/date.rs` — Date is the hardest single type (calendar
   enum, modifier, quality, sortval, dateval tuple, newyear, text override,
   range support). Get it right once, reuse everywhere.
5. Write `src/gramps/enums.rs` with `EventType`, `EventRoleType`, `NameType`,
   `NameOriginType`, `AttributeType`, `UrlType`, `NoteType`, `PlaceType`,
   `SourceMediaType`, `RepositoryType`, `MarkerType`, `FamilyRelType`,
   `ChildRefType` — hardcoded value→label lookups from Gramps core source
6. Write stubs for family, event, place, source, citation, media, note,
   repository — start minimal (just fields that appear in sample.db)
7. Write `examples/dump_db.rs`:
   - Open `test-fixtures/sample.db` read-only
   - Query `SELECT json_data FROM person` and deserialize each row
   - Same for family, event, place, source, citation, media, note, repository
   - Print a tree summary (counts, first 3 of each type)
8. Iterate on struct definitions until every row in the sample parses without
   `serde::de::Error` — **this is the exit criterion for Phase 1**
9. Commit: `feat(desktop): gramps object model with serde bindings`

**Risks:** Some object fields may be missing from the sample. Solution: make
optional fields `Option<T>` or `#[serde(default)]` liberally. The sample is
small — if the app later encounters a tree with fields we haven't modeled,
serde will fail loudly and we add them.

**Research tasks during this phase (do these inline, document findings in
this PLAN.md under "Appendix — notes from Phase 1"):**

- [ ] Is there a Gramps 6.x schema reference doc? Check `gramps-project/gramps`
      repo, `data/grampsdb/*.py` files
- [ ] Does the `reference` table get auto-maintained by Gramps triggers, or
      is it maintained in application code? Look at `gen/db/sqlite.py` in
      gramps core
- [ ] What are the exact value→label mappings for all tagged enums? Source:
      `gen/lib/eventtype.py`, `gen/lib/nametype.py`, etc. in gramps core
- [ ] Is there a DB version metadata field we can read to refuse older
      schemas gracefully? Check `metadata` table contents
- [ ] What files does Gramps create alongside `sqlite.db` and do we need to
      respect any of them? (`meta_data.db`, `undo.db`, `name.txt`,
      `database.txt` — likely format version)

### Phase 2 — Read-only UI skeleton (1 session)

**Goal:** Functional window showing the person list and person detail view
with real data from the sample.

**Work order:**

1. Replace `cargo init --lib` output with a `main.rs` + iced Application
2. Open-database file picker menu item (use `rfd` crate)
3. `src/views/person_list.rs` — sortable list, sort by surname, filter by
   substring match on given_name+surname
4. `src/views/person_detail.rs` — name, gender, birth date, death date,
   list of events, list of families
5. Wire messages: `OpenDb(path)`, `SelectPerson(handle)`, `SearchPersons(query)`
6. Keyboard: ↑/↓ navigate person list, Enter opens detail, Cmd+F search
7. Commit: `feat(desktop): iced app shell and person views`

### Phase 3 — All read views (1–2 sessions)

**Goal:** Read-only browser parity with the web UI.

- Family list/detail with father/mother/children links
- Event list/detail with Date rendering widget (inline helper in
  `views/widgets/date_display.rs`)
- Place list/detail with hierarchical tree (parent → child via
  `enclosed_by` column)
- Source + Citation list/detail (mirrors web app's UX)
- Media list with thumbnails loaded from `/app/media/`
- Note list with StyledText rendering (bold/italic/underline spans)
- Repository list/detail
- Tag list + filter-by-tag
- Global search across all primary types

Commit once per view type.

### Phase 4 — Write path, simple objects (1 session)

**Goal:** Transactional create/update/delete for types with minimal
cross-refs, proving the write architecture before tackling Person/Family.

**Scope:** Tag, Note, Repository

**Must implement once and reuse:**

1. `Database::write_txn<F>(f: F)` — opens a SQLite transaction, runs a
   closure, commits or rolls back
2. Backup-before-write policy: before opening any write txn, if it's been
   more than N minutes since the last backup, copy `sqlite.db` to
   `sqlite.db.bak-<timestamp>`. Keep the last 10 backups.
3. `change` column updated to current unix timestamp on every write
4. `reference` table maintenance: on every cross-ref change, delete old
   rows for the object, insert new rows
5. Handle generation: UUID v4 as 32-char hex (matches Gramps format)
6. `gramps_id` auto-assignment: query `SELECT MAX(gramps_id) FROM person`,
   increment the numeric suffix. Prefix letter is per-type:
   I=person, F=family, E=event, P=place, S=source, C=citation, O=media,
   N=note, R=repository, T=tag

Commit: `feat(desktop): transactional write path for tags/notes/repositories`

### Phase 5 — Write path for Person (1 session)

**Goal:** Full CRUD on Person, including cross-ref cascade handling.

- Create Person form (name, gender, dates)
- Edit Person form (all fields the detail view shows)
- Delete Person with dependency check:
  "This person is in 3 families and referenced by 5 events. Delete anyway?"
  Offer: Cancel / Remove from family, keep person / Delete everything
- Inline "new event" when adding birth/death refs (Phase 6 has full event
  form; Phase 5 uses a minimal date-only subform)
- Commit: `feat(desktop): Person CRUD`

### Phase 6 — Write path for remaining types (2 sessions)

**Goal:** Full CRUD parity.

**Session 6a:** Family, Event, Place

- Family: Create/edit/delete, child management, parent linkage, event
  association. Date handling widget introduced here (full version).
- Event: Create/edit/delete, place link, participant list, date widget
- Place: Create/edit/delete, parent place selection, hierarchy moves,
  latitude/longitude editing

**Session 6b:** Source, Citation, Media, Note

- Source + Citation: mirrors the web app's simplified citation flow —
  user can enter URL + description, auto-create source, dedupe. See
  `packages/frontend/src/citationOrchestrator.js` for the pattern; port
  the dedup logic to Rust.
- Media: reference existing files, read-only thumbnails v1
- Note: StyledText editor (tag intervals: Bold(start, end), Italic(...))

Commit per session.

### Phase 7 — Cross-cutting concerns (1 session)

- **File lock on open.** Gramps core uses `.lck` files in the tree directory
  to coordinate with the desktop app. Read the exact protocol from Gramps
  source (`gen/db/generic.py` likely). Implement and respect it. This means:
  if the webapi container is running and has the DB open, our Rust app
  refuses to open it (with clear error message). If Rust has it open and
  webapi tries to open it, webapi blocks. This is the point.
- **Backup rotation.** Keep last 10 backups in `sqlite.db.bak-*` format.
  Prune older.
- **Undo/redo.** Gramps has an `undo.db` file per tree. Check if the schema
  is documented. If yes, write compatible undo records so desktop Gramps
  and our Rust app share undo history. If no, maintain our own `.undo.db`
  sidecar that the Rust app owns exclusively.
- **Preferences.** `~/Library/Application Support/gramps-desktop/prefs.toml`
  — recent trees list, window size, view preferences.
- **Error reporting.** All writes go through `Result<Outcome, DomainError>`;
  UI surfaces errors in a banner with a "report to log file" action.

### Phase 8 — Distribution (1 session)

- `cargo bundle` for `.app` generation
- Info.plist metadata (bundle id, display name, icon, min macOS)
- Icon (derive from existing Gramps icon or make simple one)
- README with install instructions
- Optional: GitHub Actions to build `.app` on tag push

## Tech stack (Cargo.toml preview)

```toml
[package]
name = "gramps-desktop"
version = "0.0.1"
edition = "2021"
rust-version = "1.80"

[dependencies]
iced = { version = "0.13", features = ["tokio", "image", "svg", "advanced"] }
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "fast-rng"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
rfd = "0.15"                        # file pickers
dirs = "5"                          # platform-specific paths

[dev-dependencies]
pretty_assertions = "1"

[profile.release]
lto = "thin"
codegen-units = 1
strip = true
```

## Risks & mitigations

| # | Risk | Impact | Mitigation |
|---|------|--------|-----------|
| 1 | Concurrent writer: webapi container writes to same DB | DB corruption | Phase 7 file lock. **Temporary until Phase 7:** tell user to `docker compose -p gramps stop grampsweb grampsweb_celery` before opening Rust app |
| 2 | JSON schema drift between Gramps versions | Rust structs break | Pin to 6.x; read `database.txt` version file on open; refuse to open incompatible versions; document upgrade path |
| 3 | Tagged enum `value` constants undocumented | Enums need manual mapping | Read gramps core source once during Phase 1; hardcode into `gramps/enums.rs`; add test that exhaustively parses every `value` in sample.db |
| 4 | `reference` table invariants unclear | Broken cross-refs after write | Phase 1 research task: read gramps core's db.py. Implement `ReferenceRepo::rebuild_for(obj_handle)` that re-derives all cross-refs from a fresh object JSON |
| 5 | Date object is extremely complex | Wrong date rendering/editing | Dedicated `src/gramps/date.rs` + `views/widgets/date_display.rs`, tested with every date shape found in sample and a synthetic test tree with edge cases |
| 6 | `undo.db` schema compat with desktop Gramps | Desktop undo breaks | Phase 7 decision: compat-mode writes vs sidecar file. Default to sidecar (safer) |
| 7 | First build of iced pulls ~800 crates | Slow initial `cargo build` (~5 min) | One-time cost. Documented in README. Expect it |
| 8 | Full CRUD on 9 types is weeks of work | Project stalls | Phases 4-6 are sequential — you can ship a read-only v1 at end of Phase 3 and use it while writing phases 4-6 |

## Sample JSON — Person object (reference during struct writing)

From `test-fixtures/sample.db`, person `5a720cc1-0e35-466a-9bbd-82992227e9d6`:

```json
{
  "_class": "Person",
  "handle": "5a720cc1-0e35-466a-9bbd-82992227e9d6",
  "gramps_id": "I0000",
  "gender": 1,
  "change": 1775578939,
  "private": false,
  "primary_name": {
    "_class": "Name",
    "first_name": "Maxwell Zachary",
    "suffix": "", "title": "", "call": "", "nick": "", "famnick": "",
    "group_as": "", "sort_as": 0, "display_as": 0,
    "type": {"_class": "NameType", "value": 2, "string": ""},
    "date": {
      "_class": "Date",
      "format": null, "calendar": 0, "modifier": 0, "quality": 0,
      "dateval": [0, 0, 0, false],
      "text": "", "sortval": 0, "newyear": 0
    },
    "surname_list": [
      {
        "_class": "Surname",
        "surname": "Hansen", "prefix": "", "primary": true,
        "connector": "",
        "origintype": {"_class": "NameOriginType", "value": 1, "string": ""}
      }
    ],
    "private": false, "citation_list": [], "note_list": []
  },
  "alternate_names": [],
  "event_ref_list": [
    {
      "_class": "EventRef",
      "ref": "4517f76a-8fdb-4322-ae43-ae285bd248da",
      "role": {"_class": "EventRoleType", "value": 1, "string": ""},
      "private": false, "citation_list": [], "note_list": [], "attribute_list": []
    }
  ],
  "birth_ref_index": 0,
  "death_ref_index": -1,
  "family_list": ["1026152306b3546c5272219269c7"],
  "parent_family_list": ["1026178513706463acf3cd7a4cb"],
  "person_ref_list": [],
  "address_list": [], "urls": [], "lds_ord_list": [],
  "media_list": [], "attribute_list": [],
  "citation_list": [], "note_list": [], "tag_list": []
}
```

**Notes:**
- Handles are either 32-char UUID format (`5a720cc1-0e35-...`) or 28-char short
  hash format (`1026152306b3546c5272219269c7`). Both appear in the same tree.
  Treat as opaque `String` in Rust.
- `dateval` is a 4-tuple: `[day, month, year, slash]`. Slash is bool for
  dual-date ("1700/01").
- `calendar`, `modifier`, `quality` are int enums into the Date type's own
  hardcoded tables.
- `birth_ref_index` / `death_ref_index` are indices into `event_ref_list`,
  not handles.

## Current state of surrounding monorepo

- **Monorepo root:** `/Users/danielbednarski/gramps-monorepo`
- **packages/frontend/** — Lit web UI, Phase 1-3 complete, 215/215 tests passing
- **packages/backend/** — vendored `gramps-webapi v3.10.0` source, **NOT YET BUILT**, paused mid-Phase-1 of backend vendoring milestone
- **packages/desktop/** — this project, scaffolded only with `PLAN.md` and
  `test-fixtures/sample.db`, no Cargo.toml yet
- **infra/docker-compose.yml** — still references `build: ../packages/backend`
  but that build was never executed. The running stack (if any) uses the
  OLD upstream image from before the swap was attempted
- **Running container state at time of planning:** `gramps-grampsweb-1` up,
  serving the patched frontend at http://localhost, data volumes attached.
  Desktop Rust app will eventually coexist with or replace this backend

## What to do when resuming

**Starting a fresh session after `/clear`:**

1. Paste this prompt to Claude:

   > Read `/Users/danielbednarski/gramps-monorepo/packages/desktop/PLAN.md`
   > completely, then execute Phase 1 "Research spike + data model" work order.
   > Stop after Phase 1 exit criterion (dump_db.rs runs cleanly against
   > test-fixtures/sample.db with all 9 primary types parsing). Do not start
   > Phase 2 without asking.

2. Claude should:
   - Read this plan
   - Scaffold `packages/desktop/` with `cargo init --lib`
   - Add the dependencies listed in "Tech stack" section
   - Write the gramps/ module structs referencing the sample JSON
   - Write `examples/dump_db.rs`
   - Iterate until `cargo run --example dump_db` prints a clean summary
   - Commit atomically by task (struct module, dump example, etc.)
   - Report back with Phase 1 findings for the research tasks

3. **Before starting code,** Claude should confirm the current Phase 3 is
   still relevant (verify git log shows the `docs(03-03): complete...`
   commit) and that `test-fixtures/sample.db` still exists.

**Files Claude needs:**

- `/Users/danielbednarski/gramps-monorepo/packages/desktop/PLAN.md` (this file)
- `/Users/danielbednarski/gramps-monorepo/packages/desktop/test-fixtures/sample.db`
- Read access to `/Users/danielbednarski/gramps-monorepo/packages/frontend/src/citationOrchestrator.js` when Phase 6b ports the citation dedup logic

**Docker container may or may not still be running.** Plan does not depend on
it. If Claude needs additional schema research, spin up a temporary container:
`docker run --rm -it -v gramps_gramps_db:/data alpine sh` and run
`sqlite3 /data/<uuid>/sqlite.db`.

## Out of scope for this project

- Web interface (stays in `packages/frontend/`)
- REST API / webapi (stays in `packages/backend/`, if ever built)
- Mobile (iOS/Android)
- Multi-user (single-user design, file lock)
- Cloud sync (manual file copy only)
- Report generation (Gramps core has this; we won't reimplement for v1)
- GEDCOM import/export (Gramps core has this; v1 uses existing trees)
- Media file upload/editing (v1 is reference-only)
- Backend customization (the other paused milestone; not blocked by this one)

## Open questions deferred to Session 1

- [ ] Should `packages/desktop/` eventually become a Cargo workspace root
      (with sub-crates for `gramps-model`, `gramps-db`, `gramps-ui`)? Defer
      decision until Phase 3 — premature to split
- [ ] Should the Rust object model be published as its own `gramps-rs` crate
      for other Rust consumers? Defer — ship the app first
- [ ] Do we want the app to mount the running `gramps_gramps_db` volume
      directly, or make the user copy `sqlite.db` to `~/Library/...`? Decide
      in Phase 2 based on file-lock semantics from Phase 1 research

---

*This plan is self-contained. A fresh Claude context reading only this file
plus the sample DB should be able to execute Phase 1 without additional
guidance. If you find gaps while executing, update this file first, then code.*
