//! Explicit fixture-only actions; actual UI/controller methods, no native input.
use crate::model::{Action, Model};
use std::path::Path;
pub fn command(m: &mut Model, root: &Path, command: &str) -> Result<(), String> {
    if command.starts_with("move-resize-") {
        return Ok(());
    }
    if command.starts_with("create-") {
        let destination = match command {
            "create-cancel-form" => "cancelled.zip",
            "create-stale-form" => "stale.zip",
            "create-close-form" => "close.zip",
            _ => "bundle.zip",
        };
        if !m.loaded || m.selection_refreshing() {
            return Err("listing not ready".into());
        }
        if matches!(
            command,
            "create-form" | "create-cancel-form" | "create-stale-form" | "create-close-form"
        ) {
            let names: [&[u8]; 4] = [b"Pack", b"zero", b"raw-\xff", b"link"];
            for (n, name) in names.iter().enumerate() {
                let i = m
                    .entries
                    .iter()
                    .position(|e| e.name.as_encoded_bytes() == *name)
                    .ok_or("create fixture entry absent")?;
                m.select_entry(i, n > 0, false);
            }
            m.action(Action::CreateArchive);
            let d = m
                .copy_form
                .as_mut()
                .filter(|d| d.creating)
                .ok_or("create form absent")?;
            d.capture_destination(root.join("target").join(destination));
            d.start_paused = true;
            return Ok(());
        }
        return Err("unknown create fixture".into());
    }
    if command.starts_with("extract-") {
        return extract_command(m, root, command);
    }
    match command {
        "move-exdev-form" => {
            if !m.loaded || m.selection_refreshing() {
                return Err("listing not ready".into());
            }
            let i = m
                .entries
                .iter()
                .position(|e| e.name == "item")
                .ok_or("EXDEV item fixture absent")?;
            m.select_entry(i, false, false);
            m.action(Action::MoveFiles);
            let form = m.copy_form.as_mut().ok_or("move form absent")?;
            form.destination = std::env::var("FILES_TEST_MOVE_DESTINATION")
                .map_err(|_| "private destination absent")?;
            form.replace = false;
            form.start_paused = true;
            return Ok(());
        }
        "move-exdev-target-tab" => {
            m.action(Action::Tab(crate::tabs::Command::New));
            m.action(Action::Navigate(
                std::env::var_os("FILES_TEST_MOVE_DESTINATION")
                    .ok_or("private destination absent")?
                    .into(),
            ));
            return Ok(());
        }
        "move-recovery-list" => {
            m.action(Action::RecoveryCatalog);
            return Ok(());
        }
        "move-recovery-select" => {
            m.action(Action::RecoverySelect(0));
            return Ok(());
        }
        "move-recovery-finish" => {
            m.action(Action::RecoverMove(
                crate::move_operation::RecoveryAction::Finish,
            ));
            return Ok(());
        }
        "move-recovery-restore" => {
            m.action(Action::RecoverMove(
                crate::move_operation::RecoveryAction::Restore,
            ));
            return Ok(());
        }
        _ => (),
    }
    if command.starts_with("move-") {
        match command {
            "move-form" | "move-collision" | "move-exdev" | "move-cancel-form"
            | "move-close-form" => {
                if !m.loaded || m.selection_refreshing() {
                    return Err("listing not ready".into());
                }
                let names: Vec<&[u8]> = match command {
                    "move-form" => vec![b"payload.bin", b"Nested", b"link.txt", b"raw-\xff.txt"],
                    "move-cancel-form" => vec![b"cancel.txt"],
                    "move-close-form" => vec![b"close.txt"],
                    _ => vec![b"collision.txt"],
                };
                for (n, name) in names.iter().enumerate() {
                    let i = m
                        .entries
                        .iter()
                        .position(|e| e.name.as_encoded_bytes() == *name)
                        .ok_or("move fixture absent")?;
                    m.select_entry(i, n != 0, false);
                }
                m.action(Action::MoveFiles);
                let form = m.copy_form.as_mut().ok_or("move form absent")?;
                form.destination = if command == "move-exdev" {
                    "/proc".into()
                } else {
                    root.join(match command {
                        "move-cancel-form" => "queued",
                        "move-close-form" => "close",
                        _ => "target",
                    })
                    .to_str()
                    .ok_or("fixture path not UTF8")?
                    .into()
                };
                form.replace = false;
                form.start_paused = matches!(
                    command,
                    "move-form" | "move-cancel-form" | "move-close-form"
                );
            }
            "move-review" => m.action(Action::CopyStart),
            "move-confirm" => {
                if !m.copy_form.as_ref().is_some_and(|d| d.prepared.is_some()) {
                    return Err("review not ready".into());
                }
                m.action(Action::CopyStart);
                if m.copy_form.is_some() {
                    return Err("move queue rejected".into());
                }
            }
            "move-dialog-cancel" => m.action(Action::CopyDialogCancel),
            "move-target-tab" => {
                m.action(Action::Tab(crate::tabs::Command::New));
                m.action(Action::Navigate(root.join("target")));
            }
            "move-source-tab" => {
                m.action(Action::Tab(crate::tabs::Command::Select(m.tabs.rows[0].id)))
            }
            "move-focus-untouched" => {
                let i = m
                    .entries
                    .iter()
                    .position(|e| e.name == "untouched.txt")
                    .ok_or("target untouched absent")?;
                m.select_entry(i, false, false);
            }
            _ => return Err("unknown move fixture action".into()),
        }
        return Ok(());
    }
    match command {
        "form" | "extra" | "collision" | "close-form" | "cancel-form" => {
            if !m.loaded || m.selection_refreshing() {
                return Err("listing not ready".into());
            }
            let names: &[&str] = match command {
                "form" => &["payload.bin", "Nested", "link.txt"],
                "extra" => &["cancel.txt"],
                "cancel-form" => &["Nested"],
                _ => &["payload.bin"],
            };
            for (n, name) in names.iter().enumerate() {
                let i = m
                    .entries
                    .iter()
                    .position(|e| e.name == *name)
                    .ok_or("fixture source absent")?;
                m.select_entry(i, n != 0, false);
            }
            m.action(Action::CopyFiles);
            let form = m.copy_form.as_mut().ok_or("copy form absent")?;
            form.destination = root
                .join(match command {
                    "extra" => "queued",
                    "close-form" => "close",
                    "cancel-form" => "cancelled",
                    _ => "target",
                })
                .to_str()
                .ok_or("fixture path not UTF8")?
                .into();
            form.replace = false;
            form.start_paused = command == "form" || command == "close-form";
        }
        "enqueue" => {
            m.action(Action::CopyStart);
            if m.copy_form.is_some() {
                return Err("copy was rejected".into());
            }
        }
        "resume" => {
            let id = m.copy.active_id().ok_or("no active copy")?;
            m.action(Action::CopyPause(id, false));
        }
        "cancel-active" => {
            let id = m.copy.active_id().ok_or("no active copy")?;
            m.action(Action::CopyCancel(id));
        }
        "cancel-pending" => m.action(Action::CopyClearPending),
        "target" => m.navigate(root.join("target")),
        "source" => m.navigate(root.join("source")),
        "continue" => m.action(Action::DismissNotice),
        "close" => {
            m.closing = true;
            m.mutations.cancel();
        }
        _ => return Err("unknown copy fixture action".into()),
    }
    Ok(())
}

// Explicit owned-fixture extension of the existing copy/move scene.
fn extract_command(m: &mut Model, root: &Path, command: &str) -> Result<(), String> {
    if let Some(value) = command.strip_prefix("extract-old-confirm:") {
        let stamp = value.parse::<u64>().map_err(|_| "invalid stamp")?;
        m.action(Action::ArchiveCommit(stamp));
        return Ok(());
    }
    match command {
        "extract-form"
        | "extract-close-form"
        | "extract-cleanup-seed"
        | "extract-sync-seed"
        | "extract-corrupt"
        | "extract-unsafe" => {
            if !m.loaded || m.selection_refreshing() {
                return Err("listing not ready".into());
            }
            let name = match command {
                "extract-corrupt" => "corrupt.zip",
                "extract-unsafe" => "unsafe.zip",
                _ => "source.zip",
            };
            let i = m
                .entries
                .iter()
                .position(|e| e.name == name)
                .ok_or("ZIP fixture absent")?;
            m.select_entry(i, false, false);
            m.action(Action::ExtractArchive);
            let d = m
                .copy_form
                .as_mut()
                .filter(|d| d.extracting)
                .ok_or("extraction form absent")?;
            d.capture_destination(
                root.join("target")
                    .join(if command == "extract-cleanup-seed" {
                        "cleanup-output"
                    } else if command == "extract-sync-seed" {
                        "sync-output"
                    } else if command == "extract-close-form" {
                        "close"
                    } else if command == "extract-corrupt" {
                        "bad"
                    } else if command == "extract-unsafe" {
                        "unsafe"
                    } else {
                        "unpacked"
                    }),
            );
            d.start_paused = true;
        }
        "extract-fixture-pause" | "extract-fixture-sync" => {
            let prepared = m
                .copy_form
                .as_mut()
                .and_then(|d| d.archive.as_mut())
                .ok_or("prepared ZIP required")?;
            prepared.enable_fixture(if command == "extract-fixture-pause" {
                crate::archive_operation::Fixture::PauseAfterProgress
            } else {
                crate::archive_operation::Fixture::FailPublishNotice
            })?;
        }
        "extract-cleanup-review" | "extract-sync-review" => {
            let id = m.copy_notice_id.ok_or("ZIP notice absent")?;
            let action = if command == "extract-cleanup-review" {
                crate::archive_operation::RecoveryAction::Cleanup
            } else {
                crate::archive_operation::RecoveryAction::Sync
            };
            let hit=m.hits.iter().find(|h|matches!(h.action,Action::ArchiveReview(current,a) if current==id && a==action)).ok_or("painted recovery review absent")?;
            let r = hit.rect;
            m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, false);
            m.pointer_up();
        }
        "extract-recovery-pause"
        | "extract-recovery-confirm"
        | "extract-recovery-back"
        | "extract-recovery-clear" => {
            let c = m.archive_confirmation.ok_or("recovery review absent")?;
            let action = match command {
                "extract-recovery-pause" => Action::ArchivePause(c.stamp),
                "extract-recovery-back" => Action::ArchiveBack(c.stamp),
                "extract-recovery-clear" => Action::ArchiveClearPending(c.stamp),
                _ => Action::ArchiveCommit(c.stamp),
            };
            let r=m.hits.iter().find(|h|matches!((&h.action,&action),(Action::ArchivePause(a),Action::ArchivePause(b)) | (Action::ArchiveBack(a),Action::ArchiveBack(b)) | (Action::ArchiveCommit(a),Action::ArchiveCommit(b)) | (Action::ArchiveClearPending(a),Action::ArchiveClearPending(b)) if a==b)).map(|h|h.rect).ok_or("painted recovery control absent")?;
            m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, false);
            m.pointer_up();
        }
        "extract-fixture-fill-queue" => {
            if m.copy.active_id().is_some() || !m.notice_open {
                return Err("idle result required".into());
            }
            for i in 0..8 {
                m.copy_id = m.copy_id.checked_add(1).ok_or("id limit")?;
                m.copy
                    .enqueue(crate::copy_job::Request::new(
                        m.copy_id,
                        vec![files_core::copy_engine::CopyRequest {
                            source: root.join("source/source.zip"),
                            destination: root.join("queue").join(format!("pending-{i}")),
                        }],
                        Default::default(),
                    ))
                    .map_err(|e| e.reason.to_string())?;
            }
        }
        "extract-review" | "extract-apply" => {
            let d = m
                .copy_form
                .as_ref()
                .filter(|d| d.extracting)
                .ok_or("extraction form absent")?;
            if (command == "extract-apply") != d.archive.is_some() {
                return Err("unexpected review stage".into());
            }
            let action = Action::ApplyArchive(d.editor.epoch, d.prepare_id);
            let r = m.hits.iter().find(|h| matches!((&h.action,&action), (Action::ApplyArchive(a,b),Action::ApplyArchive(c,d)) if a==c && b==d)).map(|h|h.rect).ok_or("painted extraction Apply absent")?;
            m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, false);
            m.pointer_up();
        }
        "extract-cancel" => m.action(Action::CopyDialogCancel),
        "extract-source" => m.action(Action::Navigate(root.join("source"))),
        "extract-target" => {
            m.action(Action::Tab(crate::tabs::Command::New));
            m.action(Action::Navigate(root.join("target")));
        }
        "extract-details" => m.action(Action::CopyDetails),
        _ => return Err("unknown extraction fixture command".into()),
    }
    Ok(())
}
