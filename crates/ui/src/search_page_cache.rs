//! Bounded, process-local page history. The current Report stays in Search State.
use files_core::{search::Report, search_page::Page};
use std::collections::VecDeque;
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
pub const PAGE_RESERVE: usize = 512 * 1024;
const MAX_PAGES: usize = 8;
#[derive(Clone, Copy)]
pub struct Meta {
    pub sequence: u64,
    pub more: bool,
}
#[derive(Clone, Copy, Default)]
pub struct Position {
    pub selected: usize,
    pub scroll: usize,
}
struct Saved {
    meta: Meta,
    report: Report,
    position: Position,
}
#[derive(Default)]
pub struct Cache {
    pub current: Option<Meta>,
    history: VecDeque<Saved>,
    pub loading: bool,
    advance: bool,
    pub ended: bool,
    pub complete: bool,
    pub cancelled: bool,
    pub errors: u64,
    pub mounts: u64,
    pub found: u64,
}
fn cost(report: &Report) -> usize {
    std::mem::size_of::<Report>()
        + report.hits.capacity() * std::mem::size_of::<files_core::search::Hit>()
        + report
            .hits
            .iter()
            .map(|h| h.relative.capacity() + h.snippet.as_ref().map_or(0, |s| s.text.capacity()))
            .sum::<usize>()
        + report.reason.as_ref().map_or(0, String::capacity)
}
impl Cache {
    pub fn latest(&self) -> Option<u64> {
        self.current
            .iter()
            .map(|m| m.sequence)
            .chain(self.history.iter().map(|s| s.meta.sequence))
            .max()
    }
    pub fn oldest(&self) -> Option<u64> {
        self.current
            .iter()
            .map(|m| m.sequence)
            .chain(self.history.iter().map(|s| s.meta.sequence))
            .min()
    }
    pub fn previous(&self) -> bool {
        self.current.is_some_and(|m| {
            m.sequence > 0
                && self
                    .history
                    .iter()
                    .any(|s| s.meta.sequence == m.sequence - 1)
        })
    }
    pub fn next_cached(&self) -> bool {
        self.current.is_some_and(|m| {
            self.history
                .iter()
                .any(|s| s.meta.sequence == m.sequence + 1)
        })
    }
    pub fn can_request(&self) -> bool {
        !self.loading
            && !self.ended
            && self
                .current
                .is_some_and(|m| m.more && Some(m.sequence) == self.latest())
    }
    pub fn bytes(&self, report: &Option<Report>) -> usize {
        std::mem::size_of::<Self>()
            + self.history.capacity() * std::mem::size_of::<Saved>()
            + report.as_ref().map_or(0, cost)
            + self.history.iter().map(|s| cost(&s.report)).sum::<usize>()
    }
    fn insert(&mut self, saved: Saved) {
        let index = self
            .history
            .iter()
            .position(|s| s.meta.sequence > saved.meta.sequence)
            .unwrap_or(self.history.len());
        self.history.insert(index, saved);
    }
    fn trim(&mut self, report: &Option<Report>, reserve: usize) {
        while !self.history.is_empty()
            && (self.bytes(report).saturating_add(reserve) > MAX_BYTES
                || self.history.len() + 1 + usize::from(reserve > 0) > MAX_PAGES)
        {
            self.history.pop_front();
        }
    }
    pub fn reserve(&mut self, report: &Option<Report>) {
        self.trim(report, PAGE_RESERVE + 16 * std::mem::size_of::<Saved>());
    }
    pub fn stopped(&mut self) {
        self.loading = false;
        self.advance = false;
        self.ended = true;
        self.complete = false;
        self.cancelled = true;
        if let Some(meta) = self.current.as_mut() {
            meta.more = false;
        }
        for saved in &mut self.history {
            saved.meta.more = false;
        }
    }
    pub fn requested(&mut self) {
        self.loading = true;
        self.advance = true;
    }
    pub fn seek(
        &mut self,
        delta: i8,
        report: &mut Option<Report>,
        position: Position,
    ) -> Option<Position> {
        let current = self.current?;
        let next = if delta < 0 {
            current.sequence.checked_sub(1)?
        } else {
            current.sequence.checked_add(1)?
        };
        let index = self.history.iter().position(|s| s.meta.sequence == next)?;
        let saved = self.history.remove(index)?;
        if let Some(report) = report.replace(saved.report) {
            self.insert(Saved {
                meta: current,
                report,
                position,
            });
        }
        self.current = Some(saved.meta);
        self.advance = false;
        Some(saved.position)
    }
    pub fn admit(
        &mut self,
        page: Page,
        report: &mut Option<Report>,
        position: Position,
    ) -> Result<Option<Position>, String> {
        let expected = self.latest().map_or(0, |n| n + 1);
        if page.sequence != expected {
            return Err("Secuencia de caché obsoleta".into());
        }
        if cost(&page.report) > PAGE_RESERVE {
            return Err("Página fuera del presupuesto retenido512KiB".into());
        }
        self.found = page.found_total;
        self.loading = false;
        self.ended = !page.more;
        self.complete = !page.more && page.report.complete;
        self.errors = page.report.errors;
        self.mounts = page.report.mounts_excluded;
        if page.report.hits.is_empty() && self.current.is_some() {
            let latest = self.latest();
            let target = if self.current.is_some_and(|m| Some(m.sequence) == latest) {
                report.as_mut()
            } else {
                self.history
                    .iter_mut()
                    .find(|s| Some(s.meta.sequence) == latest)
                    .map(|s| &mut s.report)
            };
            if let Some(target) = target {
                let hits = std::mem::take(&mut target.hits);
                *target = page.report;
                target.hits = hits;
            }
            if let Some(meta) = self.current.as_mut().filter(|m| Some(m.sequence) == latest) {
                meta.more = false;
            }
            if let Some(last) = self
                .history
                .iter_mut()
                .find(|s| Some(s.meta.sequence) == latest)
            {
                last.meta.more = false;
            }
            return Ok(None);
        }
        let meta = Meta {
            sequence: page.sequence,
            more: page.more,
        };
        let changed = if self.current.is_none() || self.advance {
            if let Some(old) = report.take()
                && let Some(old_meta) = self.current
            {
                self.insert(Saved {
                    meta: old_meta,
                    report: old,
                    position,
                });
            }
            *report = Some(page.report);
            self.current = Some(meta);
            Some(Position::default())
        } else {
            self.insert(Saved {
                meta,
                report: page.report,
                position: Position::default(),
            });
            None
        };
        self.trim(report, 0);
        self.advance = false;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(sequence: u64, more: bool) -> Page {
        Page {
            version: 1,
            id: 1,
            sequence,
            found_total: sequence + 1,
            more,
            report: Report {
                hits: vec![files_core::search::Hit {
                    relative: format!("{sequence}.txt").into(),
                    kind: files_core::listing::Kind::File,
                    content: None,
                    snippet: None,
                }],
                visited: sequence + 1,
                complete: !more,
                reason: more.then(|| "page".into()),
                ..Default::default()
            },
        }
    }
    #[test]
    fn cached_navigation_restores_position_and_arrival_never_steals_backward_reading() {
        let mut cache = Cache::default();
        let mut report = None;
        cache
            .admit(page(0, true), &mut report, Position::default())
            .unwrap();
        cache.reserve(&report);
        cache.requested();
        cache
            .admit(
                page(1, true),
                &mut report,
                Position {
                    selected: 4,
                    scroll: 3,
                },
            )
            .unwrap();
        cache.reserve(&report);
        cache.requested();
        let position = cache
            .seek(
                -1,
                &mut report,
                Position {
                    selected: 2,
                    scroll: 1,
                },
            )
            .unwrap();
        assert_eq!((position.selected, position.scroll), (4, 3));
        assert!(
            cache
                .admit(page(2, false), &mut report, position)
                .unwrap()
                .is_none()
        );
        assert_eq!(cache.current.unwrap().sequence, 0);
        assert_eq!(
            report.as_ref().unwrap().hits[0].relative,
            PathBuf::from("0.txt")
        );
        let position = cache.seek(1, &mut report, position).unwrap();
        assert_eq!((position.selected, position.scroll), (2, 1));
        cache.seek(1, &mut report, position).unwrap();
        assert_eq!(cache.current.unwrap().sequence, 2);
        assert!(!cache.can_request());
    }
    #[test]
    fn empty_terminal_keeps_last_nonempty_page_and_updates_only_latest_summary() {
        let mut cache = Cache::default();
        let mut report = None;
        cache
            .admit(page(0, true), &mut report, Position::default())
            .unwrap();
        cache.reserve(&report);
        cache.requested();
        cache
            .admit(page(1, true), &mut report, Position::default())
            .unwrap();
        cache.reserve(&report);
        cache.requested();
        cache.seek(-1, &mut report, Position::default()).unwrap();
        let mut end = page(2, false);
        end.report.hits.clear();
        end.found_total = 2;
        end.report.visited = 8;
        assert!(
            cache
                .admit(end, &mut report, Position::default())
                .unwrap()
                .is_none()
        );
        assert_eq!(cache.current.unwrap().sequence, 0);
        assert!(!report.as_ref().unwrap().complete);
        cache.seek(1, &mut report, Position::default()).unwrap();
        assert_eq!(cache.current.unwrap().sequence, 1);
        assert!(report.as_ref().unwrap().complete && report.as_ref().unwrap().visited == 8);
        assert_eq!(
            report.as_ref().unwrap().hits[0].relative,
            PathBuf::from("1.txt")
        );
        assert!(!cache.can_request());
    }
    #[test]
    fn history_capacity_and_credit_reservation_evict_explicitly_without_cloning_reports() {
        let mut cache = Cache::default();
        let mut report = None;
        for sequence in 0..20 {
            if sequence > 0 {
                cache.reserve(&report);
                assert!(cache.bytes(&report) + PAGE_RESERVE <= MAX_BYTES);
                cache.requested();
            }
            let mut next = page(sequence, true);
            next.report.hits = (0..500)
                .map(|i| files_core::search::Hit {
                    relative: format!("{}-{i}", "x".repeat(490)).as_str().into(),
                    kind: files_core::listing::Kind::File,
                    content: None,
                    snippet: None,
                })
                .collect();
            next.report.visited = (sequence + 1) * 500;
            next.found_total = (sequence + 1) * 500;
            next.report.hits.sort_by(|a, b| a.relative.cmp(&b.relative));
            files_core::search::validate(&next.report).unwrap();
            cache.admit(next, &mut report, Position::default()).unwrap();
            assert!(cache.bytes(&report) <= MAX_BYTES && cache.history.len() < MAX_PAGES);
        }
        assert!(cache.oldest().unwrap() > 0);
        assert_eq!(cache.current.unwrap().sequence, 19);
    }
    use std::path::PathBuf;
}
