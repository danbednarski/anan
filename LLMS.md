# LLMS.md — agent guide for anan-cli

This file teaches an LLM agent how to read and edit a Gramps genealogy
tree through `anan-cli`. Read it once at the start of a session, then
issue commands.

## What this tool is

`anan-cli` is a JSON-in / JSON-out command-line wrapper around a Gramps
6.x SQLite tree. It opens the tree, runs one operation, prints JSON,
exits. There is no daemon, no server, no auth.

You point it at a tree file with `--tree <path>` or the `ANAN_TREE`
environment variable. You can pipe its output into `jq` for filtering.

## Object model in 60 seconds

A Gramps tree is built from a small set of primary types, each stored
in its own SQLite table with a `handle` (32-char UUID) and a
human-readable `gramps_id` (e.g. `I0001`, `F0007`, `E0011`, `P0003`).

| Type         | Prefix | What it represents                              |
| ------------ | ------ | ----------------------------------------------- |
| `person`     | I      | A human                                         |
| `family`     | F      | A union: father, mother, relationship type, children |
| `event`      | E      | A dated occurrence: birth, death, marriage, burial, ... |
| `place`      | P      | A location: city, country, parish, ...          |
| `source`     | S      | A genealogical source document                  |
| `citation`   | C      | A citation of a source                          |
| `media`      | O      | Image / file attachment                         |
| `note`       | N      | Free-text note                                  |
| `repository` | R      | An archive holding sources                      |
| `tag`        | T      | A label                                         |

A **person** holds references to events (birth, death, christening),
places of address, families they parent in (`family_handles`), and
families they were a child in (`parent_family_handles`).

A **family** holds references to a father (Person), a mother (Person),
a list of children (Persons), and family-scope events (e.g. Marriage).

Both `handle` and `gramps_id` work as identifiers in every CLI command —
use whichever you have.

## How to identify what's there

Always start with these three commands:

```bash
anan-cli stats              # how big is the tree
anan-cli person list        # who's in it
anan-cli search "Smith"     # find someone by name
```

Then drill in:

```bash
anan-cli person get I0001   # full detail, includes events with dates
anan-cli family get F0007   # parents + children handles
anan-cli event get E0011    # type, date, place
```

Pass `--compact` for one-line JSON if you're piping; default is pretty.

## Schema-aware fields

### Gender

`female | male | unknown | other` (or the integer codes 0,1,2,3).

### Date

ISO-style strings: `YYYY`, `YYYY-MM`, `YYYY-MM-DD`. The CLI also returns
dates in this shape:

```json
{ "iso": "1990-05-15", "year": 1990, "month": 5, "day": 15, "text": "" }
```

`day=0` or `month=0` means that component is unknown.

### Event type

Pass the English label or the integer code. Common ones:

| Code | Label             |
| ---- | ----------------- |
| 1    | Marriage          |
| 7    | Divorce           |
| 12   | Birth             |
| 13   | Death             |
| 15   | Baptism           |
| 19   | Burial            |
| 22   | Christening       |
| 23   | Confirmation      |
| 21   | Census            |

(Run `anan-cli event get <id>` on any existing event to see its
`type_value` if you need to mirror its kind.)

### Family relationship type

`married | unmarried | civil-union | unknown` (or 0,1,2,3).

### Place type

Pass the label (`Country`, `State`, `City`, `Village`, `Parish`, ...) or
the integer code 1-20. Use `Unknown` (-1) when you don't know.

## Writes

Every write is one transaction. Before each write, anan auto-saves a
`<tree>.bak-<unix>` snapshot in the same directory (at most every 5
minutes — the file isn't backed up on every single call). The 10 most
recent snapshots are kept; older ones are pruned.

### Add a person

```bash
anan-cli person add \
  --first "Jane" \
  --surname "Doe" \
  --gender female \
  --birth 1990-05-15 \
  --death 2070-03-20
```

Returns the full new person detail. The birth/death flags create the
matching events and link them. Skip them to leave dates blank.

### Update a person

```bash
anan-cli person update I0048 --surname "Smith-Doe"
anan-cli person update I0048 --birth 1991-01-01
anan-cli person update I0048 --clear-death
```

Unspecified fields keep their existing values. `--clear-birth` /
`--clear-death` remove the event link without setting a new date.

### Delete a person

```bash
anan-cli person delete I0048                   # leaves their events alone
anan-cli person delete I0048 --cascade-events  # also deletes events that
                                               # only this person referenced
```

The delete cascades through any family they were in: if a person was a
parent, the family's parent slot is cleared; if they were a child, the
family's child list is updated.

### Build a family

```bash
# Father + mother known, no children yet:
anan-cli family add --father I0001 --mother I0002 --rel married

# Add an existing person as a child:
anan-cli family add-child F0007 I0048

# Change the relationship type:
anan-cli family update F0007 --rel unmarried

# Unset a parent (empty string):
anan-cli family update F0007 --father ""
```

### Events and places

```bash
anan-cli place add --name "Springfield" --type City
anan-cli event add --type Birth --date 1990-05-15 --place P0006

# Wire the new event onto a person by re-using update:
anan-cli person update I0048 --birth 1990-05-15
```

For now, attaching a free-form (non-birth/death) event to a specific
person via the CLI requires SQL or extending the library. Birth and
death events are wired automatically when you set --birth / --death on a
person.

## Discovery patterns

**"Who are X's children?"**

```bash
anan-cli person get I0001 | jq -r '.family_handles[]' \
  | xargs -I {} anan-cli family get {} \
  | jq -r '.child_handles[]' \
  | xargs -I {} anan-cli person get {} \
  | jq -r '.name'
```

**"What events happened in 1850?"**

```bash
anan-cli event list --limit 0 \
  | jq '.events[] | select(.date.year == 1850)'
```

**"Who has no recorded death?"**

```bash
anan-cli person list --limit 0 \
  | jq '.persons[] | select(.death_event == null) | .name'
```

## Errors

Errors print to stderr and exit with a non-zero status. Common ones:

- `no tree path: pass --tree <path> or set ANAN_TREE` — you forgot to
  point it at a file.
- `person <id> not found` — the handle/gramps_id isn't in the tree.
- `cannot delete event <handle>: still referenced by N object(s)` —
  unlink it from the persons/families that point to it first, or use a
  cascading delete on those.
- `unsupported schema "X"; this build supports "21"` — the tree is from
  a different Gramps version.

## Things this CLI does not do (yet)

- Citation / source / media / note / repository / tag CRUD. Read access
  works via `dump --table source`, `dump --table tag`, etc.
- Adding non-birth/death events to a specific person.
- Bulk import from GEDCOM or CSV.

For these, drop down to `sqlite3 <tree>` directly — the JSON columns are
self-describing and Gramps' upstream Python tools also work.
