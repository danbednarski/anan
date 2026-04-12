//! "Full Network" view - shows every person connected to the home
//! person via any family relationship (parents, siblings, children,
//! aunts/uncles, cousins, in-laws, etc.).
//!
//! Performs a BFS from the home person through all family links:
//! - person → parent_family_list → family → father/mother/children
//! - person → family_list → family → father/mother/children
//!
//! Groups results by generation distance from home (0 = home,
//! -1 = parents, +1 = children, etc.) and renders them in a
//! scrollable list. Click any person to re-home the tree on them.

use std::collections::{HashMap, HashSet, VecDeque};

use iced::widget::{button, column, container, mouse_area, row, scrollable, text};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::{Alignment, Element, Length, Theme};

use crate::app::Message;
use crate::db::Snapshot;
use crate::theme;
use crate::views::widgets::date_display;

/// Walk all family links from `home_handle` and return every reachable
/// person grouped by generation distance. Negative = ancestor
/// direction, positive = descendant direction, 0 = home's generation.
pub fn walk_network(snap: &Snapshot, home_handle: &str) -> Vec<(i32, Vec<PersonInfo>)> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut generation: HashMap<String, i32> = HashMap::new();
    let mut queue: VecDeque<(String, i32)> = VecDeque::new();

    visited.insert(home_handle.to_string());
    generation.insert(home_handle.to_string(), 0);
    queue.push_back((home_handle.to_string(), 0));

    while let Some((handle, gen)) = queue.pop_front() {
        let Some(person) = snap.person(&handle) else { continue };

        // Walk parent families: parents are gen-1, siblings are gen+0.
        for fam_handle in &person.parent_family_list {
            let Some(fam) = snap.family(fam_handle) else { continue };
            // Parents.
            for parent_h in [&fam.father_handle, &fam.mother_handle].iter().filter_map(|h| h.as_ref()) {
                if visited.insert(parent_h.clone()) {
                    generation.insert(parent_h.clone(), gen - 1);
                    queue.push_back((parent_h.clone(), gen - 1));
                }
            }
            // Siblings (same generation).
            for cr in &fam.child_ref_list {
                if visited.insert(cr.r#ref.clone()) {
                    generation.insert(cr.r#ref.clone(), gen);
                    queue.push_back((cr.r#ref.clone(), gen));
                }
            }
        }

        // Walk own families: spouse is gen+0, children are gen+1.
        for fam_handle in &person.family_list {
            let Some(fam) = snap.family(fam_handle) else { continue };
            // Spouse.
            for spouse_h in [&fam.father_handle, &fam.mother_handle].iter().filter_map(|h| h.as_ref()) {
                if visited.insert(spouse_h.clone()) {
                    generation.insert(spouse_h.clone(), gen);
                    queue.push_back((spouse_h.clone(), gen));
                }
            }
            // Children.
            for cr in &fam.child_ref_list {
                if visited.insert(cr.r#ref.clone()) {
                    generation.insert(cr.r#ref.clone(), gen + 1);
                    queue.push_back((cr.r#ref.clone(), gen + 1));
                }
            }
        }
    }

    // Group by generation.
    let mut groups: HashMap<i32, Vec<PersonInfo>> = HashMap::new();
    for (handle, gen) in &generation {
        if let Some(person) = snap.person(handle) {
            let birth = if person.birth_ref_index >= 0 {
                person.event_ref_list.get(person.birth_ref_index as usize)
                    .and_then(|er| snap.event(&er.r#ref))
                    .and_then(|e| e.date.as_ref())
                    .map(date_display::format)
                    .filter(|s| !s.is_empty())
            } else { None };
            groups.entry(*gen).or_default().push(PersonInfo {
                handle: handle.clone(),
                name: person.primary_name.display(),
                gramps_id: person.gramps_id.clone(),
                birth: birth.unwrap_or_default(),
                is_home: handle == home_handle,
            });
        }
    }

    // Sort groups by gen and people within each group by name.
    let mut sorted: Vec<(i32, Vec<PersonInfo>)> = groups.into_iter().collect();
    sorted.sort_by_key(|(gen, _)| *gen);
    for (_, people) in &mut sorted {
        people.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }
    sorted
}

#[derive(Clone)]
pub struct PersonInfo {
    pub handle: String,
    pub name: String,
    pub gramps_id: String,
    pub birth: String,
    pub is_home: bool,
}

fn gen_label(gen: i32) -> String {
    match gen {
        0 => "Your generation".to_string(),
        -1 => "Parents' generation".to_string(),
        -2 => "Grandparents' generation".to_string(),
        -3 => "Great-grandparents' generation".to_string(),
        1 => "Children's generation".to_string(),
        2 => "Grandchildren's generation".to_string(),
        n if n < 0 => format!("{}x great-grandparents' gen", -n - 1),
        n => format!("{}x great-grandchildren's gen", n - 1),
    }
}

pub fn view<'a>(snap: &'a Snapshot, home_handle: &str) -> Element<'a, Message> {
    let groups = walk_network(snap, home_handle);

    let total: usize = groups.iter().map(|(_, v)| v.len()).sum();

    let mut col = column![
        text(format!("{total} people connected to your family"))
            .size(18)
            .color(theme::TEXT),
    ]
    .spacing(16)
    .padding(24);

    for (gen, people) in groups {
        let label = gen_label(gen);
        let mut gen_col = column![
            text(format!("{label}  ({} people)", people.len()))
                .size(13)
                .color(theme::ACCENT),
        ]
        .spacing(4);

        for person in people {
            let birth_str = if person.birth.is_empty() {
                String::new()
            } else {
                format!("  -  b. {}", person.birth)
            };
            let label = format!(
                "{}  ({}){}",
                person.name, person.gramps_id, birth_str
            );
            let handle = person.handle;
            let is_home = person.is_home;

            let btn = button(text(label).size(12))
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
                });
            gen_col = gen_col.push(btn);
        }

        col = col.push(gen_col);
    }

    container(scrollable(col).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

// ====================================================================
// Full Network as Tree: all BFS people rendered as card rows per gen
// ====================================================================

/// Render the full network as a proper branching family tree.
/// Each couple has their children directly below them with
/// connecting lines. Children who have their own families are
/// rendered recursively as nested family trees.
pub fn tree_view<'a>(
    snap: &'a Snapshot,
    home_handle: &str,
    context_target: Option<&str>,
) -> Element<'a, Message> {
    // Find all people in the network.
    let network_handles: HashSet<String> = {
        let groups = walk_network(snap, home_handle);
        groups.into_iter().flat_map(|(_, p)| p.into_iter().map(|pi| pi.handle)).collect()
    };

    // Find root families: families where neither parent has parents
    // in the network (i.e., they're the oldest generation).
    let mut root_families: Vec<String> = Vec::new();
    let mut child_families: HashSet<String> = HashSet::new();

    for fam in &snap.families {
        let any_member_in = fam.father_handle.as_ref().map(|h| network_handles.contains(h)).unwrap_or(false)
            || fam.mother_handle.as_ref().map(|h| network_handles.contains(h)).unwrap_or(false)
            || fam.child_ref_list.iter().any(|cr| network_handles.contains(&cr.r#ref));
        if !any_member_in { continue; }

        // Check if any parent has their own parent family in the network.
        let has_parent_family = |handle: &Option<String>| -> bool {
            handle.as_ref()
                .and_then(|h| snap.person(h))
                .map(|p| p.parent_family_list.iter().any(|pfh| {
                    snap.family(pfh).map(|pf| {
                        pf.father_handle.as_ref().map(|h| network_handles.contains(h)).unwrap_or(false)
                            || pf.mother_handle.as_ref().map(|h| network_handles.contains(h)).unwrap_or(false)
                    }).unwrap_or(false)
                }))
                .unwrap_or(false)
        };

        if !has_parent_family(&fam.father_handle) && !has_parent_family(&fam.mother_handle) {
            root_families.push(fam.handle.clone());
        } else {
            child_families.insert(fam.handle.clone());
        }
    }

    let total = network_handles.len();
    let mut rendered: HashSet<String> = HashSet::new();

    let mut col = column![
        text(format!("{total} people in your family network")).size(16).color(theme::TEXT),
    ]
    .spacing(16)
    .padding(32)
    .align_x(Alignment::Center);

    // Render each root family as a tree.
    let mut trees_row = row![].spacing(32).align_y(Alignment::Start);
    for fam_handle in &root_families {
        trees_row = trees_row.push(render_family(
            snap, fam_handle, home_handle, context_target,
            &network_handles, &mut rendered, 0,
        ));
    }
    col = col.push(trees_row);

    let scroll = scrollable(container(col).width(Length::Shrink).padding([0, 40]))
        .direction(Direction::Both {
            horizontal: Scrollbar::default(),
            vertical: Scrollbar::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill);

    container(scroll).width(Length::Fill).height(Length::Fill).into()
}

/// Recursively render a family: parents on top, children below.
/// Children who have their own families render as nested family trees.
fn render_family(
    snap: &Snapshot,
    fam_handle: &str,
    home_handle: &str,
    context_target: Option<&str>,
    network: &HashSet<String>,
    rendered: &mut HashSet<String>,
    depth: usize,
) -> Element<'static, Message> {
    let Some(fam) = snap.family(fam_handle) else {
        return text("").into();
    };

    // Limit recursion depth to prevent infinite loops.
    if depth > 6 { return text("...").size(10).into(); }

    let mut family_col = column![].spacing(4).align_x(Alignment::Center);

    // Parents row.
    let mut parents = row![].spacing(0).align_y(Alignment::Center);
    if let Some(father) = fam.father_handle.as_ref().and_then(|h| snap.person(h)) {
        let is_dup = !rendered.insert(father.handle.clone());
        let info = mk_info(father, snap, home_handle);
        if is_dup {
            parents = parents.push(ref_card(&info));
        } else {
            parents = parents.push(network_card(info, context_target));
        }
    }
    if fam.father_handle.is_some() && fam.mother_handle.is_some() {
        parents = parents.push(couple_connector());
    }
    if let Some(mother) = fam.mother_handle.as_ref().and_then(|h| snap.person(h)) {
        let is_dup = !rendered.insert(mother.handle.clone());
        let info = mk_info(mother, snap, home_handle);
        if is_dup {
            parents = parents.push(ref_card(&info));
        } else {
            parents = parents.push(network_card(info, context_target));
        }
    }
    family_col = family_col.push(parents);

    // Children: each child is either a simple card or a nested
    // family tree if they have their own family.
    let children: Vec<&crate::gramps::family::ChildRef> = fam.child_ref_list.iter()
        .filter(|cr| network.contains(&cr.r#ref))
        .collect();

    if !children.is_empty() {
        family_col = family_col.push(vert_line());

        let mut children_row = row![].spacing(16).align_y(Alignment::Start);
        for cr in children {
            let Some(child) = snap.person(&cr.r#ref) else { continue };
            let is_dup = !rendered.insert(child.handle.clone());

            // Does this child have their own family (as parent)?
            let child_family = if !is_dup {
                child.family_list.iter().find(|fh| {
                    snap.family(fh).map(|f| {
                        f.child_ref_list.iter().any(|c| network.contains(&c.r#ref))
                            || f.mother_handle.as_ref().map(|h| network.contains(h)).unwrap_or(false)
                            || f.father_handle.as_ref().map(|h| network.contains(h)).unwrap_or(false)
                    }).unwrap_or(false)
                }).cloned()
            } else {
                None
            };

            if let Some(child_fam_handle) = child_family {
                // Render child's family as a nested tree.
                children_row = children_row.push(
                    render_family(snap, &child_fam_handle, home_handle, context_target, network, rendered, depth + 1)
                );
            } else {
                let info = mk_info(child, snap, home_handle);
                if is_dup {
                    children_row = children_row.push(ref_card(&info));
                } else {
                    children_row = children_row.push(network_card(info, context_target));
                }
            }
        }
        family_col = family_col.push(children_row);
    }

    family_col.into()
}

fn mk_info(person: &crate::gramps::Person, snap: &Snapshot, home_handle: &str) -> PersonInfo {
    let birth = if person.birth_ref_index >= 0 {
        person.event_ref_list.get(person.birth_ref_index as usize)
            .and_then(|er| snap.event(&er.r#ref))
            .and_then(|e| e.date.as_ref())
            .map(crate::views::widgets::date_display::format)
            .filter(|s| !s.is_empty())
            .unwrap_or_default()
    } else { String::new() };
    PersonInfo {
        handle: person.handle.clone(),
        name: person.primary_name.display(),
        gramps_id: person.gramps_id.clone(),
        birth,
        is_home: person.handle == home_handle,
    }
}

/// Small reference card for already-rendered person.
fn ref_card(person: &PersonInfo) -> Element<'static, Message> {
    let handle = person.handle.clone();
    button(
        container(text(format!("{} *", person.name)).size(10).color(theme::TEXT_MUTED))
            .padding([4, 8]),
    )
    .on_press(Message::TreeHome(handle))
    .style(|_: &Theme, _| button::Style {
        background: None,
        text_color: theme::TEXT_MUTED,
        border: iced::Border { color: theme::BORDER, width: 1.0, radius: 6.0.into() },
        shadow: iced::Shadow::default(),
    })
    .into()
}

/// Vertical connector line from parents to children.
fn vert_line() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(2.0))
        .height(Length::Fixed(16.0))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(theme::CONNECTOR)),
            ..Default::default()
        })
        .into()
}

/// Short horizontal connector between a couple.
fn couple_connector() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(2.0))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(theme::ACCENT)),
            ..Default::default()
        })
        .into()
}

fn network_card(
    person: PersonInfo,
    context_target: Option<&str>,
) -> Element<'static, Message> {
    let is_home = person.is_home;
    let show_menu = context_target == Some(person.handle.as_str());

    let mut card_col = column![
        text(person.name).size(13),
    ]
    .spacing(2);
    if !person.birth.is_empty() {
        card_col = card_col.push(
            text(person.birth)
                .size(10)
                .color(if is_home {
                    iced::Color::from_rgba(1.0, 1.0, 1.0, 0.7)
                } else {
                    theme::TEXT_MUTED
                }),
        );
    }
    card_col = card_col.push(
        text(person.gramps_id)
            .size(9)
            .color(if is_home {
                iced::Color::from_rgba(1.0, 1.0, 1.0, 0.5)
            } else {
                iced::Color::from_rgb(0.7, 0.7, 0.7)
            }),
    );

    let handle = person.handle.clone();
    let right_handle = person.handle.clone();
    let menu_handle = person.handle;
    let card = button(container(card_col).padding([8, 12]).width(Length::Shrink))
        .on_press(Message::TreeHome(handle))
        .style(move |_: &Theme, status| {
            if is_home {
                let bg = match status {
                    button::Status::Hovered | button::Status::Pressed => theme::HOME_HOVER,
                    _ => theme::HOME_BG,
                };
                button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: iced::Color::WHITE,
                    border: iced::Border { color: theme::PRIMARY, width: 2.0, radius: 8.0.into() },
                    shadow: iced::Shadow {
                        color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.1),
                        offset: iced::Vector::new(0.0, 2.0),
                        blur_radius: 6.0,
                    },
                }
            } else {
                let bg = match status {
                    button::Status::Hovered | button::Status::Pressed => theme::ANCESTOR_HOVER,
                    _ => theme::ANCESTOR_BG,
                };
                button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: theme::TEXT,
                    border: iced::Border { color: theme::BORDER, width: 1.0, radius: 8.0.into() },
                    shadow: iced::Shadow {
                        color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.05),
                        offset: iced::Vector::new(0.0, 1.0),
                        blur_radius: 3.0,
                    },
                }
            }
        });

    let ma = mouse_area(card)
        .on_right_press(Message::TreeContextMenu(right_handle));

    if show_menu {
        column![ma, super::tree::context_menu_widget(menu_handle)]
            .spacing(4)
            .align_x(Alignment::Center)
            .into()
    } else {
        ma.into()
    }
}

