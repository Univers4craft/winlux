//! Bienvenue : visite guidée du bureau au premier login (Démarrer, Explorer,
//! Paramètres, Terminal, raccourcis, confidentialité, thème).
//! Rouvrable à tout moment depuis le menu Démarrer.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

struct Step {
    title: Tr4,
    text: Tr4,
    tips: [Tr4; 3],
    action: Tr4,
}

const NONE: Tr4 = [b"", b"", b"", b""];

const STEPS: [Step; 8] = [
    Step {
        title: [b"Bienvenue dans Winlux", b"Welcome to Winlux", b"Bienvenido a Winlux", b"Willkommen bei Winlux"],
        text: [b"Winlux est un syst\xE8me Linux qui lance vos programmes Windows d'un double-clic, sans machine virtuelle cach\xE9e. Cette visite de deux minutes pr\xE9sente l'essentiel.", b"Winlux is a Linux system that runs your Windows programs with a double-click, without a hidden virtual machine. This two-minute tour shows you the essentials.", b"Winlux es un sistema Linux que abre sus programas de Windows con un doble clic, sin m\xE1quina virtual oculta. Este recorrido de dos minutos le muestra lo esencial.", b"Winlux ist ein Linux-System, das Ihre Windows-Programme per Doppelklick startet, ohne versteckte virtuelle Maschine. Diese zweimin\xFCtige Tour zeigt das Wichtigste."],
        tips: [
            [b"Les fichiers .exe s'ouvrent comme sous Windows.", b".exe files open just like on Windows.", b"Los archivos .exe se abren igual que en Windows.", b".exe-Dateien \xF6ffnen sich wie unter Windows."],
            [b"Vos fichiers Linux sont accessibles depuis tous les programmes.", b"Your Linux files are available to every program.", b"Sus archivos de Linux est\xE1n disponibles en todos los programas.", b"Ihre Linux-Dateien sind in allen Programmen verf\xFCgbar."],
            [b"Tout reste modifiable plus tard dans Param\xE8tres.", b"Everything can be changed later in Settings.", b"Todo se puede cambiar m\xE1s tarde en Configuraci\xF3n.", b"Alles l\xE4sst sich sp\xE4ter in den Einstellungen \xE4ndern."],
        ],
        action: NONE,
    },
    Step {
        title: [b"Le menu D\xE9marrer", b"The Start menu", b"El men\xFA Inicio", b"Das Startmen\xFC"],
        text: [b"Cliquez sur le logo de la barre des t\xE2ches ou appuyez sur la touche Windows. Tapez directement pour rechercher une application ou un fichier.", b"Click the logo on the taskbar or press the Windows key. Just start typing to search for an app or a file.", b"Haga clic en el logotipo de la barra de tareas o pulse la tecla Windows. Escriba directamente para buscar una aplicaci\xF3n o un archivo.", b"Klicken Sie auf das Logo in der Taskleiste oder dr\xFCcken Sie die Windows-Taste. Tippen Sie einfach los, um eine App oder Datei zu suchen."],
        tips: [
            [b"Applications \xE9pingl\xE9es en haut, toutes les applications \xE0 c\xF4t\xE9.", b"Pinned apps at the top, all apps next to them.", b"Aplicaciones ancladas arriba, todas las aplicaciones al lado.", b"Angeheftete Apps oben, alle Apps daneben."],
            [b"Le bouton d'alimentation verrouille, red\xE9marre ou arr\xEAte.", b"The power button locks, restarts or shuts down.", b"El bot\xF3n de inicio/apagado bloquea, reinicia o apaga.", b"Die Ein/Aus-Taste sperrt, startet neu oder f\xE4hrt herunter."],
            [b"Param\xE8tres > Apparence centre la barre ou l'aligne \xE0 gauche.", b"Settings > Appearance centers the taskbar or aligns it to the left.", b"Configuraci\xF3n > Apariencia centra la barra o la alinea a la izquierda.", b"Einstellungen > Darstellung zentriert die Leiste oder richtet sie links aus."],
        ],
        action: [b"Ouvrir le menu D\xE9marrer", b"Open the Start menu", b"Abrir el men\xFA Inicio", b"Startmen\xFC \xF6ffnen"],
    },
    Step {
        title: [b"L'Explorateur de fichiers", b"File Explorer", b"El Explorador de archivos", b"Der Datei-Explorer"],
        text: [b"Explorer parcourt vos dossiers Linux et vos disques. Double-cliquez sur un .exe pour le lancer, sur un document pour l'ouvrir.", b"Explorer browses your Linux folders and drives. Double-click an .exe to run it, or a document to open it.", b"El Explorador recorre sus carpetas de Linux y sus discos. Haga doble clic en un .exe para ejecutarlo o en un documento para abrirlo.", b"Der Explorer durchsucht Ihre Linux-Ordner und Laufwerke. Doppelklicken Sie auf eine .exe, um sie zu starten, oder auf ein Dokument, um es zu \xF6ffnen."],
        tips: [
            [b"Onglets et double volet pour copier d'un dossier \xE0 l'autre.", b"Tabs and dual pane to copy between folders.", b"Pesta\xF1as y doble panel para copiar de una carpeta a otra.", b"Registerkarten und Doppelansicht zum Kopieren zwischen Ordnern."],
            [b"Clic droit : renommer, compresser, r\xE9glages d'un .exe.", b"Right-click: rename, compress, .exe settings.", b"Clic derecho: cambiar nombre, comprimir, ajustes de un .exe.", b"Rechtsklick: umbenennen, komprimieren, Einstellungen einer .exe."],
            [b"Corbeille pour r\xE9cup\xE9rer un fichier supprim\xE9.", b"Recycle Bin to recover a deleted file.", b"Papelera para recuperar un archivo eliminado.", b"Papierkorb zum Wiederherstellen gel\xF6schter Dateien."],
        ],
        action: [b"Ouvrir l'Explorateur", b"Open Explorer", b"Abrir el Explorador", b"Explorer \xF6ffnen"],
    },
    Step {
        title: [b"Les Param\xE8tres", b"Settings", b"La Configuraci\xF3n", b"Die Einstellungen"],
        text: [b"R\xE9seau, son, clavier, \xE9cran, apparence, mises \xE0 jour et confidentialit\xE9 sont r\xE9unis au m\xEAme endroit. Chaque r\xE9glage agit sur le vrai syst\xE8me Linux.", b"Network, sound, keyboard, display, appearance, updates and privacy are all in one place. Every setting acts on the real Linux system.", b"Red, sonido, teclado, pantalla, apariencia, actualizaciones y privacidad est\xE1n en un mismo lugar. Cada ajuste act\xFAa sobre el sistema Linux real.", b"Netzwerk, Sound, Tastatur, Bildschirm, Darstellung, Updates und Datenschutz an einem Ort. Jede Einstellung wirkt auf das echte Linux-System."],
        tips: [
            [b"Th\xE8me clair ou sombre, automatique selon l'heure.", b"Light or dark theme, automatic by time of day.", b"Tema claro u oscuro, autom\xE1tico seg\xFAn la hora.", b"Helles oder dunkles Design, automatisch nach Tageszeit."],
            [b"D\xE9finition de l'\xE9cran et \xE9chelle du texte.", b"Screen resolution and text scaling.", b"Resoluci\xF3n de pantalla y escala del texto.", b"Bildschirmaufl\xF6sung und Textskalierung."],
            [b"Navigateur par d\xE9faut : Edge, Chrome ou Firefox.", b"Default browser: Edge, Chrome or Firefox.", b"Navegador predeterminado: Edge, Chrome o Firefox.", b"Standardbrowser: Edge, Chrome oder Firefox."],
        ],
        action: [b"Ouvrir les Param\xE8tres", b"Open Settings", b"Abrir Configuraci\xF3n", b"Einstellungen \xF6ffnen"],
    },
    Step {
        title: [b"Le Terminal", b"The Terminal", b"El Terminal", b"Das Terminal"],
        text: [b"Le Terminal donne acc\xE8s \xE0 la ligne de commande Linux, pour les utilisateurs avanc\xE9s. Il n'est jamais n\xE9cessaire pour utiliser Winlux au quotidien.", b"The Terminal gives advanced users access to the Linux command line. You never need it for everyday use of Winlux.", b"El Terminal da acceso a la l\xEDnea de comandos de Linux para usuarios avanzados. Nunca es necesario para usar Winlux a diario.", b"Das Terminal bietet erfahrenen Benutzern Zugriff auf die Linux-Befehlszeile. F\xFCr die t\xE4gliche Nutzung von Winlux wird es nie ben\xF6tigt."],
        tips: [
            [b"Les commandes Linux habituelles : ls, cd, apt...", b"The usual Linux commands: ls, cd, apt...", b"Los comandos habituales de Linux: ls, cd, apt...", b"Die \xFCblichen Linux-Befehle: ls, cd, apt..."],
            [b"Clic droit copie la s\xE9lection ; Ctrl+V ou clic droit colle.", b"Right-click copies the selection; Ctrl+V or right-click pastes.", b"Clic derecho copia la selecci\xF3n; Ctrl+V o clic derecho pega.", b"Rechtsklick kopiert die Auswahl; Strg+V oder Rechtsklick f\xFCgt ein."],
            [b"Fermez-le comme n'importe quelle fen\xEAtre.", b"Close it like any other window.", b"Ci\xE9rrelo como cualquier otra ventana.", b"Schlie\xDFen Sie es wie jedes andere Fenster."],
        ],
        action: [b"Ouvrir le Terminal", b"Open the Terminal", b"Abrir el Terminal", b"Terminal \xF6ffnen"],
    },
    Step {
        title: [b"Raccourcis clavier", b"Keyboard shortcuts", b"M\xE9todos abreviados de teclado", b"Tastenkombinationen"],
        text: [b"Les raccourcis de Windows 11 fonctionnent dans Winlux :", b"Windows 11 shortcuts work in Winlux:", b"Los atajos de Windows 11 funcionan en Winlux:", b"Die Tastenkombinationen von Windows 11 funktionieren in Winlux:"],
        tips: [NONE, NONE, NONE],
        action: NONE,
    },
    Step {
        title: [b"Votre vie priv\xE9e", b"Your privacy", b"Su privacidad", b"Ihre Privatsph\xE4re"],
        text: [b"Winlux ne collecte aucune donn\xE9e d'utilisation : la t\xE9l\xE9m\xE9trie et l'historique de localisation sont d\xE9sactiv\xE9s par d\xE9faut. Vous d\xE9cidez de tout.", b"Winlux collects no usage data: telemetry and location history are off by default. You decide everything.", b"Winlux no recopila ning\xFAn dato de uso: la telemetr\xEDa y el historial de ubicaciones est\xE1n desactivados de forma predeterminada. Usted decide todo.", b"Winlux sammelt keine Nutzungsdaten: Telemetrie und Standortverlauf sind standardm\xE4\xDFig deaktiviert. Sie entscheiden \xFCber alles."],
        tips: [
            [b"Autorisez ou bloquez le micro, la cam\xE9ra et la localisation.", b"Allow or block the microphone, camera and location.", b"Permita o bloquee el micr\xF3fono, la c\xE1mara y la ubicaci\xF3n.", b"Mikrofon, Kamera und Standort zulassen oder blockieren."],
            [b"Mises \xE0 jour : automatiques, manuelles ou planifi\xE9es.", b"Updates: automatic, manual or scheduled.", b"Actualizaciones: autom\xE1ticas, manuales o programadas.", b"Updates: automatisch, manuell oder geplant."],
            [b"Param\xE8tres > Confidentialit\xE9 pour tout v\xE9rifier.", b"Settings > Privacy to check everything.", b"Configuraci\xF3n > Privacidad para revisarlo todo.", b"Einstellungen > Datenschutz, um alles zu pr\xFCfen."],
        ],
        action: [b"Ouvrir Confidentialit\xE9", b"Open Privacy", b"Abrir Privacidad", b"Datenschutz \xF6ffnen"],
    },
    Step {
        title: [b"Choisissez votre style", b"Choose your style", b"Elija su estilo", b"W\xE4hlen Sie Ihren Stil"],
        text: [b"Clair ou sombre ? Barre centr\xE9e ou \xE0 gauche ? Votre choix s'applique tout de suite et reste modifiable dans Param\xE8tres > Apparence.", b"Light or dark? Centered or left taskbar? Your choice applies right away and can be changed in Settings > Appearance.", b"\xBFClaro u oscuro? \xBFBarra centrada o a la izquierda? Su elecci\xF3n se aplica al momento y se puede cambiar en Configuraci\xF3n > Apariencia.", b"Hell oder dunkel? Leiste zentriert oder links? Ihre Wahl gilt sofort und l\xE4sst sich unter Einstellungen > Darstellung \xE4ndern."],
        tips: [NONE, NONE, NONE],
        action: NONE,
    },
];

const SHORTCUTS: [(Tr4, Tr4); 10] = [
    ([b"Windows", b"Windows", b"Windows", b"Windows"], [b"Menu D\xE9marrer", b"Start menu", b"Men\xFA Inicio", b"Startmen\xFC"]),
    ([b"Windows + E", b"Windows + E", b"Windows + E", b"Windows + E"], [b"Explorateur de fichiers", b"File Explorer", b"Explorador de archivos", b"Datei-Explorer"]),
    ([b"Windows + I", b"Windows + I", b"Windows + I", b"Windows + I"], [b"Param\xE8tres", b"Settings", b"Configuraci\xF3n", b"Einstellungen"]),
    ([b"Windows + S", b"Windows + S", b"Windows + S", b"Windows + S"], [b"Recherche", b"Search", b"B\xFAsqueda", b"Suche"]),
    ([b"Windows + D", b"Windows + D", b"Windows + D", b"Windows + D"], [b"Afficher le bureau", b"Show desktop", b"Mostrar el escritorio", b"Desktop anzeigen"]),
    ([b"Windows + L", b"Windows + L", b"Windows + L", b"Windows + L"], [b"Verrouiller la session", b"Lock the session", b"Bloquear la sesi\xF3n", b"Sitzung sperren"]),
    ([b"Windows + Tab", b"Windows + Tab", b"Windows + Tab", b"Windows + Tab"], [b"Vue des t\xE2ches", b"Task view", b"Vista de tareas", b"Aktive Anwendungen"]),
    ([b"Windows + fl\xE8ches", b"Windows + arrows", b"Windows + flechas", b"Windows + Pfeiltasten"], [b"Ancrer une fen\xEAtre", b"Snap a window", b"Acoplar una ventana", b"Fenster andocken"]),
    ([b"Alt + Tab", b"Alt + Tab", b"Alt + Tab", b"Alt + Tab"], [b"Passer d'une fen\xEAtre \xE0 l'autre", b"Switch between windows", b"Cambiar de ventana", b"Zwischen Fenstern wechseln"]),
    ([b"Ctrl + Maj + \xC9chap", b"Ctrl + Shift + Esc", b"Ctrl + May\xFAs + Esc", b"Strg + Umschalt + Esc"], [b"Gestionnaire des t\xE2ches", b"Task Manager", b"Administrador de tareas", b"Task-Manager"]),
];

static mut STEP: usize = 0;
static mut HOVER: i32 = -1;
static mut CHECKED: bool = false;
static mut ANIM: u32 = 0;
const ANIM_MS: i32 = 220;

const BTN_NEXT: i32 = 0;
const BTN_BACK: i32 = 1;
const BTN_SKIP: i32 = 2;
const BTN_ACTION: i32 = 3;
const BTN_DARK: i32 = 4;
const BTN_LIGHT: i32 = 5;
const BTN_CENTER: i32 = 6;
const BTN_LEFT: i32 = 7;
const BTN_DOT: i32 = 10;

/// Ouvre la visite une seule fois, au premier login d'une session Winlux.
pub unsafe fn maybe_show() -> bool {
    if CHECKED || crate::wm::native_session() || !crate::settings::service_available() {
        return false;
    }
    CHECKED = true;
    if crate::settings::status_value(b"welcome_done") == b"1" {
        return false;
    }
    // Marquée vue dès l'ouverture : fermer la fenêtre suffit à ne plus la revoir.
    crate::settings::control(c"welcome-done");
    open();
    true
}

pub unsafe fn open() {
    STEP = 0;
    HOVER = -1;
    ANIM = GetTickCount();
    crate::wm::open(crate::wm::APP_WELCOME);
}

fn panel_rect(f: &Frame) -> Rect {
    let c = f.client;
    let w = ((c.right - c.left) * 2 / 5).clamp(220, 380);
    rect(c.left, c.top, c.left + w, c.bottom)
}
fn body_left(f: &Frame) -> i32 {
    panel_rect(f).right + 32
}
fn footer_y(f: &Frame) -> i32 {
    f.client.bottom - 58
}

fn button_rect(f: &Frame, id: i32) -> Rect {
    let c = f.client;
    let y = footer_y(f) + 12;
    let x = body_left(f);
    match id {
        BTN_NEXT => rect(c.right - 152, y, c.right - 24, y + 34),
        BTN_BACK => rect(c.right - 272, y, c.right - 160, y + 34),
        BTN_SKIP => rect(x - 8, y, x + 120, y + 34),
        BTN_ACTION => {
            let t = tips_top(f) + 3 * 30 + 12;
            rect(x, t, x + 230, t + 34)
        }
        BTN_DARK | BTN_LIGHT => {
            let t = tips_top(f);
            let l = x + (id - BTN_DARK) * 170;
            rect(l, t, l + 156, t + 104)
        }
        BTN_CENTER | BTN_LEFT => {
            let t = tips_top(f) + 150;
            let l = x + (id - BTN_CENTER) * 170;
            rect(l, t, l + 156, t + 34)
        }
        _ => {
            let i = id - BTN_DOT;
            let n = STEPS.len() as i32;
            let cx = (panel_rect(f).left + panel_rect(f).right) / 2 - n * 8 + i * 16;
            let cy = f.client.bottom - 34;
            rect(cx, cy, cx + 10, cy + 10)
        }
    }
}

fn tips_top(f: &Frame) -> i32 {
    f.client.top + 196
}

fn visible(id: i32) -> bool {
    let s = unsafe { STEP };
    match id {
        BTN_BACK => s > 0,
        BTN_SKIP => s + 1 < STEPS.len(),
        BTN_ACTION => !STEPS[s].action[0].is_empty(),
        BTN_DARK | BTN_LIGHT | BTN_CENTER | BTN_LEFT => s + 1 == STEPS.len(),
        _ => true,
    }
}

fn hit(f: &Frame, x: i32, y: i32) -> i32 {
    let mut ids = [BTN_NEXT, BTN_BACK, BTN_SKIP, BTN_ACTION, BTN_DARK, BTN_LIGHT, BTN_CENTER, BTN_LEFT, 0];
    for i in 0..STEPS.len() as i32 {
        ids[8] = BTN_DOT + i;
        let r = button_rect(f, ids[8]);
        if inside(&rect(r.left - 3, r.top - 3, r.right + 3, r.bottom + 3), x, y) {
            return ids[8];
        }
    }
    for &id in &ids[..8] {
        if visible(id) && inside(&button_rect(f, id), x, y) {
            return id;
        }
    }
    -1
}

/// Paragraphe coupé aux espaces ; renvoie la hauteur utilisée.
unsafe fn paragraph(g: &mut Gfx, font: Handle, size: i32, color: u32, x: i32, y: i32, w: i32, s: &[u8]) -> i32 {
    let mut rest = s;
    let mut line = 0;
    while !rest.is_empty() {
        let mut end = rest.len();
        if text_width(rest, size) > w {
            end = 0;
            let mut i = 0;
            while i < rest.len() && text_width(&rest[..i + 1], size) <= w {
                if rest[i] == b' ' {
                    end = i;
                }
                i += 1;
            }
            if end == 0 {
                end = i.max(1);
            }
        }
        text(g.hdc, font, color, x, y + line * (size + 9), &rest[..end]);
        line += 1;
        rest = &rest[end..];
        while rest.first() == Some(&b' ') {
            rest = &rest[1..];
        }
    }
    line * (size + 9)
}

unsafe fn pill(g: &mut Gfx, r: &Rect, label: &[u8], primary: bool, hover: bool, selected: bool) {
    let p = pal();
    let border = if primary || selected { p.accent } else { p.border };
    let fill_c = if primary {
        if hover { blend(rgb(255, 255, 255), p.accent, 30) } else { p.accent }
    } else if selected {
        p.selected
    } else if hover {
        p.hover
    } else {
        p.surface2
    };
    panel(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, fill_c, border);
    text_centered(
        g.hdc,
        g.f.semibold,
        13,
        if primary { p.on_accent } else { p.text },
        (r.left + r.right) / 2,
        (r.top + r.bottom) / 2 - 8,
        label,
    );
}

/// Mini bureau commun aux illustrations : écran, fond et barre des tâches.
unsafe fn mini_screen(g: &mut Gfx, r: &Rect) -> Rect {
    let p = pal();
    panel(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 10, p.window, p.border);
    let s = rect(r.left + 6, r.top + 6, r.right - 6, r.bottom - 6);
    fill(g.hdc, g.b, s.left, s.top, s.right, s.bottom, blend(p.accent, p.mica, 22));
    fill(g.hdc, g.b, s.left, s.bottom - 16, s.right, s.bottom, p.surface);
    let cx = (s.left + s.right) / 2;
    for i in -2..3 {
        round_rect(g.hdc, g.b, cx + i * 16 - 5, s.bottom - 13, cx + i * 16 + 5, s.bottom - 3, 2, if i == -2 { p.accent } else { p.text3 });
    }
    s
}

unsafe fn illustration(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let pr = panel_rect(f);
    fill(g.hdc, g.b, pr.left, pr.top, pr.right, pr.bottom, blend(p.accent, p.surface, 18));
    let w = (pr.right - pr.left - 56).min(300);
    let h = w * 2 / 3;
    let cx = (pr.left + pr.right) / 2;
    let cy = (pr.top + pr.bottom) / 2 - 20;
    // Légère montée à chaque étape, comme les pages d'accueil de Windows 11.
    let t = ease_out(GetTickCount().wrapping_sub(ANIM).min(10_000) as i32, ANIM_MS);
    let dy = 14 - 14 * t / 1000;
    let r = rect(cx - w / 2, cy - h / 2 + dy, cx + w / 2, cy + h / 2 + dy);
    match STEP {
        0 => {
            let s = w / 3;
            glyph(g.hdc, g.b, g.f, Glyph::Start, cx - s / 2, r.top + 10, s);
            text_centered(g.hdc, g.f.display, 28, p.text, cx, r.top + s + 30, b"Winlux");
            text_centered(g.hdc, g.f.body, 13, p.text2, cx, r.top + s + 70, tr4(&[b"Linux + programmes Windows", b"Linux + Windows programs", b"Linux + programas de Windows", b"Linux + Windows-Programme"]));
        }
        1 => {
            let s = mini_screen(g, &r);
            let mw = (s.right - s.left) / 2;
            let m = rect((s.left + s.right) / 2 - mw / 2, s.top + 14, (s.left + s.right) / 2 + mw / 2, s.bottom - 22);
            panel(g.hdc, g.b, m.left, m.top, m.right, m.bottom, 6, p.surface, p.border);
            round_rect(g.hdc, g.b, m.left + 8, m.top + 8, m.right - 8, m.top + 18, 4, p.surface2);
            for i in 0..8 {
                let x = m.left + 10 + (i % 4) * ((m.right - m.left - 20) / 4);
                let y = m.top + 28 + (i / 4) * 24;
                round_rect(g.hdc, g.b, x, y, x + 14, y + 14, 3, if i == 0 { p.accent } else { p.text3 });
            }
            let b = rect((s.left + s.right) / 2 - 37, s.bottom - 15, (s.left + s.right) / 2 - 27, s.bottom - 1);
            ring(g.hdc, g.b, b.left - 6, b.top - 6, b.right + 6, b.bottom + 4, 2, p.accent, blend(p.accent, p.surface, 30));
        }
        2 | 3 => {
            let s = mini_screen(g, &r);
            let win = rect(s.left + 16, s.top + 12, s.right - 16, s.bottom - 24);
            panel(g.hdc, g.b, win.left, win.top, win.right, win.bottom, 6, p.surface, p.border);
            fill(g.hdc, g.b, win.left + 1, win.top + 1, win.right - 1, win.top + 12, p.surface2);
            let nav = win.left + (win.right - win.left) / 4;
            fill(g.hdc, g.b, nav, win.top + 12, nav + 1, win.bottom - 1, p.border);
            for i in 0..4 {
                let y = win.top + 20 + i * 14;
                fill(g.hdc, g.b, win.left + 6, y, nav - 6, y + 6, if i == 1 { p.accent } else { p.text3 });
            }
            if STEP == 2 {
                for i in 0..6 {
                    let x = nav + 10 + (i % 3) * ((win.right - nav - 16) / 3);
                    let y = win.top + 20 + (i / 3) * 28;
                    glyph(g.hdc, g.b, g.f, if i == 4 { Glyph::Exe } else { Glyph::Folder }, x, y, 18);
                }
            } else {
                for i in 0..4 {
                    let y = win.top + 20 + i * 16;
                    round_rect(g.hdc, g.b, nav + 8, y, win.right - 8, y + 12, 3, p.surface2);
                    let on = i % 2 == 0;
                    round_rect(g.hdc, g.b, win.right - 30, y + 2, win.right - 12, y + 10, 4, if on { p.accent } else { p.text3 });
                    ellipse(g.hdc, g.b, if on { win.right - 20 } else { win.right - 29 }, y + 3, if on { win.right - 13 } else { win.right - 22 }, y + 9, p.surface);
                }
            }
        }
        4 => {
            panel(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, rgb(0x0C, 0x0C, 0x0C), p.border);
            fill(g.hdc, g.b, r.left + 1, r.top + 1, r.right - 1, r.top + 22, rgb(0x20, 0x20, 0x20));
            text(g.hdc, g.f.small, rgb(0xCC, 0xCC, 0xCC), r.left + 10, r.top + 5, b"Terminal");
            let lines: [&[u8]; 4] = [tr4(&[b"$ ls ~/T\xE9l\xE9chargements", b"$ ls ~/Downloads", b"$ ls ~/Descargas", b"$ ls ~/Downloads"]), b"setup.exe  photos  notes.txt", b"$ winlux setup.exe", b"$ _"];
            for (i, l) in lines.iter().enumerate() {
                let c = if l.starts_with(b"$") { rgb(0x6C, 0xD1, 0x7A) } else { rgb(0xD0, 0xD0, 0xD0) };
                text(g.hdc, g.f.small, c, r.left + 12, r.top + 32 + i as i32 * 18, l);
            }
        }
        5 => {
            let key = |g: &mut Gfx, x: i32, y: i32, w: i32, label: &[u8]| {
                let p = pal();
                panel(g.hdc, g.b, x, y + 4, x + w, y + 48, 7, p.border, p.border);
                panel(g.hdc, g.b, x, y, x + w, y + 42, 7, p.surface, p.border);
                text_centered(g.hdc, g.f.title, 15, p.text, x + w / 2, y + 12, label);
            };
            let y = cy - 24 + dy;
            key(g, cx - 120, y, 110, b"Windows");
            text_centered(g.hdc, g.f.large, 22, p.text2, cx + 6, y + 8, b"+");
            key(g, cx + 26, y, 54, b"E");
            glyph(g.hdc, g.b, g.f, Glyph::Folder, cx + 96, y + 8, 26);
        }
        6 => {
            let s = h * 2 / 3;
            let l = cx - s / 2;
            let top = r.top + 10;
            ring(g.hdc, g.b, l + s / 6, top, l + s - s / 6, top + s * 2 / 3, s / 9, p.text2, blend(p.accent, p.surface, 18));
            fill(g.hdc, g.b, l, top + s / 3, l + s, top + s / 3 + 4, blend(p.accent, p.surface, 18));
            round_rect(g.hdc, g.b, l, top + s / 3, l + s, top + s + 10, 10, p.accent);
            ellipse(g.hdc, g.b, cx - 8, top + s * 2 / 3 - 6, cx + 8, top + s * 2 / 3 + 10, p.on_accent);
            fill(g.hdc, g.b, cx - 3, top + s * 2 / 3 + 6, cx + 3, top + s * 2 / 3 + 22, p.on_accent);
            text_centered(g.hdc, g.f.semibold, 13, p.text, cx, top + s + 28, tr4(&[b"Aucune t\xE9l\xE9m\xE9trie", b"No telemetry", b"Sin telemetr\xEDa", b"Keine Telemetrie"]));
        }
        _ => {
            for (i, dark) in [true, false].iter().enumerate() {
                let w2 = w / 2 - 8;
                let l = r.left + i as i32 * (w2 + 16);
                let (bg, fg, bar) = if *dark {
                    (rgb(0x20, 0x20, 0x20), rgb(0x2C, 0x2C, 0x2C), rgb(0x1A, 0x1A, 0x1A))
                } else {
                    (rgb(0xF3, 0xF3, 0xF3), rgb(0xFF, 0xFF, 0xFF), rgb(0xE8, 0xE8, 0xE8))
                };
                panel(g.hdc, g.b, l, r.top + 20, l + w2, r.bottom - 20, 8, bg, p.border);
                round_rect(g.hdc, g.b, l + 10, r.top + 34, l + w2 - 10, r.bottom - 50, 5, fg);
                fill(g.hdc, g.b, l + 1, r.bottom - 38, l + w2 - 1, r.bottom - 21, bar);
                round_rect(g.hdc, g.b, l + w2 / 2 - 5, r.bottom - 34, l + w2 / 2 + 5, r.bottom - 25, 2, p.accent);
            }
        }
    }
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let c = f.client;
    fill(g.hdc, g.b, c.left, c.top, c.right, c.bottom, p.mica);
    illustration(g, f);
    let step = &STEPS[STEP];
    let x = body_left(f);
    let w = c.right - x - 32;
    // « ÉTAPE n SUR m » : positions des deux chiffres selon la langue.
    let (model, a, b): (&[u8], usize, usize) = match ui_lang() {
        1 => (b"STEP 0 OF 0", 5, 10),
        2 => (b"PASO 0 DE 0", 5, 10),
        3 => (b"SCHRITT 0 VON 0", 8, 14),
        _ => (b"\xC9TAPE 0 SUR 0", 6, 12),
    };
    let mut label = [0u8; 16];
    label[..model.len()].copy_from_slice(model);
    label[a] = b'1' + STEP as u8;
    label[b] = b'0' + STEPS.len() as u8;
    text(g.hdc, g.f.small, p.accent, x, c.top + 36, &label[..model.len()]);
    text(g.hdc, g.f.display, p.text, x, c.top + 56, tr4(&step.title));
    paragraph(g, g.f.body, 13, p.text2, x, c.top + 108, w, tr4(&step.text));
    let top = tips_top(f);
    match STEP {
        5 => {
            let col = (w / 2).max(140);
            for (i, (keys, what)) in SHORTCUTS.iter().enumerate() {
                let y = top - 24 + (i as i32 / 2) * 44;
                let l = x + (i as i32 % 2) * col;
                if y + 36 > footer_y(f) {
                    break;
                }
                text_fit(g.hdc, g.f.semibold, 13, p.text, l, y, tr4(keys), col - 12);
                text_fit(g.hdc, g.f.small, 11, p.text2, l, y + 18, tr4(what), col - 12);
            }
        }
        7 => {
            let dark = crate::settings::status_value(b"theme") != b"light";
            let left = crate::desktop::ALIGN_LEFT;
            for (id, label, sel) in [(BTN_DARK, tr4(&[b"Sombre", b"Dark", b"Oscuro", b"Dunkel"]), dark), (BTN_LIGHT, tr4(&[b"Clair", b"Light", b"Claro", b"Hell"]), !dark)] {
                let r = button_rect(f, id);
                let (bg, fg) = if id == BTN_DARK {
                    (rgb(0x20, 0x20, 0x20), rgb(0xF0, 0xF0, 0xF0))
                } else {
                    (rgb(0xF6, 0xF6, 0xF6), rgb(0x1A, 0x1A, 0x1A))
                };
                round_rect(g.hdc, g.b, r.left - 2, r.top - 2, r.right + 2, r.bottom + 2, 9, if sel { p.accent } else if HOVER == id { p.text3 } else { p.border });
                round_rect(g.hdc, g.b, r.left + 1, r.top + 1, r.right - 1, r.bottom - 1, 7, bg);
                round_rect(g.hdc, g.b, r.left + 14, r.top + 14, r.right - 14, r.bottom - 40, 4, blend(fg, bg, 12));
                text(g.hdc, g.f.semibold, fg, r.left + 14, r.bottom - 28, label);
            }
            text(g.hdc, g.f.semibold, p.text, x, top + 124, tr4(&[b"Barre des t\xE2ches", b"Taskbar", b"Barra de tareas", b"Taskleiste"]));
            pill(g, &button_rect(f, BTN_CENTER), tr4(&[b"Centr\xE9e", b"Centered", b"Centrada", b"Zentriert"]), false, HOVER == BTN_CENTER, !left);
            pill(g, &button_rect(f, BTN_LEFT), tr4(&[b"\xC0 gauche", b"Left", b"A la izquierda", b"Links"]), false, HOVER == BTN_LEFT, left);
        }
        _ => {
            for (i, tip) in step.tips.iter().enumerate() {
                let y = top + i as i32 * 30;
                ellipse(g.hdc, g.b, x, y + 4, x + 8, y + 12, p.accent);
                text_fit(g.hdc, g.f.body, 13, p.text, x + 20, y, tr4(tip), w - 20);
            }
            if visible(BTN_ACTION) {
                pill(g, &button_rect(f, BTN_ACTION), tr4(&step.action), false, HOVER == BTN_ACTION, false);
            }
        }
    }
    fill(g.hdc, g.b, panel_rect(f).right, footer_y(f), c.right, footer_y(f) + 1, p.border);
    for i in 0..STEPS.len() as i32 {
        let r = button_rect(f, BTN_DOT + i);
        let current = i as usize == STEP;
        if current {
            round_rect(g.hdc, g.b, r.left - 4, r.top, r.right + 4, r.bottom, 5, p.accent);
        } else {
            ellipse(g.hdc, g.b, r.left + 1, r.top + 1, r.right - 1, r.bottom - 1, if HOVER == BTN_DOT + i { p.text2 } else { p.text3 });
        }
    }
    if visible(BTN_SKIP) {
        let r = button_rect(f, BTN_SKIP);
        if HOVER == BTN_SKIP {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, p.hover);
        }
        text(g.hdc, g.f.body, p.text2, r.left + 12, r.top + 9, tr4(&[b"Passer la visite", b"Skip the tour", b"Omitir el recorrido", b"Tour \xFCberspringen"]));
    }
    if visible(BTN_BACK) {
        pill(g, &button_rect(f, BTN_BACK), tr4(&[b"Pr\xE9c\xE9dent", b"Back", b"Anterior", b"Zur\xFCck"]), false, HOVER == BTN_BACK, false);
    }
    let last = STEP + 1 == STEPS.len();
    pill(g, &button_rect(f, BTN_NEXT), if last { tr4(&[b"Commencer", b"Get started", b"Empezar", b"Los geht's"]) } else { tr4(&[b"Suivant", b"Next", b"Siguiente", b"Weiter"]) }, true, HOVER == BTN_NEXT, false);
}

unsafe fn go(step: usize) {
    if step != STEP && step < STEPS.len() {
        STEP = step;
        ANIM = GetTickCount();
    }
}

unsafe fn finish() {
    crate::wm::close(crate::wm::APP_WELCOME);
}

unsafe fn activate(id: i32) -> bool {
    match id {
        BTN_NEXT if STEP + 1 == STEPS.len() => finish(),
        BTN_NEXT => go(STEP + 1),
        BTN_BACK if STEP > 0 => go(STEP - 1),
        BTN_SKIP => finish(),
        BTN_ACTION => match STEP {
            1 => crate::desktop::open_start_menu(),
            2 => crate::explorer::open_home(),
            3 => crate::settings::open_page(0),
            4 => crate::wm::open(crate::wm::APP_TERMINAL),
            6 => crate::settings::open_page(crate::settings::PAGE_PRIVACY),
            _ => {}
        },
        BTN_DARK | BTN_LIGHT => {
            let dark = id == BTN_DARK;
            set_theme(dark);
            crate::settings::control(if dark { c"theme-dark" } else { c"theme-light" });
        }
        BTN_CENTER | BTN_LEFT => {
            crate::desktop::ALIGN_LEFT = id == BTN_LEFT;
            crate::settings::control(if id == BTN_LEFT { c"align-left" } else { c"align-center" });
        }
        _ if id >= BTN_DOT => go((id - BTN_DOT) as usize),
        _ => return false,
    }
    true
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    match ev {
        Mouse::Move => {
            let h = hit(f, x, y);
            let changed = h != HOVER;
            HOVER = h;
            changed
        }
        Mouse::Down | Mouse::DoubleClick => {
            let h = hit(f, x, y);
            h >= 0 && activate(h)
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    match (vk, ch) {
        (0x27, 0) | (0, 13) => activate(BTN_NEXT),
        (0x25, 0) => activate(BTN_BACK),
        (0x24, 0) => {
            go(0);
            true
        }
        _ => false,
    }
}

pub unsafe fn tick(_f: &Frame) -> bool {
    false
}

/// Vrai pendant l'animation d'entrée d'une étape (la boucle doit repeindre).
pub fn animating() -> bool {
    unsafe { crate::wm::is_visible(crate::wm::APP_WELCOME) && (GetTickCount().wrapping_sub(ANIM) as i32) < ANIM_MS + 40 }
}

pub unsafe fn regression_checks() {
    let saved = STEP;
    let f = Frame {
        outer: rect(0, 0, 900, 600),
        title: rect(0, 0, 900, 40),
        client: rect(0, 40, 900, 600),
    };
    STEP = 0;
    check!(!visible(BTN_BACK) && visible(BTN_SKIP), "Bienvenue : pas de bouton Precedent a la premiere etape");
    let next = button_rect(&f, BTN_NEXT);
    check!(hit(&f, next.left + 5, next.top + 5) == BTN_NEXT, "Bienvenue : Suivant est cliquable");
    activate(BTN_NEXT);
    check!(STEP == 1 && visible(BTN_ACTION), "Bienvenue : l'etape Demarrer propose de l'essayer");
    let dot = button_rect(&f, BTN_DOT + 5);
    check!(hit(&f, dot.left + 4, dot.top + 4) == BTN_DOT + 5, "Bienvenue : les points de progression menent a une etape");
    go(STEPS.len() - 1);
    check!(visible(BTN_DARK) && !visible(BTN_SKIP), "Bienvenue : la derniere etape propose le theme clair ou sombre");
    let r = button_rect(&f, BTN_LEFT);
    check!(r.bottom < footer_y(&f), "Bienvenue : les choix de la barre ne chevauchent pas le pied de page");
    STEP = saved;
}
