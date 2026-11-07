//! Utilitaires : la page des outils d'administration du shell (comme le
//! dossier « Outils Windows » de Windows 11). Chaque ligne ouvre l'outil ;
//! souris, flèches, Entrée et Échap fonctionnent (#209 : la fenêtre était vide).

use crate::gfx::*;
use crate::wm::{self, Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

struct Tool {
    app: usize,
    name: [&'static [u8]; 4],
    about: [&'static [u8]; 4],
}

const TOOLS: [Tool; 10] = [
    Tool {
        app: wm::APP_TASKMGR,
        name: [
            b"Gestionnaire des t\xE2ches",
            b"Task Manager",
            b"Administrador de tareas",
            b"Task-Manager",
        ],
        about: [
            b"Processus, performances et applications au d\xE9marrage",
            b"Processes, performance and startup apps",
            b"Procesos, rendimiento y aplicaciones de inicio",
            b"Prozesse, Leistung und Autostart-Apps",
        ],
    },
    Tool {
        app: wm::APP_SYSINFO,
        name: [
            b"Informations syst\xE8me",
            b"System Information",
            b"Informaci\xF3n del sistema",
            b"Systeminformationen",
        ],
        about: [
            b"Processeur, m\xE9moire, r\xE9seau et p\xE9riph\xE9riques",
            b"Processor, memory, network and devices",
            b"Procesador, memoria, red y dispositivos",
            b"Prozessor, Speicher, Netzwerk und Ger\xE4te",
        ],
    },
    Tool {
        app: wm::APP_DISKMGR,
        name: [
            b"Gestion des disques",
            b"Disk Management",
            b"Administraci\xF3n de discos",
            b"Datentr\xE4gerverwaltung",
        ],
        about: [
            b"Disques, partitions et lecteurs",
            b"Disks, partitions and drives",
            b"Discos, particiones y unidades",
            b"Datentr\xE4ger, Partitionen und Laufwerke",
        ],
    },
    Tool {
        app: wm::APP_CLEANUP,
        name: [
            b"Nettoyage de disque",
            b"Disk Cleanup",
            b"Liberador de espacio",
            b"Datentr\xE4gerbereinigung",
        ],
        about: [
            b"Lib\xE9rer de l'espace : fichiers temporaires, caches, Corbeille",
            b"Free up space: temporary files, caches, Recycle Bin",
            b"Liberar espacio: archivos temporales, cach\xE9s, Papelera",
            b"Speicher freigeben: tempor\xE4re Dateien, Caches, Papierkorb",
        ],
    },
    Tool {
        app: wm::APP_EVENTVIEWER,
        name: [
            b"Observateur d'\xE9v\xE9nements",
            b"Event Viewer",
            b"Visor de eventos",
            b"Ereignisanzeige",
        ],
        about: [
            b"Journaux du syst\xE8me et des applications",
            b"System and application logs",
            b"Registros del sistema y de las aplicaciones",
            b"System- und Anwendungsprotokolle",
        ],
    },
    Tool {
        app: wm::APP_TASKSCHD,
        name: [
            b"Planificateur de t\xE2ches",
            b"Task Scheduler",
            b"Programador de tareas",
            b"Aufgabenplanung",
        ],
        about: [
            b"T\xE2ches lanc\xE9es automatiquement",
            b"Tasks that run automatically",
            b"Tareas que se ejecutan autom\xE1ticamente",
            b"Automatisch ausgef\xFChrte Aufgaben",
        ],
    },
    Tool {
        app: wm::APP_FIREWALL,
        name: [b"Pare-feu", b"Firewall", b"Firewall", b"Firewall"],
        about: [
            b"Applications autoris\xE9es \xE0 recevoir des connexions",
            b"Apps allowed to receive connections",
            b"Aplicaciones que pueden recibir conexiones",
            b"Apps, die Verbindungen empfangen d\xFCrfen",
        ],
    },
    Tool {
        app: wm::APP_REGEDIT,
        name: [
            b"\xC9diteur du Registre",
            b"Registry Editor",
            b"Editor del Registro",
            b"Registrierungs-Editor",
        ],
        about: [
            b"Cl\xE9s et valeurs du Registre des programmes Windows",
            b"Registry keys and values of Windows programs",
            b"Claves y valores del Registro de los programas de Windows",
            b"Registrierungsschl\xFCssel und -werte der Windows-Programme",
        ],
    },
    Tool {
        app: wm::APP_RESTORE,
        name: [
            b"Restauration du syst\xE8me",
            b"System Restore",
            b"Restaurar sistema",
            b"Systemwiederherstellung",
        ],
        about: [
            b"Points de restauration et retour en arri\xE8re",
            b"Restore points and roll back",
            b"Puntos de restauraci\xF3n y volver atr\xE1s",
            b"Wiederherstellungspunkte und Zur\xFCcksetzen",
        ],
    },
    Tool {
        app: wm::APP_FONTS,
        name: [b"Polices", b"Fonts", b"Fuentes", b"Schriftarten"],
        about: [
            b"Polices install\xE9es et aper\xE7u",
            b"Installed fonts and preview",
            b"Fuentes instaladas y vista previa",
            b"Installierte Schriftarten und Vorschau",
        ],
    },
];

const HEADER: i32 = 72;
const ROW: i32 = 52;

static mut HOVER: i32 = -1;
static mut FOCUS: i32 = 0;

fn row_at(f: &Frame, x: i32, y: i32) -> i32 {
    let r = f.client;
    if x < r.left + 16 || x >= r.right - 16 || y < r.top + HEADER {
        return -1;
    }
    let i = (y - r.top - HEADER) / ROW;
    if (i as usize) < TOOLS.len() && r.top + HEADER + (i + 1) * ROW <= r.bottom {
        i
    } else {
        -1
    }
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    let p = pal();
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.window);
    text(
        g.hdc,
        g.f.title,
        p.text,
        r.left + 24,
        r.top + 18,
        tr4(&[
            b"Utilitaires",
            b"Utilities",
            b"Utilidades",
            b"Dienstprogramme",
        ]),
    );
    text(
        g.hdc,
        g.f.small,
        p.text2,
        r.left + 24,
        r.top + 46,
        tr4(&[
            b"Outils d'administration de Winlux",
            b"Winlux administrative tools",
            b"Herramientas administrativas de Winlux",
            b"Verwaltungstools von Winlux",
        ]),
    );
    for (i, tool) in TOOLS.iter().enumerate() {
        let i = i as i32;
        let top = r.top + HEADER + i * ROW;
        if top + ROW > r.bottom {
            break;
        }
        let bg = if i == HOVER {
            p.hover
        } else if i == FOCUS {
            p.surface2
        } else {
            p.surface
        };
        round_rect(
            g.hdc,
            g.b,
            r.left + 16,
            top + 2,
            r.right - 16,
            top + ROW - 2,
            4,
            bg,
        );
        if i == FOCUS {
            fill(
                g.hdc,
                g.b,
                r.left + 16,
                top + 12,
                r.left + 19,
                top + ROW - 12,
                p.accent,
            );
        }
        text(
            g.hdc,
            g.f.body,
            p.text,
            r.left + 32,
            top + 8,
            tr4(&tool.name),
        );
        text(
            g.hdc,
            g.f.small,
            p.text2,
            r.left + 32,
            top + 28,
            tr4(&tool.about),
        );
    }
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let i = row_at(f, x, y);
    match ev {
        Mouse::Move => {
            let old = HOVER;
            HOVER = i;
            old != i
        }
        Mouse::Down | Mouse::DoubleClick if i >= 0 => {
            FOCUS = i;
            wm::open(TOOLS[i as usize].app);
            true
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, _ch: u16) -> bool {
    let n = TOOLS.len() as i32;
    match vk {
        0x26 => FOCUS = (FOCUS + n - 1) % n,    // Haut
        0x28 | 0x09 => FOCUS = (FOCUS + 1) % n, // Bas, Tab
        0x24 => FOCUS = 0,                      // Début
        0x23 => FOCUS = n - 1,                  // Fin
        0x0D | 0x20 => wm::open(TOOLS[FOCUS as usize].app),
        _ => return false,
    }
    true
}

pub unsafe fn tick(_f: &Frame) -> bool {
    false
}
