//! Multi-line text editing: [`TextArea`] (an Omarchy-styled editor with
//! word wrap, auto-grow, undo, input methods and chips) and [`Composer`]
//! (chat input built on it: Enter sends, trigger completions, attachment
//! chips, file drops).
//!
//! ```no_run
//! use fltk_omarchy::edit::{Composer, CompletionItem, TextArea, Trigger};
//! use fltk_omarchy::{layout::FlexExt, State};
//! # let mut col = fltk_omarchy::layout::column();
//! let notes = State::new(String::new());
//! col.fit(&*TextArea::new().placeholder("Notes").lines(3, 10).bind(&notes));
//!
//! let draft = State::new(String::new());
//! let busy = State::new(false);
//! let composer = Composer::new()
//!     .bind(&draft)
//!     .busy(&busy)
//!     .trigger('@', Trigger::WordStart, |q, reply| {
//!         reply.set(vec![CompletionItem::new("src/main.rs").chip("/repo/src/main.rs")]);
//!     })
//!     .on_submit(|text| println!("send {text}"))
//!     .on_files(|paths| println!("attach {paths:?}"));
//! col.fit(&*composer);
//! ```
//!
//! The editor is drawn by the toolkit (not `fltk::text::TextEditor`):
//! grapheme-correct editing, atomic chips, coalesced undo and caret
//! geometry for completion popups need control over the text model, and
//! layout reuses the `text` module's wrap (FLTK measurement, CJK breaks).
//! Input methods go through FLTK's `Fl::compose` (dead keys, compose
//! sequences, Wayland text-input preedit, X11 XIM, macOS marked text) and
//! the caret position is reported with `fl_set_spot`. Guide:
//! docs/COMPONENTS.md ("TextArea and Composer").

mod area;
pub mod buffer;
mod composer;
pub mod layout;

pub use area::{TabKey, TextArea};
pub use composer::{
    parse_files, size_text, Attachment, CompletionFeed, CompletionItem, CompletionState,
    Completions, Composer, Query, SendKey, Trigger, SEND,
};

pub use buffer::{Chip, EditBuffer, EditKind};
