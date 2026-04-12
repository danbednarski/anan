//! Family tree view - the hero view of the app.
//!
//! Shows ancestors above, the home person + siblings + spouse in the
//! center, and descendants below. The entire tree is scrollable in
//! both directions for large families. Right-click any person card
//! to open the context menu.

use iced::widget::{button, column, container, mouse_area, row, scrollable, text};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::{Alignment, Element, Length, Theme};

use crate::app::Message;
use crate::db::Snapshot;
use crate::gramps::Person;
use crate::views::widgets::date_display;

const MAX_DEPTH: usize = 3;
const MAX_DESC_DEPTH: usize = 2;

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub handle: String,
    pub name: String,
    pub gramps_id: String,
    pub years: String,
}

#[derive(Debug, Clone)]
pub struct AncestorNode {
    pub person: TreeNode,
    pub father: Option<Box<AncestorNode>>,
    pub mother: Option<Box<AncestorNode>>,
}

fn node_from_person(person: &Person, snap: &Snapshot) -> TreeNode {
    let birth = if person.birth_ref_index >= 0 {
        person
            .event_ref_list
            .get(person.birth_ref_index as usize)
            .and_then(|er| snap.event(&er.r#ref))
            .and_then(|e| e.date.as_ref())
            .map(date_display::format)
            .filter(|s| !s.is_empty())
    } else {
        None
    };
    let death = if person.death_ref_index >= 0 {
        person
            .event_ref_list
            .get(person.death_ref_index as usize)
            .and_then(|er| snap.event(&er.r#ref))
            .and_then(|e| e.date.as_ref())
            .map(date_display::format)
            .filter(|s| !s.is_empty())
    } else {
        None
    };
    let years = match (birth, death) {
        (Some(b), Some(d)) => format!("{b} - {d}"),
        (Some(b), None) => format!("b. {b}"),
        (None, Some(d)) => format!("d. {d}"),
        (None, None) => String::new(),
    };
    TreeNode {
        handle: person.handle.clone(),
        name: person.primary_name.display(),
        gramps_id: person.gramps_id.clone(),
        years,
    }
}

pub fn build_ancestors(snap: &Snapshot, handle: &str, depth: usize) -> Option<AncestorNode> {
    let person = snap.person(handle)?;
    let node = node_from_person(person, snap);
    if depth == 0 {
        return Some(AncestorNode {
            person: node,
            father: None,
            mother: None,
        });
    }
    let (father, mother) = person
        .parent_family_list
        .first()
        .and_then(|fh| snap.family(fh))
        .map(|fam| {
            let f = fam.father_handle.as_ref()
                .and_then(|h| build_ancestors(snap, h, depth - 1))
                .map(Box::new);
            let m = fam.mother_handle.as_ref()
                .and_then(|h| build_ancestors(snap, h, depth - 1))
                .map(Box::new);
            (f, m)
        })
        .unwrap_or((None, None));
    Some(AncestorNode { person: node, father, mother })
}

/// Siblings of the home person (other children of same parents).
fn collect_siblings(snap: &Snapshot, home_handle: &str) -> Vec<TreeNode> {
    let Some(person) = snap.person(home_handle) else { return Vec::new() };
    let Some(fam_handle) = person.parent_family_list.first() else { return Vec::new() };
    let Some(fam) = snap.family(fam_handle) else { return Vec::new() };
    fam.child_ref_list
        .iter()
        .filter(|cr| cr.r#ref != home_handle)
        .filter_map(|cr| snap.person(&cr.r#ref))
        .map(|p| node_from_person(p, snap))
        .collect()
}

/// Spouses of the home person (the other parent in each of their families).
fn collect_spouses(snap: &Snapshot, home_handle: &str) -> Vec<TreeNode> {
    let Some(person) = snap.person(home_handle) else { return Vec::new() };
    let mut spouses = Vec::new();
    for fam_handle in &person.family_list {
        let Some(fam) = snap.family(fam_handle) else { continue };
        let spouse_handle = if fam.father_handle.as_deref() == Some(home_handle) {
            fam.mother_handle.as_deref()
        } else {
            fam.father_handle.as_deref()
        };
        if let Some(sh) = spouse_handle {
            if let Some(sp) = snap.person(sh) {
                spouses.push(node_from_person(sp, snap));
            }
        }
    }
    spouses
}

fn collect_children(snap: &Snapshot, handle: &str) -> Vec<TreeNode> {
    let Some(person) = snap.person(handle) else { return Vec::new() };
    let mut children = Vec::new();
    for fam_handle in &person.family_list {
        let Some(fam) = snap.family(fam_handle) else { continue };
        for cr in &fam.child_ref_list {
            if let Some(child) = snap.person(&cr.r#ref) {
                children.push(node_from_person(child, snap));
            }
        }
    }
    children
}

fn collect_descendants(
    snap: &Snapshot,
    handle: &str,
    depth: usize,
) -> Vec<(TreeNode, Vec<TreeNode>)> {
    if depth == 0 { return Vec::new() }
    collect_children(snap, handle)
        .into_iter()
        .map(|child| {
            let gc = if depth > 1 { collect_children(snap, &child.handle) } else { Vec::new() };
            (child, gc)
        })
        .collect()
}

fn collect_generation_nodes(
    node: &AncestorNode,
    current_depth: usize,
    max_depth: usize,
    layers: &mut Vec<Vec<TreeNode>>,
) {
    while layers.len() <= current_depth {
        layers.push(Vec::new());
    }
    layers[current_depth].push(node.person.clone());
    if current_depth < max_depth {
        if let Some(f) = &node.father {
            collect_generation_nodes(f, current_depth + 1, max_depth, layers);
        }
        if let Some(m) = &node.mother {
            collect_generation_nodes(m, current_depth + 1, max_depth, layers);
        }
    }
}

/// Render the full tree with pan/scroll in both directions.
pub fn view<'a>(snap: &'a Snapshot, home_handle: &str) -> Element<'a, Message> {
    let ancestors = build_ancestors(snap, home_handle, MAX_DEPTH);
    let descendants = collect_descendants(snap, home_handle, MAX_DESC_DEPTH);
    let siblings = collect_siblings(snap, home_handle);
    let spouses = collect_spouses(snap, home_handle);

    let Some(tree) = ancestors else {
        return container(text("Home person not found in tree.").size(16))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
    };

    let mut col = column![].spacing(4).padding(24).align_x(Alignment::Center);

    // Ancestor generations.
    let mut layers: Vec<Vec<TreeNode>> = Vec::new();
    collect_generation_nodes(&tree, 0, MAX_DEPTH, &mut layers);
    layers.reverse();

    for (gen_idx, layer) in layers.into_iter().enumerate() {
        if gen_idx == MAX_DEPTH { break; }
        let gen_label: String = match MAX_DEPTH - gen_idx {
            1 => "Parents".to_string(),
            2 => "Grandparents".to_string(),
            3 => "Great-grandparents".to_string(),
            n => format!("{n}x great-grandparents"),
        };
        let mut r = row![].spacing(12).align_y(Alignment::Center);
        for node in layer {
            r = r.push(person_card(node, false));
        }
        col = col.push(
            column![
                text(gen_label).size(11).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                r,
            ]
            .spacing(4)
            .align_x(Alignment::Center),
        );
        col = col.push(connector_vertical());
    }

    // Home row: siblings + HOME + spouse.
    let mut home_row = row![].spacing(12).align_y(Alignment::Center);

    if !siblings.is_empty() {
        let mut sib_col = column![
            text("Siblings").size(10).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        ].spacing(4).align_x(Alignment::Center);
        let mut sib_row = row![].spacing(8);
        for sib in siblings {
            sib_row = sib_row.push(person_card(sib, false));
        }
        sib_col = sib_col.push(sib_row);
        home_row = home_row.push(sib_col);
        home_row = home_row.push(connector_horizontal());
    }

    home_row = home_row.push(person_card(tree.person, true));

    if !spouses.is_empty() {
        home_row = home_row.push(connector_horizontal());
        for sp in spouses {
            home_row = home_row.push(
                column![
                    text("Spouse").size(10).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                    person_card(sp, false),
                ]
                .spacing(4)
                .align_x(Alignment::Center),
            );
        }
    }

    col = col.push(home_row);

    // Descendants.
    if !descendants.is_empty() {
        col = col.push(connector_vertical());
        let mut children_row = row![].spacing(12).align_y(Alignment::Start);
        for (child, grandchildren) in descendants {
            let mut child_col = column![person_card(child, false)]
                .spacing(6)
                .align_x(Alignment::Center);
            if !grandchildren.is_empty() {
                child_col = child_col.push(connector_vertical_small());
                let mut gc_row = row![].spacing(6);
                for gc in grandchildren {
                    gc_row = gc_row.push(person_card_small(gc));
                }
                child_col = child_col.push(gc_row);
            }
            children_row = children_row.push(child_col);
        }
        col = col.push(
            column![
                text("Children").size(11).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                children_row,
            ]
            .spacing(4)
            .align_x(Alignment::Center),
        );
    }

    // Wrap in a bi-directional scrollable for large trees.
    // Content must be Shrink width so horizontal scroll works.
    let scroll = scrollable(
        container(col)
            .width(Length::Shrink)
            .padding([0, 40]),
    )
    .direction(Direction::Both {
        horizontal: Scrollbar::default(),
        vertical: Scrollbar::default(),
    })
    .width(Length::Fill)
    .height(Length::Fill);

    container(scroll)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

// ---- visual connectors ------------------------------------------------

fn connector_vertical() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(2.0))
        .height(Length::Fixed(20.0))
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(iced::Background::Color(palette.background.strong.color)),
                ..Default::default()
            }
        })
        .into()
}

fn connector_vertical_small() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(12.0))
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(iced::Background::Color(palette.background.strong.color)),
                ..Default::default()
            }
        })
        .into()
}

fn connector_horizontal() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(24.0))
        .height(Length::Fixed(2.0))
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(iced::Background::Color(palette.background.strong.color)),
                ..Default::default()
            }
        })
        .into()
}

// ---- person cards ------------------------------------------------------

fn person_card(node: TreeNode, is_home: bool) -> Element<'static, Message> {
    let name_size = if is_home { 20 } else { 14 };
    let years_size = if is_home { 14 } else { 11 };
    let padding = if is_home { [16, 24] } else { [10, 16] };

    let mut col = column![text(node.name).size(name_size)].spacing(2);
    if !node.years.is_empty() {
        col = col.push(
            text(node.years)
                .size(years_size)
                .color(iced::Color::from_rgb(0.4, 0.4, 0.4)),
        );
    }
    col = col.push(
        text(node.gramps_id)
            .size(10)
            .color(iced::Color::from_rgb(0.6, 0.6, 0.6)),
    );

    let handle = node.handle.clone();
    let right_handle = node.handle;
    let card = button(container(col).padding(padding).width(Length::Shrink))
        .on_press(Message::TreeHome(handle))
        .style(move |theme: &Theme, status| card_style(theme, status, is_home));

    mouse_area(card)
        .on_right_press(Message::TreeContextMenu(right_handle))
        .into()
}

fn person_card_small(node: TreeNode) -> Element<'static, Message> {
    let col = column![
        text(node.name).size(11),
        text(node.years)
            .size(9)
            .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
    ]
    .spacing(2);

    let handle = node.handle.clone();
    let right_handle = node.handle;
    let card = button(container(col).padding([6, 10]))
        .on_press(Message::TreeHome(handle))
        .style(|theme: &Theme, status| card_style(theme, status, false));

    mouse_area(card)
        .on_right_press(Message::TreeContextMenu(right_handle))
        .into()
}

fn card_style(theme: &Theme, status: button::Status, is_home: bool) -> button::Style {
    let palette = theme.extended_palette();
    let bg = if is_home {
        palette.primary.weak.color
    } else {
        palette.background.weak.color
    };
    let text_color = if is_home {
        palette.primary.weak.text
    } else {
        palette.background.base.text
    };
    let base = button::Style {
        background: Some(iced::Background::Color(bg)),
        text_color,
        border: iced::Border {
            color: if is_home {
                palette.primary.base.color
            } else {
                palette.background.strong.color
            },
            width: if is_home { 2.0 } else { 1.0 },
            radius: 8.0.into(),
        },
        shadow: iced::Shadow::default(),
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(iced::Background::Color(if is_home {
                palette.primary.base.color
            } else {
                palette.background.strong.color
            })),
            text_color: if is_home {
                palette.primary.base.text
            } else {
                text_color
            },
            ..base
        },
        _ => base,
    }
}
