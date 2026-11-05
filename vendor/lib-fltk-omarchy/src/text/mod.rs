//! Rich text and Markdown: a document model, a streaming Markdown parser,
//! line layout with FLTK measurement, and two widgets.
//!
//! - [`RichText`]: a virtualized, selectable view for long documents and
//!   chat transcripts that stream (append messages, push tokens into the
//!   last one, stick to the bottom).
//! - [`RichLabel`]: one paragraph of inline Markdown for headers and toasts.
//!
//! ```no_run
//! use fltk_omarchy::text::{Block, ColorRole, RichText, Style};
//! let view = RichText::new().stick_to_bottom(true).on_link(|url| println!("{url}"));
//! view.append_block(Block::paragraph("").push("Agent", &Style::plain().bold().color(ColorRole::Accent)));
//! view.append_markdown("Here is the fix:\n\n```diff\n- old\n+ new\n```\n");
//! let tx = view.sender();                       // Send: stream from a worker thread
//! std::thread::spawn(move || tx.push("More **tokens**"));
//! ```
//!
//! Cost model (see docs/PERFORMANCE.md): a push re-parses only the open
//! tail of the last message; blocks that did not change keep their layout;
//! the block that grew keeps the lines before the change; only blocks in
//! view (plus half a view around) are laid out, the others use a height
//! estimate. Nothing runs while idle. Blocks are stored packed (about
//! 1.5 bytes per byte of dense Markdown, see `store`); only the blocks
//! near the view exist as [`Block`]s with a layout.
//!
//! With the `a11y` feature a `RichText` is a Document for screen readers:
//! the blocks near the view as Paragraph, Heading, List/ListItem, Code and
//! Table nodes with their text and Link children (docs/ACCESSIBILITY.md).
//!
//! Limitations: no bidi reordering (each fragment is shaped by FLTK/Pango,
//! so an Arabic run reads right to left, but lines fill left to right and
//! selection measures left to right); long code lines wrap; images show as
//! their alt text with the link.

pub mod doc;
pub mod highlight;
pub mod label;
pub mod layout;
pub mod markdown;
pub(crate) mod store;
pub mod view;

pub use doc::*;
pub use label::RichLabel;
pub use markdown::{MarkdownOptions, MarkdownStream, StreamUpdate};
pub use view::{RichText, TextPos, TextSender};
