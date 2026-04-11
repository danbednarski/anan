//! Top-level iced application — state, messages, update, view, subscription.
//!
//! State model:
//!
//! - `snapshot` holds the currently-loaded `Snapshot` (all persons, families,
//!   events, places). `None` until a DB is opened.
//! - `order` is the filtered+sorted list of indices into `snapshot.persons`.
//! - `selected` is a position inside `order`, not inside `snapshot.persons`,
//!   so that clearing the filter doesn't strand the highlight.
//!
//! Message flow:
//!
//! ```text
//!   OpenDbDialog  ──►  rfd async picker  ──►  FilePicked(Some(path))
//!                                                      │
//!                                                      ▼
//!                                       spawn_blocking(load_snapshot)
//!                                                      │
//!                                                      ▼
//!                                              DbOpened(Result)
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Subscription, Task, Theme};

use crate::db::{self, Snapshot};
use crate::gramps::Person;
use crate::views::{person_detail, person_list};

/// Stable id for the search `text_input` so ⌘F can focus it.
pub const SEARCH_INPUT_ID: &str = "search-input";

#[derive(Debug)]
pub struct App {
    snapshot: Option<Snapshot>,
    /// Indices into `snapshot.persons`, filtered by `query` and sorted.
    order: Vec<usize>,
    /// Position inside `order` (not inside `snapshot.persons`).
    selected: Option<usize>,
    query: String,
    error: Option<String>,
    loading: bool,
    /// Denormalized lookup from person handle → person, rebuilt whenever
    /// the snapshot changes. Used by the detail view to resolve family
    /// father/mother references without scanning the Vec each time.
    persons_by_handle: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub enum Message {
    OpenDbDialog,
    FilePicked(Option<PathBuf>),
    DbOpened(Result<Snapshot, String>),
    SearchChanged(String),
    SelectPerson(usize),
    NavigateUp,
    NavigateDown,
    FocusSearch,
    Dismiss,
}

impl App {
    /// Build initial app state. If `test-fixtures/sample.db` exists next to
    /// the binary's manifest, auto-load it so developers get an instant
    /// populated view. Otherwise start empty.
    pub fn new() -> (Self, Task<Message>) {
        // Auto-load the test fixture when present. Path is resolved at
        // compile time so `cargo run` works from any cwd.
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/sample.db");
        let (loading, initial) = if fixture.exists() {
            (true, Task::perform(load_async(fixture), Message::DbOpened))
        } else {
            (false, Task::none())
        };

        (
            App {
                snapshot: None,
                order: Vec::new(),
                selected: None,
                query: String::new(),
                error: None,
                loading,
                persons_by_handle: HashMap::new(),
            },
            initial,
        )
    }

    pub fn title(&self) -> String {
        match &self.snapshot {
            Some(snap) => format!(
                "Gramps Desktop — {}",
                snap.path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| snap.path.display().to_string())
            ),
            None => "Gramps Desktop".to_string(),
        }
    }

    pub fn theme(&self) -> Theme {
        Theme::Light
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenDbDialog => {
                self.error = None;
                Task::perform(pick_file(), Message::FilePicked)
            }
            Message::FilePicked(None) => Task::none(),
            Message::FilePicked(Some(path)) => {
                self.loading = true;
                Task::perform(load_async(path), Message::DbOpened)
            }
            Message::DbOpened(result) => {
                self.loading = false;
                match result {
                    Ok(snap) => {
                        tracing::info!(
                            path = %snap.path.display(),
                            persons = snap.persons.len(),
                            families = snap.families.len(),
                            events = snap.events.len(),
                            "loaded snapshot"
                        );
                        self.persons_by_handle = snap
                            .persons
                            .iter()
                            .enumerate()
                            .map(|(i, p)| (p.handle.clone(), i))
                            .collect();
                        self.snapshot = Some(snap);
                        self.recompute_order();
                        self.selected = if self.order.is_empty() { None } else { Some(0) };
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "failed to load snapshot");
                        self.error = Some(err);
                    }
                }
                Task::none()
            }
            Message::SearchChanged(q) => {
                self.query = q;
                self.recompute_order();
                self.selected = if self.order.is_empty() { None } else { Some(0) };
                Task::none()
            }
            Message::SelectPerson(idx) => {
                if idx < self.order.len() {
                    self.selected = Some(idx);
                }
                Task::none()
            }
            Message::NavigateUp => {
                if let Some(i) = self.selected {
                    if i > 0 {
                        self.selected = Some(i - 1);
                    }
                } else if !self.order.is_empty() {
                    self.selected = Some(0);
                }
                Task::none()
            }
            Message::NavigateDown => {
                if let Some(i) = self.selected {
                    if i + 1 < self.order.len() {
                        self.selected = Some(i + 1);
                    }
                } else if !self.order.is_empty() {
                    self.selected = Some(0);
                }
                Task::none()
            }
            Message::FocusSearch => text_input::focus(text_input::Id::new(SEARCH_INPUT_ID)),
            Message::Dismiss => {
                self.error = None;
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let menu_bar = row![
            button(text("Open DB…")).on_press(Message::OpenDbDialog),
            text(if self.loading { "loading…" } else { "" }).size(12),
        ]
        .spacing(12)
        .padding(8)
        .align_y(Alignment::Center);

        let body: Element<'_, Message> = match &self.snapshot {
            Some(snap) => {
                let list = person_list::view(
                    &snap.persons,
                    &self.order,
                    self.selected,
                    &self.query,
                );
                let detail = match self.selected_person() {
                    Some(person) => person_detail::view(
                        person,
                        &snap.events,
                        &snap.families,
                        &snap.places,
                        &snap.persons,
                        &self.persons_by_handle,
                    ),
                    None => person_detail::placeholder(),
                };
                row![list, vertical_separator(), detail]
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
            None => container(text(if self.loading {
                "Loading…"
            } else {
                "No tree loaded. Use Open DB… to pick a Gramps SQLite file."
            }))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
        };

        let error_bar: Element<'_, Message> = match &self.error {
            Some(err) => container(
                row![
                    text(err.clone()).size(13),
                    button(text("×")).on_press(Message::Dismiss),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding(8)
            .width(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    background: Some(iced::Background::Color(palette.danger.weak.color)),
                    text_color: Some(palette.danger.weak.text),
                    ..Default::default()
                }
            })
            .into(),
            None => container(text("")).height(Length::Shrink).into(),
        };

        column![menu_bar, error_bar, body]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        keyboard::on_key_press(global_key)
    }

    fn selected_person(&self) -> Option<&Person> {
        let snap = self.snapshot.as_ref()?;
        let i = self.selected?;
        let idx = *self.order.get(i)?;
        snap.persons.get(idx)
    }

    /// Recompute `order` from `snapshot.persons` + current `query`.
    /// Sorts by primary surname then given name, case-insensitive.
    fn recompute_order(&mut self) {
        let Some(snap) = self.snapshot.as_ref() else {
            self.order.clear();
            return;
        };
        let q = self.query.trim().to_lowercase();

        self.order = snap
            .persons
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                if q.is_empty() {
                    return true;
                }
                let hay = format!(
                    "{} {} {}",
                    p.primary_name.first_name,
                    p.primary_name
                        .surname_list
                        .iter()
                        .map(|s| s.surname.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                    p.gramps_id,
                )
                .to_lowercase();
                hay.contains(&q)
            })
            .map(|(i, _)| i)
            .collect();

        self.order.sort_by(|&a, &b| {
            let pa = &snap.persons[a];
            let pb = &snap.persons[b];
            let sa = primary_surname(pa).to_lowercase();
            let sb = primary_surname(pb).to_lowercase();
            sa.cmp(&sb)
                .then_with(|| {
                    pa.primary_name
                        .first_name
                        .to_lowercase()
                        .cmp(&pb.primary_name.first_name.to_lowercase())
                })
        });
    }
}

fn primary_surname(p: &Person) -> &str {
    p.primary_name
        .surname_list
        .iter()
        .find(|s| s.primary)
        .or_else(|| p.primary_name.surname_list.first())
        .map(|s| s.surname.as_str())
        .unwrap_or("")
}

fn global_key(key: Key, modifiers: Modifiers) -> Option<Message> {
    match key.as_ref() {
        Key::Named(Named::ArrowUp) => Some(Message::NavigateUp),
        Key::Named(Named::ArrowDown) => Some(Message::NavigateDown),
        Key::Named(Named::Escape) => Some(Message::Dismiss),
        Key::Character("f") if modifiers.command() => Some(Message::FocusSearch),
        _ => None,
    }
}

fn vertical_separator<'a>() -> Element<'a, Message> {
    container(text(""))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(iced::Background::Color(palette.background.strong.color)),
                ..Default::default()
            }
        })
        .into()
}

/// Open a native file dialog and return the chosen path, if any.
async fn pick_file() -> Option<PathBuf> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Open Gramps family tree")
        .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
        .pick_file()
        .await?;
    Some(handle.path().to_path_buf())
}

/// Load a snapshot from disk on a blocking worker so the UI thread stays
/// responsive. Errors are flattened to `String` for iced message-safety.
async fn load_async(path: PathBuf) -> Result<Snapshot, String> {
    match tokio::task::spawn_blocking(move || db::load_snapshot(&path)).await {
        Ok(Ok(snap)) => Ok(snap),
        Ok(Err(err)) => Err(format!("{err:#}")),
        Err(join) => Err(format!("task panicked: {join}")),
    }
}
