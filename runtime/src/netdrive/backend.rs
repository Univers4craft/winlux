//! Moteurs de montage : quel outil du système monte quel protocole, et
//! comment construire sa ligne de commande sans jamais exposer le mot de
//! passe dans les arguments (visibles de tous via /proc).
//!
//! Les protocoles réseau gérés par le noyau Linux (SMB, NFS) passent par
//! `mount` ; les autres par un système de fichiers FUSE présent sur la
//! machine. Un protocole n'est proposé que si au moins un moteur est
//! réellement utilisable ici.

use super::{Drive, Protocol};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Sshfs,
    CurlFtpFs,
    Cifs,
    Nfs,
    Davfs,
    S3fs,
    Rclone,
}

impl Backend {
    pub fn id(self) -> &'static str {
        match self {
            Backend::Sshfs => "sshfs",
            Backend::CurlFtpFs => "curlftpfs",
            Backend::Cifs => "cifs",
            Backend::Nfs => "nfs",
            Backend::Davfs => "davfs",
            Backend::S3fs => "s3fs",
            Backend::Rclone => "rclone",
        }
    }

    /// Programme qui doit être présent pour utiliser ce moteur.
    fn program(self) -> &'static str {
        match self {
            Backend::Sshfs => "sshfs",
            Backend::CurlFtpFs => "curlftpfs",
            Backend::Cifs => "mount.cifs",
            Backend::Nfs => "mount.nfs",
            Backend::Davfs => "mount.davfs",
            Backend::S3fs => "s3fs",
            Backend::Rclone => "rclone",
        }
    }

    /// Montage par le noyau (`mount -t …`) : demande les droits root.
    pub fn kernel(self) -> bool {
        matches!(self, Backend::Cifs | Backend::Nfs | Backend::Davfs)
    }
}

const ALL: [Backend; 7] = [
    Backend::Sshfs,
    Backend::CurlFtpFs,
    Backend::Cifs,
    Backend::Nfs,
    Backend::Davfs,
    Backend::S3fs,
    Backend::Rclone,
];

/// Moteurs possibles pour un protocole, du préféré au moins préféré.
pub fn candidates(p: Protocol) -> &'static [Backend] {
    match p {
        Protocol::Ftp | Protocol::Ftps => &[Backend::CurlFtpFs, Backend::Rclone],
        Protocol::Sftp => &[Backend::Sshfs, Backend::Rclone],
        Protocol::WebDav | Protocol::WebDavs => &[Backend::Davfs, Backend::Rclone],
        Protocol::Smb => &[Backend::Cifs, Backend::Rclone],
        Protocol::Nfs => &[Backend::Nfs],
        Protocol::S3 => &[Backend::S3fs, Backend::Rclone],
    }
}

/// Ce que la machine sait faire.
#[derive(Clone, Debug, Default)]
pub struct System {
    pub programs: Vec<String>,
    pub root: bool,
    pub fuse: bool,
    /// Systèmes de fichiers que le noyau sait monter (déjà chargés ou
    /// disponibles en module).
    pub kernel_fs: Vec<String>,
}

impl System {
    pub fn detect() -> System {
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        for extra in ["/sbin", "/usr/sbin", "/bin", "/usr/bin"] {
            if !dirs.iter().any(|d| d == Path::new(extra)) {
                dirs.push(PathBuf::from(extra));
            }
        }
        let mut wanted: Vec<&str> = ALL.iter().map(|b| b.program()).collect();
        wanted.extend(["mount", "umount", "fusermount3", "fusermount"]);
        let programs = wanted
            .into_iter()
            .filter(|name| dirs.iter().any(|d| is_executable(&d.join(name))))
            .map(str::to_string)
            .collect();
        System {
            programs,
            root: unsafe { libc::geteuid() } == 0,
            fuse: Path::new("/dev/fuse").exists(),
            kernel_fs: kernel_filesystems(),
        }
    }

    pub fn has(&self, program: &str) -> bool {
        self.programs.iter().any(|p| p == program)
    }

    pub fn usable(&self, b: Backend) -> bool {
        if !self.has(b.program()) {
            return false;
        }
        if b.kernel() {
            let fs = match b {
                Backend::Cifs => "cifs",
                Backend::Nfs => "nfs",
                _ => "fuse",
            };
            self.root && self.has("mount") && self.kernel_fs.iter().any(|f| f == fs)
        } else {
            self.fuse
        }
    }

    pub fn backend_for(&self, p: Protocol) -> Option<Backend> {
        candidates(p).iter().copied().find(|b| self.usable(*b))
    }
}

/// `/proc/filesystems` plus les modules présents (chargés à la demande).
fn kernel_filesystems() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_to_string("/proc/filesystems")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_whitespace().last().map(str::to_string))
        .collect();
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let base = Path::new("/lib/modules")
        .join(release.trim())
        .join("kernel/fs");
    for (fs, rel) in [
        ("cifs", "smb/client/cifs.ko"),
        ("cifs", "cifs/cifs.ko"),
        ("nfs", "nfs/nfs.ko"),
    ] {
        let found = ["", ".zst", ".xz", ".gz"]
            .iter()
            .any(|ext| base.join(format!("{rel}{ext}")).exists());
        if found && !out.iter().any(|f| f == fs) {
            out.push(fs.to_string());
        }
    }
    out
}

/// Pourquoi un protocole n'est pas proposé et quoi installer (texte court
/// affiché dans les Paramètres). `None` si le protocole est utilisable.
pub fn missing_hint(sys: &System, p: Protocol) -> Option<String> {
    if sys.backend_for(p).is_some() {
        return None;
    }
    if !sys.fuse && p != Protocol::Nfs {
        return Some("FUSE indisponible (paquet fuse3, /dev/fuse)".into());
    }
    let hint = match p {
        Protocol::Ftp | Protocol::Ftps => "installer rclone",
        Protocol::Sftp => "installer sshfs ou rclone",
        Protocol::WebDav | Protocol::WebDavs => "installer rclone",
        Protocol::Smb => "installer rclone, ou cifs-utils avec le module noyau cifs",
        Protocol::Nfs if !sys.has("mount.nfs") => "installer nfs-common",
        Protocol::Nfs if !sys.root => "droits administrateur requis",
        Protocol::Nfs => "module noyau nfs absent",
        Protocol::S3 => "installer rclone",
    };
    Some(hint.into())
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Commande de montage prête à lancer.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Écrit sur l'entrée standard (mot de passe).
    pub stdin: Option<String>,
    /// Fichiers secrets temporaires (nom relatif au dossier privé, contenu),
    /// créés en 0600 juste avant le montage et effacés juste après.
    pub files: Vec<(String, String)>,
    /// Variable d'environnement à remplir avec le mot de passe « obscurci »
    /// par `rclone obscure` (format exigé par rclone).
    pub obscured_env: Option<String>,
}

fn host_port(d: &Drive) -> String {
    let host = if d.host.contains(':') {
        format!("[{}]", d.host)
    } else {
        d.host.clone()
    };
    format!("{host}:{}", d.port)
}

fn http_url(d: &Drive) -> String {
    let scheme = if d.protocol == Protocol::WebDavs || d.protocol == Protocol::S3 {
        "https"
    } else {
        "http"
    };
    let path = d.path.trim_start_matches('/');
    format!("{scheme}://{}/{path}", host_port(d))
}

/// Construit la commande. `secret_dir` est le dossier privé (0700) où les
/// fichiers secrets temporaires seront écrits.
pub fn plan(
    b: Backend,
    d: &Drive,
    password: Option<&str>,
    mountpoint: &Path,
    secret_dir: &Path,
) -> Plan {
    let mp = mountpoint.to_string_lossy().into_owned();
    let pw = password.filter(|p| !p.is_empty());
    let mut plan = Plan::default();
    match b {
        Backend::Sshfs => {
            let user = if d.user.is_empty() {
                String::new()
            } else {
                format!("{}@", d.user)
            };
            let host = if d.host.contains(':') {
                format!("[{}]", d.host)
            } else {
                d.host.clone()
            };
            let mut opts = String::from(
                "reconnect,ServerAliveInterval=15,ServerAliveCountMax=3,\
                 ConnectTimeout=10,StrictHostKeyChecking=accept-new",
            );
            match pw {
                Some(p) => {
                    opts.push_str(",password_stdin");
                    plan.stdin = Some(format!("{p}\n"));
                }
                // Sans mot de passe : clé SSH uniquement, jamais d'invite bloquante.
                None => opts.push_str(",BatchMode=yes"),
            }
            plan.program = "sshfs".into();
            plan.args = vec![
                format!("{user}{host}:{}", d.path),
                mp,
                "-p".into(),
                d.port.to_string(),
                "-o".into(),
                opts,
            ];
        }
        Backend::CurlFtpFs => {
            // curlftpfs lit ~/.netrc : on lui donne un HOME privé et
            // temporaire plutôt que `-o user=nom:motdepasse`.
            let mut opts = String::from("connect_timeout=10");
            if d.protocol == Protocol::Ftps {
                opts.push_str(",ssl");
            }
            if !d.user.is_empty() {
                plan.files.push((
                    ".netrc".into(),
                    format!(
                        "machine {} login {} password \"{}\"\n",
                        d.host,
                        d.user,
                        pw.unwrap_or("").replace('\\', "\\\\").replace('"', "\\\"")
                    ),
                ));
                plan.env
                    .push(("HOME".into(), secret_dir.to_string_lossy().into_owned()));
            }
            plan.program = "curlftpfs".into();
            plan.args = vec![
                format!("{}/{}", host_port(d), d.path.trim_start_matches('/')),
                mp,
                "-o".into(),
                opts,
            ];
        }
        Backend::Cifs => {
            let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
            let mut opts = format!(
                "port={},uid={uid},gid={gid},iocharset=utf8,soft,echo_interval=10",
                d.port
            );
            if d.user.is_empty() {
                opts.push_str(",guest");
            } else {
                opts.push_str(&format!(",username={}", d.user));
                if let Some(p) = pw {
                    // mount.cifs lit le mot de passe dans $PASSWD.
                    plan.env.push(("PASSWD".into(), p.into()));
                }
            }
            plan.program = "mount".into();
            plan.args = vec![
                "-t".into(),
                "cifs".into(),
                format!("//{}/{}", d.host, d.path.trim_start_matches('/')),
                mp,
                "-o".into(),
                opts,
            ];
        }
        Backend::Nfs => {
            let path = if d.path.starts_with('/') {
                d.path.clone()
            } else {
                format!("/{}", d.path)
            };
            plan.program = "mount".into();
            plan.args = vec![
                "-t".into(),
                "nfs".into(),
                format!("{}:{path}", d.host),
                mp,
                "-o".into(),
                format!("port={},soft,timeo=50,retrans=2", d.port),
            ];
        }
        Backend::Davfs => {
            let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
            // mount.davfs demande l'identifiant puis le mot de passe.
            plan.stdin = Some(format!("{}\n{}\n", d.user, pw.unwrap_or("")));
            plan.program = "mount".into();
            plan.args = vec![
                "-t".into(),
                "davfs".into(),
                http_url(d),
                mp,
                "-o".into(),
                format!("uid={uid},gid={gid}"),
            ];
        }
        Backend::S3fs => {
            let (bucket, prefix) = split_bucket(&d.path);
            plan.files.push((
                "s3-credentials".into(),
                format!("{}:{}\n", d.user, pw.unwrap_or("")),
            ));
            let target = if prefix.is_empty() {
                bucket.to_string()
            } else {
                format!("{bucket}:/{prefix}")
            };
            plan.program = "s3fs".into();
            plan.args = vec![
                target,
                mp,
                "-o".into(),
                format!(
                    "passwd_file={},url=https://{},use_path_request_style,connect_timeout=10",
                    secret_dir.join("s3-credentials").display(),
                    host_port(d)
                ),
            ];
        }
        Backend::Rclone => {
            let (kind, remote) = rclone_remote(d, &mut plan.env);
            let up = kind.to_ascii_uppercase();
            if !d.user.is_empty() {
                let key = if d.protocol == Protocol::S3 {
                    "S3_ACCESS_KEY_ID".to_string()
                } else {
                    format!("{up}_USER")
                };
                plan.env.push((format!("RCLONE_{key}"), d.user.clone()));
            }
            if let Some(p) = pw {
                if d.protocol == Protocol::S3 {
                    plan.env
                        .push(("RCLONE_S3_SECRET_ACCESS_KEY".into(), p.to_string()));
                } else {
                    plan.stdin = Some(format!("{p}\n"));
                    plan.obscured_env = Some(format!("RCLONE_{up}_PASS"));
                }
            }
            plan.program = "rclone".into();
            plan.args = vec![
                "mount".into(),
                format!(":{kind}:{remote}"),
                mp,
                "--daemon".into(),
                "--vfs-cache-mode".into(),
                "writes".into(),
                "--contimeout".into(),
                "10s".into(),
                "--timeout".into(),
                "30s".into(),
            ];
        }
    }
    plan
}

fn split_bucket(path: &str) -> (&str, &str) {
    let p = path.trim_matches('/');
    match p.split_once('/') {
        Some((b, rest)) => (b, rest),
        None => (p, ""),
    }
}

fn rclone_remote(d: &Drive, env: &mut Vec<(String, String)>) -> (&'static str, String) {
    let path = d.path.trim_start_matches('/').to_string();
    match d.protocol {
        Protocol::Ftp | Protocol::Ftps => {
            env.push(("RCLONE_FTP_HOST".into(), d.host.clone()));
            env.push(("RCLONE_FTP_PORT".into(), d.port.to_string()));
            if d.protocol == Protocol::Ftps {
                env.push(("RCLONE_FTP_EXPLICIT_TLS".into(), "true".into()));
            }
            ("ftp", path)
        }
        Protocol::Sftp => {
            env.push(("RCLONE_SFTP_HOST".into(), d.host.clone()));
            env.push(("RCLONE_SFTP_PORT".into(), d.port.to_string()));
            ("sftp", d.path.clone())
        }
        Protocol::WebDav | Protocol::WebDavs => {
            env.push(("RCLONE_WEBDAV_URL".into(), http_url(d)));
            ("webdav", String::new())
        }
        Protocol::Smb => {
            env.push(("RCLONE_SMB_HOST".into(), d.host.clone()));
            env.push(("RCLONE_SMB_PORT".into(), d.port.to_string()));
            ("smb", path)
        }
        Protocol::S3 | Protocol::Nfs => {
            env.push(("RCLONE_S3_PROVIDER".into(), "Other".into()));
            env.push((
                "RCLONE_S3_ENDPOINT".into(),
                format!("https://{}", host_port(d)),
            ));
            ("s3", path)
        }
    }
}

/// Commandes de démontage, à essayer dans l'ordre (la dernière est un
/// démontage paresseux qui ne bloque jamais, utile si le serveur a disparu).
pub fn unmount_commands(b: Backend, sys: &System, mountpoint: &Path) -> Vec<Vec<String>> {
    let mp = mountpoint.to_string_lossy().into_owned();
    let mut out = Vec::new();
    if !b.kernel() {
        for f in ["fusermount3", "fusermount"] {
            if sys.has(f) {
                out.push(vec![f.to_string(), "-u".into(), mp.clone()]);
                out.push(vec![f.to_string(), "-uz".into(), mp.clone()]);
                break;
            }
        }
    }
    out.push(vec!["umount".into(), mp.clone()]);
    out.push(vec!["umount".into(), "-l".into(), mp]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(p: Protocol) -> Drive {
        Drive {
            letter: 'N',
            name: "NAS".into(),
            protocol: p,
            host: "nas.local".into(),
            port: p.default_port(),
            user: "alice".into(),
            path: "/partage/photos".into(),
            auto: true,
            remember: true,
        }
    }

    fn all_text(p: &Plan) -> String {
        format!("{} {}", p.program, p.args.join(" "))
    }

    #[test]
    fn password_never_in_arguments() {
        for proto in Protocol::ALL {
            for b in candidates(proto) {
                let d = drive(proto);
                let p = plan(
                    *b,
                    &d,
                    Some("s3cr3t!"),
                    Path::new("/run/d/N"),
                    Path::new("/run/d/.N"),
                );
                assert!(
                    !all_text(&p).contains("s3cr3t!"),
                    "{proto:?} via {b:?} expose le mot de passe"
                );
            }
        }
    }

    #[test]
    fn sshfs_uses_stdin_or_batch_mode() {
        let d = drive(Protocol::Sftp);
        let p = plan(
            Backend::Sshfs,
            &d,
            Some("pw"),
            Path::new("/m"),
            Path::new("/s"),
        );
        assert_eq!(p.args[0], "alice@nas.local:/partage/photos");
        assert!(p.args[5].contains("password_stdin"));
        assert_eq!(p.stdin.as_deref(), Some("pw\n"));
        let p = plan(Backend::Sshfs, &d, None, Path::new("/m"), Path::new("/s"));
        assert!(p.args[5].contains("BatchMode=yes"));
        assert!(p.stdin.is_none());
    }

    #[test]
    fn ftp_credentials_go_to_private_netrc() {
        let mut d = drive(Protocol::Ftps);
        d.port = 2121;
        let p = plan(
            Backend::CurlFtpFs,
            &d,
            Some("a\"b"),
            Path::new("/m"),
            Path::new("/s"),
        );
        assert_eq!(p.args[0], "nas.local:2121/partage/photos");
        assert!(p.args[3].contains("ssl"));
        assert_eq!(p.files[0].0, ".netrc");
        assert!(p.files[0].1.contains("password \"a\\\"b\""));
        assert!(p.env.contains(&("HOME".into(), "/s".into())));
    }

    #[test]
    fn smb_and_nfs_use_kernel_mount() {
        let d = drive(Protocol::Smb);
        let p = plan(
            Backend::Cifs,
            &d,
            Some("pw"),
            Path::new("/m"),
            Path::new("/s"),
        );
        assert_eq!(p.program, "mount");
        assert_eq!(p.args[2], "//nas.local/partage/photos");
        assert!(p.args[5].contains("username=alice"));
        assert!(p.env.contains(&("PASSWD".into(), "pw".into())));
        let mut d = drive(Protocol::Nfs);
        d.path = "export".into();
        let p = plan(Backend::Nfs, &d, None, Path::new("/m"), Path::new("/s"));
        assert_eq!(p.args[2], "nas.local:/export");
        assert!(p.args[5].starts_with("port=2049,soft"));
    }

    #[test]
    fn s3_bucket_and_prefix() {
        let d = drive(Protocol::S3);
        let p = plan(
            Backend::S3fs,
            &d,
            Some("secret"),
            Path::new("/m"),
            Path::new("/s"),
        );
        assert_eq!(p.args[0], "partage:/photos");
        assert_eq!(p.files[0].1, "alice:secret\n");
        assert!(p.args[3].contains("url=https://nas.local:443"));
    }

    #[test]
    fn rclone_obscures_password() {
        let d = drive(Protocol::WebDavs);
        let p = plan(
            Backend::Rclone,
            &d,
            Some("pw"),
            Path::new("/m"),
            Path::new("/s"),
        );
        assert_eq!(p.args[1], ":webdav:");
        assert_eq!(p.obscured_env.as_deref(), Some("RCLONE_WEBDAV_PASS"));
        assert!(p.env.contains(&(
            "RCLONE_WEBDAV_URL".into(),
            "https://nas.local:443/partage/photos".into()
        )));
    }

    #[test]
    fn protocol_offered_only_when_usable() {
        let sys = System {
            programs: vec!["sshfs".into(), "mount.cifs".into(), "mount".into()],
            root: false,
            fuse: true,
            kernel_fs: vec!["cifs".into()],
        };
        assert_eq!(sys.backend_for(Protocol::Sftp), Some(Backend::Sshfs));
        // mount.cifs présent mais pas root : SMB n'est pas proposé.
        assert_eq!(sys.backend_for(Protocol::Smb), None);
        assert_eq!(sys.backend_for(Protocol::Ftp), None);
        let sys = System { fuse: false, ..sys };
        assert_eq!(sys.backend_for(Protocol::Sftp), None);
        // Root mais noyau sans module cifs (ISO live minimale) : non proposé.
        let sys = System {
            root: true,
            kernel_fs: vec![],
            ..sys
        };
        assert_eq!(sys.backend_for(Protocol::Smb), None);
        let sys = System {
            kernel_fs: vec!["cifs".into()],
            ..sys
        };
        assert_eq!(sys.backend_for(Protocol::Smb), Some(Backend::Cifs));
        let sys = System { fuse: true, ..sys };
        assert!(missing_hint(&sys, Protocol::Smb).is_none());
        assert_eq!(
            missing_hint(&sys, Protocol::Nfs).as_deref(),
            Some("installer nfs-common")
        );
        assert_eq!(
            missing_hint(&sys, Protocol::Ftp).as_deref(),
            Some("installer rclone")
        );
        let sys = System { fuse: false, ..sys };
        assert!(missing_hint(&sys, Protocol::Sftp)
            .unwrap()
            .contains("fuse3"));
    }

    #[test]
    fn ipv6_hosts_are_bracketed() {
        let mut d = drive(Protocol::Sftp);
        d.host = "fe80::1".into();
        let p = plan(Backend::Sshfs, &d, None, Path::new("/m"), Path::new("/s"));
        assert!(p.args[0].starts_with("alice@[fe80::1]:"));
    }
}
