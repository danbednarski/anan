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
    gender: i32, is_home: bool, handle: String,
}

#[derive(Clone)]
struct Conn { from: Point, to: Point }

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
    // Add spouses.
    let mut spouses = Vec::new();
    for h in lineage.iter() {
        if let Some(p) = snap.person(h) {
            for fh in &p.family_list {
                if let Some(fam) = snap.family(fh) {
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

/// Core layout: assign positions using generation rows.
fn build_layout(
    snap: &Snapshot,
    home_handle: &str,
    scope: &HashSet<String>,
    gen_map: &HashMap<String, i32>,
) -> TreeLayout {
    // Group people by generation.
    let mut by_gen: HashMap<i32, Vec<String>> = HashMap::new();
    for (handle, g) in gen_map {
        if scope.contains(handle) {
            by_gen.entry(*g).or_default().push(handle.clone());
        }
    }

    let mut sorted_gens: Vec<i32> = by_gen.keys().copied().collect();
    sorted_gens.sort();

    // For each generation, order people: couples together, children
    // roughly grouped under their parents.
    // First pass: figure out couple groupings within each generation.
    let mut person_x: HashMap<String, f32> = HashMap::new();

    for (gen_idx, gen) in sorted_gens.iter().enumerate() {
        let people = &by_gen[gen];
        let y = PADDING + (gen_idx as f32) * (CARD_H + GEN_GAP);

        // Find couples within this generation.
        let mut paired: HashSet<String> = HashSet::new();
        let mut ordered: Vec<OrderedItem> = Vec::new();

        for fam in &snap.families {
            let fh = fam.father_handle.as_ref().filter(|h| people.contains(h) && !paired.contains(*h));
            let mh = fam.mother_handle.as_ref().filter(|h| people.contains(h) && !paired.contains(*h));
            if let (Some(f), Some(m)) = (fh, mh) {
                paired.insert(f.clone());
                paired.insert(m.clone());
                // Position couple near their parents from the generation above.
                let k1 = parent_center_x(snap, f, &person_x);
                let k2 = parent_center_x(snap, m, &person_x);
                let sort_key = if k1 < f32::MAX && k2 < f32::MAX { (k1 + k2) / 2.0 }
                    else if k1 < f32::MAX { k1 }
                    else { k2 };
                ordered.push(OrderedItem::Couple(f.clone(), m.clone(), sort_key));
            }
        }

        // Singles (not paired).
        for h in people {
            if !paired.contains(h) {
                let sort_key = parent_center_x(snap, h, &person_x);
                ordered.push(OrderedItem::Single(h.clone(), sort_key));
            }
        }

        // Sort by hint.
        ordered.sort_by(|a, b| a.sort_key().partial_cmp(&b.sort_key()).unwrap_or(std::cmp::Ordering::Equal));

        // Assign x positions.
        let mut x = PADDING;
        for item in &ordered {
            match item {
                OrderedItem::Couple(f, m, _) => {
                    person_x.insert(f.clone(), x);
                    person_x.insert(m.clone(), x + CARD_W + COUPLE_GAP);
                    x += CARD_W * 2.0 + COUPLE_GAP + H_GAP;
                }
                OrderedItem::Single(h, _) => {
                    person_x.insert(h.clone(), x);
                    x += CARD_W + H_GAP;
                }
            }
        }
    }

    // Build cards.
    let mut cards: Vec<CardInfo> = Vec::new();
    for (gen_idx, gen) in sorted_gens.iter().enumerate() {
        let y = PADDING + (gen_idx as f32) * (CARD_H + GEN_GAP);
        let people = &by_gen[gen];
        for h in people {
            let Some(person) = snap.person(h) else { continue };
            let x = person_x.get(h).copied().unwrap_or(0.0);
            cards.push(make_card(person, snap, home_handle, x, y));
        }
    }

    // Build connections: for each family, draw from couple junction to children.
    let mut conns: Vec<Conn> = Vec::new();
    for fam in &snap.families {
        let fh = fam.father_handle.as_ref().filter(|h| scope.contains(*h));
        let mh = fam.mother_handle.as_ref().filter(|h| scope.contains(*h));

        // Couple connector.
        if let (Some(f), Some(m)) = (fh, mh) {
            if let (Some(&fx), Some(&mx)) = (person_x.get(f), person_x.get(m)) {
                let fg = gen_map.get(f).copied().unwrap_or(0);
                let gen_idx = sorted_gens.iter().position(|g| *g == fg).unwrap_or(0);
                let y = PADDING + (gen_idx as f32) * (CARD_H + GEN_GAP) + CARD_H / 2.0;
                conns.push(Conn {
                    from: Point::new(fx + CARD_W, y),
                    to: Point::new(mx, y),
                });
            }
        }

        // Parent → child connections.
        let parent_handle = fh.or(mh);
        let Some(ph) = parent_handle else { continue };
        let Some(&parent_x) = person_x.get(ph) else { continue };
        let parent_gen = gen_map.get(ph).copied().unwrap_or(0);
        let parent_gen_idx = sorted_gens.iter().position(|g| *g == parent_gen).unwrap_or(0);
        let parent_y = PADDING + (parent_gen_idx as f32) * (CARD_H + GEN_GAP);

        // Junction: bottom center of couple.
        let junction_x = if let (Some(f), Some(m)) = (fh, mh) {
            let fx = person_x.get(f).copied().unwrap_or(0.0);
            let mx = person_x.get(m).copied().unwrap_or(0.0);
            (fx + CARD_W + mx) / 2.0
        } else {
            parent_x + CARD_W / 2.0
        };
        let junction = Point::new(junction_x, parent_y + CARD_H);

        for cr in &fam.child_ref_list {
            if !scope.contains(&cr.r#ref) { continue; }
            let Some(&cx) = person_x.get(&cr.r#ref) else { continue };
            let child_gen = gen_map.get(&cr.r#ref).copied().unwrap_or(0);
            let child_gen_idx = sorted_gens.iter().position(|g| *g == child_gen).unwrap_or(0);
            let child_y = PADDING + (child_gen_idx as f32) * (CARD_H + GEN_GAP);
            let child_top = Point::new(cx + CARD_W / 2.0, child_y);
            conns.push(Conn { from: junction, to: child_top });
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
    Couple(String, String, f32),
    Single(String, f32),
}

impl OrderedItem {
    fn sort_key(&self) -> f32 {
        match self {
            OrderedItem::Couple(_, _, k) => *k,
            OrderedItem::Single(_, k) => *k,
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
    home_handle: &str, x: f32, y: f32,
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

        for conn in &self.layout.conns {
            let is_horiz = (conn.from.y - conn.to.y).abs() < 1.0;
            if is_horiz {
                let path = Path::line(conn.from, conn.to);
                frame.stroke(&path, Stroke::default().with_color(theme::ACCENT).with_width(2.0));
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
                frame.stroke(&path, Stroke::default().with_color(theme::CONNECTOR).with_width(1.5));
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
                        return (canvas::event::Status::Captured, Some(Message::TreeContextMenu(card.handle.clone())));
                    }
                }
                (canvas::event::Status::Ignored, None)
            }
            _ => (canvas::event::Status::Ignored, None),
        }
    }
}

fn draw_card(frame: &mut Frame, card: &CardInfo) {
    let bg = if card.is_home { theme::HOME_BG } else { theme::CARD };
    let text_color = if card.is_home { Color::WHITE } else { theme::TEXT };
    let border_color = if card.is_home { theme::PRIMARY } else { theme::BORDER };

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
