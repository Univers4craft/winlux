//! Vrai moteur web du contrôle WebBrowser (#2784), premier pas.
//!
//! Le contrôle montrait la page en texte, sans exécuter ses scripts. Les
//! installeurs dont l'interface est une page HTML (Creative Cloud) restent
//! alors sans fenêtre. Quand le moteur `winlux-webview` (WebKitGTK, celui
//! du WebView2 maison, #236) est présent, la fenêtre du contrôle reçoit
//! une vue WebKitGTK qui charge la page et exécute ses scripts ; sinon le
//! contrôle garde la zone de texte.
//!
//! `window.external` (l'objet que l'hôte rend par
//! IDocHostUIHandler::GetExternal) n'est pas encore relié à l'hôte : dans
//! ce premier pas, chaque appel de la page à `window.external.<nom>(…)` est
//! seulement journalisé (« window.external.<nom> appelée par la page »),
//! pour mesurer ce que la page attend de son hôte.

use super::super::webview2::{helper_available, helper_command, json_escape, parse_flat, Val};
use std::io::{BufRead, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

/// Script posé avant ceux de la page : `window.external` journalise ses
/// appels par le canal de messages du moteur (window.chrome.webview).
pub(super) const EXTERNAL_JS: &str = r#"(function () {
  function send(name, args) {
    try {
      var list = [];
      for (var i = 0; i < args.length; i++) { list.push(String(args[i])); }
      window.chrome.webview.postMessage({ winluxExternal: String(name), args: list });
    } catch (e) {}
  }
  var external = new Proxy({}, {
    get: function (_t, name) {
      if (typeof name !== "string") { return undefined; }
      return function () { send(name, arguments); return undefined; };
    }
  });
  try {
    Object.defineProperty(window, "external", { value: external, configurable: true });
  } catch (e) {}
  window.addEventListener("error", function (e) {
    try {
      window.chrome.webview.postMessage({ winluxError: String(e.message) + " (" + String(e.lineno) + ")" });
    } catch (x) {}
  });
})();"#;

/// Le moteur WebKitGTK est-il déjà installé ? Le contrôle ne fait jamais
/// proposer son téléchargement (winlux-webview le propose quand il
/// manque) : une boîte de licence se contente alors du texte.
fn engine_installed() -> bool {
    if let Some(p) = std::env::var_os("WINLUX_WEBBROWSER_TYPELIB") {
        return std::path::Path::new(&p).is_file();
    }
    [
        "/usr/lib/x86_64-linux-gnu/girepository-1.0/WebKit2-4.1.typelib",
        "/usr/lib/i386-linux-gnu/girepository-1.0/WebKit2-4.1.typelib",
        "/usr/lib/girepository-1.0/WebKit2-4.1.typelib",
    ]
    .iter()
    .any(|p| std::path::Path::new(p).is_file())
}

/// Une vue WebKitGTK dans la fenêtre du contrôle.
pub(super) struct Moteur {
    child: Option<Child>,
    input: Option<ChildStdin>,
}

impl Moteur {
    /// Lance le moteur dans la fenêtre X11 de `hwnd`, à la taille `w`×`h`.
    /// None si le moteur est absent ou si la fenêtre n'a pas d'XID.
    pub(super) fn start(hwnd: usize, w: i32, h: i32) -> Option<Moteur> {
        if std::env::var_os("WINLUX_WEBBROWSER_TEXTE").is_some()
            || !helper_available()
            || !engine_installed()
        {
            return None;
        }
        let xid = super::super::gui::native_window(hwnd)?;
        let cmd = helper_command();
        let mut child = Command::new(&cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .env_remove(crate::dlls::inherit::ENV)
            .env_remove(crate::dlls::handle_broker::ENV)
            .spawn()
            .map_err(|e| {
                crate::log_warn!("webbrowser", "lancement du moteur web impossible : {e}");
            })
            .ok()?;
        let input = child.stdin.take();
        let output = child.stdout.take()?;
        let _ = std::thread::Builder::new()
            .name("webbrowser-moteur".into())
            .spawn(move || read_events(output));
        let mut m = Moteur {
            child: Some(child),
            input,
        };
        m.send(&format!(
            "{{\"op\":\"script_doc\",\"js\":{}}}",
            json_escape(EXTERNAL_JS)
        ));
        m.send(&format!(
            "{{\"op\":\"create\",\"parent\":{xid},\"x\":0,\"y\":0,\"w\":{},\"h\":{},\"visible\":true}}",
            w.max(1),
            h.max(1)
        ));
        crate::log_info!("webbrowser", "moteur web lancé dans la fenêtre {hwnd:#x}");
        Some(m)
    }

    fn send(&mut self, line: &str) {
        if let Some(i) = self.input.as_mut() {
            let _ = i.write_all(line.as_bytes());
            let _ = i.write_all(b"\n");
            let _ = i.flush();
        }
    }

    /// Charge une page : un fichier par son adresse file:// (ses scripts et
    /// feuilles voisins se chargent alors), sinon le HTML lui-même.
    pub(super) fn show(&mut self, linux_file: Option<&std::path::Path>, html: &str) {
        match linux_file {
            Some(p) => {
                let uri = format!("file://{}", p.display());
                self.send(&format!(
                    "{{\"op\":\"navigate\",\"uri\":{}}}",
                    json_escape(&uri)
                ));
            }
            None => self.send(&format!(
                "{{\"op\":\"html\",\"html\":{}}}",
                json_escape(html)
            )),
        }
    }

    pub(super) fn bounds(&mut self, w: i32, h: i32) {
        self.send(&format!(
            "{{\"op\":\"bounds\",\"x\":0,\"y\":0,\"w\":{},\"h\":{}}}",
            w.max(0),
            h.max(0)
        ));
    }
}

impl Drop for Moteur {
    fn drop(&mut self) {
        self.send("{\"op\":\"close\"}");
        self.input = None;
        if let Some(mut child) = self.child.take() {
            std::thread::spawn(move || {
                for _ in 0..30 {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}

/// Événements du moteur : navigation achevée, messages de la page.
fn read_events(output: std::process::ChildStdout) {
    for line in std::io::BufReader::new(output).lines() {
        let Ok(line) = line else { break };
        let Some(ev) = parse_flat(&line) else {
            continue;
        };
        let text = |k: &str| match ev.get(k) {
            Some(Val::Str(s)) => s.clone(),
            Some(Val::Num(n)) => n.to_string(),
            Some(Val::Bool(b)) => b.to_string(),
            _ => String::new(),
        };
        match text("ev").as_str() {
            "nav_completed" => crate::log_info!(
                "webbrowser",
                "moteur : page chargée (ok {}, statut {})",
                text("ok"),
                text("status")
            ),
            "message" => external_call(&text("json")),
            "nav_starting" => crate::log_info!(
                "webbrowser",
                "moteur : navigation demandée par la page vers « {} »",
                text("uri")
            ),
            "error" => crate::log_warn!("webbrowser", "moteur : erreur {}", text("code")),
            _ => {}
        }
    }
}

/// Message de la page : appel de `window.external` à journaliser.
fn external_call(json: &str) {
    if let Some(rest) = json.split("\"winluxError\":").nth(1) {
        let what: String = rest.chars().take(200).collect();
        crate::log_warn!("webbrowser", "script de la page en erreur : {what}");
        return;
    }
    let Some(rest) = json.split("\"winluxExternal\":").nth(1) else {
        return;
    };
    let name: String = rest
        .trim_start()
        .trim_start_matches('"')
        .chars()
        .take_while(|c| *c != '"')
        .take(128)
        .collect();
    crate::log_warn!(
        "webbrowser",
        "window.external.{name} appelée par la page : pas encore reliée à l'hôte (#2784)"
    );
}
