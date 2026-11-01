//! Proxy système commun aux applis Windows et Linux.
//!
//! Source unique : `/etc/winlux/proxy.conf` (`enabled=1`, `http=hôte:port`,
//! `https=hôte:port`, `bypass=localhost;127.0.0.1`). Sans ce fichier, Winlux
//! reprend le proxy Linux (`http_proxy`, `https_proxy`, `no_proxy` de
//! l'environnement, puis `/etc/environment` écrit par l'installateur).
//!
//! - Côté Windows, WinHTTP (accès par défaut ou automatique) et les valeurs
//!   `ProxyEnable`, `ProxyServer`, `ProxyOverride` de la clé
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`
//!   reflètent ce proxy.
//! - Une appli Windows qui change ces valeurs du registre (outil de VPN,
//!   réglages d'un navigateur…) met à jour `proxy.conf` et
//!   `/etc/winlux/proxy.env`, que la session charge pour les applis Linux.

use crate::admin;

pub const CONFIG: &str = "etc/winlux/proxy.conf";
pub const ENV_FILE: &str = "etc/winlux/proxy.env";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Proxy {
    /// `hôte:port` pour HTTP.
    pub http: String,
    /// `hôte:port` pour HTTPS (souvent le même).
    pub https: String,
    /// Exceptions au format Windows : `localhost;127.0.0.1;*.lan`.
    pub bypass: String,
}

/// `http://user@hôte:3128/` → `hôte:3128` (le reste n'a pas d'équivalent
/// dans `ProxyServer`).
pub fn host_port(url: &str) -> String {
    let s = url.trim().trim_matches('"');
    let s = s.split_once("://").map_or(s, |(_, r)| r);
    let s = s.rsplit_once('@').map_or(s, |(_, r)| r);
    s.trim_end_matches('/').to_string()
}

fn env_lookup(get: &dyn Fn(&str) -> Option<String>) -> Option<Proxy> {
    let pick = |names: &[&str]| {
        names
            .iter()
            .find_map(|n| get(n).filter(|v| !v.trim().is_empty()))
            .map(|v| host_port(&v))
    };
    let http = pick(&["http_proxy", "HTTP_PROXY"]);
    let https = pick(&["https_proxy", "HTTPS_PROXY"]);
    if http.is_none() && https.is_none() {
        return None;
    }
    let bypass = pick(&["no_proxy", "NO_PROXY"]).unwrap_or_default();
    Some(Proxy {
        http: http.clone().or_else(|| https.clone()).unwrap_or_default(),
        https: https.or(http).unwrap_or_default(),
        bypass: bypass
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(";"),
    })
}

/// Proxy en vigueur, `None` = connexion directe.
pub fn load() -> Option<Proxy> {
    if let Some(t) = admin::read_text(&admin::path(CONFIG)) {
        if admin::field(&t, "enabled") != Some("1") {
            return None;
        }
        let http = admin::field(&t, "http").unwrap_or("").trim().to_string();
        let https = admin::field(&t, "https").unwrap_or("").trim().to_string();
        if http.is_empty() && https.is_empty() {
            return None;
        }
        return Some(Proxy {
            http: if http.is_empty() {
                https.clone()
            } else {
                http.clone()
            },
            https: if https.is_empty() { http } else { https },
            bypass: admin::field(&t, "bypass").unwrap_or("").trim().to_string(),
        });
    }
    if let Some(p) = env_lookup(&|n| std::env::var(n).ok()) {
        return Some(p);
    }
    let etc = admin::read_text(&admin::path("etc/environment"))?;
    env_lookup(&|n| {
        etc.lines().find_map(|l| {
            l.trim()
                .strip_prefix(n)
                .and_then(|r| r.strip_prefix('='))
                .map(|v| v.trim().trim_matches('"').to_string())
        })
    })
}

/// Valeur `ProxyServer` : `hôte:port`, ou `http=…;https=…` s'ils diffèrent.
pub fn windows_server(p: &Proxy) -> String {
    if p.http == p.https {
        p.http.clone()
    } else {
        format!("http={};https={}", p.http, p.https)
    }
}

/// Lit `ProxyServer` (`hôte:port` ou `http=…;https=…;ftp=…`).
pub fn from_windows(server: &str, bypass: &str) -> Option<Proxy> {
    let server = server.trim();
    if server.is_empty() {
        return None;
    }
    let (mut http, mut https) = (String::new(), String::new());
    if server.contains('=') {
        for part in server.split(';') {
            match part.split_once('=') {
                Some(("http", v)) => http = host_port(v),
                Some(("https", v)) => https = host_port(v),
                _ => {}
            }
        }
    } else {
        http = host_port(server);
        https = http.clone();
    }
    if http.is_empty() && https.is_empty() {
        return None;
    }
    Some(Proxy {
        http: if http.is_empty() {
            https.clone()
        } else {
            http.clone()
        },
        https: if https.is_empty() { http } else { https },
        bypass: bypass.trim().to_string(),
    })
}

/// URL de proxy pour une requête (curl), `None` si l'hôte est exempté.
pub fn for_request(p: &Proxy, host: &str, secure: bool) -> Option<String> {
    let h = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    for rule in p.bypass.split(';').map(|r| r.trim().to_ascii_lowercase()) {
        let hit = match rule.as_str() {
            "" => false,
            "<local>" => !h.contains('.') && !h.contains(':'),
            r if r.starts_with('*') => h.ends_with(r.trim_start_matches('*')),
            r if r.starts_with('.') => h.ends_with(r) || h == r[1..],
            r => h == r,
        };
        if hit {
            return None;
        }
    }
    if matches!(h.as_str(), "localhost" | "127.0.0.1" | "::1") {
        return None;
    }
    let hp = if secure { &p.https } else { &p.http };
    (!hp.is_empty()).then(|| format!("http://{hp}"))
}

/// Script shell pour les applis Linux de la session.
pub fn env_script(p: Option<&Proxy>) -> String {
    let mut s = String::from("# Proxy commun Windows/Linux (Winlux).\n");
    match p {
        None => s.push_str(
            "unset http_proxy https_proxy ftp_proxy no_proxy HTTP_PROXY HTTPS_PROXY FTP_PROXY NO_PROXY\n",
        ),
        Some(p) => {
            let clean = |v: &str| v.replace(['"', '\\', '$', '`', '\n'], "");
            let no: Vec<String> = p
                .bypass
                .split(';')
                .map(str::trim)
                .filter(|r| !r.is_empty() && *r != "<local>")
                .map(|r| r.trim_start_matches('*').to_string())
                .chain(["localhost", "127.0.0.1", "::1"].map(String::from))
                .collect();
            let mut seen = Vec::new();
            for n in no {
                if !seen.contains(&n) {
                    seen.push(n);
                }
            }
            let http = format!("http://{}/", clean(&p.http));
            let https = format!("http://{}/", clean(&p.https));
            let no = clean(&seen.join(","));
            s.push_str(&format!(
                "export http_proxy=\"{http}\" HTTP_PROXY=\"{http}\" https_proxy=\"{https}\" HTTPS_PROXY=\"{https}\" ftp_proxy=\"{http}\" FTP_PROXY=\"{http}\"\nexport no_proxy=\"{no}\" NO_PROXY=\"{no}\"\n"
            ));
        }
    }
    s
}

/// Enregistre le proxy choisi côté Windows pour les deux mondes.
pub fn save(p: Option<&Proxy>) -> std::io::Result<()> {
    let conf = match p {
        None => "enabled=0\n".to_string(),
        Some(p) => format!(
            "enabled=1\nhttp={}\nhttps={}\nbypass={}\n",
            p.http.replace('\n', ""),
            p.https.replace('\n', ""),
            p.bypass.replace('\n', "")
        ),
    };
    admin::write_text(&admin::path(CONFIG), &conf)?;
    admin::write_text(&admin::path(ENV_FILE), &env_script(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_and_windows_formats() {
        assert_eq!(
            host_port("http://user:pw@proxy.lan:3128/"),
            "proxy.lan:3128"
        );
        assert_eq!(host_port("10.0.0.1:8080"), "10.0.0.1:8080");
        let env = |n: &str| match n {
            "http_proxy" => Some("http://proxy:3128/".to_string()),
            "no_proxy" => Some("localhost, .corp".to_string()),
            _ => None,
        };
        let p = env_lookup(&env).unwrap();
        assert_eq!(p.https, "proxy:3128");
        assert_eq!(p.bypass, "localhost;.corp");
        assert_eq!(windows_server(&p), "proxy:3128");
        let w = from_windows("http=a:1;https=b:2;ftp=c:3", "<local>").unwrap();
        assert_eq!((w.http.as_str(), w.https.as_str()), ("a:1", "b:2"));
        assert_eq!(windows_server(&w), "http=a:1;https=b:2");
        assert!(from_windows("", "").is_none());
    }

    #[test]
    fn bypass_rules() {
        let p = Proxy {
            http: "p:1".into(),
            https: "p:2".into(),
            bypass: "<local>;*.corp;intranet.lan".into(),
        };
        assert_eq!(
            for_request(&p, "example.com", true).as_deref(),
            Some("http://p:2")
        );
        assert_eq!(
            for_request(&p, "example.com", false).as_deref(),
            Some("http://p:1")
        );
        assert_eq!(for_request(&p, "nas", false), None);
        assert_eq!(for_request(&p, "wiki.corp", false), None);
        assert_eq!(for_request(&p, "intranet.lan", false), None);
        assert_eq!(for_request(&p, "127.0.0.1", false), None);
    }

    #[test]
    fn config_file_wins_and_env_script() {
        let root = crate::admin::test_root::Root::new("proxy");
        let p = Proxy {
            http: "proxy:3128".into(),
            https: "proxy:3128".into(),
            bypass: "<local>;*.lan".into(),
        };
        save(Some(&p)).unwrap();
        assert_eq!(load(), Some(p));
        let env = std::fs::read_to_string(root.dir.join(ENV_FILE)).unwrap();
        assert!(env.contains("export http_proxy=\"http://proxy:3128/\""));
        assert!(env.contains("no_proxy=\".lan,localhost,127.0.0.1,::1\""));
        save(None).unwrap();
        assert_eq!(load(), None);
        assert!(std::fs::read_to_string(root.dir.join(ENV_FILE))
            .unwrap()
            .contains("unset http_proxy"));
    }
}
