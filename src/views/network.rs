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

use iced::widget::{button, column, container, scrollable, text};
use iced::{Element, Length, Theme};

use crate::app::Message;
use crate::db::Snapshot;
use crate::theme;
use crate::views::widgets::date_display;

/// Walk all family links from `home_handle` and return every reachable
/// person grouped by generation distance. Negative = ancestor
/// direction, positive = descendant direction, 0 = home's generation.
fn walk_network(snap: &Snapshot, home_handle: &str) -> Vec<(i32, Vec<PersonInfo>)> {
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

struct PersonInfo {
    handle: String,
    name: String,
    gramps_id: String,
    birth: String,
    is_home: bool,
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
