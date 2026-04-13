//! Generic list pane used by every read view.
//!
//! Takes a title, a list of pre-computed row labels (already sorted and
//! filtered), the current selection, and the current query text. Emits
//! `Message::SelectIndex(usize)` and `Message::SearchChanged(String)` so
//! the caller doesn't have to distinguish between views — the app
//! dispatches those messages to whichever view is currently active.

use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Theme};

use crate::app::{Message, SEARCH_INPUT_ID};

/// Per-view list state. Each primary-type view in `App` owns one.
#[derive(Debug, Clone, Default)]
pub struct ListState {
    /// Indices into the underlying `Vec<T>`, filtered and sorted.
    pub order: Vec<usize>,
    /// Position *inside `order`*, not inside the underlying Vec.
    pub selected: Option<usize>,
    /// Current search query.
    pub query: String,
}

impl ListState {
    /// Return the underlying item index for the currently-selected row.
    pub fn selected_item(&self) -> Option<usize> {
        self.selected.and_then(|i| self.order.get(i).copied())
    }

    /// Move the highlight up one row (no wrap, stops at 0).
    pub fn navigate_up(&mut self) {
        match self.selected {
            Some(i) if i > 0 => self.selected = Some(i - 1),
            None if !self.order.is_empty() => self.selected = Some(0),
            _ => {}
        }
    }

    /// Move the highlight down one row (no wrap, stops at last).
    pub fn navigate_down(&mut self) {
        match self.selected {
            Some(i) if i + 1 < self.order.len() => self.selected = Some(i + 1),
            None if !self.order.is_empty() => self.selected = Some(0),
            _ => {}
        }
    }

    /// Clamp `selected` to the current `order`, dropping it if the
    /// filter cleared everything.
    pub fn clamp_selection(&mut self) {
        if self.order.is_empty() {
            self.selected = None;
        } else if self.selected.map(|i| i >= self.order.len()).unwrap_or(false) {
            self.selected = Some(self.order.len() - 1);
        } else if self.selected.is_none() {
            self.selected = Some(0);
        }
    }
}

/// Generic filter-and-sort for a list view.
///
/// `items` is the full Vec of a primary type. `matches` decides whether
/// a given item survives the current query. `sort_by` orders the
/// survivors. After the recompute, the state's `selected` is clamped to
/// the new `order`.
pub fn recompute<T>(
    items: &[T],
    state: &mut ListState,
    matches: impl Fn(&T, &str) -> bool,
    sort_by: impl Fn(&T, &T) -> std::cmp::Ordering,
) {
    let q = state.query.trim().to_lowercase();
    state.order = items
        .iter()
        .enumerate()
        .filter(|(_, it)| q.is_empty() || matches(it, &q))
        .map(|(i, _)| i)
        .collect();
    state.order.sort_by(|&a, &b| sort_by(&items[a], &items[b]));
    state.clamp_selection();
}

/// Build a list-pane widget from a pre-rendered list of row labels.
pub fn view<'a>(
    title: &'a str,
    rows: Vec<String>,
    selected: Option<usize>,
    query: &'a str,
    placeholder: &'a str,
) -> Element<'a, Message> {
    let header = row![text(format!("{title}  ·  {}", rows.len())).size(13)].padding(6);

    let search = text_input(placeholder, query)
        .id(SEARCH_INPUT_ID)
        .on_input(Message::SearchChanged)
        .padding(6);

    let mut list = column![].spacing(2).padding(6);
    for (display_idx, label) in rows.into_iter().enumerate() {
        let is_selected = selected == Some(display_idx);
        let btn = button(text(label).size(13))
            .width(Length::Fill)
            .on_press(Message::SelectIndex(display_idx))
            .style(move |theme: &Theme, status| row_style(theme, status, is_selected));
        list = list.push(btn);
    }

    let body = scrollable(list).height(Length::Fill).width(Length::Fill);

    container(
        column![header, search, body]
            .spacing(4)
            .align_x(Alignment::Start),
    )
    .width(Length::Fixed(320.0))
    .height(Length::Fill)
    .into()
}

fn row_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
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
    if selected {
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
