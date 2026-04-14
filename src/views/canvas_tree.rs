//! Canvas-based family tree with generation-aligned rows and curved
//! Bezier connector lines. Uses BFS for vertical (generation) placement
//! and couple-aware horizontal ordering.
//!
//! Key insight: every person gets a generation number from BFS.
//! People at the same generation are at the same y level. Within
//! each generation, couples are grouped together and children are
//! positioned roughly under their parents. Bezier curves connect
//! specific parent couples to their specific children.

use std::collections::{HashMap, HashSet};

use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};

use crate::app::Message;
use crate::db::Snapshot;
use crate::theme;
use crate::views::widgets::date_display;

const CARD_W: f32 = 195.0;
const CARD_H: f32 = 70.0;
const CARD_R: f32 = 8.0;
const COUPLE_GAP: f32 = 10.0;
const H_GAP: f32 = 30.0;
const GEN_GAP: f32 = 90.0;
const GENDER_STRIP_W: f32 = 4.0;
const PADDING: f32 = 40.0;

#[derive(Clone)]
struct CardInfo {
    x: f32, y: f32,
    name: String, detail: String, gramps_id: String,
    gender: i32, is_home: bool, in_lineage: bool, handle: String,
}

#[derive(Clone)]
struct Conn { from: Point, to: Point, highlighted: bool }

pub struct TreeLayout {
    cards: Vec<CardInfo>,
    conns: Vec<Conn>,
    pub width: f32,
    pub height: f32,
}

// ---- BFS generation walker (shared with network.rs) -------------------

fn walk_generations(
    snap: &Snapshot,
    home_handle: &str,
    scope: &HashSet<String>,
) -> HashMap<String, i32> {
    use std::collections::VecDeque;
    let mut gen: HashMap<String, i32> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, i32)> = VecDeque::new();

    visited.insert(home_handle.to_string());
    gen.insert(home_handle.to_string(), 0);
    queue.push_back((home_handle.to_string(), 0));

    while let Some((handle, g)) = queue.pop_front() {
        let Some(person) = snap.person(&handle) else { continue };
        for fh in &person.parent_family_list {
            let Some(fam) = snap.family(fh) else { continue };
            for ph in [&fam.father_handle, &fam.mother_handle].iter().filter_map(|h| h.as_ref()) {
                if scope.contains(ph) && visited.insert(ph.clone()) {
                    gen.insert(ph.clone(), g - 1);
                    queue.push_back((ph.clone(), g - 1));
                }
            }
            for cr in &fam.child_ref_list {
                if scope.contains(&cr.r#ref) && visited.insert(cr.r#ref.clone()) {
                    gen.insert(cr.r#ref.clone(), g);
                    queue.push_back((cr.r#ref.clone(), g));
                }
            }
        }
        for fh in &person.family_list {
            let Some(fam) = snap.family(fh) else { continue };
            for sh in [&fam.father_handle, &fam.mother_handle].iter().filter_map(|h| h.as_ref()) {
                if scope.contains(sh) && visited.insert(sh.clone()) {
                    gen.insert(sh.clone(), g);
                    queue.push_back((sh.clone(), g));
                }
            }
            for cr in &fam.child_ref_list {
                if scope.contains(&cr.r#ref) && visited.insert(cr.r#ref.clone()) {
                    gen.insert(cr.r#ref.clone(), g + 1);
                    queue.push_back((cr.r#ref.clone(), g + 1));
                }
            }
        }
    }
    gen
}

// ---- layout algorithm -------------------------------------------------

/// Build layout for the full network.
pub fn compute_layout(snap: &Snapshot, home_handle: &str) -> TreeLayout {
    let network: HashSet<String> = {
        let groups = super::network::walk_network(snap, home_handle);
        groups.into_iter().flat_map(|(_, p)| p.into_iter().map(|pi| pi.handle)).collect()
    };
    let gen_map = walk_generations(snap, home_handle, &network);
    build_layout(snap, home_handle, &network, &gen_map)
}

/// Build layout for personal tree only.
pub fn compute_layout_personal(snap: &Snapshot, home_handle: &str) -> TreeLayout {
    let mut lineage: HashSet<String> = HashSet::new();
    lineage.insert(home_handle.to_string());
    collect_ancestors(snap, home_handle, &mut lineage);
    collect_descendants_handles(snap, home_handle, &mut lineage);
    // Add spouses - but only from families that are on the direct path.
    // A family is relevant if the person IS the home person (always show
    // their marriages) or the family has at least one child in the lineage.
    let mut spouses = Vec::new();
    for h in lineage.iter() {
        if let Some(p) = snap.person(h) {
            for fh in &p.family_list {
                if let Some(fam) = snap.family(fh) {
                    let dominated = h == home_handle
                        || fam.child_ref_list.iter().any(|cr| lineage.contains(&cr.r#ref));
                    if !dominated { continue; }
                    if let Some(ref s) = fam.father_handle { if s != h { spouses.push(s.clone()); } }
                    if let Some(ref s) = fam.mother_handle { if s != h { spouses.push(s.clone()); } }
                }
            }
        }
    }
    for s in spouses { lineage.insert(s); }

    let gen_map = walk_generations(snap, home_handle, &lineage);
    build_layout(snap, home_handle, &lineage, &gen_map)
}

fn collect_ancestors(snap: &Snapshot, handle: &str, out: &mut HashSet<String>) {
    let Some(person) = snap.person(handle) else { return };
    for pfh in &person.parent_family_list {
        let Some(fam) = snap.family(pfh) else { continue };
        if let Some(ref fh) = fam.father_handle {
            if out.insert(fh.clone()) { collect_ancestors(snap, fh, out); }
        }
        if let Some(ref mh) = fam.mother_handle {
            if out.insert(mh.clone()) { collect_ancestors(snap, mh, out); }
        }
    }
}

fn collect_descendants_handles(snap: &Snapshot, handle: &str, out: &mut HashSet<String>) {
    let Some(person) = snap.person(handle) else { return };
    for fh in &person.family_list {
        let Some(fam) = snap.family(fh) else { continue };
        for cr in &fam.child_ref_list {
            if out.insert(cr.r#ref.clone()) {
                collect_descendants_handles(snap, &cr.r#ref, out);
            }
        }
    }
}

/// Collect the "lineage" set: home person + ancestors + descendants +
/// siblings + spouses of all of the above. Connections between these
/// people get highlighted; everyone else is dimmed.
fn collect_lineage(snap: &Snapshot, home_handle: &str, scope: &HashSet<String>) -> HashSet<String> {
    let mut lineage: HashSet<String> = HashSet::new();
    lineage.insert(home_handle.to_string());
    collect_ancestors(snap, home_handle, &mut lineage);
    collect_descendants_handles(snap, home_handle, &mut lineage);
    // Siblings: other children of home's parent families.
    if let Some(person) = snap.person(home_handle) {
        for pfh in &person.parent_family_list {
            if let Some(fam) = snap.family(pfh) {
                for cr in &fam.child_ref_list {
                    if scope.contains(&cr.r#ref) {
                        lineage.insert(cr.r#ref.clone());
                    }
                }
            }
        }
    }
    // Spouses of everyone in lineage so far.
    let current: Vec<String> = lineage.iter().cloned().collect();
    for h in &current {
        if let Some(p) = snap.person(h) {
            for fh in &p.family_list {
                if let Some(fam) = snap.family(fh) {
                    if let Some(ref s) = fam.father_handle { if scope.contains(s) { lineage.insert(s.clone()); } }
                    if let Some(ref s) = fam.mother_handle { if scope.contains(s) { lineage.insert(s.clone()); } }
                }
            }
        }
    }
    lineage
}

/// Core layout: assign positions using generation rows.
fn build_layout(
    snap: &Snapshot,
    home_handle: &str,
    scope: &HashSet<String>,
    gen_map: &HashMap<String, i32>,
) -> TreeLayout {
    let lineage = collect_lineage(snap, home_handle, scope);

    // Group people by generation.
    let mut by_gen: HashMap<i32, Vec<String>> = HashMap::new();
    for (handle, g) in gen_map {
        if scope.contains(handle) {
            by_gen.entry(*g).or_default().push(handle.clone());
        }
    }

    let mut sorted_gens: Vec<i32> = by_gen.keys().copied().collect();
    sorted_gens.sort();

    // person_x: last-write-wins position per person (used for child sort keys
    // and single-person card positions).
    // family_pos: (father_x, mother_x) per family handle - the authoritative
    // position for each couple appearance. This is what fixes remarriage:
    // a person in 2 families gets 2 entries here with different x values.
    let mut person_x: HashMap<String, f32> = HashMap::new();
    let mut family_pos: HashMap<String, (f32, f32)> = HashMap::new();

    let mut cards: Vec<CardInfo> = Vec::new();

    for (gen_idx, gen) in sorted_gens.iter().enumerate() {
        let people = &by_gen[gen];
        let y = PADDING + (gen_idx as f32) * (CARD_H + GEN_GAP);

        // Find couples within this generation.
        let mut placed_couples: HashSet<(String, String)> = HashSet::new();
        let mut ordered: Vec<OrderedItem> = Vec::new();
        let mut in_couple: HashSet<String> = HashSet::new();

        for fam in &snap.families {
            let fh = fam.father_handle.as_ref().filter(|h| people.contains(h));
            let mh = fam.mother_handle.as_ref().filter(|h| people.contains(h));
            if let (Some(f), Some(m)) = (fh, mh) {
                let pair = (f.clone(), m.clone());
                if placed_couples.contains(&pair) { continue; }
                placed_couples.insert(pair);
                in_couple.insert(f.clone());
                in_couple.insert(m.clone());
                let k1 = parent_center_x(snap, f, &person_x);
                let k2 = parent_center_x(snap, m, &person_x);
                let sort_key = if k1 < f32::MAX && k2 < f32::MAX { (k1 + k2) / 2.0 }
                    else if k1 < f32::MAX { k1 }
                    else { k2 };
                // Place partner whose parents are further left on the left
                // side, so connector lines don't cross unnecessarily.
                let (left, right) = if k2 < k1 { (m.clone(), f.clone()) } else { (f.clone(), m.clone()) };
                ordered.push(OrderedItem::Couple(left, right, fam.handle.clone(), sort_key));
            }
        }

        // Singles (not in any couple in this gen).
        for h in people {
            if !in_couple.contains(h) {
                let sort_key = parent_center_x(snap, h, &person_x);
                ordered.push(OrderedItem::Single(h.clone(), sort_key));
            }
        }

        // Sort: lineage items first (they'll cluster in the center after
        // the post-layout centering pass), then by parent position hint.
        ordered.sort_by(|a, b| {
            let a_lin = a.has_lineage(&lineage);
            let b_lin = b.has_lineage(&lineage);
            // Lineage items sort before non-lineage.
            match (a_lin, b_lin) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }
            a.sort_key().partial_cmp(&b.sort_key()).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Assign x positions and build cards.
        let mut x = PADDING;
        for item in &ordered {
            match item {
                OrderedItem::Couple(f, m, fam_handle, _) => {
                    let fx = x;
                    let mx = x + CARD_W + COUPLE_GAP;
                    family_pos.insert(fam_handle.clone(), (fx, mx));
                    person_x.insert(f.clone(), fx);
                    person_x.insert(m.clone(), mx);
                    if let Some(person) = snap.person(f) {
                        cards.push(make_card(&person, snap, home_handle, &lineage, fx, y));
                    }
                    if let Some(person) = snap.person(m) {
                        cards.push(make_card(&person, snap, home_handle, &lineage, mx, y));
                    }
                    x += CARD_W * 2.0 + COUPLE_GAP + H_GAP;
                }
                OrderedItem::Single(h, _) => {
                    person_x.insert(h.clone(), x);
                    if let Some(person) = snap.person(h) {
                        cards.push(make_card(&person, snap, home_handle, &lineage, x, y));
                    }
                    x += CARD_W + H_GAP;
                }
            }
        }
    }

    // Post-layout centering: shift everything so the lineage's horizontal
    // center aligns with the overall canvas center.
    let lineage_xs: Vec<f32> = cards.iter()
        .filter(|c| c.in_lineage)
        .map(|c| c.x + CARD_W / 2.0)
        .collect();
    if !lineage_xs.is_empty() {
        let lineage_center = lineage_xs.iter().sum::<f32>() / lineage_xs.len() as f32;
        let all_max_x = cards.iter().map(|c| c.x + CARD_W).fold(0.0f32, f32::max) + PADDING;
        let canvas_center = all_max_x / 2.0;
        let shift = canvas_center - lineage_center;
        // Only shift if it wouldn't push anything off the left edge.
        let min_x = cards.iter().map(|c| c.x).fold(f32::MAX, f32::min);
        let clamped_shift = shift.max(PADDING - min_x);
        if clamped_shift.abs() > 1.0 {
            for card in &mut cards {
                card.x += clamped_shift;
            }
            for (_, (fx, mx)) in &mut family_pos {
                *fx += clamped_shift;
                *mx += clamped_shift;
            }
            for (_, px) in &mut person_x {
                *px += clamped_shift;
            }
        }
    }

    // Normalize: ensure leftmost card starts at PADDING (no dead space
    // on the left from the centering shift).
    let final_min = cards.iter().map(|c| c.x).fold(f32::MAX, f32::min);
    if final_min > PADDING + 1.0 {
        let norm = PADDING - final_min;
        for card in &mut cards { card.x += norm; }
        for (_, (fx, mx)) in &mut family_pos { *fx += norm; *mx += norm; }
        for (_, px) in &mut person_x { *px += norm; }
    }

    // Build connections: for each family, draw from couple junction to children.
    let mut conns: Vec<Conn> = Vec::new();
    for fam in &snap.families {
        let fh = fam.father_handle.as_ref().filter(|h| scope.contains(*h));
        let mh = fam.mother_handle.as_ref().filter(|h| scope.contains(*h));

        // Couple connector: use family_pos for this specific family's positions.
        if let (Some(_f), Some(_m)) = (fh, mh) {
            if let Some(&(fx, mx)) = family_pos.get(&fam.handle) {
                let fg = gen_map.get(_f).copied().unwrap_or(0);
                let gen_idx = sorted_gens.iter().position(|g| *g == fg).unwrap_or(0);
                let y = PADDING + (gen_idx as f32) * (CARD_H + GEN_GAP) + CARD_H / 2.0;
                let (left, right) = if fx < mx { (fx, mx) } else { (mx, fx) };
                let hl = lineage.contains(_f) || lineage.contains(_m);
                conns.push(Conn {
                    from: Point::new(left + CARD_W, y),
                    to: Point::new(right, y),
                    highlighted: hl,
                });
            }
        }

        // Parent -> child connections.
        let parent_handle = fh.or(mh);
        let Some(ph) = parent_handle else { continue };
        let parent_gen = gen_map.get(ph).copied().unwrap_or(0);
        let parent_gen_idx = sorted_gens.iter().position(|g| *g == parent_gen).unwrap_or(0);
        let parent_y = PADDING + (parent_gen_idx as f32) * (CARD_H + GEN_GAP);

        // Junction: use family_pos if available, else fall back to person_x.
        let junction_x = if let Some(&(fx, mx)) = family_pos.get(&fam.handle) {
            (fx + CARD_W + mx) / 2.0
        } else {
            let (fx_opt, mx_opt) = (
                fh.and_then(|f| person_x.get(f).copied()),
                mh.and_then(|m| person_x.get(m).copied()),
            );
            match (fx_opt, mx_opt) {
                (Some(fx), _) => fx + CARD_W / 2.0,
                (_, Some(mx)) => mx + CARD_W / 2.0,
                (None, None) => continue,
            }
        };
        let junction = Point::new(junction_x, parent_y + CARD_H);

        // Is at least one parent in the lineage?
        let parent_in_lineage = fh.map_or(false, |h| lineage.contains(h))
            || mh.map_or(false, |h| lineage.contains(h));

        for cr in &fam.child_ref_list {
            if !scope.contains(&cr.r#ref) { continue; }
            let Some(&cx) = person_x.get(&cr.r#ref) else { continue };
            let child_gen = gen_map.get(&cr.r#ref).copied().unwrap_or(0);
            if child_gen != parent_gen + 1 { continue; }
            let child_gen_idx = sorted_gens.iter().position(|g| *g == child_gen).unwrap_or(0);
            let child_y = PADDING + (child_gen_idx as f32) * (CARD_H + GEN_GAP);
            let child_top = Point::new(cx + CARD_W / 2.0, child_y);
            let hl = parent_in_lineage && lineage.contains(&cr.r#ref);
            conns.push(Conn { from: junction, to: child_top, highlighted: hl });
        }
    }

    let max_x = cards.iter().map(|c| c.x + CARD_W).fold(0.0f32, f32::max) + PADDING;
    let max_y = cards.iter().map(|c| c.y + CARD_H).fold(0.0f32, f32::max) + PADDING;

    TreeLayout {
        cards, conns,
        width: max_x.max(400.0),
        height: max_y.max(300.0),
    }
}

enum OrderedItem {
    Couple(String, String, String, f32), // father, mother, family_handle, sort_key
    Single(String, f32),
}

impl OrderedItem {
    fn sort_key(&self) -> f32 {
        match self {
            OrderedItem::Couple(_, _, _, k) => *k,
            OrderedItem::Single(_, k) => *k,
        }
    }
    fn has_lineage(&self, lineage: &HashSet<String>) -> bool {
        match self {
            OrderedItem::Couple(f, m, _, _) => lineage.contains(f) || lineage.contains(m),
            OrderedItem::Single(h, _) => lineage.contains(h),
        }
    }
}

/// Get the average x-position of a person's parents (from the
/// generation row above). Returns f32::MAX if no parents are positioned.
fn parent_center_x(
    snap: &Snapshot,
    handle: &str,
    person_x: &HashMap<String, f32>,
) -> f32 {
    let Some(person) = snap.person(handle) else { return f32::MAX };
    let mut xs = Vec::new();
    for pfh in &person.parent_family_list {
        let Some(fam) = snap.family(pfh) else { continue };
        if let Some(ref fh) = fam.father_handle {
            if let Some(&x) = person_x.get(fh) { xs.push(x); }
        }
        if let Some(ref mh) = fam.mother_handle {
            if let Some(&x) = person_x.get(mh) { xs.push(x); }
        }
    }
    if xs.is_empty() { f32::MAX } else {
        xs.iter().sum::<f32>() / xs.len() as f32
    }
}

fn make_card(
    person: &crate::gramps::Person, snap: &Snapshot,
    home_handle: &str, lineage: &HashSet<String>, x: f32, y: f32,
) -> CardInfo {
    let birth = if person.birth_ref_index >= 0 {
        person.event_ref_list.get(person.birth_ref_index as usize)
            .and_then(|er| snap.event(&er.r#ref))
            .and_then(|e| e.date.as_ref())
            .map(date_display::format)
            .filter(|s| !s.is_empty())
            .unwrap_or_default()
    } else { String::new() };
    let death = if person.death_ref_index >= 0 {
        person.event_ref_list.get(person.death_ref_index as usize)
            .and_then(|er| snap.event(&er.r#ref))
            .and_then(|e| e.date.as_ref())
            .map(date_display::format)
            .filter(|s| !s.is_empty())
            .unwrap_or_default()
    } else { String::new() };
    let detail = match (birth.is_empty(), death.is_empty()) {
        (false, false) => format!("*{birth}  +{death}"),
        (false, true) => format!("*{birth}"),
        (true, false) => format!("+{death}"),
        (true, true) => String::new(),
    };
    CardInfo {
        x, y,
        name: person.primary_name.display(),
        detail, gramps_id: person.gramps_id.clone(),
        gender: person.gender,
        is_home: person.handle == home_handle,
        in_lineage: lineage.contains(&person.handle),
        handle: person.handle.clone(),
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars { s.to_string() }
    else {
        let t: String = s.chars().take(max_chars - 1).collect();
        format!("{t}...")
    }
}

// ---- canvas rendering --------------------------------------------------

pub struct FamilyTreeProgram {
    pub layout: TreeLayout,
}

impl canvas::Program<Message> for FamilyTreeProgram {
    type State = ();

    fn draw(
        &self, _state: &(), renderer: &Renderer,
        _theme: &Theme, bounds: Rectangle, _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        // Draw non-highlighted connections first (behind).
        for conn in self.layout.conns.iter().filter(|c| !c.highlighted) {
            let color = Color::from_rgba(0.78, 0.81, 0.82, 0.35);
            let is_horiz = (conn.from.y - conn.to.y).abs() < 1.0;
            if is_horiz {
                let path = Path::line(conn.from, conn.to);
                frame.stroke(&path, Stroke::default().with_color(color).with_width(1.0));
            } else {
                let mid_y = (conn.from.y + conn.to.y) / 2.0;
                let path = Path::new(|b| {
                    b.move_to(conn.from);
                    b.bezier_curve_to(
                        Point::new(conn.from.x, mid_y),
                        Point::new(conn.to.x, mid_y),
                        conn.to,
                    );
                });
                frame.stroke(&path, Stroke::default().with_color(color).with_width(1.0));
            }
        }
        // Draw highlighted connections on top.
        for conn in self.layout.conns.iter().filter(|c| c.highlighted) {
            let is_horiz = (conn.from.y - conn.to.y).abs() < 1.0;
            if is_horiz {
                let path = Path::line(conn.from, conn.to);
                frame.stroke(&path, Stroke::default().with_color(theme::ACCENT).with_width(3.0));
            } else {
                let mid_y = (conn.from.y + conn.to.y) / 2.0;
                let path = Path::new(|b| {
                    b.move_to(conn.from);
                    b.bezier_curve_to(
                        Point::new(conn.from.x, mid_y),
                        Point::new(conn.to.x, mid_y),
                        conn.to,
                    );
                });
                frame.stroke(&path, Stroke::default().with_color(theme::ACCENT).with_width(2.5));
            }
        }

        for card in &self.layout.cards {
            draw_card(&mut frame, card);
        }

        vec![frame.into_geometry()]
    }

    fn update(
        &self, _state: &mut (), event: canvas::Event,
        bounds: Rectangle, cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        let Some(pos) = cursor.position_in(bounds) else {
            return (canvas::event::Status::Ignored, None);
        };
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                for card in &self.layout.cards {
                    if pos.x >= card.x && pos.x <= card.x + CARD_W
                        && pos.y >= card.y && pos.y <= card.y + CARD_H
                    {
                        return (canvas::event::Status::Captured, Some(Message::TreeHome(card.handle.clone())));
                    }
                }
                (canvas::event::Status::Ignored, None)
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                for card in &self.layout.cards {
                    if pos.x >= card.x && pos.x <= card.x + CARD_W
                        && pos.y >= card.y && pos.y <= card.y + CARD_H
                    {
                        return (canvas::event::Status::Captured, Some(Message::TreeContextMenu(card.handle.clone(), pos.x, pos.y)));
                    }
                }
                (canvas::event::Status::Ignored, None)
            }
            _ => (canvas::event::Status::Ignored, None),
        }
    }
}

fn draw_card(frame: &mut Frame, card: &CardInfo) {
    let (bg, text_color, border_color) = if card.is_home {
        (theme::HOME_BG, Color::WHITE, theme::PRIMARY)
    } else if card.in_lineage {
        (theme::CARD, theme::TEXT, theme::ACCENT)
    } else {
        // Dimmed card but keep text readable.
        (Color::from_rgba(1.0, 1.0, 1.0, 0.45),
         Color::from_rgba(0.18, 0.20, 0.21, 0.75),
         Color::from_rgba(0.875, 0.902, 0.914, 0.45))
    };

    let rect = Path::rectangle(Point::new(card.x, card.y), Size::new(CARD_W, CARD_H));
    frame.fill(&rect, bg);

    let border = Path::new(|b| {
        b.move_to(Point::new(card.x + CARD_R, card.y));
        b.line_to(Point::new(card.x + CARD_W - CARD_R, card.y));
        b.arc_to(Point::new(card.x + CARD_W, card.y), Point::new(card.x + CARD_W, card.y + CARD_R), CARD_R);
        b.line_to(Point::new(card.x + CARD_W, card.y + CARD_H - CARD_R));
        b.arc_to(Point::new(card.x + CARD_W, card.y + CARD_H), Point::new(card.x + CARD_W - CARD_R, card.y + CARD_H), CARD_R);
        b.line_to(Point::new(card.x + CARD_R, card.y + CARD_H));
        b.arc_to(Point::new(card.x, card.y + CARD_H), Point::new(card.x, card.y + CARD_H - CARD_R), CARD_R);
        b.line_to(Point::new(card.x, card.y + CARD_R));
        b.arc_to(Point::new(card.x, card.y), Point::new(card.x + CARD_R, card.y), CARD_R);
    });
    frame.stroke(&border, Stroke::default().with_color(border_color).with_width(1.0));

    let g_color = match card.gender {
        0 => Color::from_rgb(0.85, 0.45, 0.55),
        1 => Color::from_rgb(0.4, 0.6, 0.85),
        _ => Color::from_rgb(0.65, 0.65, 0.65),
    };
    frame.fill(&Path::rectangle(Point::new(card.x, card.y), Size::new(GENDER_STRIP_W, CARD_H)), g_color);

    frame.fill_text(Text {
        content: truncate(&card.name, 26),
        position: Point::new(card.x + 12.0, card.y + 14.0),
        color: text_color, size: 13.0.into(), ..Text::default()
    });
    if !card.detail.is_empty() {
        frame.fill_text(Text {
            content: truncate(&card.detail, 30),
            position: Point::new(card.x + 12.0, card.y + 32.0),
            color: if card.is_home { Color::from_rgba(1.0,1.0,1.0,0.7) } else { theme::TEXT_MUTED },
            size: 10.0.into(), ..Text::default()
        });
    }
    frame.fill_text(Text {
        content: card.gramps_id.clone(),
        position: Point::new(card.x + 12.0, card.y + CARD_H - 16.0),
        color: if card.is_home { Color::from_rgba(1.0,1.0,1.0,0.5) } else { Color::from_rgb(0.7,0.7,0.7) },
        size: 9.0.into(), ..Text::default()
    });
}

// ---- public view functions ---------------------------------------------

pub fn view<'a>(snap: &'a Snapshot, home_handle: &str) -> Element<'a, Message> {
    render_layout(compute_layout(snap, home_handle))
}

pub fn view_personal<'a>(snap: &'a Snapshot, home_handle: &str) -> Element<'a, Message> {
    render_layout(compute_layout_personal(snap, home_handle))
}

fn render_layout(layout: TreeLayout) -> Element<'static, Message> {
    let w = layout.width;
    let h = layout.height;
    iced::widget::canvas(FamilyTreeProgram { layout })
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .into()
}
