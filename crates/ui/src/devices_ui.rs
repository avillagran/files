//! Sidebar devices: a watcher thread re-lists drives only when the mount
//! table or the set of devices changes (no polling), and mount / unmount /
//! eject run one at a time in their own thread.
use crate::model::{Action, Model};
use files_core::devices::{Device, Op};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

/// Free space is refreshed this often even without events (cheap: one lsblk).
const REFRESH_MS: i32 = 120_000;

/// Mount point (Mount) or nothing, or why it failed.
type JobResult = Result<Option<PathBuf>, String>;

#[derive(Default)]
pub struct State {
    pub list: Vec<Device>,
    updates: Option<Receiver<Vec<Device>>>,
    job: Option<(Op, PathBuf, Receiver<JobResult>)>,
    pub notice: Option<String>,
}
impl State {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn busy_node(&self) -> Option<&PathBuf> {
        self.job.as_ref().map(|(_, n, _)| n)
    }
}

/// The device whose mount contains `path`.
pub fn mounted_at<'a>(list: &'a [Device], path: &std::path::Path) -> Option<&'a Device> {
    list.iter()
        .filter(|d| d.mount.as_deref().is_some_and(|m| path.starts_with(m)))
        .max_by_key(|d| d.mount.as_ref().map_or(0, |m| m.as_os_str().len()))
}

impl Model {
    /// Starts the watcher (once).
    pub fn start_devices(&mut self) {
        if self.devices.updates.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("files-devices".into())
            .spawn(move || {
                #[cfg(target_os = "linux")]
                let mut watch = files_core::devices::Watch::new().ok();
                loop {
                    if let Ok(list) = files_core::devices::scan()
                        && tx.send(list).is_err()
                    {
                        return;
                    }
                    #[cfg(target_os = "linux")]
                    {
                        match watch.as_mut() {
                            Some(w) => {
                                if w.wait(REFRESH_MS) {
                                    // Let a burst (partition table, automount) settle.
                                    std::thread::sleep(std::time::Duration::from_millis(400));
                                    while w.wait(0) {}
                                }
                            }
                            None => std::thread::sleep(std::time::Duration::from_millis(
                                REFRESH_MS as u64,
                            )),
                        }
                    }
                    #[cfg(not(target_os = "linux"))]
                    std::thread::sleep(std::time::Duration::from_millis(REFRESH_MS as u64));
                }
            });
        if spawned.is_ok() {
            self.devices.updates = Some(rx);
        }
    }
    pub fn device_action(&mut self, node: PathBuf, op: Option<Op>) {
        let Some(device) = self.devices.list.iter().find(|d| d.node == node).cloned() else {
            return;
        };
        // Plain click: open it (mounting first when needed).
        let op = match op {
            Some(op) => op,
            None => match &device.mount {
                Some(m) => {
                    let m = m.clone();
                    self.navigate(m);
                    return;
                }
                None => Op::Mount,
            },
        };
        if self.devices.busy() {
            self.devices.notice = Some("Espera la operación de dispositivo en curso".into());
            return;
        }
        // Leave the device before unmounting it.
        if matches!(op, Op::Unmount | Op::Eject)
            && let Some(m) = &device.mount
            && self.path.starts_with(m)
        {
            self.navigate(files_core::theme::home());
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let d = device.clone();
        let _ = std::thread::Builder::new()
            .name("files-udisks".into())
            .spawn(move || {
                let _ = tx.send(files_core::devices::run(op, &d));
            });
        self.operation_message = Some(match op {
            Op::Mount => format!("Montando {}…", device.title()),
            Op::Unmount => format!("Desmontando {}…", device.title()),
            Op::Eject => format!("Expulsando {}…", device.title()),
        });
        self.devices.job = Some((op, device.node.clone(), rx));
    }
    pub fn pump_devices(&mut self) -> bool {
        let mut dirty = false;
        if let Some(rx) = &self.devices.updates {
            let mut latest = None;
            while let Ok(list) = rx.try_recv() {
                latest = Some(list);
            }
            if let Some(list) = latest
                && list != self.devices.list
            {
                self.devices.list = list;
                dirty = true;
            }
        }
        if let Some((op, node, rx)) = &self.devices.job
            && let Ok(result) = rx.try_recv()
        {
            let (op, node) = (*op, node.clone());
            self.devices.job = None;
            let title = self
                .devices
                .list
                .iter()
                .find(|d| d.node == node)
                .map(|d| d.title())
                .unwrap_or_else(|| node.to_string_lossy().into_owned());
            let text = match (&result, op) {
                (Ok(Some(path)), Op::Mount) => {
                    let path = path.clone();
                    self.navigate(path);
                    format!("{title} montado")
                }
                (Ok(_), Op::Mount) => format!("{title} montado"),
                (Ok(_), Op::Unmount) => format!("{title} desmontado"),
                (Ok(_), Op::Eject) => format!("{title} expulsado: ya puedes retirarlo"),
                (Err(e), _) => format!("{title}: {e}"),
            };
            eprintln!(
                "FILES_DEVICE_DONE op={op:?} node={} result={text}",
                node.display()
            );
            self.operation_message = Some(text);
            dirty = true;
        }
        dirty
    }
}

/// Context-menu entries for the device that holds the current folder.
pub fn current_device_actions(m: &Model) -> Option<(Action, Action, bool)> {
    let d = mounted_at(&m.devices.list, &m.path)?;
    Some((
        Action::Device(d.node.clone(), Some(Op::Unmount)),
        Action::Device(d.node.clone(), Some(Op::Eject)),
        d.removable,
    ))
}
