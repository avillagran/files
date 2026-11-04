//! Documento Markdown acotado para un renderer nativo (sin HTML/WebView/red).
//! Requiere pulldown-cmark 0.13.5 con default-features=false al integrarlo.
//! Los límites cubren entrada y representación retenida, no el pico del parser:
//! ejecutar fuera del callback de dibujo y medir proceso/árbol al integrar.
use crate::preview_link::{self, Target};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag};
use std::ops::Range;

pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_TEXT_BYTES: usize = 384 * 1024;
const MAX_EVENTS: usize = 8192;
const MAX_DEPTH: usize = 64;
const MAX_LINK_BYTES: usize = 64 * 1024;
const MAX_COLUMNS: usize = 64;

#[derive(Debug)]
pub enum Container {
    Paragraph,
    Heading(u8),
    Quote,
    CodeBlock { language: Range<usize> },
    List { first: Option<u64> },
    Item,
    Table(Vec<Alignment>),
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strike,
    Link { target: Option<Target> },
    Image { target: Option<Target> },
    LiteralHtml,
}

#[derive(Debug)]
pub enum Atom {
    Open(Container),
    Close,
    Text(Range<usize>),
    InlineCode(Range<usize>),
    LiteralHtml(Range<usize>),
    SoftBreak,
    HardBreak,
    Rule,
    Task(bool),
}

#[derive(Debug)]
pub struct Item {
    pub atom: Atom,
    /// Bytes de origen del evento; no equivalen a posiciones del texto visible
    /// cuando hay entidades, escapes o delimitadores Markdown.
    pub source: Range<usize>,
}

#[derive(Debug)]
pub struct Document {
    text: String,
    items: Vec<Item>,
    pub blocked_targets: usize,
    pub literal_html: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Limit {
    Source,
    Text,
    Events,
    Depth,
    Links,
    Columns,
    Unsupported,
}

impl Document {
    pub fn parse(source: &str) -> Result<Self, Limit> {
        // Antes de construir Parser: no se duplica una entrada sin límite.
        if source.len() > MAX_SOURCE_BYTES {
            return Err(Limit::Source);
        }
        let options =
            Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
        let mut doc = Self {
            text: String::new(),
            items: Vec::new(),
            blocked_targets: 0,
            literal_html: false,
        };
        let mut depth = 0usize;
        let mut link_bytes = 0usize;
        for (event, source) in Parser::new_ext(source, options).into_offset_iter() {
            if doc.items.len() == MAX_EVENTS {
                return Err(Limit::Events);
            }
            let atom = match event {
                Event::Start(tag) => {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return Err(Limit::Depth);
                    }
                    let kind = match tag {
                        Tag::Paragraph => Container::Paragraph,
                        Tag::Heading { level, .. } => Container::Heading(level as u8),
                        Tag::BlockQuote(_) => Container::Quote,
                        Tag::CodeBlock(kind) => {
                            let language = match kind {
                                CodeBlockKind::Indented => doc.store("")?,
                                CodeBlockKind::Fenced(info) => doc.store(&info)?,
                            };
                            Container::CodeBlock { language }
                        }
                        Tag::List(first) => Container::List { first },
                        Tag::Item => Container::Item,
                        Tag::Table(columns) => {
                            if columns.len() > MAX_COLUMNS {
                                return Err(Limit::Columns);
                            }
                            Container::Table(columns)
                        }
                        Tag::TableHead => Container::TableHead,
                        Tag::TableRow => Container::TableRow,
                        Tag::TableCell => Container::TableCell,
                        Tag::Emphasis => Container::Emphasis,
                        Tag::Strong => Container::Strong,
                        Tag::Strikethrough => Container::Strike,
                        Tag::Link { dest_url, .. } => Container::Link {
                            target: doc.target(&dest_url, &mut link_bytes)?,
                        },
                        Tag::Image { dest_url, .. } => Container::Image {
                            target: doc.target(&dest_url, &mut link_bytes)?,
                        },
                        Tag::HtmlBlock => {
                            doc.literal_html = true;
                            Container::LiteralHtml
                        }
                        _ => return Err(Limit::Unsupported),
                    };
                    Atom::Open(kind)
                }
                Event::End(_) => {
                    depth = depth.checked_sub(1).ok_or(Limit::Unsupported)?;
                    Atom::Close
                }
                Event::Text(value) => Atom::Text(doc.store(&value)?),
                Event::Code(value) => Atom::InlineCode(doc.store(&value)?),
                Event::Html(value) | Event::InlineHtml(value) => {
                    doc.literal_html = true;
                    Atom::LiteralHtml(doc.store(&value)?)
                }
                Event::SoftBreak => Atom::SoftBreak,
                Event::HardBreak => Atom::HardBreak,
                Event::Rule => Atom::Rule,
                Event::TaskListMarker(checked) => Atom::Task(checked),
                _ => return Err(Limit::Unsupported),
            };
            doc.items.push(Item { atom, source });
        }
        if depth != 0 {
            return Err(Limit::Unsupported);
        }
        Ok(doc)
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }
    pub fn text(&self, range: Range<usize>) -> Option<&str> {
        self.text.get(range)
    }

    fn store(&mut self, text: &str) -> Result<Range<usize>, Limit> {
        let start = self.text.len();
        let end = start
            .checked_add(text.len())
            .filter(|n| *n <= MAX_TEXT_BYTES)
            .ok_or(Limit::Text)?;
        self.text.push_str(text);
        Ok(start..end)
    }

    fn target(&mut self, value: &str, total: &mut usize) -> Result<Option<Target>, Limit> {
        *total = total
            .checked_add(value.len())
            .filter(|n| *n <= MAX_LINK_BYTES)
            .ok_or(Limit::Links)?;
        // Un destino bloqueado conserva el texto/alt visible, sin acción ni carga.
        match preview_link::parse(value) {
            Ok(target) => Ok(Some(target)),
            Err(_) => {
                self.blocked_targets += 1;
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_and_entities_preserve_visible_unicode() {
        let doc = Document::parse("# Título\n\n**fuerte** y *suave* &amp; `a<b`\n").unwrap();
        assert!(matches!(
            doc.items[0].atom,
            Atom::Open(Container::Heading(1))
        ));
        assert!(
            doc.items
                .iter()
                .any(|i| matches!(i.atom, Atom::Open(Container::Strong)))
        );
        assert!(
            doc.items
                .iter()
                .any(|i| matches!(i.atom, Atom::InlineCode(_)))
        );
        assert!(doc.text.contains(" & "));
        for item in &doc.items {
            if let Atom::Text(range) | Atom::InlineCode(range) = &item.atom {
                assert!(doc.text(range.clone()).is_some());
            }
        }
    }

    #[test]
    fn tables_lists_tasks_and_code_are_structural() {
        let doc = Document::parse("| A | B |\n| :- | -: |\n| uno | dos |\n\n3. ítem\n\n- [x] hecho\n\n```rust\nlet a = 1;\n```\n").unwrap();
        assert!(doc.items.iter().any(|i| matches!(&i.atom,
            Atom::Open(Container::Table(a)) if a == &[Alignment::Left, Alignment::Right])));
        assert!(
            doc.items
                .iter()
                .any(|i| matches!(i.atom, Atom::Open(Container::List { first: Some(3) })))
        );
        assert!(doc.items.iter().any(|i| matches!(i.atom, Atom::Task(true))));
        assert!(
            doc.items
                .iter()
                .any(|i| matches!(i.atom, Atom::Open(Container::CodeBlock { .. })))
        );
    }

    #[test]
    fn raw_html_is_literal_and_external_images_never_load() {
        let doc = Document::parse("<script>alert(1)</script>\n\n![alt](https://example.invalid/p.png) [local](guide.md#top) [escape](../secret)\n").unwrap();
        assert!(doc.literal_html);
        assert_eq!(doc.blocked_targets, 1);
        let pending = doc
            .items
            .iter()
            .find_map(|item| match &item.atom {
                Atom::Open(Container::Link {
                    target: Some(Target::Relative { path, .. }),
                }) if path == std::path::Path::new("../secret") => Some(path),
                _ => None,
            })
            .expect("parents are resolved against the current document later");
        assert_eq!(
            preview_link::resolve_relative(std::path::Path::new("overview.md"), pending),
            Err(preview_link::Rejected::OutsideRoot)
        );
        assert!(
            doc.items
                .iter()
                .any(|i| matches!(i.atom, Atom::Open(Container::Image { target: None })))
        );
        assert!(
            doc.items
                .iter()
                .any(|i| matches!(i.atom, Atom::Open(Container::Link { target: Some(_) })))
        );
    }

    #[test]
    fn rejects_source_event_and_depth_excess_without_partial_document() {
        assert!(matches!(
            Document::parse(&"a".repeat(MAX_SOURCE_BYTES + 1)),
            Err(Limit::Source)
        ));
        assert!(matches!(
            Document::parse(&"a\n\n".repeat(MAX_EVENTS)),
            Err(Limit::Events)
        ));
        let nested = format!("{}text", "> ".repeat(MAX_DEPTH + 1));
        assert!(matches!(Document::parse(&nested), Err(Limit::Depth)));
    }

    #[test]
    fn source_ranges_reference_original_bytes_after_entity_expansion() {
        let source = "á &amp; **β**";
        let doc = Document::parse(source).unwrap();
        for item in &doc.items {
            assert!(source.get(item.source.clone()).is_some());
        }
        assert!(doc.text.contains("á & "));
        assert!(!doc.text.contains("&amp;"));
    }
}
