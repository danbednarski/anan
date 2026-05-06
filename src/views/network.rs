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
/// Each couple has their children directly below them. Children
/// who have their own families nest recursively.
///
/// Uses BFS generation data to find the actual oldest generation
/// and starts the tree from there. No dedup - everyone is a full
/// card (a person who connects two families appears in both).
pub fn tree_view<'a>(
    snap: &'a Snapshot,
    home_handle: &str,
    _context_target: Option<&str>,
) -> Element<'a, Message> {
    let groups = walk_network(snap, home_handle);
    let network_handles: HashSet<String> = groups.iter()
        .flat_map(|(_, p)| p.iter().map(|pi| pi.handle.clone()))
        .collect();
    let total = network_handles.len();

    // Build a generation map: handle -> gen number.
    let mut gen_map: HashMap<String, i32> = HashMap::new();
    for (gen, people) in &groups {
        for p in people {
            gen_map.insert(p.handle.clone(), *gen);
        }
    }

    // Find the oldest generation number.
    let oldest_gen = groups.first().map(|(g, _)| *g).unwrap_or(0);

    // Root families: families where at least one parent is in the
    // oldest generation.
    let mut root_families: Vec<String> = Vec::new();
    let mut used_families: HashSet<String> = HashSet::new();
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
            used_families.insert(fam.handle.clone());
        }
    }

    // Also find families that aren't reachable from roots (e.g.,
    // in-law families that connect at a middle generation). These
    // are families where parents are in the network but weren't
    // children of any root family.
    // We handle these by letting the recursive renderer pick them up
    // when it encounters a child who has their own family.

    let mut col = column![
        text(format!("{total} people in your family network")).size(16).color(theme::TEXT),
    ]
    .spacing(16)
    .padding(32)
    .align_x(Alignment::Center);

    let mut trees_row = row![].spacing(40).align_y(Alignment::Start);
    for fam_handle in &root_families {
        trees_row = trees_row.push(render_family(
            snap, fam_handle, home_handle, _context_target,
            &network_handles, 0,
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

/// Recursively render one family: parents on top, children below.
/// No dedup - everyone gets a full card.
fn render_family(
    snap: &Snapshot,
    fam_handle: &str,
    home_handle: &str,
    _context_target: Option<&str>,
    network: &HashSet<String>,
    depth: usize,
) -> Element<'static, Message> {
    let Some(fam) = snap.family(fam_handle) else {
        return text("").into();
    };
    if depth > 8 { return text("...").size(10).into(); }

    let mut family_col = column![].spacing(4).align_x(Alignment::Center);

    // Parents row.
    let mut parents = row![].spacing(0).align_y(Alignment::Center);
    let has_father = fam.father_handle.as_ref()
        .and_then(|h| snap.person(h)).is_some();
    let has_mother = fam.mother_handle.as_ref()
        .and_then(|h| snap.person(h)).is_some();

    if let Some(father) = fam.father_handle.as_ref().and_then(|h| snap.person(h)) {
        parents = parents.push(network_card(mk_info(father, snap, home_handle), father.gender));
    }
    if has_father && has_mother {
        parents = parents.push(couple_connector());
    }
    if let Some(mother) = fam.mother_handle.as_ref().and_then(|h| snap.person(h)) {
        parents = parents.push(network_card(mk_info(mother, snap, home_handle), mother.gender));
    }
    family_col = family_col.push(parents);

    // Children with bracket connector.
    let children: Vec<&crate::gramps::family::ChildRef> = fam.child_ref_list.iter()
        .filter(|cr| network.contains(&cr.r#ref))
        .collect();

    if !children.is_empty() {
        // Vertical stem from parents down to bracket.
        family_col = family_col.push(vert_line());

        // Build children elements first.
        let mut child_elements: Vec<Element<'static, Message>> = Vec::new();
        for cr in &children {
            let Some(child) = snap.person(&cr.r#ref) else { continue };
            let child_family = child.family_list.iter().find(|fh| {
                snap.family(fh).map(|f| {
                    f.child_ref_list.iter().any(|c| network.contains(&c.r#ref))
                        || f.mother_handle.as_ref().map(|h| network.contains(h)).unwrap_or(false)
                        || f.father_handle.as_ref().map(|h| network.contains(h)).unwrap_or(false)
                }).unwrap_or(false)
            }).cloned();

            if let Some(child_fam_handle) = child_family {
                child_elements.push(render_family(
                    snap, &child_fam_handle, home_handle, _context_target, network, depth + 1
                ));
            } else {
                child_elements.push(network_card(
                    mk_info(child, snap, home_handle), child.gender,
                ));
            }
        }

        if children.len() > 1 {
            // Bracket: horizontal line spanning all children, then
            // each child has a vertical drop from the bracket.
            family_col = family_col.push(bracket_connector(child_elements));
        } else {
            // Single child - just a vertical line, no bracket needed.
            let mut single_col = column![].align_x(Alignment::Center);
            for el in child_elements {
                single_col = single_col.push(el);
            }
            family_col = family_col.push(single_col);
        }
    }

    family_col.into()
}

/// Render the bracket connector: each child gets a vertical stub
/// above it from a shared horizontal bracket line. Uses a styled
/// top-border container to avoid Length::Fill (which crashes in
/// Direction::Both scrollables).
fn bracket_connector(children: Vec<Element<'static, Message>>) -> Element<'static, Message> {
    let mut children_row = row![].spacing(20).align_y(Alignment::Start);
    for child in children {
        children_row = children_row.push(
            column![
                // Vertical drop from bracket to child.
                container(text(""))
                    .width(Length::Fixed(2.0))
                    .height(Length::Fixed(14.0))
                    .style(|_: &Theme| container::Style {
                        background: Some(iced::Background::Color(theme::CONNECTOR)),
                        ..Default::default()
                    }),
                child,
            ]
            .spacing(0)
            .align_x(Alignment::Center),
        );
    }

    // Use a container with a top-styled border to create the
    // horizontal bracket effect without Length::Fill.
    // Wrap in a container with a visible top border to create
    // the bracket effect.
    container(children_row)
        .style(|_: &Theme| container::Style {
            border: iced::Border {
                color: theme::CONNECTOR,
                width: 2.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
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

/// Gender colors for the left border accent (like Gramps Web).
const MALE_COLOR: iced::Color = iced::Color::from_rgb(0.4, 0.6, 0.85);
const FEMALE_COLOR: iced::Color = iced::Color::from_rgb(0.85, 0.45, 0.55);
const UNKNOWN_COLOR: iced::Color = iced::Color::from_rgb(0.65, 0.65, 0.65);

fn gender_color(gender: i32) -> iced::Color {
    match gender {
        0 => FEMALE_COLOR,
        1 => MALE_COLOR,
        _ => UNKNOWN_COLOR,
    }
}

fn network_card(
    person: PersonInfo,
    gender: i32,
) -> Element<'static, Message> {
    let is_home = person.is_home;
    let g_color = gender_color(gender);

    let mut card_col = column![
        text(person.name).size(13),
    ].spacing(2);
    if !person.birth.is_empty() {
        card_col = card_col.push(
            text(person.birth).size(10).color(
                if is_home { iced::Color::from_rgba(1.0, 1.0, 1.0, 0.7) }
                else { theme::TEXT_MUTED }
            ),
        );
    }
    card_col = card_col.push(
        text(person.gramps_id).size(9).color(
            if is_home { iced::Color::from_rgba(1.0, 1.0, 1.0, 0.5) }
            else { iced::Color::from_rgb(0.7, 0.7, 0.7) }
        ),
    );

    // Gender accent: a colored strip on the left + the card content.
    let gender_strip = container(text(""))
        .width(Length::Fixed(4.0))
        .height(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(iced::Background::Color(g_color)),
            ..Default::default()
        });

    let inner = row![gender_strip, container(card_col).padding([8, 12])]
        .spacing(0)
        .align_y(Alignment::Center);

    let handle = person.handle.clone();
    let right_handle = person.handle;
    let card = button(inner)
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

    // Right-click for context menu (menu itself renders as overlay in app.rs).
    mouse_area(card)
        .on_right_press(Message::TreeContextMenu(right_handle, 200.0, 100.0, 800.0, 600.0))
        .into()
}

