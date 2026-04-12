//! Family tree view - the hero view of the app.
//!
//! Shows ancestors above, the home person + siblings + spouse in the
//! center, and descendants below. The entire tree is scrollable in
//! both directions for large families. Right-click any person card
//! to open a floating context menu next to that card.

use iced::widget::{button, column, container, mouse_area, row, scrollable, text, Space};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::{Alignment, Element, Length, Theme};

use crate::app::{AddRelationship, Message};
use crate::db::Snapshot;
use crate::gramps::Person;
use crate::theme;
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
        person.event_ref_list.get(person.birth_ref_index as usize)
            .and_then(|er| snap.event(&er.r#ref))
            .and_then(|e| e.date.as_ref())
            .map(date_display::format)
            .filter(|s| !s.is_empty())
    } else { None };
    let death = if person.death_ref_index >= 0 {
        person.event_ref_list.get(person.death_ref_index as usize)
            .and_then(|er| snap.event(&er.r#ref))
            .and_then(|e| e.date.as_ref())
            .map(date_display::format)
            .filter(|s| !s.is_empty())
    } else { None };
    let years = match (birth, death) {
        (Some(b), Some(d)) => format!("{b} - {d}"),
        (Some(b), None) => format!("b. {b}"),
        (None, Some(d)) => format!("d. {d}"),
        (None, None) => String::new(),
    };
    TreeNode { handle: person.handle.clone(), name: person.primary_name.display(), gramps_id: person.gramps_id.clone(), years }
}

pub fn build_ancestors(snap: &Snapshot, handle: &str, depth: usize) -> Option<AncestorNode> {
    let person = snap.person(handle)?;
    let node = node_from_person(person, snap);
    if depth == 0 {
        return Some(AncestorNode { person: node, father: None, mother: None });
    }
    let (father, mother) = person.parent_family_list.first()
        .and_then(|fh| snap.family(fh))
        .map(|fam| {
            let f = fam.father_handle.as_ref().and_then(|h| build_ancestors(snap, h, depth - 1)).map(Box::new);
            let m = fam.mother_handle.as_ref().and_then(|h| build_ancestors(snap, h, depth - 1)).map(Box::new);
            (f, m)
        })
        .unwrap_or((None, None));
    Some(AncestorNode { person: node, father, mother })
}

fn collect_siblings(snap: &Snapshot, home_handle: &str) -> Vec<TreeNode> {
    let Some(person) = snap.person(home_handle) else { return Vec::new() };
    let Some(fam_handle) = person.parent_family_list.first() else { return Vec::new() };
    let Some(fam) = snap.family(fam_handle) else { return Vec::new() };
    fam.child_ref_list.iter()
        .filter(|cr| cr.r#ref != home_handle)
        .filter_map(|cr| snap.person(&cr.r#ref))
        .map(|p| node_from_person(p, snap))
        .collect()
}

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

fn collect_descendants(snap: &Snapshot, handle: &str, depth: usize) -> Vec<(TreeNode, Vec<TreeNode>)> {
    if depth == 0 { return Vec::new() }
    collect_children(snap, handle).into_iter().map(|child| {
        let gc = if depth > 1 { collect_children(snap, &child.handle) } else { Vec::new() };
        (child, gc)
    }).collect()
}

fn collect_generation_nodes(node: &AncestorNode, depth: usize, max: usize, layers: &mut Vec<Vec<TreeNode>>) {
    while layers.len() <= depth { layers.push(Vec::new()); }
    layers[depth].push(node.person.clone());
    if depth < max {
        if let Some(f) = &node.father { collect_generation_nodes(f, depth + 1, max, layers); }
        if let Some(m) = &node.mother { collect_generation_nodes(m, depth + 1, max, layers); }
    }
}

/// Render the full tree. `context_target` is the handle of the person
/// whose inline context menu should be shown (from right-click).
pub fn view<'a>(
    snap: &'a Snapshot,
    home_handle: &str,
    context_target: Option<&str>,
) -> Element<'a, Message> {
    let ancestors = build_ancestors(snap, home_handle, MAX_DEPTH);
    let descendants = collect_descendants(snap, home_handle, MAX_DESC_DEPTH);
    let siblings = collect_siblings(snap, home_handle);
    let spouses = collect_spouses(snap, home_handle);

    let Some(tree) = ancestors else {
        return container(text("Home person not found in tree.").size(16))
            .width(Length::Fill).height(Length::Fill)
            .center_x(Length::Fill).center_y(Length::Fill)
            .into();
    };

    let mut col = column![].spacing(6).padding(32).align_x(Alignment::Center);

    // Ancestor generations. Layer 0 = home person, 1 = parents,
    // 2 = grandparents, etc. After reversing, the last entry is
    // always the home person — skip it here, render it separately.
    let mut layers: Vec<Vec<TreeNode>> = Vec::new();
    collect_generation_nodes(&tree, 0, MAX_DEPTH, &mut layers);
    layers.reverse();

    let num_layers = layers.len();
    for (gen_idx, layer) in layers.into_iter().enumerate() {
        // Last layer after reverse is the home person — skip.
        if gen_idx >= num_layers - 1 {
            break;
        }
        // Distance from the home layer.
        let distance = num_layers - 1 - gen_idx;
        let gen_label: String = match distance {
            1 => "Parents".to_string(),
            2 => "Grandparents".to_string(),
            3 => "Great-grandparents".to_string(),
            n => format!("{}x great-grandparents", n - 1),
        };
        let mut r = row![].spacing(16).align_y(Alignment::Start);
        for node in layer {
            r = r.push(card_with_menu(node, false, context_target));
        }
        col = col.push(gen_header(&gen_label));
        col = col.push(r);
        col = col.push(connector_v());
    }

    // Home row: siblings + HOME + spouse.
    let mut home_row = row![].spacing(16).align_y(Alignment::Center);

    if !siblings.is_empty() {
        let mut sib_items = row![].spacing(8).align_y(Alignment::Start);
        for sib in siblings {
            sib_items = sib_items.push(card_with_menu(sib, false, context_target));
        }
        home_row = home_row.push(
            column![gen_header("Siblings"), sib_items].spacing(4).align_x(Alignment::Center)
        );
        home_row = home_row.push(connector_h());
    }

    home_row = home_row.push(card_with_menu(tree.person, true, context_target));

    if !spouses.is_empty() {
        for sp in spouses {
            home_row = home_row.push(connector_h());
            home_row = home_row.push(
                column![gen_header("Spouse"), card_with_menu(sp, false, context_target)]
                    .spacing(4).align_x(Alignment::Center)
            );
        }
    }

    col = col.push(home_row);

    // Descendants.
    if !descendants.is_empty() {
        col = col.push(connector_v());
        col = col.push(gen_header("Children"));
        let mut children_row = row![].spacing(16).align_y(Alignment::Start);
        for (child, grandchildren) in descendants {
            let mut child_col = column![card_with_menu(child, false, context_target)]
                .spacing(6).align_x(Alignment::Center);
            if !grandchildren.is_empty() {
                child_col = child_col.push(connector_v_small());
                let mut gc_row = row![].spacing(8);
                for gc in grandchildren {
                    gc_row = gc_row.push(card_small_with_menu(gc, context_target));
                }
                child_col = child_col.push(gc_row);
            }
            children_row = children_row.push(child_col);
        }
        col = col.push(children_row);
    }

    let scroll = scrollable(container(col).width(Length::Shrink).padding([0, 40]))
        .direction(Direction::Both {
            horizontal: Scrollbar::default(),
            vertical: Scrollbar::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill);

    container(scroll).width(Length::Fill).height(Length::Fill).into()
}

// ---- generation header -------------------------------------------------

fn gen_header(label: &str) -> Element<'static, Message> {
    text(label.to_string())
        .size(11)
        .color(theme::TEXT_MUTED)
        .into()
}

// ---- cards with inline context menu ------------------------------------

/// Person card - context menu is now a floating overlay in app.rs.
fn card_with_menu(
    node: TreeNode,
    is_home: bool,
    _context_target: Option<&str>,
) -> Element<'static, Message> {
    person_card(node, is_home)
}

fn card_small_with_menu(
    node: TreeNode,
    _context_target: Option<&str>,
) -> Element<'static, Message> {
    person_card_small(node)
}

/// The floating-style context menu rendered inline below the card.
/// Public so network::tree_view can reuse it.
pub fn context_menu_widget(handle: String) -> Element<'static, Message> {
    let menu_btn = |label: &str, msg: Message| {
        button(
            text(label.to_string()).size(12)
        )
        .on_press(msg)
        .width(Length::Fill)
        .style(|_theme: &Theme, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => theme::ANCESTOR_HOVER,
                _ => theme::MENU_BG,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: theme::TEXT,
                border: iced::Border { color: iced::Color::TRANSPARENT, width: 0.0, radius: 4.0.into() },
                shadow: iced::Shadow::default(),
            }
        })
    };

    let menu = container(
        column![
            menu_btn("Add child", Message::TreeStartAdd(AddRelationship::Child)),
            menu_btn("Add father", Message::TreeStartAdd(AddRelationship::Father)),
            menu_btn("Add mother", Message::TreeStartAdd(AddRelationship::Mother)),
            menu_btn("Add sibling", Message::TreeStartAdd(AddRelationship::Sibling)),
            Space::with_height(4),
            menu_btn("Center on this person", Message::TreeHome(handle.clone())),
            Space::with_height(4),
            menu_btn("Dismiss", Message::TreeDismissContext),
        ]
        .spacing(2)
        .padding(6)
        .width(Length::Fixed(180.0)),
    )
    .style(|_theme: &Theme| container::Style {
        background: Some(iced::Background::Color(theme::MENU_BG)),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: iced::Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.15),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    });

    menu.into()
}

// ---- connectors --------------------------------------------------------

fn connector_v() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(2.0))
        .height(Length::Fixed(18.0))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(theme::CONNECTOR)),
            ..Default::default()
        })
        .into()
}

fn connector_v_small() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(10.0))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(theme::CONNECTOR)),
            ..Default::default()
        })
        .into()
}

fn connector_h() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(20.0))
        .height(Length::Fixed(2.0))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(theme::CONNECTOR)),
            ..Default::default()
        })
        .into()
}

// ---- person cards with themed styling ----------------------------------

fn person_card(node: TreeNode, is_home: bool) -> Element<'static, Message> {
    let name_size: u16 = if is_home { 18 } else { 13 };
    let years_size: u16 = if is_home { 13 } else { 10 };
    let padding: [u16; 2] = if is_home { [14, 20] } else { [10, 14] };

    let mut col = column![
        text(node.name).size(name_size),
    ].spacing(2);
    if !node.years.is_empty() {
        col = col.push(text(node.years).size(years_size).color(
            if is_home { iced::Color::from_rgba(1.0, 1.0, 1.0, 0.75) }
            else { theme::TEXT_MUTED }
        ));
    }
    col = col.push(text(node.gramps_id).size(9).color(
        if is_home { iced::Color::from_rgba(1.0, 1.0, 1.0, 0.5) }
        else { iced::Color::from_rgb(0.7, 0.7, 0.7) }
    ));

    let handle = node.handle.clone();
    let right_handle = node.handle;
    let card = button(container(col).padding(padding).width(Length::Shrink))
        .on_press(Message::TreeHome(handle))
        .style(move |_theme: &Theme, status| {
            if is_home { home_card_style(status) } else { ancestor_card_style(status) }
        });

    mouse_area(card)
        .on_right_press(Message::TreeContextMenu(right_handle))
        .into()
}

fn person_card_small(node: TreeNode) -> Element<'static, Message> {
    let col = column![
        text(node.name).size(11),
        text(node.years).size(8).color(theme::TEXT_MUTED),
    ].spacing(1);

    let handle = node.handle.clone();
    let right_handle = node.handle;
    let card = button(container(col).padding([6, 10]))
        .on_press(Message::TreeHome(handle))
        .style(|_: &Theme, status| ancestor_card_style(status));

    mouse_area(card)
        .on_right_press(Message::TreeContextMenu(right_handle))
        .into()
}

fn home_card_style(status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => theme::HOME_HOVER,
        _ => theme::HOME_BG,
    };
    button::Style {
        background: Some(iced::Background::Color(bg)),
        text_color: iced::Color::WHITE,
        border: iced::Border {
            color: theme::PRIMARY,
            width: 2.0,
            radius: 10.0.into(),
        },
        shadow: iced::Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.12),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
    }
}

fn ancestor_card_style(status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => theme::ANCESTOR_HOVER,
        _ => theme::ANCESTOR_BG,
    };
    button::Style {
        background: Some(iced::Background::Color(bg)),
        text_color: theme::TEXT,
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: iced::Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.06),
            offset: iced::Vector::new(0.0, 1.0),
            blur_radius: 4.0,
        },
    }
}

// ====================================================================
// Family Tree as List: flat list of just the direct-tree persons
// ====================================================================

/// Render the Family Tree scope as a flat grouped list (for the
/// "List" toggle on the Family Tree view).
pub fn list_view<'a>(snap: &'a Snapshot, home_handle: &str) -> Element<'a, Message> {
    let ancestors = build_ancestors(snap, home_handle, MAX_DEPTH);
    let descendants = collect_descendants(snap, home_handle, MAX_DESC_DEPTH);
    let siblings = collect_siblings(snap, home_handle);
    let spouses = collect_spouses(snap, home_handle);

    let mut col = column![].spacing(12).padding(24);

    // Home person.
    if let Some(home) = snap.person(home_handle) {
        col = col.push(text("Home").size(12).color(theme::ACCENT));
        col = col.push(person_list_row(node_from_person(home, snap), true));
    }

    // Spouses.
    if !spouses.is_empty() {
        col = col.push(text("Spouse(s)").size(12).color(theme::ACCENT));
        for sp in spouses {
            col = col.push(person_list_row(sp, false));
        }
    }

    // Siblings.
    if !siblings.is_empty() {
        col = col.push(text("Siblings").size(12).color(theme::ACCENT));
        for sib in siblings {
            col = col.push(person_list_row(sib, false));
        }
    }

    // Ancestors (grouped by generation).
    if let Some(tree) = &ancestors {
        let mut layers: Vec<Vec<TreeNode>> = Vec::new();
        collect_generation_nodes(tree, 0, MAX_DEPTH, &mut layers);
        // Skip layer 0 (home, already shown).
        for (depth, layer) in layers.iter().enumerate().skip(1) {
            let label = match depth {
                1 => "Parents".to_string(),
                2 => "Grandparents".to_string(),
                3 => "Great-grandparents".to_string(),
                n => format!("{}x great-grandparents", n - 1),
            };
            col = col.push(text(label).size(12).color(theme::ACCENT));
            for node in layer {
                col = col.push(person_list_row(node.clone(), false));
            }
        }
    }

    // Descendants.
    if !descendants.is_empty() {
        col = col.push(text("Children").size(12).color(theme::ACCENT));
        for (child, grandchildren) in &descendants {
            col = col.push(person_list_row(child.clone(), false));
            for gc in grandchildren {
                col = col.push(
                    row![
                        Space::with_width(Length::Fixed(20.0)),
                        person_list_row(gc.clone(), false),
                    ]
                    .spacing(0),
                );
            }
        }
    }

    container(scrollable(col).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn person_list_row(node: TreeNode, is_home: bool) -> Element<'static, Message> {
    let years = if node.years.is_empty() {
        String::new()
    } else {
        format!("  -  {}", node.years)
    };
    let label = format!("{}  ({}){}", node.name, node.gramps_id, years);
    let handle = node.handle;
    button(text(label).size(13))
        .on_press(Message::TreeHome(handle))
        .width(Length::Fill)
        .style(move |_: &Theme, status| {
            let bg = if is_home {
                theme::PRIMARY
            } else {
                match status {
                    button::Status::Hovered | button::Status::Pressed => theme::ANCESTOR_HOVER,
                    _ => theme::ANCESTOR_BG,
                }
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: if is_home { iced::Color::WHITE } else { theme::TEXT },
                border: iced::Border {
                    color: theme::BORDER,
                    width: 0.5,
                    radius: 6.0.into(),
                },
                shadow: iced::Shadow::default(),
            }
        })
        .into()
}

// ====================================================================
// Full Network as Tree: pedigree tree with extended family at each gen
// ====================================================================

/// Collect siblings of a given ancestor (other children of that
/// ancestor's parents, excluding the ancestor themselves).
fn ancestor_siblings(snap: &Snapshot, handle: &str) -> Vec<TreeNode> {
    collect_siblings(snap, handle)
}

/// Render a tree that includes extended family: at each ancestor
/// generation, also show that ancestor's siblings (aunts/uncles/
/// great-aunts etc.) branching off to the side.
pub fn view_extended<'a>(
    snap: &'a Snapshot,
    home_handle: &str,
    context_target: Option<&str>,
) -> Element<'a, Message> {
    let ancestors = build_ancestors(snap, home_handle, MAX_DEPTH);
    let descendants = collect_descendants(snap, home_handle, MAX_DESC_DEPTH);
    let siblings = collect_siblings(snap, home_handle);
    let spouses = collect_spouses(snap, home_handle);

    let Some(tree) = ancestors else {
        return container(text("Home person not found.").size(16))
            .width(Length::Fill).height(Length::Fill)
            .center_x(Length::Fill).center_y(Length::Fill)
            .into();
    };

    let mut col = column![].spacing(6).padding(32).align_x(Alignment::Center);

    // Ancestor generations with extended family.
    let mut layers: Vec<Vec<TreeNode>> = Vec::new();
    collect_generation_nodes(&tree, 0, MAX_DEPTH, &mut layers);
    layers.reverse();

    let num_layers = layers.len();
    // Also collect which ancestors had siblings for extended display.
    let mut ancestor_handles: Vec<Vec<String>> = Vec::new();
    {
        let mut raw_layers: Vec<Vec<TreeNode>> = Vec::new();
        collect_generation_nodes(&tree, 0, MAX_DEPTH, &mut raw_layers);
        // raw_layers[0] = home, raw_layers[1] = parents, etc.
        for layer in &raw_layers {
            ancestor_handles.push(layer.iter().map(|n| n.handle.clone()).collect());
        }
    }

    for (gen_idx, layer) in layers.into_iter().enumerate() {
        if gen_idx >= num_layers - 1 { break; }
        let distance = num_layers - 1 - gen_idx;
        let gen_label: String = match distance {
            1 => "Parents".to_string(),
            2 => "Grandparents".to_string(),
            3 => "Great-grandparents".to_string(),
            n => format!("{}x great-grandparents", n - 1),
        };
        col = col.push(gen_header(&gen_label));

        let mut r = row![].spacing(16).align_y(Alignment::Start);
        for node in &layer {
            // Show this ancestor.
            r = r.push(card_with_menu(node.clone(), false, context_target));
            // Show their siblings (aunts/uncles etc).
            let sibs = ancestor_siblings(snap, &node.handle);
            if !sibs.is_empty() {
                r = r.push(connector_h());
                for sib in sibs {
                    r = r.push(card_with_menu(sib, false, context_target));
                }
            }
        }
        col = col.push(r);
        col = col.push(connector_v());
    }

    // Home row: siblings + HOME + spouse.
    let mut home_row = row![].spacing(16).align_y(Alignment::Center);
    if !siblings.is_empty() {
        let mut sib_items = row![].spacing(8).align_y(Alignment::Start);
        for sib in siblings {
            sib_items = sib_items.push(card_with_menu(sib, false, context_target));
        }
        home_row = home_row.push(
            column![gen_header("Siblings"), sib_items].spacing(4).align_x(Alignment::Center)
        );
        home_row = home_row.push(connector_h());
    }
    home_row = home_row.push(card_with_menu(tree.person, true, context_target));
    if !spouses.is_empty() {
        for sp in spouses {
            home_row = home_row.push(connector_h());
            home_row = home_row.push(
                column![gen_header("Spouse"), card_with_menu(sp, false, context_target)]
                    .spacing(4).align_x(Alignment::Center)
            );
        }
    }
    col = col.push(home_row);

    // Descendants.
    if !descendants.is_empty() {
        col = col.push(connector_v());
        col = col.push(gen_header("Children"));
        let mut children_row = row![].spacing(16).align_y(Alignment::Start);
        for (child, grandchildren) in descendants {
            let mut child_col = column![card_with_menu(child, false, context_target)]
                .spacing(6).align_x(Alignment::Center);
            if !grandchildren.is_empty() {
                child_col = child_col.push(connector_v_small());
                let mut gc_row = row![].spacing(6);
                for gc in grandchildren {
                    gc_row = gc_row.push(card_small_with_menu(gc, context_target));
                }
                child_col = child_col.push(gc_row);
            }
            children_row = children_row.push(child_col);
        }
        col = col.push(children_row);
    }

    let scroll = scrollable(container(col).width(Length::Shrink).padding([0, 40]))
        .direction(Direction::Both {
            horizontal: Scrollbar::default(),
            vertical: Scrollbar::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill);

    container(scroll).width(Length::Fill).height(Length::Fill).into()
}
