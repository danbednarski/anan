//! Canvas-based family tree renderer with curved Bezier connector
//! lines, gender-colored card borders, and proper parent-to-child
//! branching. Inspired by Gramps Web's D3/SVG tree chart.
//!
//! Uses iced's `Canvas` widget for full control over positioning
//! and line drawing. Cards are rounded rectangles with text drawn
//! directly on the canvas. Click any card to re-home, right-click
//! to open the context menu.

use std::collections::HashSet;

use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};

use crate::app::Message;
use crate::db::Snapshot;
use crate::theme;
use crate::views::widgets::date_display;

// ---- layout constants --------------------------------------------------

const CARD_W: f32 = 180.0;
const CARD_H: f32 = 70.0;
const CARD_R: f32 = 8.0;
const COUPLE_GAP: f32 = 12.0;
const CHILD_GAP: f32 = 24.0;
const GEN_GAP: f32 = 100.0;
const GENDER_STRIP_W: f32 = 4.0;
const PADDING: f32 = 60.0;

// ---- data structures ---------------------------------------------------

#[derive(Clone)]
struct CardInfo {
    x: f32,
    y: f32,
    name: String,
    detail: String,
    gramps_id: String,
    gender: i32,
    is_home: bool,
    handle: String,
}

#[derive(Clone)]
struct Conn {
    from: Point,
    to: Point,
}

pub struct TreeLayout {
    cards: Vec<CardInfo>,
    conns: Vec<Conn>,
    width: f32,
    height: f32,
}

// ---- layout algorithm --------------------------------------------------

/// Build the full canvas layout for the network tree. Returns card
/// positions and connection lines.
pub fn compute_layout(snap: &Snapshot, home_handle: &str) -> TreeLayout {
    let network_handles: HashSet<String> = {
        let groups = super::network::walk_network(snap, home_handle);
        groups.into_iter().flat_map(|(_, p)| p.into_iter().map(|pi| pi.handle)).collect()
    };

    // Find root families (oldest gen).
    let groups = super::network::walk_network(snap, home_handle);
    let oldest_gen = groups.first().map(|(g, _)| *g).unwrap_or(0);

    use std::collections::HashMap;
    let mut gen_map: HashMap<String, i32> = HashMap::new();
    for (gen, people) in &groups {
        for p in people {
            gen_map.insert(p.handle.clone(), *gen);
        }
    }

    let mut root_families: Vec<String> = Vec::new();
    for fam in &snap.families {
        let father_gen = fam.father_handle.as_ref().and_then(|h| gen_map.get(h)).copied();
        let mother_gen = fam.mother_handle.as_ref().and_then(|h| gen_map.get(h)).copied();
        let parent_gen = match (father_gen, mother_gen) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        if parent_gen == Some(oldest_gen) {
            root_families.push(fam.handle.clone());
        }
    }

    let mut cards: Vec<CardInfo> = Vec::new();
    let mut conns: Vec<Conn> = Vec::new();
    let mut x_offset: f32 = PADDING;

    for fam_handle in &root_families {
        let w = layout_family(
            snap, fam_handle, home_handle, &network_handles,
            x_offset, PADDING, 0,
            &mut cards, &mut conns,
        );
        x_offset += w + 60.0;
    }

    let max_x = cards.iter().map(|c| c.x + CARD_W).fold(0.0f32, f32::max) + PADDING;
    let max_y = cards.iter().map(|c| c.y + CARD_H).fold(0.0f32, f32::max) + PADDING;

    TreeLayout {
        cards,
        conns,
        width: max_x,
        height: max_y,
    }
}

/// Recursively layout a family. Returns the total width consumed.
fn layout_family(
    snap: &Snapshot,
    fam_handle: &str,
    home_handle: &str,
    network: &HashSet<String>,
    x_start: f32,
    y_start: f32,
    depth: usize,
    cards: &mut Vec<CardInfo>,
    conns: &mut Vec<Conn>,
) -> f32 {
    let Some(fam) = snap.family(fam_handle) else { return 0.0 };
    if depth > 8 { return 0.0 }

    let parent_y = y_start;
    let child_y = y_start + CARD_H + GEN_GAP;

    // Collect children and compute their widths first.
    let children: Vec<&crate::gramps::family::ChildRef> = fam.child_ref_list.iter()
        .filter(|cr| network.contains(&cr.r#ref))
        .collect();

    let mut child_widths: Vec<f32> = Vec::new();
    let mut child_layouts: Vec<(f32, Option<String>)> = Vec::new(); // (width, child_family_handle)

    for cr in &children {
        let Some(child) = snap.person(&cr.r#ref) else { continue };
        let child_family = child.family_list.iter().find(|fh| {
            snap.family(fh).map(|f| {
                f.child_ref_list.iter().any(|c| network.contains(&c.r#ref))
                    || f.mother_handle.as_ref().map(|h| network.contains(h)).unwrap_or(false)
                    || f.father_handle.as_ref().map(|h| network.contains(h)).unwrap_or(false)
            }).unwrap_or(false)
        }).cloned();

        if let Some(ref cf) = child_family {
            // Pre-compute width for this child's family subtree.
            let w = estimate_family_width(snap, cf, network, depth + 1);
            child_widths.push(w);
            child_layouts.push((w, Some(cf.clone())));
        } else {
            child_widths.push(CARD_W);
            child_layouts.push((CARD_W, None));
        }
    }

    let total_children_w: f32 = if child_widths.is_empty() {
        CARD_W * 2.0 + COUPLE_GAP
    } else {
        child_widths.iter().sum::<f32>() + CHILD_GAP * (child_widths.len() as f32 - 1.0).max(0.0)
    };
    let couple_w = CARD_W * 2.0 + COUPLE_GAP;
    let family_w = total_children_w.max(couple_w);

    // Center parents over children.
    let parents_x = x_start + (family_w - couple_w) / 2.0;

    // Father card.
    if let Some(father) = fam.father_handle.as_ref().and_then(|h| snap.person(h)) {
        cards.push(make_card(father, snap, home_handle, parents_x, parent_y));
    }
    // Mother card.
    if let Some(mother) = fam.mother_handle.as_ref().and_then(|h| snap.person(h)) {
        cards.push(make_card(mother, snap, home_handle, parents_x + CARD_W + COUPLE_GAP, parent_y));
    }

    // Junction point: bottom center of the couple.
    let junction = Point::new(
        parents_x + CARD_W + COUPLE_GAP / 2.0,
        parent_y + CARD_H,
    );

    // Now layout children.
    let mut child_x = x_start + (family_w - total_children_w) / 2.0;
    let mut child_idx = 0;
    for cr in &children {
        let Some(child) = snap.person(&cr.r#ref) else { continue };
        if child_idx >= child_layouts.len() { break; }
        let (cw, ref child_fam) = child_layouts[child_idx];

        if let Some(cf) = child_fam {
            // Recursively layout child's family.
            layout_family(
                snap, cf, home_handle, network,
                child_x, child_y, depth + 1,
                cards, conns,
            );
            // Connection to the first parent of child's family (top center).
            let child_center_top = Point::new(child_x + cw / 2.0, child_y);
            conns.push(Conn { from: junction, to: child_center_top });
        } else {
            // Simple card for leaf child.
            let cx = child_x + (cw - CARD_W) / 2.0;
            cards.push(make_card(child, snap, home_handle, cx, child_y));
            let child_top = Point::new(cx + CARD_W / 2.0, child_y);
            conns.push(Conn { from: junction, to: child_top });
        }

        child_x += cw + CHILD_GAP;
        child_idx += 1;
    }

    // Couple connector line (horizontal between father and mother).
    if fam.father_handle.is_some() && fam.mother_handle.is_some() {
        conns.push(Conn {
            from: Point::new(parents_x + CARD_W, parent_y + CARD_H / 2.0),
            to: Point::new(parents_x + CARD_W + COUPLE_GAP, parent_y + CARD_H / 2.0),
        });
    }

    family_w
}

/// Estimate width without placing cards (for pre-pass).
fn estimate_family_width(
    snap: &Snapshot,
    fam_handle: &str,
    network: &HashSet<String>,
    depth: usize,
) -> f32 {
    let Some(fam) = snap.family(fam_handle) else { return CARD_W * 2.0 + COUPLE_GAP };
    if depth > 8 { return CARD_W * 2.0 + COUPLE_GAP }

    let children: Vec<&crate::gramps::family::ChildRef> = fam.child_ref_list.iter()
        .filter(|cr| network.contains(&cr.r#ref))
        .collect();

    if children.is_empty() {
        return CARD_W * 2.0 + COUPLE_GAP;
    }

    let mut total: f32 = 0.0;
    for (i, cr) in children.iter().enumerate() {
        let Some(child) = snap.person(&cr.r#ref) else { continue };
        let child_family = child.family_list.iter().find(|fh| {
            snap.family(fh).map(|f| {
                f.child_ref_list.iter().any(|c| network.contains(&c.r#ref))
                    || f.mother_handle.as_ref().map(|h| network.contains(h)).unwrap_or(false)
                    || f.father_handle.as_ref().map(|h| network.contains(h)).unwrap_or(false)
            }).unwrap_or(false)
        }).cloned();

        let w = match child_family {
            Some(cf) => estimate_family_width(snap, &cf, network, depth + 1),
            None => CARD_W,
        };
        total += w;
        if i > 0 { total += CHILD_GAP; }
    }

    total.max(CARD_W * 2.0 + COUPLE_GAP)
}

fn make_card(
    person: &crate::gramps::Person,
    snap: &Snapshot,
    home_handle: &str,
    x: f32,
    y: f32,
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
        detail,
        gramps_id: person.gramps_id.clone(),
        gender: person.gender,
        is_home: person.handle == home_handle,
        handle: person.handle.clone(),
    }
}

// ---- canvas rendering --------------------------------------------------

pub struct FamilyTreeProgram {
    pub layout: TreeLayout,
}

impl canvas::Program<Message> for FamilyTreeProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        // Draw connections (behind cards).
        for conn in &self.layout.conns {
            // Check if this is a horizontal couple connector.
            let is_couple = (conn.from.y - conn.to.y).abs() < 1.0;
            if is_couple {
                // Straight horizontal line with accent color.
                let path = Path::line(conn.from, conn.to);
                frame.stroke(
                    &path,
                    Stroke::default()
                        .with_color(theme::ACCENT)
                        .with_width(2.0),
                );
            } else {
                // Curved Bezier from parent junction down to child.
                let mid_y = (conn.from.y + conn.to.y) / 2.0;
                let path = Path::new(|b| {
                    b.move_to(conn.from);
                    b.bezier_curve_to(
                        Point::new(conn.from.x, mid_y),
                        Point::new(conn.to.x, mid_y),
                        conn.to,
                    );
                });
                frame.stroke(
                    &path,
                    Stroke::default()
                        .with_color(theme::CONNECTOR)
                        .with_width(1.5),
                );
            }
        }

        // Draw cards.
        for card in &self.layout.cards {
            draw_card(&mut frame, card);
        }

        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        _state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        let Some(pos) = cursor.position_in(bounds) else {
            return (canvas::event::Status::Ignored, None);
        };

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                // Find which card was clicked.
                for card in &self.layout.cards {
                    if pos.x >= card.x && pos.x <= card.x + CARD_W
                        && pos.y >= card.y && pos.y <= card.y + CARD_H
                    {
                        return (
                            canvas::event::Status::Captured,
                            Some(Message::TreeHome(card.handle.clone())),
                        );
                    }
                }
                (canvas::event::Status::Ignored, None)
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                for card in &self.layout.cards {
                    if pos.x >= card.x && pos.x <= card.x + CARD_W
                        && pos.y >= card.y && pos.y <= card.y + CARD_H
                    {
                        return (
                            canvas::event::Status::Captured,
                            Some(Message::TreeContextMenu(card.handle.clone())),
                        );
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

    // Card background with rounded corners.
    let rect = Path::rectangle(
        Point::new(card.x, card.y),
        Size::new(CARD_W, CARD_H),
    );
    frame.fill(&rect, bg);

    // Border.
    let border_path = Path::new(|b| {
        b.move_to(Point::new(card.x + CARD_R, card.y));
        b.line_to(Point::new(card.x + CARD_W - CARD_R, card.y));
        b.arc_to(
            Point::new(card.x + CARD_W, card.y),
            Point::new(card.x + CARD_W, card.y + CARD_R),
            CARD_R,
        );
        b.line_to(Point::new(card.x + CARD_W, card.y + CARD_H - CARD_R));
        b.arc_to(
            Point::new(card.x + CARD_W, card.y + CARD_H),
            Point::new(card.x + CARD_W - CARD_R, card.y + CARD_H),
            CARD_R,
        );
        b.line_to(Point::new(card.x + CARD_R, card.y + CARD_H));
        b.arc_to(
            Point::new(card.x, card.y + CARD_H),
            Point::new(card.x, card.y + CARD_H - CARD_R),
            CARD_R,
        );
        b.line_to(Point::new(card.x, card.y + CARD_R));
        b.arc_to(
            Point::new(card.x, card.y),
            Point::new(card.x + CARD_R, card.y),
            CARD_R,
        );
    });
    frame.stroke(
        &border_path,
        Stroke::default().with_color(border_color).with_width(1.0),
    );

    // Gender strip on left edge.
    let g_color = match card.gender {
        0 => Color::from_rgb(0.85, 0.45, 0.55),
        1 => Color::from_rgb(0.4, 0.6, 0.85),
        _ => Color::from_rgb(0.65, 0.65, 0.65),
    };
    let strip = Path::rectangle(
        Point::new(card.x, card.y),
        Size::new(GENDER_STRIP_W, CARD_H),
    );
    frame.fill(&strip, g_color);

    // Name text.
    frame.fill_text(Text {
        content: card.name.clone(),
        position: Point::new(card.x + 12.0, card.y + 14.0),
        color: text_color,
        size: 13.0.into(),
        ..Text::default()
    });

    // Detail (birth/death).
    if !card.detail.is_empty() {
        frame.fill_text(Text {
            content: card.detail.clone(),
            position: Point::new(card.x + 12.0, card.y + 32.0),
            color: if card.is_home {
                Color::from_rgba(1.0, 1.0, 1.0, 0.7)
            } else {
                theme::TEXT_MUTED
            },
            size: 10.0.into(),
            ..Text::default()
        });
    }

    // Gramps ID.
    frame.fill_text(Text {
        content: card.gramps_id.clone(),
        position: Point::new(card.x + 12.0, card.y + CARD_H - 16.0),
        color: if card.is_home {
            Color::from_rgba(1.0, 1.0, 1.0, 0.5)
        } else {
            Color::from_rgb(0.7, 0.7, 0.7)
        },
        size: 9.0.into(),
        ..Text::default()
    });
}

// ---- public view function ----------------------------------------------

/// Render the full network as a Canvas-based tree with curved Bezier
/// connector lines.
pub fn view<'a>(snap: &'a Snapshot, home_handle: &str) -> Element<'a, Message> {
    let layout = compute_layout(snap, home_handle);
    let w = layout.width;
    let h = layout.height;

    let program = FamilyTreeProgram { layout };

    iced::widget::canvas(program)
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .into()
}
