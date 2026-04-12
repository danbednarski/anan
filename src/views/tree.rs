//! Family tree view - the default/hero view of the app.
//!
//! Centered on a "home person", renders:
//!
//! - Great-grandparents (generation -3) as small cards
//! - Grandparents (generation -2)
//! - Parents (generation -1)
//! - Home person (generation 0, prominent card)
//! - Children (generation +1)
//! - Grandchildren (generation +2)
//!
//! Clicking a person card re-homes the tree on that person.
//! The action bar above shows: + Parent / + Child / + Sibling /
//! Edit / Delete for the current home person. Right-click context
//! menus are deferred to a polish iteration.

use iced::widget::{button, column, container, mouse_area, row, scrollable, text};
use iced::{Alignment, Element, Length, Theme};

use crate::app::Message;
use crate::db::Snapshot;
use crate::gramps::Person;
use crate::views::widgets::date_display;

/// Maximum ancestor depth to render (0 = home only, 3 = up to
/// great-grandparents). Deeper trees are cut off with an indicator.
const MAX_DEPTH: usize = 3;
/// Maximum descendant depth.
const MAX_DESC_DEPTH: usize = 2;

/// A node in the precomputed ancestor/descendant tree.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub handle: String,
    pub name: String,
    pub gramps_id: String,
    pub years: String,
}

/// Precomputed ancestor tree (binary tree: father + mother).
#[derive(Debug, Clone)]
pub struct AncestorNode {
    pub person: TreeNode,
    pub father: Option<Box<AncestorNode>>,
    pub mother: Option<Box<AncestorNode>>,
}

/// Build a `TreeNode` from a `Person` + snapshot for resolving dates.
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

/// Build the ancestor tree rooted at `handle`, up to `depth` levels.
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

    // Walk the person's parent_family_list to find parents.
    let (father, mother) = person
        .parent_family_list
        .first()
        .and_then(|fh| snap.family(fh))
        .map(|fam| {
            let f = fam
                .father_handle
                .as_ref()
                .and_then(|h| build_ancestors(snap, h, depth - 1))
                .map(Box::new);
            let m = fam
                .mother_handle
                .as_ref()
                .and_then(|h| build_ancestors(snap, h, depth - 1))
                .map(Box::new);
            (f, m)
        })
        .unwrap_or((None, None));

    Some(AncestorNode {
        person: node,
        father,
        mother,
    })
}

/// Collect the direct children of `handle` (via family_list → child_ref_list).
pub fn collect_children(snap: &Snapshot, handle: &str) -> Vec<TreeNode> {
    let Some(person) = snap.person(handle) else {
        return Vec::new();
    };
    let mut children = Vec::new();
    for fam_handle in &person.family_list {
        let Some(fam) = snap.family(fam_handle) else {
            continue;
        };
        for cr in &fam.child_ref_list {
            if let Some(child) = snap.person(&cr.r#ref) {
                children.push(node_from_person(child, snap));
            }
        }
    }
    children
}

/// Collect descendants recursively up to `depth` levels.
pub fn collect_descendants(
    snap: &Snapshot,
    handle: &str,
    depth: usize,
) -> Vec<(TreeNode, Vec<TreeNode>)> {
    if depth == 0 {
        return Vec::new();
    }
    let children = collect_children(snap, handle);
    children
        .into_iter()
        .map(|child| {
            let grandchildren = if depth > 1 {
                collect_children(snap, &child.handle)
            } else {
                Vec::new()
            };
            (child, grandchildren)
        })
        .collect()
}

/// Render the full tree view: ancestors above, home in the middle,
/// descendants below.
pub fn view<'a>(
    snap: &'a Snapshot,
    home_handle: &str,
) -> Element<'a, Message> {
    let ancestors = build_ancestors(snap, home_handle, MAX_DEPTH);
    let descendants = collect_descendants(snap, home_handle, MAX_DESC_DEPTH);

    let Some(tree) = ancestors else {
        return container(text("Home person not found in tree.").size(16))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
    };

    let mut col = column![].spacing(20).padding(24).align_x(Alignment::Center);

    // Collect ancestor nodes into flat generation layers.
    let mut layers: Vec<Vec<TreeNode>> = Vec::new();
    collect_generation_nodes(&tree, 0, MAX_DEPTH, &mut layers);
    layers.reverse();

    for (gen_idx, layer) in layers.into_iter().enumerate() {
        // The last layer (after reverse) is the home person — rendered
        // prominently below rather than in this loop.
        if gen_idx == MAX_DEPTH {
            break;
        }
        let gen_label: String = match MAX_DEPTH - gen_idx {
            1 => "Parents".to_string(),
            2 => "Grandparents".to_string(),
            3 => "Great-grandparents".to_string(),
            n => format!("{n}x great-grandparents"),
        };
        let mut r = row![].spacing(16).align_y(Alignment::Center);
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
    }

    // Home person — prominent card.
    col = col.push(
        column![
            text("").size(4),
            person_card(tree.person, true),
            text("").size(4),
        ]
        .align_x(Alignment::Center),
    );

    // Descendants.
    if !descendants.is_empty() {
        let mut children_row = row![].spacing(16).align_y(Alignment::Start);
        for (child, grandchildren) in descendants {
            let mut child_col = column![person_card(child, false)]
                .spacing(8)
                .align_x(Alignment::Center);
            if !grandchildren.is_empty() {
                let mut gc_row = row![].spacing(8);
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

    container(scrollable(col))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .into()
}

/// Collect owned TreeNodes into generation layers (BFS-style).
/// Each layer owns its TreeNode clones so the returned
/// `Vec<Vec<TreeNode>>` doesn't borrow from the tree.
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

/// A clickable person card.
/// - Left-click → re-home the tree on this person.
/// - Right-click → open context menu for this person.
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
