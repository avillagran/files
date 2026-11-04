//! Inert unless --attempt. Developer probe of the production core copy engine.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).take(6).collect();
    let visible = args.len() == 4 && args[1] == "--visible-omit-symlink-times";
    if !(args.len() == 3 || visible) || args[0] != "--attempt" {
        println!(
            "Usage: copy-once --attempt [--visible-omit-symlink-times] SOURCE DESTINATION (mutates explicit destination)"
        );
        return Ok(());
    }
    let offset = usize::from(visible);
    let request = files_core::copy_engine::CopyRequest {
        source: args[1 + offset].clone().into(),
        destination: args[2 + offset].clone().into(),
    };
    let options = files_core::copy_engine::CopyOptions {
        publication: if visible {
            files_core::copy_engine::Publication::VisibleExclusive
        } else {
            files_core::copy_engine::Publication::StagedRename
        },
        preserve_symlink_times: !visible,
        ..Default::default()
    };
    let result =
        files_core::copy_engine::copy_paths(&[request], &options, &Default::default(), |_| {});
    match result {
        Ok(r) => println!(
            "{}",
            serde_json::json!({"ok":true,"completed":r.completed_count,"bytes":r.bytes_copied,"publication":format!("{:?}",r.publication),"visible_started":r.visible_started_count,"visible_completed":r.visible_completed_count,"visible_uncertain":r.visible_uncertain_count,"symlink_times_omitted":r.symlink_times_omitted})
        ),
        Err(e) => println!(
            "{}",
            serde_json::json!({"ok":false,"completed":e.report.completed_count,"kind":format!("{:?}",e.cause.kind()),"errno":e.cause.raw_os_error(),"message":e.cause.to_string(),"cleanup_errors":e.cleanup_errors,"rescue_paths":e.rescue_paths.iter().map(|p|p.as_os_str().as_encoded_bytes()).collect::<Vec<_>>()})
        ),
    }
    Ok(())
}
