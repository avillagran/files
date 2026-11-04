//! One reader worker and one owned child. Page publication is not a reap ACK.
use files_core::{
    protocol::Request,
    search_page::{Next, Page, Reader},
};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Duration,
};
pub struct Job {
    cancel: Arc<AtomicBool>,
    invalid: Arc<AtomicBool>,
    waiting: Arc<AtomicU64>,
    page: Arc<Mutex<Option<Page>>>,
    credit: SyncSender<u64>,
    result: Receiver<Result<Page, String>>,
}
const NONE: u64 = u64::MAX;
impl Job {
    pub fn start(path: PathBuf, query: String, hidden: bool) -> Result<Self, String> {
        let (metadata, content, regex) = {
            let filter = files_core::name_filter::Filter::for_search(&query);
            (
                filter.needs_metadata(),
                filter.content().is_some(),
                filter.regex().is_some(),
            )
        };
        let mut request = serde_json::to_vec(&Request::SearchPages {
            id: 1,
            path,
            query,
            hidden,
        })
        .map_err(|e| e.to_string())?;
        request.push(b'\n');
        if request.len() > 64 * 1024 {
            return Err("Petición paginada superior a64KiB".into());
        }
        let executable = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("files-helper");
        let cancel = Arc::new(AtomicBool::new(false));
        let invalid = Arc::new(AtomicBool::new(false));
        let waiting = Arc::new(AtomicU64::new(NONE));
        let page = Arc::new(Mutex::new(None));
        let (credit, input) = mpsc::sync_channel(1);
        let (tx, result) = mpsc::sync_channel(1);
        let flag = cancel.clone();
        let failed = invalid.clone();
        let wait = waiting.clone();
        let published = page.clone();
        std::thread::Builder::new()
            .name("files-search-pages".into())
            .spawn(move || {
                let decoder = RefCell::new(Reader::new(1, metadata, content).expect_regex(regex));
                let mut control = || {
                    let mut reader = decoder.borrow_mut();
                    let message = match input.try_recv() {
                        Ok(sequence) => {
                            reader.credit(sequence)?;
                            wait.store(NONE, Ordering::Release);
                            if std::env::var_os("FILES_TEST_PREVIEW").is_some() {
                                eprintln!("FILES_SEARCH_PAGE_CREDIT sequence={sequence}");
                            }

                            let mut bytes = serde_json::to_vec(&Next {
                                version: 1,
                                id: 1,
                                sequence,
                            })?;
                            bytes.push(b'\n');
                            Some(bytes)
                        }
                        Err(mpsc::TryRecvError::Empty) => None,
                        Err(_) => return Err(std::io::Error::other("Control de búsqueda cerrado")),
                    };
                    Ok(crate::space_map_job::Dialogue {
                        message,
                        idle: reader.waiting().is_some(),
                    })
                };
                let value = crate::space_map_job::run_helper_dialogue(
                    &request,
                    &flag,
                    executable,
                    Duration::from_secs(15),
                    "search_pages",
                    |bytes| {
                        let mut reader = decoder.borrow_mut();
                        let decoded = reader.feed(bytes, |line| {
                            serde_json::from_slice(line).map_err(std::io::Error::other)
                        });
                        if decoded.is_err() {
                            failed.store(true, Ordering::Release);
                            wait.store(NONE, Ordering::Release);
                            published.lock().unwrap_or_else(|e| e.into_inner()).take();
                        }
                        decoded?;
                        if reader.waiting().is_some()
                            && let Some(next) = reader.take()
                        {
                            let sequence = next.sequence;
                            wait.store(sequence, Ordering::Release);
                            let old = published
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .replace(next);
                            if old.is_some() {
                                return Err(std::io::Error::other("Página anterior sin consumir"));
                            }
                        }
                        Ok(())
                    },
                    Some(&mut control),
                )
                .and_then(|_| {
                    let mut reader = decoder.borrow_mut();
                    reader.finish()?;
                    reader
                        .take()
                        .ok_or_else(|| std::io::Error::other("Sin página terminal después del ACK"))
                })
                .map_err(|error| {
                    let mut text = error.to_string();
                    let mut end = text.len().min(512);
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    text.truncate(end);
                    text
                });
                if value.is_err() {
                    failed.store(true, Ordering::Release);
                    published.lock().unwrap_or_else(|e| e.into_inner()).take();
                }
                wait.store(NONE, Ordering::Release);
                let _ = tx.send(value);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            cancel,
            invalid,
            waiting,
            page,
            credit,
            result,
        })
    }
    pub fn invalidated(&self) -> bool {
        self.invalid.load(Ordering::Acquire)
    }
    pub fn take_page(&self) -> Option<Page> {
        self.page.try_lock().ok()?.take()
    }
    pub fn next(&self, sequence: u64) -> bool {
        !self.invalidated()
            && !self.cancel.load(Ordering::Acquire)
            && self.waiting.load(Ordering::Acquire) == sequence
            && self.credit.try_send(sequence).is_ok()
    }
    pub fn poll(&self) -> Option<Result<Page, String>> {
        match self.result.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("Búsqueda paginada sin ACK".into())),
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
