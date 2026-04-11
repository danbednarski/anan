//! Page-level view builders. Each module owns one top-level screen or
//! panel and exposes a `view(&state, ...) -> Element<Message>` function.
//! Views are stateless over iced — all state lives in `app::App`.

pub mod person_detail;
pub mod person_list;
