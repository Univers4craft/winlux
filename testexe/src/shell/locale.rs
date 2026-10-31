//! Système d'internationalisation (i18n) pour le bureau Winlux.
//! Tables de chaînes statiques : français, anglais, espagnol, allemand.

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Lang {
    Fr = 0,
    En = 1,
    Es = 2,
    De = 3,
}

pub static mut CURRENT_LANG: Lang = Lang::Fr;

pub const LANG_COUNT: usize = 4;
pub const LANG_CODES: [&[u8]; LANG_COUNT] = [b"fr", b"en", b"es", b"de"];
pub const LANG_NAMES: [&[u8]; LANG_COUNT] = [
    b"Fran\xE7ais",
    b"English",
    b"Espa\xF1ol",
    b"Deutsch",
];

pub fn lang_from_code(code: &[u8]) -> Lang {
    match code {
        b"en" => Lang::En,
        b"es" => Lang::Es,
        b"de" => Lang::De,
        _ => Lang::Fr,
    }
}

// ── Identifiants de chaînes ────────────────────────────────────────

// Pages des Paramètres
pub const S_SYSTEM_LINUX: usize = 0;
pub const S_APPS_WINDOWS: usize = 1;
pub const S_NETWORK: usize = 2;
pub const S_SOUND_BATTERY: usize = 3;
pub const S_KEYBOARD_TOUCHPAD: usize = 4;
pub const S_MOUSE: usize = 5;
pub const S_APPEARANCE: usize = 6;
pub const S_UPDATES: usize = 7;
pub const S_DISPLAY: usize = 8;
pub const S_ABOUT: usize = 9;
pub const S_EXECUTABLE: usize = 10;
pub const S_BROWSERS: usize = 11;
pub const S_ACCESSIBILITY: usize = 12;
pub const S_VISUAL_EFFECTS: usize = 13;
pub const S_LOCALIZATION: usize = 14;

// Libellés généraux des Paramètres
pub const S_SETTINGS: usize = 15;
pub const S_SETTINGS_SUBTITLE: usize = 16;
pub const S_OPERATION_IN_PROGRESS: usize = 17;
pub const S_OPERATION_DONE: usize = 18;
pub const S_OPERATION_FAILED: usize = 19;
pub const S_INFO_UNAVAILABLE: usize = 20;

// Page Système Linux
pub const S_RESTART_SESSION: usize = 21;
pub const S_REBOOT: usize = 22;
pub const S_SHUTDOWN: usize = 23;

// Page Clavier
pub const S_CURRENT_LAYOUT: usize = 24;
pub const S_ENABLE_TAP: usize = 25;
pub const S_DISABLE: usize = 26;
pub const S_TAP_STATUS: usize = 27;
pub const S_TOUCHPAD_VM: usize = 28;
pub const S_TOUCHPAD_VM_DESC: usize = 29;

// Page Son
pub const S_AUDIO_OUTPUT: usize = 30;
pub const S_VOL_DOWN: usize = 31;
pub const S_MUTE_TOGGLE: usize = 32;
pub const S_VOL_UP: usize = 33;
pub const S_BATTERY_SYSTEM: usize = 34;

// Page Souris
pub const S_MOUSE_SLOW: usize = 35;
pub const S_MOUSE_NORMAL: usize = 36;
pub const S_MOUSE_FAST: usize = 37;
pub const S_MOUSE_SPEED_LABEL: usize = 38;

// Page Apparence
pub const S_TASKBAR: usize = 39;
pub const S_PREFS_SAVED: usize = 40;
pub const S_ALIGN_LEFT: usize = 41;
pub const S_CENTER: usize = 42;
pub const S_THEME_DARK: usize = 43;
pub const S_THEME_LIGHT: usize = 44;
pub const S_AUTO_THEME: usize = 45;
pub const S_AUTO_THEME_DESC: usize = 46;
pub const S_AUTO_THEME_OFF: usize = 47;
pub const S_ENABLE: usize = 48;
pub const S_STORAGE: usize = 49;
pub const S_PERSISTENT_DESC: usize = 50;
pub const S_LIVE_DESC: usize = 51;
pub const S_DESKTOP_ICONS: usize = 52;
pub const S_DESKTOP_ICONS_DESC: usize = 53;
pub const S_SHOW_TRASH: usize = 54;
pub const S_SHOW_EXPLORER: usize = 55;
pub const S_SHOW_PC: usize = 56;
pub const S_SHOW_DOCUMENTS: usize = 57;
pub const S_DISPLAY_SETTINGS: usize = 58;

// Page Réseau
pub const S_NETWORK_RENEW: usize = 59;
pub const S_STATUS: usize = 60;

// Page Mises à jour
pub const S_CHECK_UPDATES: usize = 61;
pub const S_INSTALL_UPDATES: usize = 62;

// Page Écran
pub const S_CURRENT_RES: usize = 63;
pub const S_PREV_MODE: usize = 64;
pub const S_NEXT_MODE: usize = 65;
pub const S_APPLY: usize = 66;

// Page Accessibilité
pub const S_POINTER_NORMAL: usize = 67;
pub const S_POINTER_LARGE: usize = 68;
pub const S_POINTER_XLARGE: usize = 69;
pub const S_CONTRAST_ON: usize = 70;
pub const S_CONTRAST_OFF: usize = 71;

// Page Navigateurs
pub const S_INSTALL: usize = 72;
pub const S_DEFAULT: usize = 73;
pub const S_LAUNCH: usize = 74;

// Page Localisation
pub const S_LANGUAGE: usize = 75;
pub const S_LANGUAGE_DESC: usize = 76;
pub const S_KEYBOARD_LAYOUT: usize = 77;
pub const S_KEYBOARD_LAYOUT_DESC: usize = 78;
pub const S_TIMEZONE: usize = 79;
pub const S_TIMEZONE_DESC: usize = 80;
pub const S_CURRENT_TIMEZONE: usize = 81;
pub const S_RESTART_FOR_LANG: usize = 82;

// Desktop / Taskbar
pub const S_START: usize = 83;
pub const S_SEARCH: usize = 84;
pub const S_SEARCH_PLACEHOLDER: usize = 85;
pub const S_PINNED: usize = 86;
pub const S_RECOMMENDED: usize = 87;
pub const S_ALL_APPS: usize = 88;
pub const S_POWER: usize = 89;
pub const S_LOCK: usize = 90;
pub const S_SIGN_OUT: usize = 91;
pub const S_SLEEP: usize = 92;

// Explorer
pub const S_HOME: usize = 93;
pub const S_DOCUMENTS: usize = 94;
pub const S_IMAGES: usize = 95;
pub const S_MUSIC: usize = 96;
pub const S_VIDEOS: usize = 97;
pub const S_DOWNLOADS: usize = 98;
pub const S_DESKTOP_FOLDER: usize = 99;
pub const S_THIS_PC: usize = 100;
pub const S_TRASH: usize = 101;
pub const S_NEW_FOLDER: usize = 102;
pub const S_COPY: usize = 103;
pub const S_CUT: usize = 104;
pub const S_PASTE: usize = 105;
pub const S_DELETE: usize = 106;
pub const S_RENAME: usize = 107;
pub const S_PROPERTIES: usize = 108;
pub const S_SELECT_ALL: usize = 109;
pub const S_NAME: usize = 110;
pub const S_DATE: usize = 111;
pub const S_TYPE: usize = 112;
pub const S_SIZE: usize = 113;
pub const S_ITEMS: usize = 114;
pub const S_SELECTED: usize = 115;
pub const S_SEARCH_HERE: usize = 116;
pub const S_OPEN: usize = 117;
pub const S_EMPTY_TRASH: usize = 118;

// Applications
pub const S_FILE_EXPLORER: usize = 119;
pub const S_SETTINGS_APP: usize = 120;
pub const S_NOTEPAD: usize = 121;
pub const S_TASK_MANAGER: usize = 122;
pub const S_TERMINAL: usize = 123;
pub const S_CALCULATOR: usize = 124;
pub const S_DISK_MANAGER: usize = 125;
pub const S_DISK_CLEANER: usize = 126;
pub const S_WEATHER: usize = 127;

// Page Apps Windows
pub const S_OPEN_PC: usize = 128;
pub const S_OPEN_HOME: usize = 129;
pub const S_OPEN_UPDATES: usize = 130;

// Page Effets visuels
pub const S_BLUR: usize = 131;
pub const S_SMOOTH_ANIM: usize = 132;
pub const S_THUMBNAILS: usize = 133;

// Fuseaux horaires courants
pub const S_TZ_PARIS: usize = 134;
pub const S_TZ_LONDON: usize = 135;
pub const S_TZ_NEW_YORK: usize = 136;
pub const S_TZ_LOS_ANGELES: usize = 137;
pub const S_TZ_TOKYO: usize = 138;
pub const S_TZ_SYDNEY: usize = 139;
pub const S_TZ_BERLIN: usize = 140;
pub const S_TZ_MADRID: usize = 141;
pub const S_TZ_MOSCOW: usize = 142;
pub const S_TZ_DUBAI: usize = 143;
pub const S_TZ_SHANGHAI: usize = 144;
pub const S_TZ_SAO_PAULO: usize = 145;

// Divers
pub const S_APPS_WINDOWS_DESC: usize = 146;
pub const S_ETHERNET_VIRTUAL: usize = 147;
pub const S_VERSION: usize = 148;
pub const S_KERNEL: usize = 149;

// WM
pub const S_UTILITIES: usize = 150;
pub const S_SWITCH_WINDOWS: usize = 151;
pub const S_RUN: usize = 152;
pub const S_OPEN_COLON: usize = 153;

// Pages Parametres ajoutees
pub const S_FILE_MANAGER: usize = 154;
pub const S_ADVANCED_SOUND: usize = 155;
pub const S_SYSTEM_UPDATES: usize = 156;
pub const S_PRIVACY: usize = 157;
pub const S_PROCESSOR: usize = 158;
pub const S_NETWORK_DRIVES: usize = 159;
pub const S_PERSONALIZATION: usize = 160;
pub const S_BLUETOOTH_DEVICES: usize = 161;
pub const S_POWER_BATTERY: usize = 162;
pub const S_GRAPHICS: usize = 163;

// Signaler un problème
pub const S_REPORT_PAGE: usize = 164;
pub const S_REPORT_DESC: usize = 165;
pub const S_REPORT_NEW: usize = 166;
pub const S_REPORT_LAST: usize = 167;
pub const S_REPORT_SAVED: usize = 168;
pub const S_REPORT_OFFER: usize = 169;
pub const S_REPORT_OFFER_ON_DESC: usize = 170;
pub const S_REPORT_OFFER_OFF_DESC: usize = 171;
pub const S_REPORT_PROPOSE: usize = 172;
pub const S_REPORT_SILENT: usize = 173;
pub const S_REPORT_PRIVATE: usize = 174;
pub const S_REPORT_MASK_DESC: usize = 175;
pub const S_REPORT_NOMASK_DESC: usize = 176;
pub const S_REPORT_MASK: usize = 177;
pub const S_REPORT_NOMASK: usize = 178;
pub const S_REPORT_MODE: usize = 179;
pub const S_REPORT_MODE_RELAY: usize = 180;
pub const S_REPORT_MODE_BROWSER: usize = 181;
pub const S_REPORT_MODE_FILE: usize = 182;
pub const S_REPORT_AUTO: usize = 183;
pub const S_REPORT_BROWSER: usize = 184;
pub const S_REPORT_FILE: usize = 185;
pub const S_REPORT_MODE_AUTO_BROWSER: usize = 186;
pub const S_REPORT_BUTTON: usize = 187;

const STRING_COUNT: usize = 188;

// ── Tables de traduction ───────────────────────────────────────────

static FR: [&[u8]; STRING_COUNT] = [
    // 0-14 : Pages Paramètres
    b"Syst\xE8me Linux",
    b"Applications Windows",
    b"R\xE9seau",
    b"Son",
    b"Clavier et pav\xE9 tactile",
    b"Souris",
    b"Apparence",
    b"Mises \xE0 jour",
    b"\xC9cran et d\xE9finition",
    b"\xC0 propos",
    b"Ex\xE9cutable",
    b"Navigateurs",
    b"Accessibilit\xE9",
    b"Effets visuels",
    b"Localisation",
    // 15-20 : Généraux
    b"Param\xE8tres",
    b"WINLUX  /  PR\xC9F\xC9RENCES DE L'ORDINATEUR",
    b"Op\xE9ration en cours...",
    b"Op\xE9ration termin\xE9e.",
    b"\xC9chec de l'op\xE9ration.",
    b"Information indisponible",
    // 21-23 : Système
    b"Red\xE9marrer la session",
    b"Red\xE9marrer",
    b"\xC9teindre",
    // 24-29 : Clavier
    b"Disposition actuelle",
    b"Activer toucher-clic",
    b"D\xE9sactiver",
    b"Toucher-clic (0 = non, 1 = oui)",
    b"Pav\xE9 tactile en VM",
    b"SPICE expose souvent une tablette, pas le pav\xE9 physique.",
    // 30-34 : Son
    b"Sortie audio / volume",
    b"Volume -",
    b"Muet / r\xE9tablir",
    b"Volume +",
    b"Batterie expos\xE9e par le syst\xE8me",
    // 35-38 : Souris
    b"Lent",
    b"Normal",
    b"Rapide",
    b"Vitesse du pointeur",
    // 39-58 : Apparence
    b"Barre des t\xE2ches",
    b"Pr\xE9f\xE9rences enregistr\xE9es pour la prochaine session.",
    b"Aligner \xE0 gauche",
    b"Centrer",
    b"Th\xE8me sombre",
    b"Th\xE8me clair",
    b"Th\xE8me automatique",
    b"Jour clair / nuit sombre (8h\x9620h)",
    b"D\xE9sactiv\xE9",
    b"Activer",
    b"Stockage",
    b"Les pr\xE9f\xE9rences sont conserv\xE9es sur le disque.",
    b"Session Live : les r\xE9glages durent jusqu'au red\xE9marrage.",
    b"Ic\xF4nes du bureau",
    b"Ajoutez seulement les ic\xF4nes que vous voulez afficher.",
    b"Afficher Corbeille",
    b"Afficher Explorateur",
    b"Afficher Ce PC",
    b"Afficher Documents",
    b"R\xE9glages de l'\xE9cran",
    // 59-60 : Réseau
    b"Renouveler la connexion",
    b"\xC9tat",
    // 61-62 : Mises à jour
    b"Rechercher",
    b"Installer",
    // 63-66 : Écran
    b"D\xE9finition actuellement appliqu\xE9e",
    b"Pr\xE9c\xE9dent",
    b"Suivant",
    b"Appliquer",
    // 67-71 : Accessibilité
    b"Normal",
    b"Grand",
    b"Tr\xE8s grand",
    b"Contraste \xE9lev\xE9",
    b"Contraste normal",
    // 72-74 : Navigateurs
    b"Installer",
    b"D\xE9faut",
    b"Lancer",
    // 75-82 : Localisation
    b"Langue de l'interface",
    b"Choisissez la langue du bureau et des applications.",
    b"Disposition du clavier",
    b"Le changement est imm\xE9diat.",
    b"Fuseau horaire",
    b"L'horloge du bureau sera ajust\xE9e.",
    b"Fuseau actuel",
    b"Red\xE9marrez la session pour appliquer la langue.",
    // 83-92 : Bureau / Taskbar
    b"D\xE9marrer",
    b"Rechercher",
    b"Tapez pour rechercher...",
    b"\xC9pingl\xE9s",
    b"Recommand\xE9s",
    b"Toutes les applications",
    b"Alimentation",
    b"Verrouiller",
    b"Se d\xE9connecter",
    b"Mettre en veille",
    // 93-118 : Explorer
    b"Accueil",
    b"Documents",
    b"Images",
    b"Musique",
    b"Vid\xE9os",
    b"T\xE9l\xE9chargements",
    b"Bureau",
    b"Ce PC",
    b"Corbeille",
    b"Nouveau dossier",
    b"Copier",
    b"Couper",
    b"Coller",
    b"Supprimer",
    b"Renommer",
    b"Propri\xE9t\xE9s",
    b"Tout s\xE9lectionner",
    b"Nom",
    b"Date",
    b"Type",
    b"Taille",
    b"\xE9l\xE9ments",
    b"s\xE9lectionn\xE9(s)",
    b"Rechercher ici...",
    b"Ouvrir",
    b"Vider la corbeille",
    // 119-127 : Applications
    b"Explorateur de fichiers",
    b"Param\xE8tres",
    b"Bloc-notes",
    b"Gestionnaire des t\xE2ches",
    b"Terminal",
    b"Calculatrice",
    b"Gestion des disques",
    b"Nettoyage de disque",
    b"M\xE9t\xE9o",
    // 128-130 : Page Apps Windows
    b"Ouvrir Ce PC",
    b"Ouvrir le dossier personnel",
    b"Ouvrir les mises \xE0 jour",
    // 131-133 : Effets visuels
    b"Flou acrylique",
    b"Animations fluides",
    b"Miniatures",
    // 134-145 : Fuseaux
    b"Europe/Paris (UTC+1)",
    b"Europe/London (UTC+0)",
    b"America/New_York (UTC-5)",
    b"America/Los_Angeles (UTC-8)",
    b"Asia/Tokyo (UTC+9)",
    b"Australia/Sydney (UTC+11)",
    b"Europe/Berlin (UTC+1)",
    b"Europe/Madrid (UTC+1)",
    b"Europe/Moscow (UTC+3)",
    b"Asia/Dubai (UTC+4)",
    b"Asia/Shanghai (UTC+8)",
    b"America/Sao_Paulo (UTC-3)",
    // 146-149 : Divers
    b"Le runtime Winlux ex\xE9cute vos programmes Windows.",
    b"Ethernet virtuel ; le Wi-Fi reste g\xE9r\xE9 par l'h\xF4te.",
    b"Version",
    b"Noyau",
    // 150-153 : WM
    b"Utilitaires",
    b"Basculer entre les fen\xEAtres",
    b"Ex\xE9cuter",
    b"Ouvrir :",
    // 154-155 : Pages Parametres ajoutees
    b"Gestionnaire de fichiers",
    b"Son avanc\xE9",
    b"Mises \xE0 jour syst\xE8me",
    b"Confidentialit\xE9",
    b"Processeur",
    b"Lecteurs r\xE9seau",
    b"Personnalisation",
    b"Bluetooth et appareils",
    b"Alimentation et batterie",
    b"Graphismes",
    // 164-187 : Signaler un problème
    b"Signaler un probl\xE8me",
    b"D\xE9crivez un bug : le rapport complet s'affiche, modifiable, avant tout envoi.",
    b"Nouveau signalement",
    b"Dernier plantage",
    b"Rapports enregistr\xE9s",
    b"Apr\xE8s un plantage",
    b"Winlux propose de signaler le probl\xE8me. Rien n'est envoy\xE9 sans votre accord.",
    b"Winlux ne propose rien apr\xE8s un plantage (le journal le conserve).",
    b"Proposer",
    b"Ne rien proposer",
    b"Donn\xE9es personnelles dans les rapports",
    b"Masqu\xE9es par d\xE9faut : chemins, noms de fichiers, identifiants, Wi-Fi, num\xE9ros de s\xE9rie.",
    b"Non masqu\xE9es par d\xE9faut : relisez le rapport avant l'envoi.",
    b"Masquer",
    b"Ne pas masquer",
    b"Mode d'envoi",
    b"Automatique : service de signalement Winlux, sans compte.",
    b"Navigateur : formulaire GitHub pr\xE9rempli (compte GitHub requis).",
    b"Fichier : le rapport est seulement enregistr\xE9 sur cet ordinateur.",
    b"Automatique",
    b"Navigateur",
    b"Fichier",
    b"Automatique : aucun service Winlux configur\xE9, le navigateur sera utilis\xE9.",
    b"Signaler",
];

static EN: [&[u8]; STRING_COUNT] = [
    // 0-14 : Settings pages
    b"Linux System",
    b"Windows Applications",
    b"Network",
    b"Sound",
    b"Keyboard and Touchpad",
    b"Mouse",
    b"Appearance",
    b"Updates",
    b"Display and Resolution",
    b"About",
    b"Executable",
    b"Browsers",
    b"Accessibility",
    b"Visual Effects",
    b"Localization",
    // 15-20 : General
    b"Settings",
    b"WINLUX  /  COMPUTER PREFERENCES",
    b"Operation in progress...",
    b"Operation completed.",
    b"Operation failed.",
    b"Information unavailable",
    // 21-23 : System
    b"Restart session",
    b"Restart",
    b"Shut down",
    // 24-29 : Keyboard
    b"Current layout",
    b"Enable tap to click",
    b"Disable",
    b"Tap to click (0 = no, 1 = yes)",
    b"Touchpad in VM",
    b"SPICE often exposes a tablet, not the physical touchpad.",
    // 30-34 : Sound
    b"Audio output / volume",
    b"Volume -",
    b"Mute / unmute",
    b"Volume +",
    b"System battery",
    // 35-38 : Mouse
    b"Slow",
    b"Normal",
    b"Fast",
    b"Pointer speed",
    // 39-58 : Appearance
    b"Taskbar",
    b"Preferences saved for next session.",
    b"Align left",
    b"Center",
    b"Dark theme",
    b"Light theme",
    b"Automatic theme",
    b"Light day / dark night (8am\x9620pm)",
    b"Disabled",
    b"Enable",
    b"Storage",
    b"Preferences are saved to disk.",
    b"Live session: settings last until reboot.",
    b"Desktop icons",
    b"Only show the icons you want on your desktop.",
    b"Show Recycle Bin",
    b"Show Explorer",
    b"Show This PC",
    b"Show Documents",
    b"Display settings",
    // 59-60 : Network
    b"Renew connection",
    b"Status",
    // 61-62 : Updates
    b"Check",
    b"Install",
    // 63-66 : Display
    b"Currently applied resolution",
    b"Previous",
    b"Next",
    b"Apply",
    // 67-71 : Accessibility
    b"Normal",
    b"Large",
    b"Extra large",
    b"High contrast",
    b"Normal contrast",
    // 72-74 : Browsers
    b"Install",
    b"Default",
    b"Launch",
    // 75-82 : Localization
    b"Interface language",
    b"Choose the language for the desktop and applications.",
    b"Keyboard layout",
    b"The change is immediate.",
    b"Time zone",
    b"The desktop clock will be adjusted.",
    b"Current time zone",
    b"Restart the session to apply the language.",
    // 83-92 : Desktop / Taskbar
    b"Start",
    b"Search",
    b"Type to search...",
    b"Pinned",
    b"Recommended",
    b"All apps",
    b"Power",
    b"Lock",
    b"Sign out",
    b"Sleep",
    // 93-118 : Explorer
    b"Home",
    b"Documents",
    b"Pictures",
    b"Music",
    b"Videos",
    b"Downloads",
    b"Desktop",
    b"This PC",
    b"Recycle Bin",
    b"New folder",
    b"Copy",
    b"Cut",
    b"Paste",
    b"Delete",
    b"Rename",
    b"Properties",
    b"Select all",
    b"Name",
    b"Date",
    b"Type",
    b"Size",
    b"items",
    b"selected",
    b"Search here...",
    b"Open",
    b"Empty Recycle Bin",
    // 119-127 : Apps
    b"File Explorer",
    b"Settings",
    b"Notepad",
    b"Task Manager",
    b"Terminal",
    b"Calculator",
    b"Disk Management",
    b"Disk Cleanup",
    b"Weather",
    // 128-130 : Windows Apps page
    b"Open This PC",
    b"Open Home folder",
    b"Open Updates",
    // 131-133 : Visual effects
    b"Acrylic blur",
    b"Smooth animations",
    b"Thumbnails",
    // 134-145 : Timezones
    b"Europe/Paris (UTC+1)",
    b"Europe/London (UTC+0)",
    b"America/New_York (UTC-5)",
    b"America/Los_Angeles (UTC-8)",
    b"Asia/Tokyo (UTC+9)",
    b"Australia/Sydney (UTC+11)",
    b"Europe/Berlin (UTC+1)",
    b"Europe/Madrid (UTC+1)",
    b"Europe/Moscow (UTC+3)",
    b"Asia/Dubai (UTC+4)",
    b"Asia/Shanghai (UTC+8)",
    b"America/Sao_Paulo (UTC-3)",
    // 146-149 : Misc
    b"The Winlux runtime executes your Windows programs.",
    b"Virtual Ethernet; Wi-Fi is managed by the host.",
    b"Version",
    b"Kernel",
    // 150-153 : WM
    b"Utilities",
    b"Switch windows",
    b"Run",
    b"Open:",
    // 154-155 : Pages Parametres ajoutees
    b"File manager",
    b"Advanced sound",
    b"System updates",
    b"Privacy",
    b"Processor",
    b"Network drives",
    b"Personalization",
    b"Bluetooth & devices",
    b"Power and battery",
    b"Graphics",
    // 164-187 : Signaler un problème
    b"Report a problem",
    b"Describe a bug: the full report is shown, editable, before anything is sent.",
    b"New report",
    b"Last crash",
    b"Saved reports",
    b"After a crash",
    b"Winlux offers to report the problem. Nothing is sent without your consent.",
    b"Winlux offers nothing after a crash (the log keeps it).",
    b"Offer",
    b"Offer nothing",
    b"Personal data in reports",
    b"Hidden by default: paths, file names, identifiers, Wi-Fi, serial numbers.",
    b"Not hidden by default: review the report before sending.",
    b"Hide",
    b"Don't hide",
    b"Sending mode",
    b"Automatic: Winlux reporting service, no account.",
    b"Browser: prefilled GitHub form (GitHub account required).",
    b"File: the report is only saved on this computer.",
    b"Automatic",
    b"Browser",
    b"File",
    b"Automatic: no Winlux service configured, the browser will be used.",
    b"Report",
];

static ES: [&[u8]; STRING_COUNT] = [
    // 0-14 : Páginas
    b"Sistema Linux",
    b"Aplicaciones Windows",
    b"Red",
    b"Sonido",
    b"Teclado y panel t\xE1ctil",
    b"Rat\xF3n",
    b"Apariencia",
    b"Actualizaciones",
    b"Pantalla y resoluci\xF3n",
    b"Acerca de",
    b"Ejecutable",
    b"Navegadores",
    b"Accesibilidad",
    b"Efectos visuales",
    b"Localizaci\xF3n",
    // 15-20
    b"Configuraci\xF3n",
    b"WINLUX  /  PREFERENCIAS DEL EQUIPO",
    b"Operaci\xF3n en curso...",
    b"Operaci\xF3n completada.",
    b"Error en la operaci\xF3n.",
    b"Informaci\xF3n no disponible",
    // 21-23
    b"Reiniciar sesi\xF3n",
    b"Reiniciar",
    b"Apagar",
    // 24-29
    b"Distribuci\xF3n actual",
    b"Activar clic al tocar",
    b"Desactivar",
    b"Clic al tocar (0 = no, 1 = s\xED)",
    b"Panel t\xE1ctil en VM",
    b"SPICE suele exponer una tableta, no el panel f\xEDsico.",
    // 30-34
    b"Salida de audio / volumen",
    b"Volumen -",
    b"Silenciar / restaurar",
    b"Volumen +",
    b"Bater\xEDa del sistema",
    // 35-38
    b"Lento",
    b"Normal",
    b"R\xE1pido",
    b"Velocidad del puntero",
    // 39-58
    b"Barra de tareas",
    b"Preferencias guardadas para la pr\xF3xima sesi\xF3n.",
    b"Alinear a la izquierda",
    b"Centrar",
    b"Tema oscuro",
    b"Tema claro",
    b"Tema autom\xE1tico",
    b"D\xEDa claro / noche oscura (8h\x9620h)",
    b"Desactivado",
    b"Activar",
    b"Almacenamiento",
    b"Las preferencias se guardan en el disco.",
    b"Sesi\xF3n Live: los ajustes duran hasta reiniciar.",
    b"Iconos del escritorio",
    b"Muestre solo los iconos que desee en su escritorio.",
    b"Mostrar Papelera",
    b"Mostrar Explorador",
    b"Mostrar Este PC",
    b"Mostrar Documentos",
    b"Ajustes de pantalla",
    // 59-60
    b"Renovar conexi\xF3n",
    b"Estado",
    // 61-62
    b"Buscar",
    b"Instalar",
    // 63-66
    b"Resoluci\xF3n aplicada actualmente",
    b"Anterior",
    b"Siguiente",
    b"Aplicar",
    // 67-71
    b"Normal",
    b"Grande",
    b"Muy grande",
    b"Alto contraste",
    b"Contraste normal",
    // 72-74
    b"Instalar",
    b"Predeterminado",
    b"Iniciar",
    // 75-82
    b"Idioma de la interfaz",
    b"Elija el idioma del escritorio y las aplicaciones.",
    b"Distribuci\xF3n del teclado",
    b"El cambio es inmediato.",
    b"Zona horaria",
    b"El reloj del escritorio se ajustar\xE1.",
    b"Zona horaria actual",
    b"Reinicie la sesi\xF3n para aplicar el idioma.",
    // 83-92
    b"Inicio",
    b"Buscar",
    b"Escriba para buscar...",
    b"Anclados",
    b"Recomendados",
    b"Todas las aplicaciones",
    b"Energ\xEDa",
    b"Bloquear",
    b"Cerrar sesi\xF3n",
    b"Suspender",
    // 93-118
    b"Inicio",
    b"Documentos",
    b"Im\xE1genes",
    b"M\xFAsica",
    b"V\xEDdeos",
    b"Descargas",
    b"Escritorio",
    b"Este PC",
    b"Papelera",
    b"Nueva carpeta",
    b"Copiar",
    b"Cortar",
    b"Pegar",
    b"Eliminar",
    b"Renombrar",
    b"Propiedades",
    b"Seleccionar todo",
    b"Nombre",
    b"Fecha",
    b"Tipo",
    b"Tama\xF1o",
    b"elementos",
    b"seleccionados",
    b"Buscar aqu\xED...",
    b"Abrir",
    b"Vaciar papelera",
    // 119-127
    b"Explorador de archivos",
    b"Configuraci\xF3n",
    b"Bloc de notas",
    b"Administrador de tareas",
    b"Terminal",
    b"Calculadora",
    b"Administraci\xF3n de discos",
    b"Limpieza de disco",
    b"Clima",
    // 128-130
    b"Abrir Este PC",
    b"Abrir carpeta personal",
    b"Abrir actualizaciones",
    // 131-133
    b"Desenfoque acr\xEDlico",
    b"Animaciones fluidas",
    b"Miniaturas",
    // 134-145
    b"Europe/Paris (UTC+1)",
    b"Europe/London (UTC+0)",
    b"America/New_York (UTC-5)",
    b"America/Los_Angeles (UTC-8)",
    b"Asia/Tokyo (UTC+9)",
    b"Australia/Sydney (UTC+11)",
    b"Europe/Berlin (UTC+1)",
    b"Europe/Madrid (UTC+1)",
    b"Europe/Moscow (UTC+3)",
    b"Asia/Dubai (UTC+4)",
    b"Asia/Shanghai (UTC+8)",
    b"America/Sao_Paulo (UTC-3)",
    // 146-149
    b"El runtime Winlux ejecuta sus programas Windows.",
    b"Ethernet virtual; el Wi-Fi lo gestiona el anfitri\xF3n.",
    b"Versi\xF3n",
    b"N\xFAcleo",
    // 150-153 : WM
    b"Utilidades",
    b"Cambiar entre ventanas",
    b"Ejecutar",
    b"Abrir:",
    // 154-155 : Pages Parametres ajoutees
    b"Administrador de archivos",
    b"Sonido avanzado",
    b"Actualizaciones del sistema",
    b"Privacidad",
    b"Procesador",
    b"Unidades de red",
    b"Personalizaci\xF3n",
    b"Bluetooth y dispositivos",
    b"Energ\xEDa y bater\xEDa",
    b"Gr\xE1ficos",
    // 164-187 : Signaler un problème
    b"Informar de un problema",
    b"Describa un error: el informe completo se muestra, editable, antes de enviar nada.",
    b"Nuevo informe",
    b"\xDAltimo fallo",
    b"Informes guardados",
    b"Tras un fallo",
    b"Winlux propone informar del problema. No se env\xEDa nada sin su permiso.",
    b"Winlux no propone nada tras un fallo (el registro lo conserva).",
    b"Proponer",
    b"No proponer",
    b"Datos personales en los informes",
    b"Ocultos por defecto: rutas, nombres de archivo, identificadores, Wi-Fi, n\xFAmeros de serie.",
    b"No ocultos por defecto: revise el informe antes de enviarlo.",
    b"Ocultar",
    b"No ocultar",
    b"Modo de env\xEDo",
    b"Autom\xE1tico: servicio de informes de Winlux, sin cuenta.",
    b"Navegador: formulario de GitHub rellenado (requiere cuenta de GitHub).",
    b"Archivo: el informe solo se guarda en este equipo.",
    b"Autom\xE1tico",
    b"Navegador",
    b"Archivo",
    b"Autom\xE1tico: ning\xFAn servicio de Winlux configurado, se usar\xE1 el navegador.",
    b"Informar",
];

static DE: [&[u8]; STRING_COUNT] = [
    // 0-14 : Seiten
    b"Linux-System",
    b"Windows-Anwendungen",
    b"Netzwerk",
    b"Ton",
    b"Tastatur und Touchpad",
    b"Maus",
    b"Darstellung",
    b"Updates",
    b"Anzeige und Aufl\xF6sung",
    b"\xDCber",
    b"Programm",
    b"Browser",
    b"Barrierefreiheit",
    b"Visuelle Effekte",
    b"Lokalisierung",
    // 15-20
    b"Einstellungen",
    b"WINLUX  /  COMPUTEREINSTELLUNGEN",
    b"Vorgang l\xE4uft...",
    b"Vorgang abgeschlossen.",
    b"Vorgang fehlgeschlagen.",
    b"Information nicht verf\xFCgbar",
    // 21-23
    b"Sitzung neu starten",
    b"Neustart",
    b"Herunterfahren",
    // 24-29
    b"Aktuelle Belegung",
    b"Tippen-zum-Klicken aktivieren",
    b"Deaktivieren",
    b"Tippen-zum-Klicken (0 = nein, 1 = ja)",
    b"Touchpad in VM",
    b"SPICE zeigt oft ein Tablet statt des physischen Touchpads.",
    // 30-34
    b"Audioausgang / Lautst\xE4rke",
    b"Leiser",
    b"Stumm / Wiederherstellen",
    b"Lauter",
    b"Systembatterie",
    // 35-38
    b"Langsam",
    b"Normal",
    b"Schnell",
    b"Zeigergeschwindigkeit",
    // 39-58
    b"Taskleiste",
    b"Einstellungen f\xFCr die n\xE4chste Sitzung gespeichert.",
    b"Links ausrichten",
    b"Zentrieren",
    b"Dunkles Design",
    b"Helles Design",
    b"Automatisches Design",
    b"Tag hell / Nacht dunkel (8\x9620 Uhr)",
    b"Deaktiviert",
    b"Aktivieren",
    b"Speicher",
    b"Einstellungen werden auf der Festplatte gespeichert.",
    b"Live-Sitzung: Einstellungen gelten bis zum Neustart.",
    b"Desktopsymbole",
    b"Zeigen Sie nur die Symbole an, die Sie m\xF6chten.",
    b"Papierkorb anzeigen",
    b"Explorer anzeigen",
    b"Dieser PC anzeigen",
    b"Dokumente anzeigen",
    b"Anzeigeeinstellungen",
    // 59-60
    b"Verbindung erneuern",
    b"Status",
    // 61-62
    b"Suchen",
    b"Installieren",
    // 63-66
    b"Aktuell angewendete Aufl\xF6sung",
    b"Zur\xFCck",
    b"Weiter",
    b"Anwenden",
    // 67-71
    b"Normal",
    b"Gro\xDF",
    b"Sehr gro\xDF",
    b"Hoher Kontrast",
    b"Normaler Kontrast",
    // 72-74
    b"Installieren",
    b"Standard",
    b"Starten",
    // 75-82
    b"Oberfl\xE4chensprache",
    b"W\xE4hlen Sie die Sprache f\xFCr Desktop und Anwendungen.",
    b"Tastaturbelegung",
    b"Die \xC4nderung gilt sofort.",
    b"Zeitzone",
    b"Die Desktop-Uhr wird angepasst.",
    b"Aktuelle Zeitzone",
    b"Starten Sie die Sitzung neu, um die Sprache anzuwenden.",
    // 83-92
    b"Start",
    b"Suchen",
    b"Zum Suchen tippen...",
    b"Angepinnt",
    b"Empfohlen",
    b"Alle Apps",
    b"Ein/Aus",
    b"Sperren",
    b"Abmelden",
    b"Ruhezustand",
    // 93-118
    b"Startseite",
    b"Dokumente",
    b"Bilder",
    b"Musik",
    b"Videos",
    b"Downloads",
    b"Desktop",
    b"Dieser PC",
    b"Papierkorb",
    b"Neuer Ordner",
    b"Kopieren",
    b"Ausschneiden",
    b"Einf\xFCgen",
    b"L\xF6schen",
    b"Umbenennen",
    b"Eigenschaften",
    b"Alles ausw\xE4hlen",
    b"Name",
    b"Datum",
    b"Typ",
    b"Gr\xF6\xDFe",
    b"Elemente",
    b"ausgew\xE4hlt",
    b"Hier suchen...",
    b"\xD6ffnen",
    b"Papierkorb leeren",
    // 119-127
    b"Datei-Explorer",
    b"Einstellungen",
    b"Editor",
    b"Task-Manager",
    b"Terminal",
    b"Rechner",
    b"Datentr\xE4gerverwaltung",
    b"Datentr\xE4gerbereinigung",
    b"Wetter",
    // 128-130
    b"Dieser PC \xF6ffnen",
    b"Benutzerordner \xF6ffnen",
    b"Updates \xF6ffnen",
    // 131-133
    b"Acryl-Unsch\xE4rfe",
    b"Fl\xFCssige Animationen",
    b"Miniaturansichten",
    // 134-145
    b"Europe/Paris (UTC+1)",
    b"Europe/London (UTC+0)",
    b"America/New_York (UTC-5)",
    b"America/Los_Angeles (UTC-8)",
    b"Asia/Tokyo (UTC+9)",
    b"Australia/Sydney (UTC+11)",
    b"Europe/Berlin (UTC+1)",
    b"Europe/Madrid (UTC+1)",
    b"Europe/Moscow (UTC+3)",
    b"Asia/Dubai (UTC+4)",
    b"Asia/Shanghai (UTC+8)",
    b"America/Sao_Paulo (UTC-3)",
    // 146-149
    b"Die Winlux-Laufzeit f\xFChrt Ihre Windows-Programme aus.",
    b"Virtuelles Ethernet; WLAN wird vom Host verwaltet.",
    b"Version",
    b"Kernel",
    // 150-153 : WM
    b"Dienstprogramme",
    b"Zwischen Fenstern wechseln",
    b"Ausf\xFChren",
    b"\xD6ffnen:",
    // 154-155 : Pages Parametres ajoutees
    b"Dateimanager",
    b"Erweiterter Sound",
    b"Systemupdates",
    b"Datenschutz",
    b"Prozessor",
    b"Netzlaufwerke",
    b"Personalisierung",
    b"Bluetooth und Ger\xE4te",
    b"Energie und Akku",
    b"Grafik",
    // 164-187 : Signaler un problème
    b"Problem melden",
    b"Beschreiben Sie einen Fehler: Der vollst\xE4ndige Bericht wird vor dem Senden bearbeitbar angezeigt.",
    b"Neue Meldung",
    b"Letzter Absturz",
    b"Gespeicherte Berichte",
    b"Nach einem Absturz",
    b"Winlux bietet an, das Problem zu melden. Ohne Ihre Zustimmung wird nichts gesendet.",
    b"Winlux bietet nach einem Absturz nichts an (das Protokoll beh\xE4lt ihn).",
    b"Anbieten",
    b"Nichts anbieten",
    b"Pers\xF6nliche Daten in Berichten",
    b"Standardm\xE4\xDFig ausgeblendet: Pfade, Dateinamen, Kennungen, WLAN, Seriennummern.",
    b"Standardm\xE4\xDFig nicht ausgeblendet: Pr\xFCfen Sie den Bericht vor dem Senden.",
    b"Ausblenden",
    b"Nicht ausblenden",
    b"Versandart",
    b"Automatisch: Winlux-Meldedienst, ohne Konto.",
    b"Browser: vorausgef\xFClltes GitHub-Formular (GitHub-Konto n\xF6tig).",
    b"Datei: Der Bericht wird nur auf diesem Computer gespeichert.",
    b"Automatisch",
    b"Browser",
    b"Datei",
    b"Automatisch: Kein Winlux-Dienst eingerichtet, der Browser wird verwendet.",
    b"Melden",
];

pub fn tr(id: usize) -> &'static [u8] {
    let table: &[&[u8]; STRING_COUNT] = match unsafe { CURRENT_LANG } {
        Lang::Fr => &FR,
        Lang::En => &EN,
        Lang::Es => &ES,
        Lang::De => &DE,
    };
    if id < STRING_COUNT {
        table[id]
    } else {
        b"???"
    }
}

// ── Layouts clavier ────────────────────────────────────────────────

pub const KEYBOARD_COUNT: usize = 8;
pub const KEYBOARD_CODES: [&[u8]; KEYBOARD_COUNT] = [
    b"fr", b"us", b"de", b"es", b"gb", b"it", b"pt", b"be",
];
pub const KEYBOARD_NAMES: [&[u8]; KEYBOARD_COUNT] = [
    b"AZERTY fran\xE7ais",
    b"QWERTY US",
    b"QWERTZ deutsch",
    b"QWERTY espa\xF1ol",
    b"QWERTY UK",
    b"QWERTY italiano",
    b"QWERTY portugu\xEAs",
    b"AZERTY belge",
];

// ── Fuseaux horaires ───────────────────────────────────────────────

pub const TZ_COUNT: usize = 12;
pub const TZ_IDS: [&[u8]; TZ_COUNT] = [
    b"Europe/Paris",
    b"Europe/London",
    b"America/New_York",
    b"America/Los_Angeles",
    b"Asia/Tokyo",
    b"Australia/Sydney",
    b"Europe/Berlin",
    b"Europe/Madrid",
    b"Europe/Moscow",
    b"Asia/Dubai",
    b"Asia/Shanghai",
    b"America/Sao_Paulo",
];
pub const TZ_STRINGS: [usize; TZ_COUNT] = [
    S_TZ_PARIS,
    S_TZ_LONDON,
    S_TZ_NEW_YORK,
    S_TZ_LOS_ANGELES,
    S_TZ_TOKYO,
    S_TZ_SYDNEY,
    S_TZ_BERLIN,
    S_TZ_MADRID,
    S_TZ_MOSCOW,
    S_TZ_DUBAI,
    S_TZ_SHANGHAI,
    S_TZ_SAO_PAULO,
];

pub unsafe fn load_lang() {
    let mut buf = [0u8; 8];
    let path = c"Z:\\etc\\winlux\\locale.conf";
    let h = crate::CreateFileA(
        path.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if !h.is_null() && h as isize != -1 {
        let mut read = 0u32;
        crate::ReadFile(h, buf.as_mut_ptr(), 8, &mut read, core::ptr::null_mut());
        crate::CloseHandle(h);
        let data = &buf[..(read as usize).min(8)];
        if let Some(pos) = data.iter().position(|&v| v == b'=') {
            let code = &data[pos + 1..];
            let code = if let Some(nl) = code.iter().position(|&v| v == b'\n' || v == b'\r') {
                &code[..nl]
            } else {
                code
            };
            CURRENT_LANG = lang_from_code(code);
        }
    }
}
