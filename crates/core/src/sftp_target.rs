//! Bounded SSHFS intent/argv only. No process, network, mount, credential or config access.
use std::{ffi::OsString, path::Path};
const MAX_SOURCE: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    host: String,
    user: Option<String>,
    port: Option<u16>,
    directory: String,
}
fn safe_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
impl Target {
    /// Empty user/port inherit existing OpenSSH settings; empty directory means remote home.
    /// Whitespace is never silently trimmed and URL/percent decoding is never performed.
    pub fn parse(
        host: &str,
        user: &str,
        port: &str,
        directory: &str,
    ) -> Result<Self, &'static str> {
        let port = Self::checked_port(host, user, port, directory)?;
        Ok(Self {
            host: host.into(),
            user: (!user.is_empty()).then(|| user.into()),
            port,
            directory: directory.into(),
        })
    }
    /// Validate borrowed connection fields without allocating strings or contacting SSH.
    pub fn validate(
        host: &str,
        user: &str,
        port: &str,
        directory: &str,
    ) -> Result<(), &'static str> {
        Self::checked_port(host, user, port, directory).map(|_| ())
    }
    fn checked_port(
        host: &str,
        user: &str,
        port: &str,
        directory: &str,
    ) -> Result<Option<u16>, &'static str> {
        let ipv6 = host
            .strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
            .is_some_and(|ip| ip.len() <= 45 && ip.parse::<std::net::Ipv6Addr>().is_ok());
        if !safe_token(host, 253) && !ipv6 {
            return Err("Host inválido: usa alias SSH, nombre, IPv4 o IPv6 entre corchetes");
        }
        if !user.is_empty() && !safe_token(user, 64) {
            return Err("Usuario inválido; no incluyas contraseña ni opciones");
        }
        let port = if port.is_empty() {
            None
        } else {
            if port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Puerto inválido; usa 1–65535 o vacío para SSH config");
            }
            Some(
                port.parse::<u16>()
                    .ok()
                    .filter(|&p| p != 0)
                    .ok_or("Puerto fuera de 1–65535")?,
            )
        };
        let source_len = host
            .len()
            .checked_add(user.len())
            .and_then(|n| n.checked_add(usize::from(!user.is_empty()) + 1))
            .and_then(|n| n.checked_add(directory.len()));
        if source_len.is_none_or(|n| n > MAX_SOURCE) || directory.chars().any(char::is_control) {
            return Err("Ruta remota inválida o destino superior a4096 bytes");
        }
        Ok(port)
    }
    pub fn parts(&self) -> (&str, &str, Option<u16>, &str) {
        (
            &self.host,
            self.user.as_deref().unwrap_or(""),
            self.port,
            &self.directory,
        )
    }
    pub fn port(&self) -> Option<u16> {
        self.port
    }
    pub fn source(&self) -> String {
        match &self.user {
            Some(user) => format!("{user}@{}:{}", self.host, self.directory),
            None => format!("{}:{}", self.host, self.directory),
        }
    }
    /// Caller must independently validate ownership/identity/emptiness of the local mountpoint.
    /// Returned options are fixed; source and mountpoint are distinct arguments, never shell text.
    pub fn foreground_args(&self, mountpoint: &Path) -> Result<Vec<OsString>, &'static str> {
        let raw = mountpoint.as_os_str().as_encoded_bytes();
        if !mountpoint.is_absolute() || raw.len() > 4096 || raw.contains(&0) {
            return Err("El punto de montaje debe ser absoluto y tener hasta4096 bytes");
        }
        let mut args = Vec::<OsString>::with_capacity(20);
        args.push("-f".into());
        for option in [
            "BatchMode=yes",
            "StrictHostKeyChecking=yes",
            "NumberOfPasswordPrompts=0",
            "ConnectTimeout=10",
            "ServerAliveInterval=15",
            "ServerAliveCountMax=2",
        ] {
            args.push("-o".into());
            args.push(option.into());
        }
        if let Some(port) = self.port {
            args.push("-p".into());
            args.push(port.to_string().into());
        }
        args.push(self.source().into());
        args.push(mountpoint.as_os_str().to_owned());
        Ok(args)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alias_inherits_config_and_empty_directory_means_remote_home() {
        let t = Target::parse("Work-Alias", "", "", "").unwrap();
        let a = t.foreground_args(Path::new("/owned/mount")).unwrap();
        assert_eq!(t.source(), "Work-Alias:");
        assert_eq!(a.last().unwrap(), "/owned/mount");
        assert!(!a.iter().any(|v| v == "-p"));
        assert!(a.iter().any(|v| v == "StrictHostKeyChecking=yes"));
        assert!(a.iter().any(|v| v == "BatchMode=yes"));
        assert!(
            !a.iter()
                .any(|v| v == "allow_other" || v == "reconnect" || v == "password_stdin")
        );
    }
    #[test]
    fn unicode_spaces_and_shell_metacharacters_in_remote_path_remain_one_literal_argument() {
        let t = Target::parse("example.test", "alice", "2222", "/Fotos ñ/%20/$() `x`;a:b").unwrap();
        let a = t
            .foreground_args(Path::new("/owned/mount with spaces"))
            .unwrap();
        assert_eq!(
            a[a.len() - 2],
            "alice@example.test:/Fotos ñ/%20/$() `x`;a:b"
        );
        assert_eq!(a[a.len() - 1], "/owned/mount with spaces");
        let p = a.iter().position(|v| v == "-p").unwrap();
        assert_eq!(a[p + 1], "2222");
    }
    #[test]
    fn ipv6_brackets_preserved_without_reinterpreting_colons() {
        assert_eq!(
            Target::parse("[2001:db8::1]", "u", "65535", "folder")
                .unwrap()
                .source(),
            "u@[2001:db8::1]:folder"
        );
        assert!(Target::parse("::1", "", "", "").is_err());
        assert!(Target::parse("[::1%eth0]", "", "", "").is_err());
    }
    #[test]
    fn options_credentials_and_control_characters_cannot_enter_host_user_or_port() {
        for h in [
            "-oProxyCommand=x",
            "a b",
            "a@b",
            "ssh://host",
            "a\n",
            "",
            "[not-ip]",
            "a;id",
        ] {
            assert!(Target::parse(h, "", "", "").is_err(), "{h}");
        }
        for u in ["-o", "a:b", "a@b", "a b", "a\0"] {
            assert!(Target::parse("host", u, "", "").is_err());
        }
        for p in ["0", "65536", "22x", " 22", "-1", "000022"] {
            assert!(Target::parse("host", "", p, "").is_err());
        }
        assert!(Target::parse("host", "", "", "/bad\n").is_err());
    }
    #[test]
    fn exact_source_budget_checked_before_allocating_owned_fields() {
        let path = "x".repeat(4091);
        assert!(Target::parse("host", "", "", &path).is_ok());
        assert!(Target::parse("host", "", "", &"x".repeat(1024 * 1024)).is_err());
        let path = "x".repeat(4092);
        assert!(Target::parse("host", "", "", &path).is_err());
        assert!(Target::parse(&"a".repeat(254), "", "", "").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn local_raw_mountpoint_preserved_and_invalid_destination_rejected() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        let t = Target::parse("host", "", "", "").unwrap();
        let p = Path::new(OsStr::from_bytes(b"/owned/mount-\xff"));
        assert_eq!(
            t.foreground_args(p)
                .unwrap()
                .last()
                .unwrap()
                .as_encoded_bytes(),
            p.as_os_str().as_encoded_bytes()
        );
        for p in ["relative", "/bad\0", ""] {
            assert!(t.foreground_args(Path::new(p)).is_err());
        }
    }
}
