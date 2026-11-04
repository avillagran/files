pub mod archive_create;
pub mod archive_extract;
pub mod archive_extract_fs;
pub mod archive_list;
pub mod archive_member;
pub mod bookmarks;
pub mod copy_engine;
pub mod folder_usage;
pub mod folder_views;
pub mod listing;
pub mod media_source;
pub mod mime;
pub mod mount_table;
pub mod multi_origin;
pub mod name_filter;
pub mod natural_sort;
pub mod navigation_history;
pub mod persistence;
pub mod places;
pub mod protocol;
pub mod relative_source;
pub mod rename_plan;
pub mod rename_transaction;
pub mod search;
pub mod search_content;
pub mod search_dates;
pub mod search_regex;
pub mod selection_source;
pub mod sftp_profiles;
pub mod sftp_target;
pub mod spatial;
pub mod system;
pub mod theme;

pub fn size_parts(bytes: u64) -> (String, &'static str) {
    let units = ["B", "kB", "MB", "GB", "TB"];
    let mut n = bytes as f64;
    let mut u = 0;
    while n >= 1000.0 && u < 4 {
        n /= 1000.0;
        u += 1;
    }
    (
        if u == 0 || n >= 100.0 {
            format!("{n:.0}")
        } else {
            format!("{n:.1}")
        },
        units[u],
    )
}
pub fn format_size(bytes: u64) -> String {
    let (n, u) = size_parts(bytes);
    format!("{n} {u}")
}
pub fn relative_date(time: std::time::SystemTime, now: std::time::SystemTime) -> String {
    let s = now.duration_since(time).unwrap_or_default().as_secs();
    match s {
        0..60 => "ahora".into(),
        60..3600 => format!("hace {} min", s / 60),
        3600..86400 => format!("hace {} h", s / 3600),
        86400..172800 => "ayer".into(),
        _ => format!("hace {} d", s / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spanish_units_and_relative_time() {
        use std::time::{Duration, SystemTime};
        assert_eq!(size_parts(774_000), ("774".into(), "kB"));
        assert_eq!(size_parts(1_200_000), ("1.2".into(), "MB"));
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(
            relative_date(now - Duration::from_secs(90_000), now),
            "ayer"
        );
        assert_eq!(
            relative_date(now - Duration::from_secs(3 * 86_400), now),
            "hace 3 d"
        );
        assert_eq!(relative_date(now + Duration::from_secs(60), now), "ahora");
    }
}

/// "Create link" to selected items.
pub mod links;
/// Permanent deletion (Shift+Delete, empty trash).
pub mod remove_tree;
pub mod trash;

pub mod file_properties;

pub mod space_map;

pub mod visibility;

pub mod move_engine;

pub mod move_manifest;

pub mod move_journal;

pub mod exdev_source;

pub mod exdev_move;

pub mod file_transfer;

mod search_predicate;

pub mod search_batch;

pub mod search_snippet;

pub mod copy_unique_name;

pub mod sftp_lifecycle;

pub mod sftp_mount_point;

pub mod sftp_connection;

pub mod sftp_registry;

pub mod sftp_recovered;

pub mod saved_search;

pub mod search_page;

pub mod terminal_directory;

pub mod checksum;

pub mod duplicates;

pub mod name_label;

pub mod directory_compare;

pub mod directory_compare_wire;

pub mod directory_sync;

pub mod directory_sync_wire;

pub mod directory_sync_manifest;

pub mod directory_sync_record;

pub mod directory_sync_plan;

pub mod directory_sync_physical;

pub mod directory_sync_metadata;

pub mod directory_sync_store;

pub mod directory_sync_journal;

pub mod directory_sync_admission;
