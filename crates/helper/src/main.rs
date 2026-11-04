mod archive_preview;
mod checksum;
mod directory_compare;
mod directory_sync;
mod du;
mod duplicates;
mod image_preview;
mod jpeg_scaled;
mod office_preview;
mod png_rows;
#[cfg(test)]
mod relative_resource;
mod search;
#[cfg(target_os = "linux")]
mod search_content;
mod space_scan;
mod terminal_launch;
mod text_preview;
mod zip_member;
use files_core::protocol::{MAX_LINE_BYTES, Request, Response};
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

fn read_request(input: impl Read) -> io::Result<Vec<u8>> {
    read_request_mode(input, false)
}
fn read_request_mode(mut input: impl Read, strict: bool) -> io::Result<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut line = Vec::new();
    loop {
        #[cfg(target_os = "linux")]
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Tiempo de petición agotado",
                ));
            }
            let mut fd = libc::pollfd {
                fd: 0,
                events: libc::POLLIN,
                revents: 0,
            };
            let result = unsafe {
                libc::poll(
                    &mut fd,
                    1,
                    remaining.as_millis().min(i32::MAX as u128) as i32,
                )
            };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if result == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Tiempo de petición agotado",
                ));
            }
        }
        let mut chunk = [0u8; 4096];
        let count = input.read(&mut chunk)?;
        if count == 0 {
            return Ok(line);
        }
        let end = chunk[..count]
            .iter()
            .position(|b| *b == b'\n')
            .map(|i| i + 1);
        if strict && end.is_some_and(|end| end < count) {
            return Err(io::Error::other(
                "Espera una página antes de enviar crédito",
            ));
        }
        let data = &chunk[..end.unwrap_or(count)];
        if line.len() + data.len() > MAX_LINE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Petición demasiado grande",
            ));
        }
        line.extend_from_slice(data);
        if end.is_some() {
            return Ok(line);
        }
    }
}

fn run() -> io::Result<()> {
    let mut args = std::env::args_os();
    let _ = args.next();
    let command = args.next().unwrap_or_default();
    if command == "terminal" {
        let encoded = args.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Terminal: ruta hexadecimal requerida",
            )
        })?;
        if args.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Terminal: argumentos adicionales rechazados",
            ));
        }
        return terminal_launch::run(encoded.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Terminal: hexadecimal ASCII requerido",
            )
        })?);
    }
    let command = command
        .into_string()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Comando no UTF8"))?;
    if !matches!(
        command.as_str(),
        "space_map"
            | "hash_files"
            | "find_duplicates"
            | "compare_directories"
            | "prepare_directory_sync"
            | "search"
            | "search_stream"
            | "search_batches"
            | "search_pages"
            | "measure_directory"
            | "measure_directory_allocated"
            | "du"
            | "preview_office"
            | "preview_archive"
            | "preview_directory"
            | "preview_text"
            | "preview_relative_text"
            | "preview_thumbnail"
            | "preview_image_checked"
            | "preview_image"
            | "preview_relative_image"
            | "probe_relative_file"
            | "preview_relative_image_checked"
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Uso: files-helper space_map|du|preview_archive|preview_directory|preview_text|preview_relative_text|preview_image|preview_relative_image (una petición JSON por proceso)",
        ));
    }
    #[cfg(target_os = "linux")]
    {
        let limit = libc::rlimit {
            rlim_cur: 96 * 1024 * 1024,
            rlim_max: 96 * 1024 * 1024,
        };
        if unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    // Use an unbuffered duplicate: polling stdin while a Stdin buffer already
    // holds bytes could incorrectly wait for more data from the parent.
    #[cfg(target_os = "linux")]
    let mut input = {
        use std::os::fd::BorrowedFd;
        std::fs::File::from(unsafe { BorrowedFd::borrow_raw(0) }.try_clone_to_owned()?)
    };
    #[cfg(not(target_os = "linux"))]
    let mut input = io::stdin();
    let line = if command == "search_pages" {
        read_request_mode(&mut input, true)?
    } else {
        read_request(&mut input)?
    };
    if line.is_empty() {
        return Ok(());
    }
    let request: Request = serde_json::from_slice(&line)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    if !matches!(
        (&request, command.as_str()),
        (Request::SpaceMap { .. }, "space_map")
            | (Request::HashFiles { .. }, "hash_files")
            | (Request::FindDuplicates { .. }, "find_duplicates")
            | (Request::CompareDirectories { .. }, "compare_directories")
            | (
                Request::PrepareDirectorySync { .. },
                "prepare_directory_sync"
            )
            | (Request::Search { .. }, "search")
            | (Request::SearchStream { .. }, "search_stream")
            | (Request::SearchBatches { .. }, "search_batches")
            | (Request::SearchPages { .. }, "search_pages")
            | (Request::MeasureDirectory { .. }, "measure_directory")
            | (
                Request::MeasureDirectoryAllocated { .. },
                "measure_directory_allocated"
            )
            | (Request::Du { .. }, "du")
            | (Request::PreviewOffice { .. }, "preview_office")
            | (Request::PreviewArchive { .. }, "preview_archive")
            | (Request::PreviewDirectory { .. }, "preview_directory")
            | (Request::PreviewText { .. }, "preview_text")
            | (Request::PreviewRelativeText { .. }, "preview_relative_text")
            | (Request::ProbeRelativeFile { .. }, "probe_relative_file")
            | (
                Request::PreviewRelativeImageChecked { .. },
                "preview_relative_image_checked"
            )
            | (Request::PreviewThumbnail { .. }, "preview_thumbnail")
            | (Request::PreviewImageChecked { .. }, "preview_image_checked")
            | (Request::PreviewImage { .. }, "preview_image")
            | (
                Request::PreviewRelativeImage { .. },
                "preview_relative_image"
            )
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "El comando no coincide con la petición",
        ));
    }
    if let Request::PrepareDirectorySync { intent } = &request {
        return directory_sync::run(intent, io::stdout().lock());
    }
    if let Request::CompareDirectories {
        id,
        left,
        right,
        expected,
        read_limit,
    } = &request
    {
        return directory_compare::run(
            *id,
            left,
            right,
            *expected,
            *read_limit,
            io::stdout().lock(),
        );
    }
    if let Request::FindDuplicates { id, root } = &request {
        return duplicates::run(*id, root, io::stdout().lock());
    }
    if let Request::HashFiles { id, root, names } = &request {
        return checksum::run(*id, root, names, io::stdout().lock());
    }
    if let Request::SearchPages {
        id,
        path,
        query,
        hidden,
    } = &request
    {
        let mut output = io::stdout().lock();
        return search::scan_pages(
            path,
            query,
            *hidden,
            search::Limits::default(),
            |sequence, found_total, more, report| {
                serde_json::to_writer(
                    &mut output,
                    &files_core::search_page::PageRef {
                        version: 1,
                        id: *id,
                        sequence,
                        found_total,
                        more,
                        report,
                    },
                )?;
                output.write_all(b"\n")?;
                output.flush()?;
                if more {
                    files_core::search_page::Next::read(&mut input, *id, sequence, |bytes| {
                        serde_json::from_slice(bytes).map_err(io::Error::other)
                    })?;
                }
                Ok(())
            },
        );
    }
    let batches_search = matches!(&request, Request::SearchBatches { .. });
    let streaming_search = matches!(&request, Request::SearchStream { .. });
    let mut pixels = None;
    let response = match request {
        Request::MeasureDirectoryAllocated { id, version, path } => {
            if version != files_core::folder_usage::VERSION || id == 0 {
                Response::Error {
                    id,
                    message: "Versión o identidad de cálculo inválida".into(),
                }
            } else {
                let mut out = io::stdout().lock();
                let result =
                    du::measure_allocated_progress(&path, du::Limits::default(), |usage| {
                        serde_json::to_writer(
                            &mut out,
                            &Response::UsageProgress {
                                id,
                                bytes: usage.bytes,
                                entries: usage.entries,
                            },
                        )?;
                        out.write_all(b"\n")?;
                        out.flush()
                    });
                match result {
                    Ok((usage, allocation)) => Response::AllocatedUsage {
                        id,
                        report: files_core::folder_usage::Report {
                            version: files_core::folder_usage::VERSION,
                            bytes: usage.bytes,
                            entries: usage.entries,
                            allocation,
                        },
                    },
                    Err(e) if e.kind() == io::ErrorKind::Unsupported => Response::Unsupported {
                        id,
                        message: e.to_string(),
                    },
                    Err(e) => Response::Error {
                        id,
                        message: e.to_string(),
                    },
                }
            }
        }
        Request::FindDuplicates { .. } => unreachable!("duplicates returned above"),
        Request::CompareDirectories { .. } => unreachable!("comparison returned above"),
        Request::PrepareDirectorySync { .. } => unreachable!("sync review returned above"),
        Request::HashFiles { .. } => unreachable!("checksum returned above"),
        Request::SearchPages { .. } => unreachable!("paged command returned above"),
        Request::MeasureDirectory { id, path } => {
            let mut out = io::stdout().lock();
            let result = du::measure_progress(&path, du::Limits::default(), |usage| {
                serde_json::to_writer(
                    &mut out,
                    &Response::UsageProgress {
                        id,
                        bytes: usage.bytes,
                        entries: usage.entries,
                    },
                )?;
                out.write_all(b"\n")?;
                out.flush()
            });
            match result {
                Ok(usage) => Response::Usage {
                    id,
                    bytes: usage.bytes,
                    entries: usage.entries,
                },
                Err(e) => Response::Error {
                    id,
                    message: e.to_string(),
                },
            }
        }
        Request::Search {
            id,
            path,
            query,
            hidden,
        }
        | Request::SearchStream {
            id,
            path,
            query,
            hidden,
        }
        | Request::SearchBatches {
            id,
            path,
            query,
            hidden,
        } => {
            let result = if batches_search {
                let mut out = io::stdout().lock();
                let mut sent = 0;
                let mut held = false;
                search::scan_updates(
                    &path,
                    &query,
                    hidden,
                    search::Limits::default(),
                    true,
                    |progress, hits| {
                        serde_json::to_writer(
                            &mut out,
                            &Response::SearchProgress { id, progress },
                        )?;
                        out.write_all(b"\n")?;
                        for chunk in hits[sent..].chunks(files_core::search_batch::MAX_BATCH) {
                            serde_json::to_writer(
                                &mut out,
                                &files_core::search_batch::Frame::new(id, sent, chunk),
                            )?;
                            out.write_all(b"\n")?;
                            sent += chunk.len();
                        }
                        out.flush()?;
                        // Deterministic owned-fixture hold, never enabled in normal runs.
                        if !held
                            && sent > 0
                            && [
                                "FILES_TEST_HIDDEN_CONTEXT",
                                "FILES_TEST_ISOLATE_INPUT",
                                "FILES_TEST_SEARCH_HOLD_FIRST_BATCH",
                            ]
                            .iter()
                            .all(|k| std::env::var(k).as_deref() == Ok("1"))
                        {
                            held = true;
                            eprintln!(
                                "FILES_SEARCH_BATCH_HELD pid={} count={sent}",
                                std::process::id()
                            );
                            let until = Instant::now() + Duration::from_secs(5);
                            while path.join(".files-search-gate").exists() && Instant::now() < until
                            {
                                std::thread::sleep(Duration::from_millis(10));
                            }
                        }
                        Ok(())
                    },
                )
            } else if streaming_search {
                let mut out = io::stdout().lock();
                search::scan_progress(
                    &path,
                    &query,
                    hidden,
                    search::Limits::default(),
                    |progress| {
                        serde_json::to_writer(
                            &mut out,
                            &Response::SearchProgress { id, progress },
                        )?;
                        out.write_all(b"\n")?;
                        out.flush()
                    },
                )
            } else {
                search::scan(&path, &query, hidden, search::Limits::default())
            };
            match result {
                Ok(report) => Response::Search { id, report },
                Err(error) => Response::Error {
                    id,
                    message: error.to_string(),
                },
            }
        }
        Request::SpaceMap { id, path } => match space_scan::scan(&path).and_then(|report| {
            files_core::space_map::validate_report(&report)?;
            Ok(report)
        }) {
            Ok(report) => Response::SpaceMap { id, report },
            Err(message) => Response::Error { id, message },
        },
        Request::ProbeRelativeFile {
            id,
            root,
            relative,
            expected_root,
        } => {
            match files_core::relative_source::Opened::open(&root, &relative).and_then(|opened| {
                if opened.proof.root != expected_root {
                    return Err(io::Error::other("La raíz del documento fue reemplazada"));
                }
                opened.revalidate()?;
                Ok(opened.proof)
            }) {
                Ok(proof) => Response::RelativeSource { id, proof },
                Err(error) => Response::Error {
                    id,
                    message: error.to_string(),
                },
            }
        }
        Request::PreviewRelativeImageChecked {
            id,
            root,
            relative,
            expected,
        } => match image_preview::load_relative_expected(&root, &relative, Some(expected)) {
            Ok(preview) => {
                let response = Response::Image {
                    id,
                    width: preview.width,
                    height: preview.height,
                    byte_len: preview.rgb.len(),
                    source_width: preview.source_width,
                    source_height: preview.source_height,
                    format: preview.format.into(),
                    source: preview.source,
                    relative_source: preview.relative_source,
                };
                pixels = Some(preview.rgb);
                response
            }
            Err(error) => Response::Error {
                id,
                message: error.to_string(),
            },
        },
        Request::PreviewRelativeText { id, root, relative } => {
            match text_preview::load_relative(&root, &relative) {
                Ok(preview) => Response::Text {
                    id,
                    text: preview.text,
                    encoding: preview.encoding.into(),
                    truncated: preview.truncated,
                    relative_source: preview.relative_source,
                },
                Err(error) => Response::Error {
                    id,
                    message: error.to_string(),
                },
            }
        }
        Request::PreviewRelativeImage { id, root, relative } => {
            match image_preview::load_relative(&root, &relative) {
                Ok(preview) => {
                    let response = Response::Image {
                        id,
                        width: preview.width,
                        height: preview.height,
                        byte_len: preview.rgb.len(),
                        source_width: preview.source_width,
                        source_height: preview.source_height,
                        format: preview.format.into(),
                        source: preview.source,
                        relative_source: preview.relative_source,
                    };
                    pixels = Some(preview.rgb);
                    response
                }
                Err(error) => Response::Error {
                    id,
                    message: error.to_string(),
                },
            }
        }
        Request::PreviewThumbnail {
            id,
            raw_path,
            revision,
            max_dimension,
        } => {
            match files_core::media_source::raw_path(raw_path)
                .and_then(|path| image_preview::thumbnail(&path, revision, max_dimension))
            {
                Ok(preview) => {
                    let response = Response::Image {
                        id,
                        width: preview.width,
                        height: preview.height,
                        byte_len: preview.rgb.len(),
                        source_width: preview.source_width,
                        source_height: preview.source_height,
                        format: preview.format.into(),
                        source: preview.source,
                        relative_source: preview.relative_source,
                    };
                    pixels = Some(preview.rgb);
                    response
                }
                Err(error) => Response::Error {
                    id,
                    message: error.to_string(),
                },
            }
        }
        Request::PreviewImageChecked { id, path, revision } => {
            match image_preview::load_checked(&path, revision) {
                Ok(preview) => {
                    let response = Response::Image {
                        id,
                        width: preview.width,
                        height: preview.height,
                        byte_len: preview.rgb.len(),
                        source_width: preview.source_width,
                        source_height: preview.source_height,
                        format: preview.format.into(),
                        source: preview.source,
                        relative_source: preview.relative_source,
                    };
                    pixels = Some(preview.rgb);
                    response
                }
                Err(error) => Response::Error {
                    id,
                    message: error.to_string(),
                },
            }
        }
        Request::PreviewImage { id, path } => match image_preview::load(&path) {
            Ok(preview) => {
                let response = Response::Image {
                    id,
                    width: preview.width,
                    height: preview.height,
                    byte_len: preview.rgb.len(),
                    source_width: preview.source_width,
                    source_height: preview.source_height,
                    format: preview.format.into(),
                    source: preview.source,
                    relative_source: preview.relative_source,
                };
                pixels = Some(preview.rgb);
                response
            }
            Err(error) => Response::Error {
                id,
                message: error.to_string(),
            },
        },
        Request::PreviewOffice { id, path, revision } => {
            match office_preview::load(&path, revision) {
                Ok(p) => Response::OfficeText {
                    id,
                    text: p.text,
                    truncated: p.truncated,
                    source: p.source,
                },
                Err(e) => Response::Error {
                    id,
                    message: e.to_string(),
                },
            }
        }
        Request::PreviewArchive { id, path } => match archive_preview::load(&path) {
            Ok(p) => Response::Text {
                id,
                text: p.text,
                encoding: p.encoding.into(),
                truncated: p.truncated,
                relative_source: None,
            },
            Err(e) => Response::Error {
                id,
                message: e.to_string(),
            },
        },
        Request::PreviewDirectory { id, path } => {
            match du::measure_directory(&path, du::Limits::default()) {
                Ok(usage) => Response::Usage {
                    id,
                    bytes: usage.bytes,
                    entries: usage.entries,
                },
                Err(error) => Response::Error {
                    id,
                    message: error.to_string(),
                },
            }
        }
        Request::Du { id, path } => match du::measure(&path, du::Limits::default()) {
            Ok(usage) => Response::Usage {
                id,
                bytes: usage.bytes,
                entries: usage.entries,
            },
            Err(error) => Response::Error {
                id,
                message: error.to_string(),
            },
        },
        Request::PreviewText { id, path } => match text_preview::load(&path) {
            Ok(preview) => Response::Text {
                id,
                text: preview.text,
                encoding: preview.encoding.into(),
                truncated: preview.truncated,
                relative_source: preview.relative_source,
            },
            Err(error) => Response::Error {
                id,
                message: error.to_string(),
            },
        },
    };
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, &response)?;
    writeln!(output)?;
    if let Some(pixels) = pixels {
        output.write_all(&pixels)?;
    }
    output.flush()
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
