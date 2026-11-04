//! Visible-only, latest-demand thumbnails. One worker, one in-flight reservation,
//! one owning LRU; rendered references cannot pin uncharged RGB outside it.
use crate::{
    image_preview_view::Viewport,
    painter::{Painter, Rect},
    thumbnail_cache::{Cache, MAX_ENTRIES},
    thumbnail_job::{Value, Worker},
};
use files_core::{media_source::Revision, theme::Palette};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const MAX_DEMAND_BYTES: usize = 64 * 1024;
struct Demand {
    path: PathBuf,
    verified: Option<Revision>,
    checked: Option<Instant>,
    error: Option<String>,
    failed: Option<Revision>,
}
struct Active {
    id: u64,
    generation: u64,
    path: PathBuf,
}
pub struct State {
    cache: Cache,
    worker: Worker,
    demand: Vec<Demand>,
    active: Option<Active>,
    generation: u64,
    directory_id: u64,
    directory: Option<PathBuf>,
    authorized: bool,
    next_id: u64,
    pub notice: Option<String>,
    viewport: Viewport,
    pub painted: usize,
}
impl State {
    pub fn new() -> Self {
        Self {
            cache: Cache::new(),
            worker: Worker::new(),
            demand: Vec::new(),
            active: None,
            generation: 1,
            directory_id: 1,
            directory: None,
            authorized: false,
            next_id: 0,
            notice: None,
            viewport: Default::default(),
            painted: 0,
        }
    }
    pub fn directory_target(&self) -> Option<(u64, &Path)> {
        self.directory
            .as_deref()
            .map(|p| ((1u64 << 63) | self.directory_id, p))
    }
    /// Count/bytes over borrowed paths before cloning. Selected demand is first.
    pub fn demand<'a>(&mut self, directory: &Path, paths: impl Iterator<Item = &'a Path> + Clone) {
        self.worker.resume();
        let mut bytes = 0usize;
        let mut count = 0usize;
        for path in paths.clone().take(MAX_ENTRIES) {
            let n = path.as_os_str().len();
            if n > 4096 || !path.is_absolute() {
                self.suspend();
                self.notice = Some("Ruta de miniatura fuera del límite".into());
                return;
            }
            bytes = bytes.saturating_add(n);
            count += 1;
            if bytes > MAX_DEMAND_BYTES {
                self.suspend();
                self.notice = Some("Reduce las miniaturas visibles: límite de claves 64KiB".into());
                return;
            }
        }
        if count == 0 {
            self.suspend();
            return;
        }
        if self.directory.as_deref() != Some(directory) {
            self.suspend();
            self.directory = Some(directory.to_path_buf());
            self.directory_id = self
                .directory_id
                .checked_add(1)
                .filter(|id| *id < (1u64 << 63))
                .expect("thumbnail directory generation exhausted");
        }
        let same = self
            .demand
            .iter()
            .map(|d| d.path.as_path())
            .eq(paths.clone().take(MAX_ENTRIES));
        if same {
            return;
        }
        let mut old = std::mem::take(&mut self.demand);
        self.demand = paths
            .take(MAX_ENTRIES)
            .map(|path| {
                if let Some(i) = old.iter().position(|d| d.path == path) {
                    old.swap_remove(i)
                } else {
                    Demand {
                        path: path.to_path_buf(),
                        verified: None,
                        checked: None,
                        error: None,
                        failed: None,
                    }
                }
            })
            .collect();
        if self
            .active
            .as_ref()
            .is_some_and(|active| !self.demand.iter().any(|d| d.path == active.path))
        {
            self.worker.cancel();
        }
    }
    pub fn invalidate(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("thumbnail generation exhausted");
        self.authorized = false;
        self.cache.clear();
        self.worker.cancel();
        for d in &mut self.demand {
            d.verified = None;
            d.checked = None;
        }
    }
    pub fn authorize(&mut self) {
        self.authorized = true;
        self.notice = None;
    }
    pub fn unavailable(&mut self, message: String) {
        self.invalidate();
        self.notice = Some(message);
    }
    pub fn suspend(&mut self) {
        self.invalidate();
        self.demand.clear();
        self.directory = None;
        self.cache.clear();
        self.viewport = Default::default();
        self.painted = 0;
        self.notice = None;
    }
    pub fn begin_close(&mut self) {
        self.suspend();
        self.worker.begin_close();
    }
    pub fn close_allowed(&self) -> bool {
        self.worker.close_allowed()
    }
    pub fn readonly_pending(&self) -> bool {
        self.worker.readonly_pending()
    }
    pub fn drained(&self) -> bool {
        !self.worker.busy()
    }
    fn accept_reply(&mut self, reply: crate::thumbnail_job::Reply) {
        let active = self.active.take();
        if let Some(active) = active
            && active.id == reply.id
            && active.generation == self.generation
            && self.authorized
            && let Some(d) = self.demand.iter_mut().find(|d| d.path == active.path)
        {
            d.checked = Some(Instant::now());
            match reply.result {
                Ok(Value::Unchanged(revision)) => {
                    if self.cache.revision(&d.path) == Some(revision) {
                        d.verified = Some(revision);
                        d.error = None;
                    } else if d.failed != Some(revision) {
                        d.checked = None;
                    }
                }
                Ok(Value::Unavailable(revision, error)) => {
                    self.cache.remove(&d.path);
                    d.verified = None;
                    d.failed = Some(revision);
                    d.error = Some(bounded_error(error));
                }
                Ok(Value::Bitmap(bitmap)) => {
                    let revision = bitmap.source;
                    // The reservation stays exclusive across ACK and insertion;
                    // no next request can dispatch from inside this branch.
                    match self.cache.insert(d.path.clone(), bitmap) {
                        Ok(()) => {
                            d.verified = revision;
                            d.failed = None;
                            d.error = None;
                        }
                        Err(error) => {
                            self.cache.release();
                            d.error = Some(error.into());
                        }
                    }
                }
                Err(error) => {
                    self.cache.remove(&d.path);
                    d.verified = None;
                    d.failed = None;
                    d.error = Some(bounded_error(error));
                }
            }
        }
        self.cache.release();
    }
    pub fn poll(&mut self, may_start: bool) -> bool {
        let mut changed = false;
        if let Some(reply) = self.worker.poll() {
            self.accept_reply(reply);
            changed = true;
        }
        if !may_start || !self.authorized || self.worker.busy() {
            return changed;
        }
        let now = Instant::now();
        // Expired proof hides pixels while a fresh FD probe is pending, even if
        // notify lost its event. No path stat is ever done in draw/UI.
        let next = self
            .demand
            .iter()
            .position(|d| d.checked.is_none())
            .or_else(|| {
                self.demand
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| {
                        d.checked
                            .is_some_and(|t| now.duration_since(t) >= Duration::from_secs(1))
                    })
                    .min_by_key(|(_, d)| d.checked)
                    .map(|(i, _)| i)
            });
        if let Some(i) = next
            && self.cache.reserve()
        {
            let d = &mut self.demand[i];
            d.verified = None;
            let previous = self.cache.revision(&d.path).or(d.failed);
            self.next_id = self
                .next_id
                .checked_add(1)
                .expect("thumbnail request id exhausted");
            let path = d.path.clone();
            if self.worker.request(self.next_id, path.clone(), previous) {
                self.active = Some(Active {
                    id: self.next_id,
                    generation: self.generation,
                    path,
                });
                changed = true;
            } else {
                self.cache.release();
            }
        }
        changed
    }
    pub fn paint(
        &mut self,
        p: &mut dyn Painter,
        path: &Path,
        body: Rect,
        palette: &Palette,
    ) -> bool {
        if !self.authorized {
            return false;
        }
        let Some(d) = self.demand.iter().find(|d| d.path == path) else {
            return false;
        };
        let Some(revision) = d.verified.filter(|_| {
            d.checked
                .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        }) else {
            return false;
        };
        let Some(bitmap) = self.cache.get(path, revision) else {
            return false;
        };
        let body = Rect::new(
            body.x + (body.w - 160).max(0) / 2,
            body.y + (body.h - 160).max(0) / 2,
            body.w.clamp(1, 160),
            body.h.clamp(1, 160),
        );
        self.viewport.reset_content();
        self.viewport.paint_thumbnail(p, bitmap, body, palette);
        debug_assert!(self.viewport.buffer_capacity() <= crate::thumbnail_cache::SCRATCH_BYTES);
        self.painted += 1;
        true
    }
    pub fn snapshot(&mut self) -> serde_json::Value {
        let ready=self.demand.iter().filter(|d|self.authorized && d.checked.is_some_and(|t|t.elapsed()<Duration::from_secs(1))).filter_map(|d|d.verified.map(|rev|(&d.path,rev))).filter_map(|(path,rev)|self.cache.get(path,rev).map(|b|serde_json::json!({"path":format!("{:?}",path),"pixel":b.rgb.get(..3),"revision":b.source,"width":b.width,"height":b.height,"source_width":b.source_width,"source_height":b.source_height,"format":b.format}))).collect::<Vec<_>>();
        serde_json::json!({"demand":self.count(),"ready":ready,"cached":self.cache.len(),"rgb_capacity":self.bytes(),"scratch_capacity":self.scratch_bytes(),"busy":!self.drained(),"errors":self.errors(),"notice":self.notice,"authorized":self.authorized,"painted":self.painted})
    }
    pub fn ready(&self) -> usize {
        self.demand
            .iter()
            .filter(|d| {
                self.authorized
                    && d.verified.is_some()
                    && d.checked
                        .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
            })
            .count()
    }
    pub fn errors(&self) -> usize {
        self.demand.iter().filter(|d| d.error.is_some()).count()
    }
    pub fn count(&self) -> usize {
        self.demand.len()
    }
    pub fn bytes(&self) -> usize {
        self.cache.bytes()
    }
    pub fn scratch_bytes(&self) -> usize {
        self.viewport.buffer_capacity()
    }
    pub fn error(&self, path: &Path) -> Option<&str> {
        self.demand
            .iter()
            .find(|d| d.path == path)?
            .error
            .as_deref()
    }
}

fn bounded_error(error: String) -> String {
    let mut end = error.len().min(1024);
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    error[..end].into()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn revision(inode: u64) -> Revision {
        Revision {
            device: 1,
            inode,
            bytes: 1,
            modified_seconds: 0,
            modified_nanoseconds: 0,
            changed_seconds: 0,
            changed_nanoseconds: 0,
        }
    }
    fn bitmap(inode: u64) -> crate::image_preview_job::Bitmap {
        crate::image_preview_job::Bitmap {
            source: Some(revision(inode)),
            width: 1,
            height: 1,
            source_width: 1,
            source_height: 1,
            format: "png".into(),
            rgb: vec![1, 2, 3],
        }
    }
    fn state() -> State {
        let mut s = State::new();
        s.demand(
            Path::new("/fixture"),
            [Path::new("/fixture/red.png")].into_iter(),
        );
        s.authorize();
        s
    }
    fn active(s: &mut State, id: u64) {
        assert!(s.cache.reserve());
        s.active = Some(Active {
            id,
            generation: s.generation,
            path: "/fixture/red.png".into(),
        });
    }
    #[test]
    fn dirty_discards_late_bitmap_and_its_ticket_before_replacement() {
        let mut s = state();
        active(&mut s, 1);
        s.invalidate();
        s.authorize();
        s.accept_reply(crate::thumbnail_job::Reply {
            id: 1,
            result: Ok(Value::Bitmap(bitmap(1))),
        });
        assert_eq!(s.cache.bytes(), 0);
        assert_eq!(s.ready(), 0);
        active(&mut s, 2);
        s.accept_reply(crate::thumbnail_job::Reply {
            id: 2,
            result: Ok(Value::Bitmap(bitmap(2))),
        });
        assert_eq!(s.ready(), 1);
        s.invalidate();
        assert_eq!(s.cache.bytes(), 0);
        assert_eq!(s.ready(), 0);
    }
    #[test]
    fn codec_error_is_revision_memoized_while_reprobe_reuses_no_decoder() {
        let mut s = state();
        active(&mut s, 1);
        s.accept_reply(crate::thumbnail_job::Reply {
            id: 1,
            result: Ok(Value::Unavailable(revision(1), "decoder rejected".into())),
        });
        assert_eq!(s.demand[0].failed, Some(revision(1)));
        active(&mut s, 2);
        s.accept_reply(crate::thumbnail_job::Reply {
            id: 2,
            result: Ok(Value::Unchanged(revision(1))),
        });
        assert_eq!(
            s.error(Path::new("/fixture/red.png")),
            Some("decoder rejected")
        );
        assert_eq!(s.ready(), 0);
        active(&mut s, 3);
        s.accept_reply(crate::thumbnail_job::Reply {
            id: 3,
            result: Ok(Value::Bitmap(bitmap(2))),
        });
        assert_eq!(s.ready(), 1);
        assert!(s.demand[0].failed.is_none());
    }
    #[test]
    fn expired_proof_is_not_ready_even_if_rgb_remains_in_lru() {
        let mut s = state();
        active(&mut s, 1);
        s.accept_reply(crate::thumbnail_job::Reply {
            id: 1,
            result: Ok(Value::Bitmap(bitmap(1))),
        });
        assert_eq!(s.ready(), 1);
        s.demand[0].checked = Some(Instant::now() - Duration::from_secs(2));
        assert_eq!(s.ready(), 0);
        assert!(s.snapshot()["ready"].as_array().unwrap().is_empty());
        assert!(s.bytes() > 0);
    }
    #[test]
    fn demand_preflight_rejects_excess_bytes_and_bounds_infinite_input() {
        let mut s = State::new();
        let huge = PathBuf::from(format!("/{}", "x".repeat(1_000_000)));
        s.demand(Path::new("/fixture"), std::iter::once(huge.as_path()));
        assert_eq!(s.count(), 0);
        assert!(s.notice.is_some());
        s.demand(
            Path::new("/fixture"),
            std::iter::repeat(Path::new("/fixture/red.png")),
        );
        assert_eq!(s.count(), 32);
        s.begin_close();
        assert_eq!(s.bytes(), 0);
        assert_eq!(s.scratch_bytes(), 0);
        assert!(s.close_allowed());
    }
}
