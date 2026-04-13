# Session Context for Resuming

## What was built (2026-04-11 to 2026-04-12)

A native macOS Rust desktop app for Gramps genealogy, from zero to working tree visualization in one session.

### Architecture
- **iced 0.13** for UI (elm architecture: Message -> State -> View)
- **rusqlite** reads/writes Gramps 6.x SQLite files (schema version 21, JSON columns)
- **Canvas widget** for the family tree visualization (curved Bezier lines, gender-colored cards)
- Single-threaded edit model, `Arc<Database>` shared with `tokio::spawn_blocking` for writes

### Key files
- `src/app.rs` - main app state, all messages, view routing (~2200 lines)
- `src/views/canvas_tree.rs` - Canvas-based tree with generation-aligned layout + Bezier curves
- `src/views/tree.rs` - old widget-based tree (still used for Family Tree list mode)
- `src/views/network.rs` - Full Network list view + BFS walk
- `src/db/database.rs` - Database struct (open, snapshot, write_txn, backup)
- `src/db/repo/` - per-type CRUD (person, family, event, place, source, citation, media, note, repository, tag, relationships)
- `src/theme.rs` - custom warm color palette
- `src/gramps/` - Gramps 6.x JSON object model (serde structs)

### Current state
- All 10 primary types have full CRUD
- Canvas tree view with curved Bezier lines, gender-colored card borders
- Generation-aligned layout (BFS assigns gen numbers, everyone at same gen = same y)
- Right-click context menu (floating overlay via stack)
- Add person modal with name/gender/dates/source fields
- Collapsible sidebar with search
- Frameless macOS window (transparent titlebar)
- Tree/List toggle preserves mode across view switches

### Remaining known bugs
1. **Stray connecting lines**: Some couple connectors still stretch when partners in a remarriage aren't positioned adjacently. The Gramps-style duplicate approach is partially implemented but person_x HashMap only stores one position per handle, so the second couple's connector may use stale x-coordinates. Fix: use family-scoped position keys or duplicate the CardInfo entries properly.
2. **Cards at top of canvas cut off**: When scrolled to top, the first generation's cards may be partially hidden. The PADDING constant (40px) may need increasing, or the macOS traffic light area needs accounting.

### Key lessons learned
- `Direction::Both` scrollable panics if content uses `Length::Fill` width - must use `Length::Shrink` or fixed
- macOS frameless: use `titlebar_transparent + fullsize_content_view + title_hidden` (not `decorations: false` which removes traffic lights)
- iced 0.13 Canvas: `Path::new(|b| b.bezier_curve_to(...))` for cubic Bezier curves; control points at vertical midpoint create smooth S-curves
- Unicode icons render poorly in iced's default font - use text labels instead
- `person_x` positioning: children need parent positions from the row above (process top-down), use `parent_center_x()` helper for sort keys
- Gramps data: a person in 2 families = 2 couple appearances; BFS generation assignment handles this naturally
- The `reference` table has no SQLite triggers - app code maintains it via `rewrite_references()` on every write
- Schema version is `metadata.version = "21"` for Gramps 6.0.x
- Auto-load copies fixture to `/tmp/gramps-desktop-scratch.db` to avoid mutating committed test file

### To resume
```
cd /Users/danielbednarski/gramps-monorepo/packages/desktop
cargo run
```

Read this file + `PLAN.md` + recent git log for full context.
