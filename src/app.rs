//! Top-level iced application — state, messages, update, view, subscription.
//!
//! State model:
//!
//! - `snapshot` holds the currently-loaded [`Snapshot`]. `None` until a
//!   DB is opened.
//! - `current_view` discriminates between primary-type panels. Each
//!   type owns its own [`ListState`] (filter query + sort order +
//!   selection) so switching tabs preserves each tab's context.
//! - Navigation and search messages always apply to the *currently
//!   visible* view — the update fn dispatches on `current_view`.
//!
//! Message flow for Open-DB:
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

use std::path::PathBuf;
use std::sync::Arc;

use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Subscription, Task, Theme};

use crate::db::{repo as dbrepo, Database, Snapshot};
use crate::views::list_pane::ListState;
use crate::views::search::{SearchHit, SearchState};
use crate::views::{
    citation, detail_ui, event, family, media, network, note, person, place, repository, search,
    source, tag, tree,
};

/// Stable id for the search `text_input` so ⌘F can focus it.
pub const SEARCH_INPUT_ID: &str = "search-input";

/// Which primary-type panel is currently shown in the middle + detail
/// columns. Order matches the sidebar navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Tree,
    Network,
    Persons,
    Families,
    Events,
    Places,
    Sources,
    Citations,
    Media,
    Notes,
    Repositories,
    Tags,
    Search,
}

impl View {
    const NAV_ITEMS: &'static [View] = &[
        View::Tree,
        View::Network,
    ];

    /// Secondary items accessible via the "Browse" section in the
    /// sidebar.
    const BROWSE_ITEMS: &'static [View] = &[
        View::Persons,
        View::Families,
        View::Events,
        View::Places,
        View::Sources,
        View::Citations,
        View::Media,
        View::Notes,
        View::Repositories,
        View::Tags,
    ];

    fn label(self) -> &'static str {
        match self {
            View::Tree => "Family Tree",
            View::Network => "Full Network",
            View::Persons => "Persons",
            View::Families => "Families",
            View::Events => "Events",
            View::Places => "Places",
            View::Sources => "Sources",
            View::Citations => "Citations",
            View::Media => "Media",
            View::Notes => "Notes",
            View::Repositories => "Repositories",
            View::Tags => "Tags",
            View::Search => "Search",
        }
    }
}

#[derive(Debug)]
pub struct App {
    /// Persistent read-write handle to the open tree. `None` until a DB
    /// is opened. Wrapped in [`Arc`] so it can be cloned into
    /// `tokio::spawn_blocking` closures for write operations.
    db: Option<Arc<Database>>,
    /// Most recent in-memory view of the tree. Refreshed after every
    /// successful write.
    snapshot: Option<Snapshot>,
    current: View,

    persons: ListState,
    families: ListState,
    events: ListState,
    places: ListState,
    sources: ListState,
    citations: ListState,
    media: ListState,
    notes: ListState,
    repositories: ListState,
    tags: ListState,
    search: SearchState,

    /// Handle of the person the tree view is centered on. Set to the
    /// first person in the snapshot on load; user re-homes by clicking
    /// any person card in the tree.
    home_person: Option<String>,

    /// Active in-place edit session, or `None` when not editing.
    edit: Option<EditSession>,
    /// Pending deletion awaiting confirmation (second click commits).
    delete_confirm: Option<PendingDelete>,

    /// Right-click context menu target — the person handle the user
    /// right-clicked in the tree. While Some, the context bar renders
    /// action buttons for that person.
    context_target: Option<String>,

    /// Modal form for adding a new person with a relationship. While
    /// Some, an overlay appears on top of the tree with name/gender/
    /// date fields. Submitting creates the person and wires the
    /// relationship.
    pending_add: Option<PendingAdd>,

    /// Whether the sidebar is expanded. Hidden by default; toggled by
    /// the hamburger button in the menu bar.
    sidebar_visible: bool,
    /// Whether the "Browse all" section in the sidebar is expanded.
    browse_expanded: bool,
    /// Display mode toggle: false = tree/map layout, true = flat list.
    /// Applies to whichever view is currently active (Family Tree or
    /// Full Network). The toggle button says "List" or "Tree"
    /// depending on current state.
    list_mode: bool,
    /// The current query in the sidebar search bar.
    search_bar_query: String,

    error: Option<String>,
    loading: bool,
    /// True while a write transaction is in flight.
    saving: bool,
}

/// What kind of relationship the "add person" modal will create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddRelationship {
    Child,
    Father,
    Mother,
    Sibling,
}

impl AddRelationship {
    fn label(self) -> &'static str {
        match self {
            AddRelationship::Child => "Add child",
            AddRelationship::Father => "Add father",
            AddRelationship::Mother => "Add mother",
            AddRelationship::Sibling => "Add sibling",
        }
    }

    fn default_gender(self) -> i32 {
        match self {
            AddRelationship::Father => 1,
            AddRelationship::Mother => 0,
            _ => 2, // unknown
        }
    }
}

/// State for the "add person" modal.
#[derive(Debug, Clone)]
pub struct PendingAdd {
    pub relationship: AddRelationship,
    /// The person in the tree the new person is relative to.
    pub target_handle: String,
    pub first_name: String,
    pub surname: String,
    pub gender_s: String,
    pub birth_year_s: String,
    pub death_year_s: String,
    /// Optional source URL/description. When non-empty, a Source +
    /// Citation are auto-created and attached to the new person.
    pub source_url: String,
}

/// One open edit — either a brand-new object that hasn't been saved
/// yet, or an in-place edit of an existing row.
#[derive(Debug, Clone)]
pub struct EditSession {
    /// `Some(handle)` when editing in place; `None` when creating.
    pub handle: Option<String>,
    pub draft: EditDraft,
}

#[derive(Debug, Clone)]
pub enum EditDraft {
    Tag(TagDraft),
    Note(NoteDraft),
    Repository(RepoDraft),
    Person(PersonDraft),
    Family(FamilyDraft),
    Event(EventDraft),
    Place(PlaceDraft),
    Source(SourceDraft),
    Citation(CitationDraft),
    Media(MediaDraft),
}

/// Family draft — father/mother referenced by Gramps ID (the visible
/// `I####` strings), resolved to handles on save. Rel type is
/// numeric (`FamilyRelType`: 0 married / 1 unmarried / 2 civil union /
/// 3 unknown / 4 custom).
#[derive(Debug, Clone, Default)]
pub struct FamilyDraft {
    pub father_gid: String,
    pub mother_gid: String,
    pub type_value_s: String,
}

/// Event draft — type value, description, place by Gramps ID, and
/// an inline Date draft.
#[derive(Debug, Clone, Default)]
pub struct EventDraft {
    pub type_value_s: String,
    pub description: String,
    pub place_gid: String,
    pub date: crate::views::widgets::date_edit::DateDraft,
}

/// Place draft — name, type value, lat/long, parent place by Gramps ID.
#[derive(Debug, Clone, Default)]
pub struct PlaceDraft {
    pub name: String,
    pub type_value_s: String,
    pub lat: String,
    pub long: String,
    pub parent_gid: String,
}

/// Source draft — title, author, pubinfo, abbrev.
#[derive(Debug, Clone, Default)]
pub struct SourceDraft {
    pub title: String,
    pub author: String,
    pub pubinfo: String,
    pub abbrev: String,
}

/// Citation draft — source by Gramps ID, page / URL, confidence
/// 0..4, and an inline date draft.
#[derive(Debug, Clone, Default)]
pub struct CitationDraft {
    pub source_gid: String,
    pub page: String,
    pub confidence_s: String,
    pub date: crate::views::widgets::date_edit::DateDraft,
}

/// Media draft — path, mime, description, optional date.
#[derive(Debug, Clone, Default)]
pub struct MediaDraft {
    pub path: String,
    pub mime: String,
    pub desc: String,
    pub date: crate::views::widgets::date_edit::DateDraft,
}

#[derive(Debug, Clone, Default)]
pub struct PersonDraft {
    pub first_name: String,
    pub surname: String,
    /// Gender as a string: 0 female, 1 male, 2 unknown. Parsed on save.
    pub gender_s: String,
    /// Empty string = "no birth year"; otherwise parsed as i32.
    pub birth_year_s: String,
    /// Empty string = "no death year"; otherwise parsed as i32.
    pub death_year_s: String,
}

#[derive(Debug, Clone, Default)]
pub struct TagDraft {
    pub name: String,
    pub color: String,
    pub priority_s: String,
}

#[derive(Debug, Clone, Default)]
pub struct NoteDraft {
    pub body: String,
    pub type_value_s: String,
}

#[derive(Debug, Clone, Default)]
pub struct RepoDraft {
    pub name: String,
    pub type_value_s: String,
}

/// A pending delete awaiting user confirmation. For simple types
/// (Tag/Note/Repository) this is just the handle + view; for Person
/// it also carries the cascade preview so the action bar can show
/// "in 3 families, 2 events" before the user clicks Confirm.
#[derive(Debug, Clone)]
pub struct PendingDelete {
    pub view: View,
    pub handle: String,
    pub cascade: Option<PersonCascade>,
}

/// Summary of what a Person delete will do. Mirrors
/// [`crate::db::repo::person::DeletePreview`] but lives in App state
/// so the UI can render it. The `delete_owned_events` toggle
/// corresponds to the `delete_owned_events` flag on the backend.
#[derive(Debug, Clone)]
pub struct PersonCascade {
    pub display_name: String,
    pub parent_of: Vec<String>,
    pub child_of: Vec<String>,
    pub event_count: usize,
    pub exclusive_event_count: usize,
    pub delete_owned_events: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// File menu: open a new Gramps DB.
    OpenDbDialog,
    /// rfd picker result.
    FilePicked(Option<PathBuf>),
    /// Blocking loader result. Boxed because `Snapshot` is large and we
    /// want small enum variants for the common messages.
    DbOpened(Box<Result<OpenedDb, String>>),
    /// Sidebar navigation click.
    ShowView(View),
    /// Search box input — applies to the current view.
    SearchChanged(String),
    /// Click on a row in the current view's list — index is a position
    /// inside the current view's filtered `order` (or `hits` for Search).
    SelectIndex(usize),
    /// Arrow key navigation on the current list.
    NavigateUp,
    NavigateDown,
    /// Cmd+F — focus the search box.
    FocusSearch,
    /// Escape — dismiss error banner.
    Dismiss,
    /// "Open in its own view" from a Search result row.
    OpenSearchHit(SearchHit),

    // ---- tree view -------------------------------------------------------
    /// Click a person card in the tree → re-home on that person.
    TreeHome(String),
    /// Right-click a person card → open context menu for that person.
    TreeContextMenu(String),
    /// Dismiss the context menu.
    TreeDismissContext,
    /// User picked an action from the context menu → open the add-person
    /// modal with the appropriate relationship pre-selected.
    TreeStartAdd(AddRelationship),
    /// Submit the add-person modal.
    TreeSubmitAdd,
    /// Cancel the add-person modal.
    TreeCancelAdd,
    /// Field edits inside the add-person modal.
    AddFirstName(String),
    AddSurname(String),
    AddGender(String),
    AddBirthYear(String),
    AddDeathYear(String),
    AddSourceUrl(String),
    /// Toggle sidebar visibility.
    ToggleSidebar,
    /// Toggle the "Browse all" section in the sidebar.
    ToggleBrowse,
    /// Toggle between tree and list display mode.
    ToggleListMode,
    /// Typing in the sidebar search bar.
    SearchBarInput(String),
    /// Hit Enter in the search bar - jump to best match.
    SearchBarSubmit,

    // ---- write / edit flow (Phase 4) -----------------------------------
    /// Start creating a new object in the given view (Tag, Note, Repository).
    StartCreate(View),
    /// Start editing the selected object in the given view.
    StartEditSelected,
    /// Drop the current edit session without saving.
    CancelEdit,
    /// Commit the current edit session to the database.
    SaveEdit,
    /// Result of a write txn + reload. `Ok(snapshot)` on success.
    WriteCompleted(Box<Result<Snapshot, String>>),
    /// First click on Delete — arm the inline confirm banner.
    StartDelete(String),
    /// Second click on Delete — actually delete.
    ConfirmDelete,
    /// Back out of a pending delete.
    CancelDelete,

    // ---- draft field edits --------------------------------------------
    EditTagName(String),
    EditTagColor(String),
    EditTagPriority(String),
    EditNoteBody(String),
    EditNoteType(String),
    EditRepoName(String),
    EditRepoType(String),
    EditPersonFirstName(String),
    EditPersonSurname(String),
    EditPersonGender(String),
    EditPersonBirthYear(String),
    EditPersonDeathYear(String),
    /// User toggled "also delete this person's exclusive events" on
    /// the cascade confirmation banner.
    ToggleDeleteOwnedEvents,

    EditFamilyFather(String),
    EditFamilyMother(String),
    EditFamilyType(String),

    EditEventType(String),
    EditEventDescription(String),
    EditEventPlace(String),
    EditEventDateYear(String),
    EditEventDateMonth(String),
    EditEventDateDay(String),
    EditEventDateModifier(String),
    EditEventDateQuality(String),
    EditEventDateText(String),

    EditPlaceName(String),
    EditPlaceType(String),
    EditPlaceLat(String),
    EditPlaceLong(String),
    EditPlaceParent(String),

    EditSourceTitle(String),
    EditSourceAuthor(String),
    EditSourcePubinfo(String),
    EditSourceAbbrev(String),

    EditCitationSource(String),
    EditCitationPage(String),
    EditCitationConfidence(String),
    EditCitationDateYear(String),
    EditCitationDateMonth(String),
    EditCitationDateDay(String),
    EditCitationDateModifier(String),
    EditCitationDateQuality(String),
    EditCitationDateText(String),

    EditMediaPath(String),
    EditMediaMime(String),
    EditMediaDesc(String),
    EditMediaDateYear(String),
    EditMediaDateMonth(String),
    EditMediaDateDay(String),
    EditMediaDateModifier(String),
    EditMediaDateQuality(String),
    EditMediaDateText(String),
}

/// Package returned by the async open-db task: both the persistent
/// [`Database`] handle and the initial [`Snapshot`] read through it.
#[derive(Debug)]
pub struct OpenedDb {
    pub db: Arc<Database>,
    pub snapshot: Snapshot,
}

// `Clone` is required on all `Message` variants because iced's runtime
// may clone pending messages. `Arc<Database>` is cheap to clone;
// `Snapshot` is already `Clone`.
impl Clone for OpenedDb {
    fn clone(&self) -> Self {
        OpenedDb {
            db: Arc::clone(&self.db),
            snapshot: self.snapshot.clone(),
        }
    }
}

impl App {
    /// Build initial app state. If `test-fixtures/sample.db` exists next
    /// to the crate manifest, auto-load it so developers get an instant
    /// populated view. Otherwise start empty.
    pub fn new() -> (Self, Task<Message>) {
        // Auto-load a scratch copy of the test fixture so the committed
        // file in `test-fixtures/` never gets mutated by edits the user
        // makes through the UI. A real tree opened via File > Open DB…
        // is modified in place.
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/sample.db");
        let (loading, initial) = if fixture.exists() {
            let scratch = std::env::temp_dir().join("gramps-desktop-scratch.db");
            // Always refresh the scratch copy on launch so developers get
            // a clean fixture each run. If the copy fails (e.g. /tmp is
            // non-writable) we fall through to opening the original,
            // which is a safer failure mode than silently hiding an
            // error — the user will see the bubble-up.
            match std::fs::copy(&fixture, &scratch) {
                Ok(_) => (
                    true,
                    Task::perform(load_async(scratch), Message::DbOpened),
                ),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "could not copy fixture to scratch; opening original read-write"
                    );
                    (true, Task::perform(load_async(fixture), Message::DbOpened))
                }
            }
        } else {
            (false, Task::none())
        };

        (
            App {
                db: None,
                snapshot: None,
                current: View::Tree,
                persons: ListState::default(),
                families: ListState::default(),
                events: ListState::default(),
                places: ListState::default(),
                sources: ListState::default(),
                citations: ListState::default(),
                media: ListState::default(),
                notes: ListState::default(),
                repositories: ListState::default(),
                tags: ListState::default(),
                search: SearchState::default(),
                home_person: None,
                edit: None,
                delete_confirm: None,
                context_target: None,
                pending_add: None,
                sidebar_visible: false,
                browse_expanded: false,
                list_mode: false,
                search_bar_query: String::new(),
                error: None,
                loading,
                saving: false,
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
        crate::theme::gramps_theme()
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
                match *result {
                    Ok(OpenedDb { db, snapshot }) => {
                        tracing::info!(
                            path = %snapshot.path.display(),
                            persons = snapshot.persons.len(),
                            families = snapshot.families.len(),
                            events = snapshot.events.len(),
                            places = snapshot.places.len(),
                            sources = snapshot.sources.len(),
                            citations = snapshot.citations.len(),
                            media = snapshot.media.len(),
                            notes = snapshot.notes.len(),
                            repositories = snapshot.repositories.len(),
                            tags = snapshot.tags.len(),
                            "loaded snapshot"
                        );
                        self.db = Some(db);
                        self.home_person = snapshot
                            .persons
                            .first()
                            .map(|p| p.handle.clone());
                        self.snapshot = Some(snapshot);
                        self.edit = None;
                        self.delete_confirm = None;
                        self.recompute_all();
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "failed to load snapshot");
                        self.error = Some(err);
                    }
                }
                Task::none()
            }
            Message::ShowView(view) => {
                self.current = view;
                Task::none()
            }
            Message::SearchChanged(q) => {
                self.set_current_query(q);
                self.recompute_current();
                Task::none()
            }
            Message::SelectIndex(idx) => {
                let state = self.current_list_state_mut();
                if idx < state.order.len() {
                    state.selected = Some(idx);
                }
                Task::none()
            }
            Message::NavigateUp => {
                self.current_list_state_mut().navigate_up();
                Task::none()
            }
            Message::NavigateDown => {
                self.current_list_state_mut().navigate_down();
                Task::none()
            }
            Message::FocusSearch => text_input::focus(text_input::Id::new(SEARCH_INPUT_ID)),
            Message::Dismiss => {
                self.error = None;
                Task::none()
            }
            Message::OpenSearchHit(hit) => {
                self.jump_to_hit(hit);
                Task::none()
            }

            // ---- tree actions -------------------------------------------
            Message::TreeHome(handle) => {
                self.context_target = None;
                self.home_person = Some(handle);
                Task::none()
            }
            Message::TreeContextMenu(handle) => {
                self.context_target = Some(handle);
                Task::none()
            }
            Message::TreeDismissContext => {
                self.context_target = None;
                Task::none()
            }
            Message::TreeStartAdd(rel) => {
                let target = self
                    .context_target
                    .clone()
                    .or_else(|| self.home_person.clone());
                let Some(target_handle) = target else {
                    return Task::none();
                };
                // Pre-fill surname from the target person.
                let surname = self
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.person(&target_handle))
                    .map(|p| {
                        p.primary_name
                            .surname_list
                            .iter()
                            .find(|s| s.primary)
                            .or_else(|| p.primary_name.surname_list.first())
                            .map(|s| s.surname.clone())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                self.context_target = None;
                self.pending_add = Some(PendingAdd {
                    relationship: rel,
                    target_handle,
                    first_name: String::new(),
                    surname,
                    gender_s: rel.default_gender().to_string(),
                    birth_year_s: String::new(),
                    death_year_s: String::new(),
                    source_url: String::new(),
                });
                Task::none()
            }
            Message::TreeCancelAdd => {
                self.pending_add = None;
                Task::none()
            }
            Message::TreeSubmitAdd => {
                let (Some(db), Some(add)) = (self.db.clone(), self.pending_add.take())
                else {
                    return Task::none();
                };
                self.saving = true;
                Task::perform(
                    tree_add_async(db, add),
                    |r| Message::WriteCompleted(Box::new(r)),
                )
            }
            Message::AddFirstName(v) => {
                if let Some(a) = self.pending_add.as_mut() {
                    a.first_name = v;
                }
                Task::none()
            }
            Message::AddSurname(v) => {
                if let Some(a) = self.pending_add.as_mut() {
                    a.surname = v;
                }
                Task::none()
            }
            Message::AddGender(v) => {
                if let Some(a) = self.pending_add.as_mut() {
                    a.gender_s = v;
                }
                Task::none()
            }
            Message::AddBirthYear(v) => {
                if let Some(a) = self.pending_add.as_mut() {
                    a.birth_year_s = v;
                }
                Task::none()
            }
            Message::AddDeathYear(v) => {
                if let Some(a) = self.pending_add.as_mut() {
                    a.death_year_s = v;
                }
                Task::none()
            }
            Message::AddSourceUrl(v) => {
                if let Some(a) = self.pending_add.as_mut() {
                    a.source_url = v;
                }
                Task::none()
            }
            Message::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                Task::none()
            }
            Message::ToggleBrowse => {
                self.browse_expanded = !self.browse_expanded;
                Task::none()
            }
            Message::ToggleListMode => {
                self.list_mode = !self.list_mode;
                Task::none()
            }
            Message::SearchBarInput(q) => {
                self.search_bar_query = q;
                Task::none()
            }
            Message::SearchBarSubmit => {
                // Find the first person whose name contains the query
                // and re-home on them.
                let q = self.search_bar_query.trim().to_lowercase();
                if !q.is_empty() {
                    if let Some(snap) = &self.snapshot {
                        if let Some(person) = snap.persons.iter().find(|p| {
                            p.primary_name.display().to_lowercase().contains(&q)
                        }) {
                            self.home_person = Some(person.handle.clone());
                            self.current = View::Tree;
                        }
                    }
                }
                self.search_bar_query.clear();
                Task::none()
            }

            // ---- edit flow -----------------------------------------------
            Message::StartCreate(view) => {
                self.delete_confirm = None;
                self.current = view;
                self.edit = Some(EditSession {
                    handle: None,
                    draft: default_draft_for(view),
                });
                Task::none()
            }
            Message::StartEditSelected => {
                self.delete_confirm = None;
                self.edit = self.populate_edit_from_selection();
                Task::none()
            }
            Message::CancelEdit => {
                self.edit = None;
                Task::none()
            }
            Message::SaveEdit => {
                let (Some(db), Some(session)) = (self.db.clone(), self.edit.take()) else {
                    return Task::none();
                };
                self.saving = true;
                Task::perform(save_async(db, session), |r| {
                    Message::WriteCompleted(Box::new(r))
                })
            }
            Message::WriteCompleted(result) => {
                self.saving = false;
                match *result {
                    Ok(snapshot) => {
                        self.snapshot = Some(snapshot);
                        self.recompute_all();
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "write failed");
                        self.error = Some(err);
                    }
                }
                Task::none()
            }
            Message::StartDelete(handle) => {
                let view = self.current;
                let cascade = if view == View::Persons {
                    self.db.as_ref().and_then(|db| {
                        match db.with_conn(|c| dbrepo::person::preview_delete(c, &handle)) {
                            Ok(preview) => Some(PersonCascade {
                                display_name: preview.display_name,
                                parent_of: preview.parent_of,
                                child_of: preview.child_of,
                                event_count: preview.event_count,
                                exclusive_event_count: preview.exclusive_event_count,
                                // Default: keep orphan events. User can
                                // opt into cascade-deleting them.
                                delete_owned_events: false,
                            }),
                            Err(e) => {
                                tracing::warn!(error = %e, "preview_delete failed");
                                None
                            }
                        }
                    })
                } else {
                    None
                };
                self.delete_confirm = Some(PendingDelete {
                    view,
                    handle,
                    cascade,
                });
                Task::none()
            }
            Message::CancelDelete => {
                self.delete_confirm = None;
                Task::none()
            }
            Message::ToggleDeleteOwnedEvents => {
                if let Some(PendingDelete {
                    cascade: Some(c), ..
                }) = self.delete_confirm.as_mut()
                {
                    c.delete_owned_events = !c.delete_owned_events;
                }
                Task::none()
            }
            Message::ConfirmDelete => {
                let (Some(db), Some(pending)) = (self.db.clone(), self.delete_confirm.take())
                else {
                    return Task::none();
                };
                self.saving = true;
                let delete_owned = pending
                    .cascade
                    .as_ref()
                    .map(|c| c.delete_owned_events)
                    .unwrap_or(false);
                Task::perform(
                    delete_async(db, pending.view, pending.handle, delete_owned),
                    |r| Message::WriteCompleted(Box::new(r)),
                )
            }

            // ---- draft field edits ---------------------------------------
            Message::EditTagName(v) => {
                if let Some(EditDraft::Tag(d)) = self.draft_mut() {
                    d.name = v;
                }
                Task::none()
            }
            Message::EditTagColor(v) => {
                if let Some(EditDraft::Tag(d)) = self.draft_mut() {
                    d.color = v;
                }
                Task::none()
            }
            Message::EditTagPriority(v) => {
                if let Some(EditDraft::Tag(d)) = self.draft_mut() {
                    d.priority_s = v;
                }
                Task::none()
            }
            Message::EditNoteBody(v) => {
                if let Some(EditDraft::Note(d)) = self.draft_mut() {
                    d.body = v;
                }
                Task::none()
            }
            Message::EditNoteType(v) => {
                if let Some(EditDraft::Note(d)) = self.draft_mut() {
                    d.type_value_s = v;
                }
                Task::none()
            }
            Message::EditRepoName(v) => {
                if let Some(EditDraft::Repository(d)) = self.draft_mut() {
                    d.name = v;
                }
                Task::none()
            }
            Message::EditRepoType(v) => {
                if let Some(EditDraft::Repository(d)) = self.draft_mut() {
                    d.type_value_s = v;
                }
                Task::none()
            }
            Message::EditPersonFirstName(v) => {
                if let Some(EditDraft::Person(d)) = self.draft_mut() {
                    d.first_name = v;
                }
                Task::none()
            }
            Message::EditPersonSurname(v) => {
                if let Some(EditDraft::Person(d)) = self.draft_mut() {
                    d.surname = v;
                }
                Task::none()
            }
            Message::EditPersonGender(v) => {
                if let Some(EditDraft::Person(d)) = self.draft_mut() {
                    d.gender_s = v;
                }
                Task::none()
            }
            Message::EditPersonBirthYear(v) => {
                if let Some(EditDraft::Person(d)) = self.draft_mut() {
                    d.birth_year_s = v;
                }
                Task::none()
            }
            Message::EditPersonDeathYear(v) => {
                if let Some(EditDraft::Person(d)) = self.draft_mut() {
                    d.death_year_s = v;
                }
                Task::none()
            }

            Message::EditFamilyFather(v) => {
                if let Some(EditDraft::Family(d)) = self.draft_mut() {
                    d.father_gid = v;
                }
                Task::none()
            }
            Message::EditFamilyMother(v) => {
                if let Some(EditDraft::Family(d)) = self.draft_mut() {
                    d.mother_gid = v;
                }
                Task::none()
            }
            Message::EditFamilyType(v) => {
                if let Some(EditDraft::Family(d)) = self.draft_mut() {
                    d.type_value_s = v;
                }
                Task::none()
            }

            Message::EditEventType(v) => {
                if let Some(EditDraft::Event(d)) = self.draft_mut() {
                    d.type_value_s = v;
                }
                Task::none()
            }
            Message::EditEventDescription(v) => {
                if let Some(EditDraft::Event(d)) = self.draft_mut() {
                    d.description = v;
                }
                Task::none()
            }
            Message::EditEventPlace(v) => {
                if let Some(EditDraft::Event(d)) = self.draft_mut() {
                    d.place_gid = v;
                }
                Task::none()
            }
            Message::EditEventDateYear(v) => {
                if let Some(EditDraft::Event(d)) = self.draft_mut() {
                    d.date.year_s = v;
                }
                Task::none()
            }
            Message::EditEventDateMonth(v) => {
                if let Some(EditDraft::Event(d)) = self.draft_mut() {
                    d.date.month_s = v;
                }
                Task::none()
            }
            Message::EditEventDateDay(v) => {
                if let Some(EditDraft::Event(d)) = self.draft_mut() {
                    d.date.day_s = v;
                }
                Task::none()
            }
            Message::EditEventDateModifier(v) => {
                if let Some(EditDraft::Event(d)) = self.draft_mut() {
                    d.date.modifier_s = v;
                }
                Task::none()
            }
            Message::EditEventDateQuality(v) => {
                if let Some(EditDraft::Event(d)) = self.draft_mut() {
                    d.date.quality_s = v;
                }
                Task::none()
            }
            Message::EditEventDateText(v) => {
                if let Some(EditDraft::Event(d)) = self.draft_mut() {
                    d.date.text_s = v;
                }
                Task::none()
            }

            Message::EditPlaceName(v) => {
                if let Some(EditDraft::Place(d)) = self.draft_mut() {
                    d.name = v;
                }
                Task::none()
            }
            Message::EditPlaceType(v) => {
                if let Some(EditDraft::Place(d)) = self.draft_mut() {
                    d.type_value_s = v;
                }
                Task::none()
            }
            Message::EditPlaceLat(v) => {
                if let Some(EditDraft::Place(d)) = self.draft_mut() {
                    d.lat = v;
                }
                Task::none()
            }
            Message::EditPlaceLong(v) => {
                if let Some(EditDraft::Place(d)) = self.draft_mut() {
                    d.long = v;
                }
                Task::none()
            }
            Message::EditPlaceParent(v) => {
                if let Some(EditDraft::Place(d)) = self.draft_mut() {
                    d.parent_gid = v;
                }
                Task::none()
            }

            Message::EditSourceTitle(v) => {
                if let Some(EditDraft::Source(d)) = self.draft_mut() {
                    d.title = v;
                }
                Task::none()
            }
            Message::EditSourceAuthor(v) => {
                if let Some(EditDraft::Source(d)) = self.draft_mut() {
                    d.author = v;
                }
                Task::none()
            }
            Message::EditSourcePubinfo(v) => {
                if let Some(EditDraft::Source(d)) = self.draft_mut() {
                    d.pubinfo = v;
                }
                Task::none()
            }
            Message::EditSourceAbbrev(v) => {
                if let Some(EditDraft::Source(d)) = self.draft_mut() {
                    d.abbrev = v;
                }
                Task::none()
            }

            Message::EditCitationSource(v) => {
                if let Some(EditDraft::Citation(d)) = self.draft_mut() {
                    d.source_gid = v;
                }
                Task::none()
            }
            Message::EditCitationPage(v) => {
                if let Some(EditDraft::Citation(d)) = self.draft_mut() {
                    d.page = v;
                }
                Task::none()
            }
            Message::EditCitationConfidence(v) => {
                if let Some(EditDraft::Citation(d)) = self.draft_mut() {
                    d.confidence_s = v;
                }
                Task::none()
            }
            Message::EditCitationDateYear(v) => {
                if let Some(EditDraft::Citation(d)) = self.draft_mut() {
                    d.date.year_s = v;
                }
                Task::none()
            }
            Message::EditCitationDateMonth(v) => {
                if let Some(EditDraft::Citation(d)) = self.draft_mut() {
                    d.date.month_s = v;
                }
                Task::none()
            }
            Message::EditCitationDateDay(v) => {
                if let Some(EditDraft::Citation(d)) = self.draft_mut() {
                    d.date.day_s = v;
                }
                Task::none()
            }
            Message::EditCitationDateModifier(v) => {
                if let Some(EditDraft::Citation(d)) = self.draft_mut() {
                    d.date.modifier_s = v;
                }
                Task::none()
            }
            Message::EditCitationDateQuality(v) => {
                if let Some(EditDraft::Citation(d)) = self.draft_mut() {
                    d.date.quality_s = v;
                }
                Task::none()
            }
            Message::EditCitationDateText(v) => {
                if let Some(EditDraft::Citation(d)) = self.draft_mut() {
                    d.date.text_s = v;
                }
                Task::none()
            }

            Message::EditMediaPath(v) => {
                if let Some(EditDraft::Media(d)) = self.draft_mut() {
                    d.path = v;
                }
                Task::none()
            }
            Message::EditMediaMime(v) => {
                if let Some(EditDraft::Media(d)) = self.draft_mut() {
                    d.mime = v;
                }
                Task::none()
            }
            Message::EditMediaDesc(v) => {
                if let Some(EditDraft::Media(d)) = self.draft_mut() {
                    d.desc = v;
                }
                Task::none()
            }
            Message::EditMediaDateYear(v) => {
                if let Some(EditDraft::Media(d)) = self.draft_mut() {
                    d.date.year_s = v;
                }
                Task::none()
            }
            Message::EditMediaDateMonth(v) => {
                if let Some(EditDraft::Media(d)) = self.draft_mut() {
                    d.date.month_s = v;
                }
                Task::none()
            }
            Message::EditMediaDateDay(v) => {
                if let Some(EditDraft::Media(d)) = self.draft_mut() {
                    d.date.day_s = v;
                }
                Task::none()
            }
            Message::EditMediaDateModifier(v) => {
                if let Some(EditDraft::Media(d)) = self.draft_mut() {
                    d.date.modifier_s = v;
                }
                Task::none()
            }
            Message::EditMediaDateQuality(v) => {
                if let Some(EditDraft::Media(d)) = self.draft_mut() {
                    d.date.quality_s = v;
                }
                Task::none()
            }
            Message::EditMediaDateText(v) => {
                if let Some(EditDraft::Media(d)) = self.draft_mut() {
                    d.date.text_s = v;
                }
                Task::none()
            }
        }
    }

    fn draft_mut(&mut self) -> Option<&mut EditDraft> {
        self.edit.as_mut().map(|s| &mut s.draft)
    }

    /// Prefill an edit session from the currently-selected row of the
    /// current view. Returns `None` if the current view isn't one of
    /// the editable types or nothing is selected.
    fn populate_edit_from_selection(&self) -> Option<EditSession> {
        let snap = self.snapshot.as_ref()?;
        match self.current {
            View::Persons => {
                let idx = self.persons.selected_item()?;
                let p = snap.persons.get(idx)?;
                let surname = p
                    .primary_name
                    .surname_list
                    .iter()
                    .find(|s| s.primary)
                    .or_else(|| p.primary_name.surname_list.first())
                    .map(|s| s.surname.clone())
                    .unwrap_or_default();
                let birth_year = if p.birth_ref_index >= 0 {
                    p.event_ref_list
                        .get(p.birth_ref_index as usize)
                        .and_then(|er| snap.event(&er.r#ref))
                        .and_then(|e| e.date.as_ref())
                        .map(|d| d.primary_year())
                        .filter(|y| *y != 0)
                } else {
                    None
                };
                let death_year = if p.death_ref_index >= 0 {
                    p.event_ref_list
                        .get(p.death_ref_index as usize)
                        .and_then(|er| snap.event(&er.r#ref))
                        .and_then(|e| e.date.as_ref())
                        .map(|d| d.primary_year())
                        .filter(|y| *y != 0)
                } else {
                    None
                };
                Some(EditSession {
                    handle: Some(p.handle.clone()),
                    draft: EditDraft::Person(PersonDraft {
                        first_name: p.primary_name.first_name.clone(),
                        surname,
                        gender_s: p.gender.to_string(),
                        birth_year_s: birth_year
                            .map(|y| y.to_string())
                            .unwrap_or_default(),
                        death_year_s: death_year
                            .map(|y| y.to_string())
                            .unwrap_or_default(),
                    }),
                })
            }
            View::Tags => {
                let idx = self.tags.selected_item()?;
                let tag = snap.tags.get(idx)?;
                Some(EditSession {
                    handle: Some(tag.handle.clone()),
                    draft: EditDraft::Tag(TagDraft {
                        name: tag.name.clone(),
                        color: tag.color.clone(),
                        priority_s: tag.priority.to_string(),
                    }),
                })
            }
            View::Families => {
                let idx = self.families.selected_item()?;
                let fam = snap.families.get(idx)?;
                let father_gid = fam
                    .father_handle
                    .as_ref()
                    .and_then(|h| snap.person(h))
                    .map(|p| p.gramps_id.clone())
                    .unwrap_or_default();
                let mother_gid = fam
                    .mother_handle
                    .as_ref()
                    .and_then(|h| snap.person(h))
                    .map(|p| p.gramps_id.clone())
                    .unwrap_or_default();
                Some(EditSession {
                    handle: Some(fam.handle.clone()),
                    draft: EditDraft::Family(FamilyDraft {
                        father_gid,
                        mother_gid,
                        type_value_s: fam.r#type.value.to_string(),
                    }),
                })
            }
            View::Events => {
                let idx = self.events.selected_item()?;
                let ev = snap.events.get(idx)?;
                let place_gid = snap
                    .place(&ev.place)
                    .map(|p| p.gramps_id.clone())
                    .unwrap_or_default();
                Some(EditSession {
                    handle: Some(ev.handle.clone()),
                    draft: EditDraft::Event(EventDraft {
                        type_value_s: ev.r#type.value.to_string(),
                        description: ev.description.clone(),
                        place_gid,
                        date: ev
                            .date
                            .as_ref()
                            .map(crate::views::widgets::date_edit::DateDraft::from_date)
                            .unwrap_or_default(),
                    }),
                })
            }
            View::Sources => {
                let idx = self.sources.selected_item()?;
                let s = snap.sources.get(idx)?;
                Some(EditSession {
                    handle: Some(s.handle.clone()),
                    draft: EditDraft::Source(SourceDraft {
                        title: s.title.clone(),
                        author: s.author.clone(),
                        pubinfo: s.pubinfo.clone(),
                        abbrev: s.abbrev.clone(),
                    }),
                })
            }
            View::Citations => {
                let idx = self.citations.selected_item()?;
                let c = snap.citations.get(idx)?;
                let source_gid = snap
                    .source(&c.source_handle)
                    .map(|s| s.gramps_id.clone())
                    .unwrap_or_default();
                Some(EditSession {
                    handle: Some(c.handle.clone()),
                    draft: EditDraft::Citation(CitationDraft {
                        source_gid,
                        page: c.page.clone(),
                        confidence_s: c.confidence.to_string(),
                        date: c
                            .date
                            .as_ref()
                            .map(crate::views::widgets::date_edit::DateDraft::from_date)
                            .unwrap_or_default(),
                    }),
                })
            }
            View::Media => {
                let idx = self.media.selected_item()?;
                let m = snap.media.get(idx)?;
                Some(EditSession {
                    handle: Some(m.handle.clone()),
                    draft: EditDraft::Media(MediaDraft {
                        path: m.path.clone(),
                        mime: m.mime.clone(),
                        desc: m.desc.clone(),
                        date: m
                            .date
                            .as_ref()
                            .map(crate::views::widgets::date_edit::DateDraft::from_date)
                            .unwrap_or_default(),
                    }),
                })
            }
            View::Places => {
                let idx = self.places.selected_item()?;
                let p = snap.places.get(idx)?;
                let parent_gid = p
                    .placeref_list
                    .first()
                    .and_then(|pr| snap.place(&pr.r#ref))
                    .map(|pp| pp.gramps_id.clone())
                    .unwrap_or_default();
                Some(EditSession {
                    handle: Some(p.handle.clone()),
                    draft: EditDraft::Place(PlaceDraft {
                        name: if p.name.value.is_empty() {
                            p.title.clone()
                        } else {
                            p.name.value.clone()
                        },
                        type_value_s: p.place_type.value.to_string(),
                        lat: p.lat.clone(),
                        long: p.long.clone(),
                        parent_gid,
                    }),
                })
            }
            View::Notes => {
                let idx = self.notes.selected_item()?;
                let note = snap.notes.get(idx)?;
                Some(EditSession {
                    handle: Some(note.handle.clone()),
                    draft: EditDraft::Note(NoteDraft {
                        body: note.text.string.clone(),
                        type_value_s: note.r#type.value.to_string(),
                    }),
                })
            }
            View::Repositories => {
                let idx = self.repositories.selected_item()?;
                let repo = snap.repositories.get(idx)?;
                Some(EditSession {
                    handle: Some(repo.handle.clone()),
                    draft: EditDraft::Repository(RepoDraft {
                        name: repo.name.clone(),
                        type_value_s: repo.r#type.value.to_string(),
                    }),
                })
            }
            _ => None,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let toolbar_btn = |label: &'static str, msg: Message| {
            button(text(label).size(12))
                .on_press(msg)
                .style(|_: &Theme, status| {
                    let bg = match status {
                        button::Status::Hovered | button::Status::Pressed => crate::theme::ANCESTOR_HOVER,
                        _ => iced::Color::TRANSPARENT,
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: crate::theme::TEXT,
                        border: iced::Border {
                            color: iced::Color::TRANSPARENT,
                            width: 0.0,
                            radius: 4.0.into(),
                        },
                        shadow: iced::Shadow::default(),
                    }
                })
        };

        let menu_label = if self.sidebar_visible { "Close" } else { "Menu" };
        let view_label = if self.list_mode { "Tree" } else { "List" };
        let view_msg = Message::ToggleListMode;

        // Left padding to clear macOS traffic lights.
        let mut menu_bar = row![
            iced::widget::Space::with_width(Length::Fixed(68.0)),
            toolbar_btn(menu_label, Message::ToggleSidebar),
            toolbar_btn(view_label, view_msg),
        ]
        .spacing(6)
        .padding([8, 12])
        .align_y(Alignment::Center);

        if self.loading {
            menu_bar = menu_bar.push(text("loading...").size(11).color(crate::theme::TEXT_MUTED));
        }
        if self.saving {
            menu_bar = menu_bar.push(text("saving...").size(11).color(crate::theme::TEXT_MUTED));
        }

        let body: Element<'_, Message> = match &self.snapshot {
            Some(snap) if matches!(self.current, View::Tree | View::Network) => {
                let content_body = match &self.home_person {
                    Some(h) => match (self.current, self.list_mode) {
                        // Family Tree: default = pedigree tree, toggle = flat list of tree scope
                        (View::Tree, false) => tree::view(snap, h, self.context_target.as_deref()),
                        (View::Tree, true) => tree::list_view(snap, h),
                        // Full Network: default = flat list, toggle = extended tree with aunts/uncles
                        (View::Network, false) => network::view(snap, h),
                        (View::Network, true) => network::tree_view(snap, h, self.context_target.as_deref()),
                        _ => detail_ui::empty(""),
                    }
                    None => detail_ui::empty("No person in tree. Open a DB with people."),
                };
                let mut content_col = column![content_body]
                    .width(Length::Fill)
                    .height(Length::Fill);

                if let Some(add) = &self.pending_add {
                    content_col = content_col.push(self.add_person_modal(add));
                }

                let mut main_row = row![].width(Length::Fill).height(Length::Fill);
                if self.sidebar_visible {
                    main_row = main_row.push(self.nav_column());
                    main_row = main_row.push(vertical_separator());
                }
                main_row = main_row.push(content_col);
                main_row.into()
            }
            Some(snap) => {
                let mut main_row = row![].width(Length::Fill).height(Length::Fill);
                if self.sidebar_visible {
                    main_row = main_row.push(self.nav_column());
                    main_row = main_row.push(vertical_separator());
                }
                main_row = main_row.push(self.list_pane(snap));
                main_row = main_row.push(vertical_separator());
                main_row = main_row.push(self.detail_pane(snap));
                main_row.into()
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

    // --- view helpers ----------------------------------------------------

    fn nav_column(&self) -> Element<'_, Message> {
        let mut col = column![].spacing(4).padding(12);

        // Search bar - always visible at top of sidebar.
        col = col.push(
            text_input("Search people...", &self.search_bar_query)
                .on_input(Message::SearchBarInput)
                .on_submit(Message::SearchBarSubmit)
                .padding(8)
                .size(13),
        );
        col = col.push(text("").size(4));

        // Open DB button.
        col = col.push(
            button(text("Open DB...").size(12))
                .width(Length::Fill)
                .on_press(Message::OpenDbDialog)
                .style(|_: &Theme, status| {
                    let bg = match status {
                        button::Status::Hovered | button::Status::Pressed => crate::theme::ANCESTOR_HOVER,
                        _ => crate::theme::ANCESTOR_BG,
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: crate::theme::TEXT,
                        border: iced::Border { color: crate::theme::BORDER, width: 1.0, radius: 6.0.into() },
                        shadow: iced::Shadow::default(),
                    }
                }),
        );
        col = col.push(text("").size(6));

        // Primary nav.
        for item in View::NAV_ITEMS {
            let label = item.label().to_string();
            let is_current = *item == self.current;
            let btn = button(text(label).size(13))
                .width(Length::Fill)
                .on_press(Message::ShowView(*item))
                .style(move |theme: &Theme, status| nav_style(theme, status, is_current));
            col = col.push(btn);
        }

        // Browse: collapsed under "..." toggle.
        col = col.push(text("").size(6));
        col = col.push(
            button(text("... Browse all").size(11).color(crate::theme::TEXT_MUTED))
                .width(Length::Fill)
                .on_press(Message::ToggleBrowse)
                .style(|_: &Theme, status| {
                    let bg = match status {
                        button::Status::Hovered | button::Status::Pressed => crate::theme::ANCESTOR_HOVER,
                        _ => iced::Color::TRANSPARENT,
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: crate::theme::TEXT_MUTED,
                        border: iced::Border { color: iced::Color::TRANSPARENT, width: 0.0, radius: 4.0.into() },
                        shadow: iced::Shadow::default(),
                    }
                }),
        );
        if self.browse_expanded {
            for item in View::BROWSE_ITEMS {
                let count = self.count_for(*item);
                let label = format!("{}  ({count})", item.label());
                let is_current = *item == self.current;
                let btn = button(text(label).size(11))
                    .width(Length::Fill)
                    .on_press(Message::ShowView(*item))
                    .style(move |theme: &Theme, status| nav_style(theme, status, is_current));
                col = col.push(btn);
            }
        }

        container(scrollable(col))
            .width(Length::Fixed(150.0))
            .height(Length::Fill)
            .style(|_: &Theme| container::Style {
                background: Some(iced::Background::Color(crate::theme::CARD)),
                border: iced::Border {
                    color: crate::theme::BORDER,
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn list_pane<'a>(&'a self, snap: &'a Snapshot) -> Element<'a, Message> {
        match self.current {
            View::Tree | View::Network => detail_ui::empty(""),
            View::Persons => person::list_view(snap, &self.persons),
            View::Families => family::list_view(snap, &self.families),
            View::Events => event::list_view(snap, &self.events),
            View::Places => place::list_view(snap, &self.places),
            View::Sources => source::list_view(snap, &self.sources),
            View::Citations => citation::list_view(snap, &self.citations),
            View::Media => media::list_view(snap, &self.media),
            View::Notes => note::list_view(snap, &self.notes),
            View::Repositories => repository::list_view(snap, &self.repositories),
            View::Tags => tag::list_view(snap, &self.tags),
            View::Search => search::list_view(snap, &self.search),
        }
    }

    fn detail_pane<'a>(&'a self, snap: &'a Snapshot) -> Element<'a, Message> {
        let action_bar = self.action_bar();
        let content = self.detail_content(snap);
        column![action_bar, content]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn detail_content<'a>(&'a self, snap: &'a Snapshot) -> Element<'a, Message> {
        // If an edit session is active for the current view, render the
        // edit form instead of the read-only detail.
        if let Some(session) = &self.edit {
            let creating = session.handle.is_none();
            match (&session.draft, self.current) {
                (EditDraft::Person(d), View::Persons) => return person::edit_view(d, creating),
                (EditDraft::Family(d), View::Families) => {
                    return family::edit_view(d, creating)
                }
                (EditDraft::Event(d), View::Events) => return event::edit_view(d, creating),
                (EditDraft::Place(d), View::Places) => return place::edit_view(d, creating),
                (EditDraft::Source(d), View::Sources) => return source::edit_view(d, creating),
                (EditDraft::Citation(d), View::Citations) => {
                    return citation::edit_view(d, creating)
                }
                (EditDraft::Media(d), View::Media) => return media::edit_view(d, creating),
                (EditDraft::Tag(d), View::Tags) => return tag::edit_view(d, creating),
                (EditDraft::Note(d), View::Notes) => return note::edit_view(d, creating),
                (EditDraft::Repository(d), View::Repositories) => {
                    return repository::edit_view(d, creating)
                }
                _ => {}
            }
        }

        let placeholder = || detail_ui::empty("Select a row to see details");
        match self.current {
            View::Tree | View::Network => detail_ui::empty(""),
            View::Persons => match self.persons.selected_item() {
                Some(i) => snap
                    .persons
                    .get(i)
                    .map(|p| person::detail_view(snap, p))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Families => match self.families.selected_item() {
                Some(i) => snap
                    .families
                    .get(i)
                    .map(|f| family::detail_view(snap, f))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Events => match self.events.selected_item() {
                Some(i) => snap
                    .events
                    .get(i)
                    .map(|e| event::detail_view(snap, e))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Places => match self.places.selected_item() {
                Some(i) => snap
                    .places
                    .get(i)
                    .map(|p| place::detail_view(snap, p))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Sources => match self.sources.selected_item() {
                Some(i) => snap
                    .sources
                    .get(i)
                    .map(|s| source::detail_view(snap, s))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Citations => match self.citations.selected_item() {
                Some(i) => snap
                    .citations
                    .get(i)
                    .map(|c| citation::detail_view(snap, c))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Media => match self.media.selected_item() {
                Some(i) => snap
                    .media
                    .get(i)
                    .map(|m| media::detail_view(snap, m))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Notes => match self.notes.selected_item() {
                Some(i) => snap
                    .notes
                    .get(i)
                    .map(|n| note::detail_view(snap, n))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Repositories => match self.repositories.selected_item() {
                Some(i) => snap
                    .repositories
                    .get(i)
                    .map(|r| repository::detail_view(snap, r))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Tags => match self.tags.selected_item() {
                Some(i) => snap
                    .tags
                    .get(i)
                    .map(|t| tag::detail_view(snap, t))
                    .unwrap_or_else(placeholder),
                None => placeholder(),
            },
            View::Search => match self
                .search
                .list
                .selected
                .and_then(|i| self.search.hits.get(i).copied())
            {
                Some(hit) => search::detail_view(snap, hit),
                None => placeholder(),
            },
        }
    }

    /// Compose an action bar above the detail pane. Buttons are gated
    /// on whether the current view is editable, whether something is
    /// selected, and whether an edit session / delete confirmation is
    /// active.
    fn action_bar<'a>(&'a self) -> Element<'a, Message> {
        let editable = !matches!(self.current, View::Search);
        let has_selection = self.current_selected_handle().is_some();

        let mut bar = row![].spacing(8).padding(8).align_y(Alignment::Center);

        if !editable {
            bar = bar.push(
                text("(read-only in Phase 5)")
                    .size(11)
                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
            );
            return container(bar).width(Length::Fill).into();
        }

        if self.edit.is_some() {
            // Save / Cancel while editing
            let save_btn: iced::widget::Button<'_, Message> = button(text("Save"));
            let save_btn = if self.saving {
                save_btn
            } else {
                save_btn.on_press(Message::SaveEdit)
            };
            bar = bar.push(save_btn);
            bar = bar.push(button(text("Cancel")).on_press(Message::CancelEdit));
            if self.saving {
                bar = bar.push(text("saving…").size(12));
            }
        } else if let Some(pending) = self.delete_confirm.as_ref() {
            return self.delete_confirm_bar(pending);
        } else {
            bar = bar.push(
                button(text("+ New")).on_press(Message::StartCreate(self.current)),
            );
            if has_selection {
                bar = bar.push(button(text("Edit")).on_press(Message::StartEditSelected));
                if let Some(h) = self.current_selected_handle() {
                    bar = bar.push(button(text("Delete")).on_press(Message::StartDelete(h)));
                }
            }
        }

        container(bar)
            .width(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    background: Some(iced::Background::Color(palette.background.weak.color)),
                    ..Default::default()
                }
            })
            .into()
    }

    fn delete_confirm_bar<'a>(
        &'a self,
        pending: &'a PendingDelete,
    ) -> Element<'a, Message> {
        let mut col = column![].spacing(6).padding(8);

        let headline = match &pending.cascade {
            Some(c) => format!(
                "Delete {} ({})? This will rewrite {} parent-of famil{}, {} child-of famil{}, and touch {} event{}.",
                c.display_name,
                short(&pending.handle),
                c.parent_of.len(),
                if c.parent_of.len() == 1 { "y" } else { "ies" },
                c.child_of.len(),
                if c.child_of.len() == 1 { "y" } else { "ies" },
                c.event_count,
                if c.event_count == 1 { "" } else { "s" },
            ),
            None => format!("Delete {}?", short(&pending.handle)),
        };
        col = col.push(
            text(headline)
                .size(13)
                .color(iced::Color::from_rgb(0.7, 0.1, 0.1)),
        );

        if let Some(c) = &pending.cascade {
            if !c.parent_of.is_empty() {
                col = col.push(
                    text(format!("  · parent of: {}", c.parent_of.join(", "))).size(12),
                );
            }
            if !c.child_of.is_empty() {
                col = col.push(
                    text(format!("  · child of: {}", c.child_of.join(", "))).size(12),
                );
            }
            if c.event_count > 0 {
                col = col.push(
                    text(format!(
                        "  · {} of {} events are exclusive to this person",
                        c.exclusive_event_count, c.event_count
                    ))
                    .size(12),
                );
                let toggle_label = if c.delete_owned_events {
                    "[x] also delete exclusive events"
                } else {
                    "[ ] also delete exclusive events"
                };
                col = col.push(
                    button(text(toggle_label).size(12))
                        .on_press(Message::ToggleDeleteOwnedEvents),
                );
            }
        }

        let controls = row![
            button(text("Confirm delete")).on_press(Message::ConfirmDelete),
            button(text("Keep")).on_press(Message::CancelDelete),
        ]
        .spacing(8);
        col = col.push(controls);

        container(col)
            .width(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    background: Some(iced::Background::Color(palette.danger.weak.color)),
                    text_color: Some(palette.danger.weak.text),
                    ..Default::default()
                }
            })
            .into()
    }

    /// Handle of the currently-selected object in the current view, if
    /// the view is one of Tag / Note / Repository and something is
    /// selected. Other views return `None`.
    fn current_selected_handle(&self) -> Option<String> {
        let snap = self.snapshot.as_ref()?;
        match self.current {
            View::Persons => {
                let i = self.persons.selected_item()?;
                Some(snap.persons.get(i)?.handle.clone())
            }
            View::Families => {
                let i = self.families.selected_item()?;
                Some(snap.families.get(i)?.handle.clone())
            }
            View::Events => {
                let i = self.events.selected_item()?;
                Some(snap.events.get(i)?.handle.clone())
            }
            View::Places => {
                let i = self.places.selected_item()?;
                Some(snap.places.get(i)?.handle.clone())
            }
            View::Sources => {
                let i = self.sources.selected_item()?;
                Some(snap.sources.get(i)?.handle.clone())
            }
            View::Citations => {
                let i = self.citations.selected_item()?;
                Some(snap.citations.get(i)?.handle.clone())
            }
            View::Media => {
                let i = self.media.selected_item()?;
                Some(snap.media.get(i)?.handle.clone())
            }
            View::Tags => {
                let i = self.tags.selected_item()?;
                Some(snap.tags.get(i)?.handle.clone())
            }
            View::Notes => {
                let i = self.notes.selected_item()?;
                Some(snap.notes.get(i)?.handle.clone())
            }
            View::Repositories => {
                let i = self.repositories.selected_item()?;
                Some(snap.repositories.get(i)?.handle.clone())
            }
            _ => None,
        }
    }

    // --- state helpers ---------------------------------------------------

    fn current_list_state_mut(&mut self) -> &mut ListState {
        match self.current {
            // Tree/Network don't use a ListState; fall through to persons
            // as a harmless default (their messages don't go through this).
            View::Tree | View::Network => &mut self.persons,
            View::Persons => &mut self.persons,
            View::Families => &mut self.families,
            View::Events => &mut self.events,
            View::Places => &mut self.places,
            View::Sources => &mut self.sources,
            View::Citations => &mut self.citations,
            View::Media => &mut self.media,
            View::Notes => &mut self.notes,
            View::Repositories => &mut self.repositories,
            View::Tags => &mut self.tags,
            View::Search => &mut self.search.list,
        }
    }

    fn set_current_query(&mut self, q: String) {
        self.current_list_state_mut().query = q;
    }

    fn recompute_current(&mut self) {
        let Some(snap) = self.snapshot.as_ref() else {
            return;
        };
        match self.current {
            View::Tree | View::Network => {}
            View::Persons => person::recompute(snap, &mut self.persons),
            View::Families => family::recompute(snap, &mut self.families),
            View::Events => event::recompute(snap, &mut self.events),
            View::Places => place::recompute(snap, &mut self.places),
            View::Sources => source::recompute(snap, &mut self.sources),
            View::Citations => citation::recompute(snap, &mut self.citations),
            View::Media => media::recompute(snap, &mut self.media),
            View::Notes => note::recompute(snap, &mut self.notes),
            View::Repositories => repository::recompute(snap, &mut self.repositories),
            View::Tags => tag::recompute(snap, &mut self.tags),
            View::Search => search::recompute(snap, &mut self.search),
        }
    }

    fn recompute_all(&mut self) {
        let Some(snap) = self.snapshot.as_ref() else {
            return;
        };
        person::recompute(snap, &mut self.persons);
        family::recompute(snap, &mut self.families);
        event::recompute(snap, &mut self.events);
        place::recompute(snap, &mut self.places);
        source::recompute(snap, &mut self.sources);
        citation::recompute(snap, &mut self.citations);
        media::recompute(snap, &mut self.media);
        note::recompute(snap, &mut self.notes);
        repository::recompute(snap, &mut self.repositories);
        tag::recompute(snap, &mut self.tags);
        search::recompute(snap, &mut self.search);
    }

    /// Modal overlay for the add-person form. Renders on top of the
    /// tree area as a centered card with fields and Save/Cancel.
    fn add_person_modal<'a>(&'a self, add: &'a PendingAdd) -> Element<'a, Message> {
        let target_name = self
            .snapshot
            .as_ref()
            .and_then(|s| s.person(&add.target_handle))
            .map(|p| p.primary_name.display())
            .unwrap_or_else(|| "?".to_string());

        let title = text(format!(
            "{} of {}",
            add.relationship.label(),
            target_name
        ))
        .size(20);

        let label_color = iced::Color::from_rgb(0.5, 0.5, 0.5);
        let label = |s: &'static str| text(s).size(11).color(label_color);

        let first_name_field = column![
            label("First name"),
            text_input("First name", &add.first_name)
                .on_input(Message::AddFirstName)
                .padding(6),
        ]
        .spacing(4);

        let surname_field = column![
            label("Surname"),
            text_input("Surname", &add.surname)
                .on_input(Message::AddSurname)
                .padding(6),
        ]
        .spacing(4);

        let gender_options = vec![
            "Female".to_string(),
            "Male".to_string(),
            "Unknown".to_string(),
        ];
        let selected_gender = match add.gender_s.as_str() {
            "0" => Some("Female".to_string()),
            "1" => Some("Male".to_string()),
            _ => Some("Unknown".to_string()),
        };
        let gender_field = column![
            label("Gender"),
            pick_list(gender_options, selected_gender, |picked: String| {
                let val = match picked.as_str() {
                    "Female" => "0",
                    "Male" => "1",
                    _ => "2",
                };
                Message::AddGender(val.to_string())
            })
            .width(Length::Fixed(120.0)),
        ]
        .spacing(4);

        let birth_field = column![
            label("Birth year (blank for unknown)"),
            text_input("e.g. 1890", &add.birth_year_s)
                .on_input(Message::AddBirthYear)
                .padding(6)
                .width(Length::Fixed(120.0)),
        ]
        .spacing(4);

        let death_field = column![
            label("Death year (blank for unknown)"),
            text_input("e.g. 1965", &add.death_year_s)
                .on_input(Message::AddDeathYear)
                .padding(6)
                .width(Length::Fixed(120.0)),
        ]
        .spacing(4);

        let source_field = column![
            label("Source URL / reference (optional)"),
            text_input("e.g. https://findagrave.com/...", &add.source_url)
                .on_input(Message::AddSourceUrl)
                .padding(6),
        ]
        .spacing(4);

        let buttons = row![
            button(text("Save")).on_press(Message::TreeSubmitAdd),
            button(text("Cancel")).on_press(Message::TreeCancelAdd),
        ]
        .spacing(12);

        let card = container(
            column![
                title,
                first_name_field,
                surname_field,
                gender_field,
                row![birth_field, death_field].spacing(16),
                source_field,
                buttons,
            ]
            .spacing(12)
            .padding(24)
            .max_width(420),
        )
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(iced::Background::Color(palette.background.base.color)),
                border: iced::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                shadow: iced::Shadow {
                    color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 12.0,
                },
                ..Default::default()
            }
        });

        container(card)
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding([40, 0])
            .into()
    }

    fn count_for(&self, view: View) -> usize {
        let Some(snap) = self.snapshot.as_ref() else {
            return 0;
        };
        match view {
            View::Tree | View::Network => snap.persons.len(),
            View::Persons => snap.persons.len(),
            View::Families => snap.families.len(),
            View::Events => snap.events.len(),
            View::Places => snap.places.len(),
            View::Sources => snap.sources.len(),
            View::Citations => snap.citations.len(),
            View::Media => snap.media.len(),
            View::Notes => snap.notes.len(),
            View::Repositories => snap.repositories.len(),
            View::Tags => snap.tags.len(),
            View::Search => self.search.hits.len(),
        }
    }

    /// Jump to the view that owns a search hit and select it.
    fn jump_to_hit(&mut self, hit: SearchHit) {
        use crate::views::search::HitKind;
        let (view, state): (View, &mut ListState) = match hit.kind {
            HitKind::Person => (View::Persons, &mut self.persons),
            HitKind::Family => (View::Families, &mut self.families),
            HitKind::Event => (View::Events, &mut self.events),
            HitKind::Place => (View::Places, &mut self.places),
            HitKind::Source => (View::Sources, &mut self.sources),
            HitKind::Citation => (View::Citations, &mut self.citations),
            HitKind::Media => (View::Media, &mut self.media),
            HitKind::Note => (View::Notes, &mut self.notes),
            HitKind::Repository => (View::Repositories, &mut self.repositories),
            HitKind::Tag => (View::Tags, &mut self.tags),
        };
        self.current = view;
        // Position *inside order* of the raw-item index we want to highlight.
        if let Some(pos) = state.order.iter().position(|&i| i == hit.index) {
            state.selected = Some(pos);
        }
    }

}

/// First 8 chars of a handle — enough to disambiguate in tracing and
/// inline confirmation banners without eating the whole row.
fn short(handle: &str) -> String {
    handle.chars().take(8).collect()
}

/// Resolve a `gramps_id` like "I0003" to a Person handle. Empty or
/// blank input returns `Ok(None)` — meaning "this field is unset".
/// A non-blank input that doesn't match any row is an error so the
/// user sees a "no such person" message instead of silent data loss.
fn resolve_person_by_gid(
    conn: &rusqlite::Connection,
    gid: &str,
) -> anyhow::Result<Option<String>> {
    let gid = gid.trim();
    if gid.is_empty() {
        return Ok(None);
    }
    let row: Result<String, rusqlite::Error> = conn.query_row(
        "SELECT handle FROM person WHERE gramps_id = ?1",
        rusqlite::params![gid],
        |r| r.get(0),
    );
    match row {
        Ok(h) => Ok(Some(h)),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            Err(anyhow::anyhow!("no person with gramps_id {gid}"))
        }
        Err(e) => Err(anyhow::anyhow!("lookup person {gid}: {e}")),
    }
}

/// Same as [`resolve_person_by_gid`] for Source.
fn resolve_source_by_gid(
    conn: &rusqlite::Connection,
    gid: &str,
) -> anyhow::Result<Option<String>> {
    let gid = gid.trim();
    if gid.is_empty() {
        return Ok(None);
    }
    let row: Result<String, rusqlite::Error> = conn.query_row(
        "SELECT handle FROM source WHERE gramps_id = ?1",
        rusqlite::params![gid],
        |r| r.get(0),
    );
    match row {
        Ok(h) => Ok(Some(h)),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            Err(anyhow::anyhow!("no source with gramps_id {gid}"))
        }
        Err(e) => Err(anyhow::anyhow!("lookup source {gid}: {e}")),
    }
}

/// Same as [`resolve_person_by_gid`] for Place.
fn resolve_place_by_gid(
    conn: &rusqlite::Connection,
    gid: &str,
) -> anyhow::Result<Option<String>> {
    let gid = gid.trim();
    if gid.is_empty() {
        return Ok(None);
    }
    let row: Result<String, rusqlite::Error> = conn.query_row(
        "SELECT handle FROM place WHERE gramps_id = ?1",
        rusqlite::params![gid],
        |r| r.get(0),
    );
    match row {
        Ok(h) => Ok(Some(h)),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            Err(anyhow::anyhow!("no place with gramps_id {gid}"))
        }
        Err(e) => Err(anyhow::anyhow!("lookup place {gid}: {e}")),
    }
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

fn nav_style(theme: &Theme, status: button::Status, is_current: bool) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: None,
        text_color: palette.background.base.text,
        border: iced::Border {
            color: iced::Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        shadow: iced::Shadow::default(),
    };
    if is_current {
        return button::Style {
            background: Some(iced::Background::Color(palette.primary.base.color)),
            text_color: palette.primary.base.text,
            ..base
        };
    }
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(iced::Background::Color(palette.background.weak.color)),
            ..base
        },
        _ => base,
    }
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

/// Open a [`Database`] and take an initial [`Snapshot`] through it,
/// both on a blocking worker. Errors are flattened to `String` for
/// iced message-safety.
async fn load_async(path: PathBuf) -> Box<Result<OpenedDb, String>> {
    let joined = tokio::task::spawn_blocking(move || -> anyhow::Result<OpenedDb> {
        let db = Arc::new(Database::open(&path)?);
        let snapshot = db.snapshot()?;
        Ok(OpenedDb { db, snapshot })
    })
    .await;
    Box::new(match joined {
        Ok(Ok(opened)) => Ok(opened),
        Ok(Err(err)) => Err(format!("{err:#}")),
        Err(join) => Err(format!("task panicked: {join}")),
    })
}

/// Run a create-or-update write transaction for the given edit session,
/// then reload the snapshot. Both steps happen on a blocking worker.
async fn save_async(db: Arc<Database>, session: EditSession) -> Result<Snapshot, String> {
    let joined = tokio::task::spawn_blocking(move || -> anyhow::Result<Snapshot> {
        db.write_txn(|txn| match session.draft {
            EditDraft::Tag(draft) => {
                let priority = draft.priority_s.parse().unwrap_or(0);
                match session.handle {
                    Some(h) => {
                        dbrepo::tag::update(txn, &h, &draft.name, &draft.color, priority)?;
                    }
                    None => {
                        dbrepo::tag::create(txn, &draft.name, &draft.color, priority)?;
                    }
                }
                Ok(())
            }
            EditDraft::Note(draft) => {
                let type_value = draft.type_value_s.parse().unwrap_or(1);
                match session.handle {
                    Some(h) => {
                        dbrepo::note::update(txn, &h, type_value, &draft.body)?;
                    }
                    None => {
                        dbrepo::note::create(txn, type_value, &draft.body)?;
                    }
                }
                Ok(())
            }
            EditDraft::Repository(draft) => {
                let type_value = draft.type_value_s.parse().unwrap_or(0);
                match session.handle {
                    Some(h) => {
                        dbrepo::repository::update(txn, &h, &draft.name, type_value)?;
                    }
                    None => {
                        dbrepo::repository::create(txn, &draft.name, type_value)?;
                    }
                }
                Ok(())
            }
            EditDraft::Person(draft) => {
                let gender = draft.gender_s.parse().unwrap_or(2);
                let birth = draft
                    .birth_year_s
                    .trim()
                    .parse::<i32>()
                    .ok()
                    .filter(|y| *y != 0);
                let death = draft
                    .death_year_s
                    .trim()
                    .parse::<i32>()
                    .ok()
                    .filter(|y| *y != 0);
                match session.handle {
                    Some(h) => {
                        dbrepo::person::update(
                            txn,
                            &h,
                            &draft.first_name,
                            &draft.surname,
                            gender,
                            birth,
                            death,
                        )?;
                    }
                    None => {
                        dbrepo::person::create(
                            txn,
                            &draft.first_name,
                            &draft.surname,
                            gender,
                            birth,
                            death,
                        )?;
                    }
                }
                Ok(())
            }
            EditDraft::Family(draft) => {
                let type_value = draft.type_value_s.parse().unwrap_or(0);
                let father = resolve_person_by_gid(txn, &draft.father_gid)?;
                let mother = resolve_person_by_gid(txn, &draft.mother_gid)?;
                match session.handle {
                    Some(h) => {
                        dbrepo::family::update(txn, &h, father, mother, type_value)?;
                    }
                    None => {
                        dbrepo::family::create(txn, father, mother, type_value)?;
                    }
                }
                Ok(())
            }
            EditDraft::Event(draft) => {
                let type_value = draft.type_value_s.parse().unwrap_or(0);
                let place = resolve_place_by_gid(txn, &draft.place_gid)?;
                let date = draft.date.to_date();
                match session.handle {
                    Some(h) => {
                        dbrepo::event::update_full(
                            txn,
                            &h,
                            type_value,
                            &draft.description,
                            place,
                            date,
                        )?;
                    }
                    None => {
                        dbrepo::event::create_full(
                            txn,
                            type_value,
                            &draft.description,
                            place,
                            date,
                        )?;
                    }
                }
                Ok(())
            }
            EditDraft::Place(draft) => {
                let type_value = draft.type_value_s.parse().unwrap_or(0);
                let parent = resolve_place_by_gid(txn, &draft.parent_gid)?;
                match session.handle {
                    Some(h) => {
                        dbrepo::place::update(
                            txn,
                            &h,
                            &draft.name,
                            type_value,
                            &draft.lat,
                            &draft.long,
                            parent,
                        )?;
                    }
                    None => {
                        dbrepo::place::create(
                            txn,
                            &draft.name,
                            type_value,
                            &draft.lat,
                            &draft.long,
                            parent,
                        )?;
                    }
                }
                Ok(())
            }
            EditDraft::Source(draft) => {
                match session.handle {
                    Some(h) => {
                        dbrepo::source::update(
                            txn,
                            &h,
                            &draft.title,
                            &draft.author,
                            &draft.pubinfo,
                            &draft.abbrev,
                        )?;
                    }
                    None => {
                        dbrepo::source::create(
                            txn,
                            &draft.title,
                            &draft.author,
                            &draft.pubinfo,
                            &draft.abbrev,
                        )?;
                    }
                }
                Ok(())
            }
            EditDraft::Citation(draft) => {
                let confidence = draft.confidence_s.parse().unwrap_or(2);
                let source_handle =
                    resolve_source_by_gid(txn, &draft.source_gid)?
                        .unwrap_or_default();
                let date = draft.date.to_date();
                match session.handle {
                    Some(h) => {
                        dbrepo::citation::update(
                            txn,
                            &h,
                            &source_handle,
                            &draft.page,
                            confidence,
                            date,
                        )?;
                    }
                    None => {
                        dbrepo::citation::create(
                            txn,
                            &source_handle,
                            &draft.page,
                            confidence,
                            date,
                        )?;
                    }
                }
                Ok(())
            }
            EditDraft::Media(draft) => {
                let date = draft.date.to_date();
                match session.handle {
                    Some(h) => {
                        dbrepo::media::update(
                            txn,
                            &h,
                            &draft.path,
                            &draft.mime,
                            &draft.desc,
                            date,
                        )?;
                    }
                    None => {
                        dbrepo::media::create(
                            txn,
                            &draft.path,
                            &draft.mime,
                            &draft.desc,
                            date,
                        )?;
                    }
                }
                Ok(())
            }
        })?;
        db.snapshot()
    })
    .await;
    match joined {
        Ok(Ok(snap)) => Ok(snap),
        Ok(Err(err)) => Err(format!("{err:#}")),
        Err(join) => Err(format!("task panicked: {join}")),
    }
}

/// Run a delete transaction for the current view's selected object,
/// then reload the snapshot.
///
/// `delete_owned_events` only matters for Person deletes — it's
/// forwarded to [`dbrepo::person::delete_with_cascade`].
async fn delete_async(
    db: Arc<Database>,
    view: View,
    handle: String,
    delete_owned_events: bool,
) -> Result<Snapshot, String> {
    let joined = tokio::task::spawn_blocking(move || -> anyhow::Result<Snapshot> {
        db.write_txn(|txn| match view {
            View::Persons => {
                dbrepo::person::delete_with_cascade(txn, &handle, delete_owned_events)
            }
            View::Families => dbrepo::family::delete_with_cascade(txn, &handle),
            View::Events => dbrepo::event::delete(txn, &handle),
            View::Places => dbrepo::place::delete_with_cascade(txn, &handle),
            View::Sources => dbrepo::source::delete(txn, &handle),
            View::Citations => dbrepo::citation::delete(txn, &handle),
            View::Media => dbrepo::media::delete(txn, &handle),
            View::Tags => dbrepo::tag::delete(txn, &handle),
            View::Notes => dbrepo::note::delete(txn, &handle),
            View::Repositories => dbrepo::repository::delete(txn, &handle),
            other => Err(anyhow::anyhow!(
                "delete not supported for view {:?}",
                other
            )),
        })?;
        db.snapshot()
    })
    .await;
    match joined {
        Ok(Ok(snap)) => Ok(snap),
        Ok(Err(err)) => Err(format!("{err:#}")),
        Err(join) => Err(format!("task panicked: {join}")),
    }
}

/// Run the add-person-with-relationship action from the modal form.
async fn tree_add_async(db: Arc<Database>, add: PendingAdd) -> Result<Snapshot, String> {
    let joined = tokio::task::spawn_blocking(move || -> anyhow::Result<Snapshot> {
        let gender: i32 = add.gender_s.parse().unwrap_or(2);
        let birth = add.birth_year_s.trim().parse::<i32>().ok().filter(|y| *y != 0);
        let death = add.death_year_s.trim().parse::<i32>().ok().filter(|y| *y != 0);

        db.write_txn(|txn| {
            let mut person = dbrepo::person::create(
                txn,
                &add.first_name,
                &add.surname,
                gender,
                birth,
                death,
            )?;

            // Auto-create Source + Citation if a source URL was provided.
            let source_url = add.source_url.trim();
            if !source_url.is_empty() {
                let src = dbrepo::source::create(txn, source_url, "", "", "")?;
                let cit = dbrepo::citation::create(txn, &src.handle, source_url, 2, None)?;
                // Attach the citation to the person's citation_list and
                // rewrite the person row.
                person.citation_list.push(cit.handle);
                dbrepo::person::save_row(txn, &mut person)?;
            }

            match add.relationship {
                AddRelationship::Child => {
                    dbrepo::relationships::add_child_existing(
                        txn,
                        &add.target_handle,
                        &person.handle,
                    )?;
                }
                AddRelationship::Father | AddRelationship::Mother => {
                    dbrepo::relationships::add_parent_existing(
                        txn,
                        &add.target_handle,
                        &person.handle,
                        gender,
                    )?;
                }
                AddRelationship::Sibling => {
                    dbrepo::relationships::add_sibling_existing(
                        txn,
                        &add.target_handle,
                        &person.handle,
                    )?;
                }
            }
            Ok(())
        })?;
        db.snapshot()
    })
    .await;
    match joined {
        Ok(Ok(snap)) => Ok(snap),
        Ok(Err(err)) => Err(format!("{err:#}")),
        Err(join) => Err(format!("task panicked: {join}")),
    }
}

fn default_draft_for(view: View) -> EditDraft {
    match view {
        View::Persons => EditDraft::Person(PersonDraft {
            first_name: String::new(),
            surname: String::new(),
            // Gender::Unknown = 2
            gender_s: "2".to_string(),
            birth_year_s: String::new(),
            death_year_s: String::new(),
        }),
        View::Families => EditDraft::Family(FamilyDraft {
            father_gid: String::new(),
            mother_gid: String::new(),
            // FamilyRelType::Married = 0
            type_value_s: "0".to_string(),
        }),
        View::Events => EditDraft::Event(EventDraft {
            // EventType::Custom = 0
            type_value_s: "1".to_string(),
            description: String::new(),
            place_gid: String::new(),
            date: crate::views::widgets::date_edit::DateDraft {
                modifier_s: "0".to_string(),
                quality_s: "0".to_string(),
                ..Default::default()
            },
        }),
        View::Places => EditDraft::Place(PlaceDraft {
            name: String::new(),
            // PlaceType::City = 4
            type_value_s: "4".to_string(),
            lat: String::new(),
            long: String::new(),
            parent_gid: String::new(),
        }),
        View::Sources => EditDraft::Source(SourceDraft::default()),
        View::Citations => EditDraft::Citation(CitationDraft {
            confidence_s: "2".to_string(),
            ..Default::default()
        }),
        View::Media => EditDraft::Media(MediaDraft::default()),
        View::Tags => EditDraft::Tag(TagDraft {
            name: "New tag".to_string(),
            color: "#888888".to_string(),
            priority_s: "0".to_string(),
        }),
        View::Notes => EditDraft::Note(NoteDraft {
            body: String::new(),
            // NoteType::General = 1
            type_value_s: "1".to_string(),
        }),
        View::Repositories => EditDraft::Repository(RepoDraft {
            name: "New repository".to_string(),
            // RepositoryType::Library = 1
            type_value_s: "1".to_string(),
        }),
        // Non-editable views fall back to an empty tag draft; the UI
        // won't actually reach this branch because the "New" button is
        // gated on an editable view.
        _ => EditDraft::Tag(TagDraft::default()),
    }
}
