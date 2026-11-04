//! Drives and partitions for the sidebar (Nautilus "Other locations" /
//! devices): mountable filesystems from `lsblk -J`, their mount points and
//! free space, and mount / unmount / eject through `udisksctl` (the polkit
//! path desktop users have; no root, no direct mount syscalls).
//!
//! The list is refreshed by events, not polling: a watcher waits on
//! `/proc/self/mountinfo` (the kernel flags it on every mount change) and on
//! `/dev/disk/by-uuid` (entries appear and vanish with devices).
use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    /// `/dev/sdb1`.
    pub node: PathBuf,
    pub label: String,
    pub fstype: String,
    pub size: u64,
    pub mount: Option<PathBuf>,
    /// Removable or hot-plugged (USB sticks, SD cards): can be ejected.
    pub removable: bool,
    /// Whole disk to power off after unmounting (`/dev/sdb`).
    pub disk: Option<PathBuf>,
    pub free: Option<u64>,
}

impl Device {
    /// Short sidebar label: the filesystem label, else size and fs.
    pub fn title(&self) -> String {
        if !self.label.is_empty() {
            return self.label.clone();
        }
        let name = self
            .node
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!(
            "{} {}",
            crate::format_size(self.size),
            if self.fstype.is_empty() {
                name
            } else {
                self.fstype.clone()
            }
        )
    }
}

/// Firmware/OS-internal partitions nobody browses (GPT type GUIDs): EFI
/// System, Microsoft reserved, Windows recovery, BIOS boot, Apple Silicon
/// boot and recovery.
const HIDDEN_PARTTYPES: [&str; 6] = [
    "c12a7328-f81f-11d2-ba4b-00a0c93ec93b",
    "e3c9e316-0b5c-4db8-817d-f92df00215ae",
    "de94bba4-06d1-4d40-a16a-bfd50179d6ac",
    "21686148-6449-6e6f-744e-656564454649",
    "69646961-6700-11aa-aa11-00306543ecac",
    "52637672-7900-11aa-aa11-00306543ecac",
];
/// Filesystems the running kernel can mount (plus FUSE-backed ones udisks
/// handles through helpers). Unknown kernels: everything is allowed.
fn mountable(fstype: &str, kernel: Option<&str>) -> bool {
    let Some(list) = kernel else { return true };
    matches!(fstype, "ntfs" | "exfat")
        || list
            .lines()
            .any(|l| l.split_whitespace().last() == Some(fstype))
}
/// Mount points of the system itself: never listed as devices.
fn system_mount(p: &Path) -> bool {
    let s = p.to_string_lossy();
    matches!(
        s.as_ref(),
        "/" | "/boot" | "/efi" | "/home" | "/usr" | "/var" | "/nix" | "/opt" | "/srv" | "/tmp"
    ) || s.starts_with("/boot/")
        || s.starts_with("/var/")
        || s.starts_with("/.snapshots")
        || s.starts_with("/swap")
}

fn s(v: &serde_json::Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}
fn flag(v: &serde_json::Value, k: &str) -> bool {
    match v.get(k) {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(t)) => t == "1" || t == "true",
        Some(serde_json::Value::Number(n)) => n.as_u64() == Some(1),
        _ => false,
    }
}
fn num(v: &serde_json::Value, k: &str) -> u64 {
    match v.get(k) {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(serde_json::Value::String(t)) => t.parse().unwrap_or(0),
        _ => 0,
    }
}
fn mountpoints(v: &serde_json::Value) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(list) = v.get("mountpoints").and_then(|m| m.as_array()) {
        out.extend(list.iter().filter_map(|m| m.as_str()).map(PathBuf::from));
    }
    if let Some(m) = v.get("mountpoint").and_then(|m| m.as_str()) {
        out.push(PathBuf::from(m));
    }
    out
}

/// Devices from `lsblk -J -b -o NAME,PATH,LABEL,FSTYPE,SIZE,MOUNTPOINTS,RM,HOTPLUG,TYPE`.
pub fn parse_lsblk(json: &[u8]) -> io::Result<Vec<Device>> {
    let kernel = std::fs::read_to_string("/proc/filesystems").ok();
    parse_lsblk_with(json, kernel.as_deref())
}
pub fn parse_lsblk_with(json: &[u8], kernel: Option<&str>) -> io::Result<Vec<Device>> {
    let root: serde_json::Value = serde_json::from_slice(json).map_err(io::Error::other)?;
    let mut out = Vec::new();
    fn walk(
        v: &serde_json::Value,
        disk: Option<(PathBuf, bool)>,
        kernel: Option<&str>,
        out: &mut Vec<Device>,
    ) {
        let kind = s(v, "type");
        let node = PathBuf::from(s(v, "path"));
        let removable = flag(v, "rm") || flag(v, "hotplug") || disk.as_ref().is_some_and(|d| d.1);
        let this_disk = if kind == "disk" {
            Some((node.clone(), removable))
        } else {
            disk.clone()
        };
        let fstype = s(v, "fstype");
        let points = mountpoints(v);
        let parttype = s(v, "parttype").to_ascii_lowercase();
        let mounted = points.iter().any(|p| !p.as_os_str().is_empty());
        let usable = !fstype.is_empty()
            && !HIDDEN_PARTTYPES.contains(&parttype.as_str())
            && (mounted || mountable(&fstype, kernel))
            && !matches!(
                fstype.as_str(),
                "swap" | "crypto_LUKS" | "LVM2_member" | "linux_raid_member" | "zfs_member"
            )
            && !points
                .iter()
                .any(|p| system_mount(p) || p.as_os_str() == "[SWAP]")
            && node.is_absolute()
            && out.len() < 64;
        if usable {
            out.push(Device {
                node: node.clone(),
                label: s(v, "label"),
                fstype,
                size: num(v, "size"),
                mount: points.into_iter().next(),
                removable,
                disk: this_disk
                    .as_ref()
                    .map(|d| d.0.clone())
                    .filter(|d| *d != node),
                free: None,
            });
        }
        if let Some(children) = v.get("children").and_then(|c| c.as_array()) {
            for c in children {
                walk(c, this_disk.clone(), kernel, out);
            }
        }
    }
    for d in root
        .get("blockdevices")
        .and_then(|b| b.as_array())
        .into_iter()
        .flatten()
    {
        walk(d, None, kernel, &mut out);
    }
    Ok(out)
}

/// Current devices with free space of the mounted ones.
pub fn scan() -> io::Result<Vec<Device>> {
    let json = match std::env::var_os("FILES_TEST_DEVICES_JSON") {
        Some(f) => std::fs::read(f)?,
        None => {
            let out = Command::new("lsblk")
                .args([
                    "-J",
                    "-b",
                    "-o",
                    "NAME,PATH,LABEL,FSTYPE,SIZE,MOUNTPOINTS,RM,HOTPLUG,TYPE,PARTTYPE",
                ])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()?;
            if !out.status.success() {
                return Err(io::Error::other("lsblk falló"));
            }
            out.stdout
        }
    };
    let mut list = parse_lsblk(&json)?;
    for d in &mut list {
        if let Some(m) = &d.mount {
            d.free = crate::system::free_space(m);
        }
    }
    Ok(list)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Mount,
    Unmount,
    /// Unmount, then power the whole disk off (safe to unplug).
    Eject,
}

/// Runs the operation; Mount returns the new mount point.
pub fn run(op: Op, device: &Device) -> Result<Option<PathBuf>, String> {
    let tool = std::env::var_os("FILES_TEST_UDISKSCTL").unwrap_or_else(|| "udisksctl".into());
    let call = |args: &[&std::ffi::OsStr]| -> Result<String, String> {
        let out = Command::new(&tool)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("udisksctl no disponible: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() {
            Ok(text)
        } else {
            let err = String::from_utf8_lossy(&out.stderr)
                .trim()
                .chars()
                .take(300)
                .collect::<String>();
            Err(if err.is_empty() {
                format!("udisksctl falló ({})", out.status)
            } else {
                err
            })
        }
    };
    let node = device.node.as_os_str();
    let b = std::ffi::OsStr::new("-b");
    let quiet = std::ffi::OsStr::new("--no-user-interaction");
    match op {
        Op::Mount => {
            let text = call(&["mount".as_ref(), b, node, quiet])?;
            // "Mounted /dev/sdb1 at /run/media/user/LABEL"
            Ok(text
                .rsplit_once(" at ")
                .map(|(_, p)| PathBuf::from(p.trim_end_matches('.'))))
        }
        Op::Unmount => call(&["unmount".as_ref(), b, node, quiet]).map(|_| None),
        Op::Eject => {
            if device.mount.is_some() {
                call(&["unmount".as_ref(), b, node, quiet])?;
            }
            if let Some(disk) = &device.disk {
                call(&["power-off".as_ref(), b, disk.as_os_str(), quiet])?;
            }
            Ok(None)
        }
    }
}

/// Blocks until the mount table or the set of devices changes (or `timeout`).
#[cfg(target_os = "linux")]
pub struct Watch {
    mountinfo: std::fs::File,
    inotify: i32,
}
#[cfg(target_os = "linux")]
impl Watch {
    pub fn new() -> io::Result<Self> {
        let mountinfo = std::fs::File::open("/proc/self/mountinfo")?;
        let inotify = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if inotify >= 0 {
            let path = std::ffi::CString::new("/dev/disk/by-uuid").expect("path");
            unsafe {
                libc::inotify_add_watch(inotify, path.as_ptr(), libc::IN_CREATE | libc::IN_DELETE);
            }
        }
        Ok(Self { mountinfo, inotify })
    }
    /// True when something changed.
    pub fn wait(&mut self, timeout_ms: i32) -> bool {
        use std::os::fd::AsRawFd;
        let mut fds = [
            libc::pollfd {
                fd: self.mountinfo.as_raw_fd(),
                events: libc::POLLPRI,
                revents: 0,
            },
            libc::pollfd {
                fd: self.inotify,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let n = unsafe {
            libc::poll(
                fds.as_mut_ptr(),
                if self.inotify >= 0 { 2 } else { 1 },
                timeout_ms,
            )
        };
        if n <= 0 {
            return false;
        }
        let mut changed = false;
        if fds[0].revents & (libc::POLLPRI | libc::POLLERR) != 0 {
            // Re-arm: the flag clears after re-reading from the start.
            use std::io::{Read, Seek};
            let _ = self.mountinfo.seek(io::SeekFrom::Start(0));
            let mut sink = Vec::new();
            let _ = (&self.mountinfo).take(1 << 20).read_to_end(&mut sink);
            changed = true;
        }
        if self.inotify >= 0 && fds[1].revents & libc::POLLIN != 0 {
            let mut buf = [0u8; 4096];
            while unsafe { libc::read(self.inotify, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
            changed = true;
        }
        changed
    }
}
#[cfg(target_os = "linux")]
impl Drop for Watch {
    fn drop(&mut self) {
        if self.inotify >= 0 {
            unsafe { libc::close(self.inotify) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSBLK: &str = r#"{"blockdevices":[
      {"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"fstype":null,"size":512110190592,"mountpoints":[null],"rm":false,"hotplug":false,"type":"disk","children":[
        {"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"fstype":"vfat","size":2147483648,"mountpoints":["/boot"],"rm":false,"hotplug":false,"type":"part"},
        {"name":"nvme0n1p2","path":"/dev/nvme0n1p2","label":null,"fstype":"crypto_LUKS","size":509961707520,"mountpoints":[null],"rm":false,"hotplug":false,"type":"part","children":[
          {"name":"root","path":"/dev/mapper/root","label":null,"fstype":"btrfs","size":509945430016,"mountpoints":["/home","/","/var/log"],"rm":false,"hotplug":false,"type":"crypt"}]}]},
      {"name":"sda","path":"/dev/sda","label":null,"fstype":null,"size":1000204886016,"mountpoints":[null],"rm":false,"hotplug":false,"type":"disk","children":[
        {"name":"sda3","path":"/dev/sda3","label":"home","fstype":"ext4","size":900000000000,"mountpoints":[null],"rm":false,"hotplug":false,"type":"part"},
        {"name":"sda4","path":"/dev/sda4","label":null,"fstype":"swap","size":8000000000,"mountpoints":["[SWAP]"],"rm":false,"hotplug":false,"type":"part"}]},
      {"name":"sdb","path":"/dev/sdb","label":null,"fstype":null,"size":32000000000,"mountpoints":[null],"rm":true,"hotplug":true,"type":"disk","children":[
        {"name":"sdb1","path":"/dev/sdb1","label":"USB KEY","fstype":"exfat","size":31999000000,"mountpoints":["/run/media/me/USB KEY"],"rm":true,"hotplug":true,"type":"part"}]}
    ]}"#;

    #[test]
    fn hides_firmware_partitions_and_unmountable_filesystems() {
        let json = br#"{"blockdevices":[{"path":"/dev/nvme0n1","type":"disk","size":1,"mountpoints":[null],"children":[
          {"path":"/dev/nvme0n1p1","type":"part","fstype":"apfs","size":1,"mountpoints":[null],"parttype":"69646961-6700-11AA-AA11-00306543ECAC"},
          {"path":"/dev/nvme0n1p2","type":"part","fstype":"apfs","size":1,"mountpoints":[null],"parttype":"7c3457ef-0000-11aa-aa11-00306543ecac"},
          {"path":"/dev/nvme0n1p3","type":"part","fstype":"vfat","size":1,"mountpoints":[null],"parttype":"c12a7328-f81f-11d2-ba4b-00a0c93ec93b"},
          {"path":"/dev/nvme0n1p4","type":"part","fstype":"ext4","size":1,"mountpoints":["/mnt/data"],"parttype":"0fc63daf-8483-4772-8e79-3d69d8477de4"},
          {"path":"/dev/nvme0n1p5","type":"part","fstype":"ntfs","size":1,"mountpoints":[null],"parttype":"ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"}]}]}"#;
        let kernel = "nodev\tproc\n\text4\n\tvfat\n\tbtrfs\n";
        let nodes: Vec<_> = parse_lsblk_with(json, Some(kernel))
            .unwrap()
            .into_iter()
            .map(|d| d.node)
            .collect();
        assert_eq!(
            nodes,
            vec![
                PathBuf::from("/dev/nvme0n1p4"),
                PathBuf::from("/dev/nvme0n1p5")
            ]
        );
    }
    #[test]
    fn lists_user_filesystems_not_system_ones() {
        let list = parse_lsblk(LSBLK.as_bytes()).unwrap();
        let nodes: Vec<_> = list
            .iter()
            .map(|d| d.node.to_string_lossy().into_owned())
            .collect();
        assert_eq!(nodes, vec!["/dev/sda3", "/dev/sdb1"], "{list:?}");
        let usb = &list[1];
        assert!(usb.removable && usb.mount.as_deref() == Some(Path::new("/run/media/me/USB KEY")));
        assert_eq!(usb.disk.as_deref(), Some(Path::new("/dev/sdb")));
        assert_eq!(usb.title(), "USB KEY");
        assert!(!list[0].removable && list[0].mount.is_none());
        assert!(parse_lsblk(b"{").is_err());
        assert!(parse_lsblk(br#"{"blockdevices":[{"path":"/dev/x","type":"part","fstype":"ext4","size":"10","mountpoint":"/mnt/x"}]}"#).unwrap()[0].mount.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn operations_call_udisksctl_and_parse_the_mount_point() {
        use std::os::unix::fs::PermissionsExt;
        let d = crate::persistence::test_dir("devices-udisks");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let log = d.join("calls");
        let tool = d.join("udisksctl");
        std::fs::write(&tool, format!("#!/bin/sh\necho \"$@\" >> {}\n[ \"$1\" = mount ] && echo \"Mounted $3 at /run/media/me/KEY\"\nexit 0\n", log.display())).unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        // SAFETY: single test touching this variable.
        unsafe { std::env::set_var("FILES_TEST_UDISKSCTL", &tool) };
        let mut dev = parse_lsblk(LSBLK.as_bytes()).unwrap().remove(1);
        dev.mount = None;
        assert_eq!(
            run(Op::Mount, &dev).unwrap(),
            Some(PathBuf::from("/run/media/me/KEY"))
        );
        dev.mount = Some("/run/media/me/KEY".into());
        assert_eq!(run(Op::Eject, &dev).unwrap(), None);
        unsafe { std::env::remove_var("FILES_TEST_UDISKSCTL") };
        let calls = std::fs::read_to_string(&log).unwrap();
        assert_eq!(
            calls,
            "mount -b /dev/sdb1 --no-user-interaction\nunmount -b /dev/sdb1 --no-user-interaction\npower-off -b /dev/sdb --no-user-interaction\n"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
