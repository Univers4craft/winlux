//! Briques communes aux outils d'administration (Planificateur de tâches,
//! Pare-feu) : zones cliquables enregistrées au dessin, champs de saisie,
//! interrupteurs et boutons Windows 11, fichiers et langue de l'interface.

use crate::gfx::*;
use crate::locale::Lang;
use crate::wm::Gfx;
use winlux_testexe::*;

// ---------- Langue ----------

static mut ENV_LANG: Option<Lang> = None;

/// Langue de l'interface : Paramètres > Langue, sinon `LANG` au démarrage.
pub unsafe fn lang() -> Lang {
    if crate::locale::CURRENT_LANG != Lang::Fr {
        return crate::locale::CURRENT_LANG;
    }
    if let Some(l) = ENV_LANG {
        return l;
    }
    let mut v = [0u8; 64];
    let n = GetEnvironmentVariableA(c"LANG".as_ptr(), v.as_mut_ptr().cast(), 64);
    let l = if (2..64).contains(&n) {
        crate::locale::lang_from_code(&v[..2])
    } else {
        Lang::Fr
    };
    ENV_LANG = Some(l);
    l
}

/// Vrai si l'interface est en anglais.
pub unsafe fn english() -> bool {
    lang() == Lang::En
}

/// Texte selon la langue (chaînes en Windows-1252). Les appels donnent le
/// français et l'anglais ; l'espagnol et l'allemand viennent de
/// `TRANSLATIONS` (l'anglais sert si une entrée manque).
pub fn t(fr: &'static [u8], en: &'static [u8]) -> &'static [u8] {
    let col = match unsafe { lang() } {
        Lang::Fr => return fr,
        Lang::En => return en,
        Lang::Es => 2,
        Lang::De => 3,
    };
    TRANSLATIONS
        .iter()
        .find(|r| r.0 == fr && r.1 == en)
        .map_or(en, |r| if col == 2 { r.2 } else { r.3 })
}

/// Traductions espagnoles et allemandes des textes `t(fr, en)` :
/// `(français, anglais, espagnol, allemand)`, retrouvées par le couple
/// français/anglais (un même mot anglais peut avoir plusieurs genres).
const TRANSLATIONS: &[(&[u8], &[u8], &[u8], &[u8])] = &[
    (
        b"R\xE9seau connect\xE9",
        b"Network connected",
        b"Red conectada",
        b"Netzwerk verbunden",
    ),
    (
        b"R\xE9seau d\xE9connect\xE9",
        b"Network disconnected",
        b"Red desconectada",
        b"Netzwerk getrennt",
    ),
    (
        b"Branchement sur secteur",
        b"Plugged in",
        b"Conectado a la corriente",
        b"Netzbetrieb",
    ),
    (
        b"Passage sur batterie",
        b"On battery",
        b"Con bater\xEDa",
        b"Akkubetrieb",
    ),
    (
        b"P\xE9riph\xE9rique USB branch\xE9",
        b"USB device connected",
        b"Dispositivo USB conectado",
        b"USB-Ger\xE4t angeschlossen",
    ),
    (
        b"Journal d'\xE9v\xE9nements Windows",
        b"Windows event log",
        b"Registro de eventos de Windows",
        b"Windows-Ereignisprotokoll",
    ),
    (b"Une fois", b"One time", b"Una vez", b"Einmal"),
    (b"Quotidien", b"Daily", b"Diariamente", b"T\xE4glich"),
    (
        b"Hebdomadaire",
        b"Weekly",
        b"Semanalmente",
        b"W\xF6chentlich",
    ),
    (b"Intervalle", b"Interval", b"Intervalo", b"Intervall"),
    (
        b"Au d\xE9marrage",
        b"At startup",
        b"Al iniciar el sistema",
        b"Beim Start",
    ),
    (
        b"\xC0 l'ouverture de session",
        b"At log on",
        b"Al iniciar sesi\xF3n",
        b"Bei Anmeldung",
    ),
    (
        b"Sur un \xE9v\xE9nement",
        b"On an event",
        b"Al producirse un evento",
        b"Bei einem Ereignis",
    ),
    (b"Le ", b"On ", b"El ", b"Am "),
    (b" \xE0 ", b" at ", b" a las ", b" um "),
    (
        b"Tous les jours \xE0 ",
        b"Daily at ",
        b"Todos los d\xEDas a las ",
        b"T\xE4glich um ",
    ),
    (b"Tous les ", b"Every ", b"Cada ", b"Alle "),
    (
        b" jours \xE0 ",
        b" days at ",
        b" d\xEDas a las ",
        b" Tage um ",
    ),
    (
        b"Chaque semaine (",
        b"Weekly (",
        b"Cada semana (",
        b"W\xF6chentlich (",
    ),
    (b") \xE0 ", b") at ", b") a las ", b") um "),
    (b"Toutes les ", b"Every ", b"Cada ", b"Alle "),
    (
        b"Sur \xE9v\xE9nement : ",
        b"On event: ",
        b"Al producirse el evento: ",
        b"Bei Ereignis: ",
    ),
    (
        b"\xC0 la demande",
        b"On demand",
        b"A petici\xF3n",
        b"Bei Bedarf",
    ),
    (
        b"En cours",
        b"Running",
        b"En ejecuci\xF3n",
        b"Wird ausgef\xFChrt",
    ),
    (b"Pr\xEAte", b"Ready", b"Lista", b"Bereit"),
    (
        b"D\xE9sactiv\xE9e",
        b"Disabled",
        b"Deshabilitada",
        b"Deaktiviert",
    ),
    (
        b"Jamais ex\xE9cut\xE9e",
        b"Has not run yet",
        b"A\xFAn no se ha ejecutado",
        b"Noch nicht ausgef\xFChrt",
    ),
    (
        b"R\xE9ussite (0x0)",
        b"Success (0x0)",
        b"Correcto (0x0)",
        b"Erfolgreich (0x0)",
    ),
    (
        b"Code de sortie ",
        b"Exit code ",
        b"C\xF3digo de salida ",
        b"Exitcode ",
    ),
    (
        b"Indiquez un nom pour la t\xE2che.",
        b"Enter a name for the task.",
        b"Escriba un nombre para la tarea.",
        b"Geben Sie einen Namen f\xFCr die Aufgabe ein.",
    ),
    (
        b"Indiquez le programme ou le script \xE0 lancer.",
        b"Enter the program or script to start.",
        b"Escriba el programa o script que se iniciar\xE1.",
        b"Geben Sie das zu startende Programm oder Skript ein.",
    ),
    (
        b"Heure invalide : utilisez HH:MM.",
        b"Invalid time: use HH:MM.",
        b"Hora no v\xE1lida: use HH:MM.",
        b"Ung\xFCltige Uhrzeit: HH:MM verwenden.",
    ),
    (
        b"Date invalide : utilisez AAAA-MM-JJ.",
        b"Invalid date: use YYYY-MM-DD.",
        b"Fecha no v\xE1lida: use AAAA-MM-DD.",
        b"Ung\xFCltiges Datum: JJJJ-MM-TT verwenden.",
    ),
    (
        b"Indiquez un nombre sup\xE9rieur \xE0 z\xE9ro.",
        b"Enter a number greater than zero.",
        b"Escriba un n\xFAmero mayor que cero.",
        b"Geben Sie eine Zahl gr\xF6\xDFer als null ein.",
    ),
    (
        b"Choisissez au moins un jour.",
        b"Choose at least one day.",
        b"Elija al menos un d\xEDa.",
        b"W\xE4hlen Sie mindestens einen Tag.",
    ),
    (
        b"T\xE2che enregistr\xE9e.",
        b"Task saved.",
        b"Tarea guardada.",
        b"Aufgabe gespeichert.",
    ),
    (
        b"Enregistrement impossible : droits insuffisants sur /etc/winlux/tasks.",
        b"Cannot save: no write access to /etc/winlux/tasks.",
        b"No se puede guardar: sin permiso de escritura en /etc/winlux/tasks.",
        b"Speichern nicht m\xF6glich: kein Schreibzugriff auf /etc/winlux/tasks.",
    ),
    (
        b"Ex\xE9cution demand\xE9e.",
        b"Run requested.",
        b"Ejecuci\xF3n solicitada.",
        b"Ausf\xFChrung angefordert.",
    ),
    (
        b"Arr\xEAt demand\xE9.",
        b"Stop requested.",
        b"Detenci\xF3n solicitada.",
        b"Beenden angefordert.",
    ),
    (
        b"T\xE2che supprim\xE9e.",
        b"Task deleted.",
        b"Tarea eliminada.",
        b"Aufgabe gel\xF6scht.",
    ),
    (
        b"Modification enregistr\xE9e.",
        b"Change saved.",
        b"Cambio guardado.",
        b"\xC4nderung gespeichert.",
    ),
    (
        b"Le service Winlux est introuvable.",
        b"The Winlux service was not found.",
        b"No se encuentra el servicio de Winlux.",
        b"Der Winlux-Dienst wurde nicht gefunden.",
    ),
    (
        b"Planificateur de t\xE2ches",
        b"Task Scheduler",
        b"Programador de tareas",
        b"Aufgabenplanung",
    ),
    (b"Biblioth\xE8que", b"Library", b"Biblioteca", b"Bibliothek"),
    (b"Historique", b"History", b"Historial", b"Verlauf"),
    (
        b"Biblioth\xE8que du Planificateur",
        b"Task Scheduler Library",
        b"Biblioteca del Programador de tareas",
        b"Aufgabenplanungsbibliothek",
    ),
    (
        b"Cr\xE9er une t\xE2che",
        b"Create task",
        b"Crear tarea",
        b"Aufgabe erstellen",
    ),
    (
        b"Propri\xE9t\xE9s",
        b"Properties",
        b"Propiedades",
        b"Eigenschaften",
    ),
    (b"Ex\xE9cuter", b"Run", b"Ejecutar", b"Ausf\xFChren"),
    (b"Fin", b"End", b"Finalizar", b"Beenden"),
    (
        b"D\xE9sactiver",
        b"Disable",
        b"Deshabilitar",
        b"Deaktivieren",
    ),
    (b"Activer", b"Enable", b"Habilitar", b"Aktivieren"),
    (b"Supprimer", b"Delete", b"Eliminar", b"L\xF6schen"),
    (b"Actualiser", b"Refresh", b"Actualizar", b"Aktualisieren"),
    (b"Nom", b"Name", b"Nombre", b"Name"),
    (b"\xC9tat", b"Status", b"Estado", b"Status"),
    (
        b"D\xE9clencheurs",
        b"Triggers",
        b"Desencadenadores",
        b"Trigger",
    ),
    (
        b"Prochaine ex\xE9cution",
        b"Next run time",
        b"Pr\xF3xima ejecuci\xF3n",
        b"N\xE4chste Ausf\xFChrung",
    ),
    (
        b"Derni\xE8re ex\xE9cution",
        b"Last run time",
        b"\xDAltima ejecuci\xF3n",
        b"Letzte Ausf\xFChrung",
    ),
    (
        b"R\xE9sultat",
        b"Last run result",
        b"Resultado",
        b"Ergebnis",
    ),
    (
        b"Aucune t\xE2che. Cliquez sur \xAB Cr\xE9er une t\xE2che \xBB pour en ajouter une.",
        b"No tasks yet. Click \"Create task\" to add one.",
        b"No hay tareas. Haga clic en \xABCrear tarea\xBB para agregar una.",
        b"Keine Aufgaben. Klicken Sie auf \"Aufgabe erstellen\", um eine hinzuzuf\xFCgen.",
    ),
    (
        b"(aucune description)",
        b"(no description)",
        b"(sin descripci\xF3n)",
        b"(keine Beschreibung)",
    ),
    (
        b"Action : d\xE9marrer ",
        b"Action: start ",
        b"Acci\xF3n: iniciar ",
        b"Aktion: starte ",
    ),
    (
        b"Conditions : ",
        b"Conditions: ",
        b"Condiciones: ",
        b"Bedingungen: ",
    ),
    (b"aucune", b"none", b"ninguna", b"keine"),
    (
        b"uniquement sur secteur",
        b"only on AC power",
        b"solo con corriente alterna",
        b"nur im Netzbetrieb",
    ),
    (
        b"seulement si le r\xE9seau est disponible",
        b"only if a network is available",
        b"solo si hay red disponible",
        b"nur wenn ein Netzwerk verf\xFCgbar ist",
    ),
    (
        b"Emplacement : ",
        b"Location: ",
        b"Ubicaci\xF3n: ",
        b"Ort: ",
    ),
    (
        b"  \xB7  Auteur : ",
        b"  \xB7  Author: ",
        b"  \xB7  Autor: ",
        b"  \xB7  Autor: ",
    ),
    (
        b"T\xE2che d\xE9marr\xE9e",
        b"Task started",
        b"Tarea iniciada",
        b"Aufgabe gestartet",
    ),
    (
        b"T\xE2che termin\xE9e",
        b"Task completed",
        b"Tarea completada",
        b"Aufgabe abgeschlossen",
    ),
    (
        b"T\xE2che non d\xE9marr\xE9e",
        b"Task not started",
        b"Tarea no iniciada",
        b"Aufgabe nicht gestartet",
    ),
    (
        b"T\xE2che arr\xEAt\xE9e (dur\xE9e d\xE9pass\xE9e)",
        b"Task stopped (time limit)",
        b"Tarea detenida (l\xEDmite de tiempo)",
        b"Aufgabe beendet (Zeitlimit)",
    ),
    (
        b"\xC9chec du lancement",
        b"Launch failure",
        b"Error al iniciar",
        b"Startfehler",
    ),
    (
        b"Lanc\xE9e manuellement",
        b"Started manually",
        b"Iniciada manualmente",
        b"Manuell gestartet",
    ),
    (
        b"D\xE9clencheur horaire",
        b"Time trigger",
        b"Desencadenador de hora",
        b"Zeittrigger",
    ),
    (
        b"D\xE9clencheur \xE0 intervalle",
        b"Interval trigger",
        b"Desencadenador de intervalo",
        b"Intervalltrigger",
    ),
    (
        b"D\xE9marrage du syst\xE8me",
        b"System startup",
        b"Inicio del sistema",
        b"Systemstart",
    ),
    (
        b"Ouverture de session",
        b"User log on",
        b"Inicio de sesi\xF3n",
        b"Benutzeranmeldung",
    ),
    (
        b"L'ordinateur est sur batterie",
        b"The computer is on battery",
        b"El equipo funciona con bater\xEDa",
        b"Der Computer l\xE4uft im Akkubetrieb",
    ),
    (
        b"Aucun r\xE9seau disponible",
        b"No network available",
        b"No hay red disponible",
        b"Kein Netzwerk verf\xFCgbar",
    ),
    (
        b"Une instance est d\xE9j\xE0 en cours",
        b"An instance is already running",
        b"Ya hay una instancia en ejecuci\xF3n",
        b"Eine Instanz wird bereits ausgef\xFChrt",
    ),
    (b"Limite : ", b"Limit: ", b"L\xEDmite: ", b"Limit: "),
    (
        b"\xC9v\xE9nement : ",
        b"Event: ",
        b"Evento: ",
        b"Ereignis: ",
    ),
    (
        b"Historique d'ex\xE9cution",
        b"Run history",
        b"Historial de ejecuci\xF3n",
        b"Ausf\xFChrungsverlauf",
    ),
    (
        b"Date et heure",
        b"Date and time",
        b"Fecha y hora",
        b"Datum und Uhrzeit",
    ),
    (b"T\xE2che", b"Task", b"Tarea", b"Aufgabe"),
    (b"\xC9v\xE9nement", b"Event", b"Evento", b"Ereignis"),
    (b"D\xE9tails", b"Details", b"Detalles", b"Details"),
    (
        b"Aucune ex\xE9cution enregistr\xE9e.",
        b"No runs recorded yet.",
        b"A\xFAn no hay ejecuciones registradas.",
        b"Noch keine Ausf\xFChrungen aufgezeichnet.",
    ),
    (
        b"Propri\xE9t\xE9s de la t\xE2che",
        b"Task properties",
        b"Propiedades de la tarea",
        b"Aufgabeneigenschaften",
    ),
    (
        b"Description",
        b"Description",
        b"Descripci\xF3n",
        b"Beschreibung",
    ),
    (b"Facultative", b"Optional", b"Opcional", b"Optional"),
    (b"D\xE9clencheur", b"Trigger", b"Desencadenador", b"Trigger"),
    (
        b"Date (AAAA-MM-JJ)",
        b"Date (YYYY-MM-DD)",
        b"Fecha (AAAA-MM-DD)",
        b"Datum (JJJJ-MM-TT)",
    ),
    (
        b"Heure (HH:MM)",
        b"Time (HH:MM)",
        b"Hora (HH:MM)",
        b"Uhrzeit (HH:MM)",
    ),
    (
        b"Tous les (jours)",
        b"Recur every (days)",
        b"Repetir cada (d\xEDas)",
        b"Wiederholen alle (Tage)",
    ),
    (
        b"R\xE9p\xE9ter toutes les (minutes)",
        b"Repeat every (minutes)",
        b"Repetir cada (minutos)",
        b"Wiederholen alle (Minuten)",
    ),
    (
        b"La t\xE2che d\xE9marre \xE0 chaque d\xE9marrage de l'ordinateur.",
        b"The task starts every time the computer starts.",
        b"La tarea se inicia cada vez que se inicia el equipo.",
        b"Die Aufgabe startet bei jedem Start des Computers.",
    ),
    (
        b"La t\xE2che d\xE9marre \xE0 chaque ouverture de session.",
        b"The task starts every time a user logs on.",
        b"La tarea se inicia cada vez que un usuario inicia sesi\xF3n.",
        b"Die Aufgabe startet bei jeder Benutzeranmeldung.",
    ),
    (
        b"Action : d\xE9marrer un programme",
        b"Action: start a program",
        b"Acci\xF3n: iniciar un programa",
        b"Aktion: Programm starten",
    ),
    (
        b"Programme ou script (.exe, .sh, .py ou commande Linux)",
        b"Program or script (.exe, .sh, .py or Linux command)",
        b"Programa o script (.exe, .sh, .py o comando de Linux)",
        b"Programm oder Skript (.exe, .sh, .py oder Linux-Befehl)",
    ),
    (b"Arguments", b"Arguments", b"Argumentos", b"Argumente"),
    (b"Facultatifs", b"Optional", b"Opcionales", b"Optional"),
    (b"Commencer dans", b"Start in", b"Iniciar en", b"Starten in"),
    (b"Facultatif", b"Optional", b"Opcional", b"Optional"),
    (b"Conditions", b"Conditions", b"Condiciones", b"Bedingungen"),
    (
        b"Ne d\xE9marrer que si l'ordinateur est sur secteur",
        b"Start only if the computer is on AC power",
        b"Iniciar solo si el equipo est\xE1 conectado a la corriente",
        b"Nur starten, wenn der Computer im Netzbetrieb l\xE4uft",
    ),
    (
        b"Ne d\xE9marrer que si une connexion r\xE9seau est disponible",
        b"Start only if a network connection is available",
        b"Iniciar solo si hay una conexi\xF3n de red disponible",
        b"Nur starten, wenn eine Netzwerkverbindung verf\xFCgbar ist",
    ),
    (
        b"T\xE2che activ\xE9e",
        b"Task enabled",
        b"Tarea habilitada",
        b"Aufgabe aktiviert",
    ),
    (b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"),
    (b"Enregistrer", b"Save", b"Guardar", b"Speichern"),
    (
        b"R\xE9seau de domaine",
        b"Domain network",
        b"Red de dominio",
        b"Dom\xE4nennetzwerk",
    ),
    (
        b"R\xE9seau priv\xE9",
        b"Private network",
        b"Red privada",
        b"Privates Netzwerk",
    ),
    (
        b"R\xE9seau public",
        b"Public network",
        b"Red p\xFAblica",
        b"\xD6ffentliches Netzwerk",
    ),
    (b"Domaine", b"Domain", b"Dominio", b"Dom\xE4ne"),
    (b"Priv\xE9", b"Private", b"Privada", b"Privat"),
    (b"Public", b"Public", b"P\xFAblica", b"\xD6ffentlich"),
    (b"Tous", b"Any", b"Cualquiera", b"Alle"),
    (
        b"Enregistrement impossible : droits insuffisants sur /etc/winlux.",
        b"Cannot save: no write access to /etc/winlux.",
        b"No se puede guardar: sin permiso de escritura en /etc/winlux.",
        b"Speichern nicht m\xF6glich: kein Schreibzugriff auf /etc/winlux.",
    ),
    (
        b"Param\xE8tres appliqu\xE9s.",
        b"Settings applied.",
        b"Configuraci\xF3n aplicada.",
        b"Einstellungen \xFCbernommen.",
    ),
    (
        b"Enregistr\xE9, mais le service Winlux est introuvable.",
        b"Saved, but the Winlux service was not found.",
        b"Guardado, pero no se encuentra el servicio de Winlux.",
        b"Gespeichert, aber der Winlux-Dienst wurde nicht gefunden.",
    ),
    (
        b" souhaite recevoir des connexions entrantes.",
        b" wants to accept incoming connections.",
        b" quiere aceptar conexiones entrantes.",
        b" m\xF6chte eingehende Verbindungen annehmen.",
    ),
    (
        b"Pare-feu Winlux",
        b"Winlux Firewall",
        b"Firewall de Winlux",
        b"Winlux-Firewall",
    ),
    (
        b"Indiquez un nom pour la r\xE8gle.",
        b"Enter a name for the rule.",
        b"Escriba un nombre para la regla.",
        b"Geben Sie einen Namen f\xFCr die Regel ein.",
    ),
    (
        b"Ports invalides : exemple 80, 443 ou 8000-8100.",
        b"Invalid ports: for example 80, 443 or 8000-8100.",
        b"Puertos no v\xE1lidos: por ejemplo 80, 443 u 8000-8100.",
        b"Ung\xFCltige Ports: zum Beispiel 80, 443 oder 8000-8100.",
    ),
    (
        b"Choisissez au moins un profil.",
        b"Choose at least one profile.",
        b"Elija al menos un perfil.",
        b"W\xE4hlen Sie mindestens ein Profil.",
    ),
    (
        b"Nombre maximal de r\xE8gles atteint.",
        b"Maximum number of rules reached.",
        b"Se alcanz\xF3 el n\xFAmero m\xE1ximo de reglas.",
        b"Maximale Anzahl von Regeln erreicht.",
    ),
    (
        b"Vue d'ensemble",
        b"Overview",
        b"Informaci\xF3n general",
        b"\xDCbersicht",
    ),
    (
        b"R\xE8gles de trafic entrant",
        b"Inbound rules",
        b"Reglas de entrada",
        b"Eingehende Regeln",
    ),
    (
        b"R\xE8gles de trafic sortant",
        b"Outbound rules",
        b"Reglas de salida",
        b"Ausgehende Regeln",
    ),
    (
        b"Connexions bloqu\xE9es",
        b"Blocked connections",
        b"Conexiones bloqueadas",
        b"Blockierte Verbindungen",
    ),
    (
        b"Pare-feu et protection du r\xE9seau",
        b"Firewall & network protection",
        b"Firewall y protecci\xF3n de red",
        b"Firewall- & Netzwerkschutz",
    ),
    (
        b"nftables est absent : les r\xE8gles ne peuvent pas \xEAtre appliqu\xE9es.",
        b"nftables is missing: rules cannot be applied.",
        b"Falta nftables: no se pueden aplicar las reglas.",
        b"nftables fehlt: Regeln k\xF6nnen nicht angewendet werden.",
    ),
    (
        b"Le pare-feu est d\xE9sactiv\xE9 pour le r\xE9seau actuel.",
        b"The firewall is off for the current network.",
        b"El firewall est\xE1 desactivado para la red actual.",
        b"Die Firewall ist f\xFCr das aktuelle Netzwerk deaktiviert.",
    ),
    (
        b"Le service de pare-feu n'a pas encore appliqu\xE9 la configuration.",
        b"The firewall service has not applied the configuration yet.",
        b"El servicio de firewall a\xFAn no ha aplicado la configuraci\xF3n.",
        b"Der Firewall-Dienst hat die Konfiguration noch nicht angewendet.",
    ),
    (
        b"Le pare-feu prot\xE8ge cet ordinateur.",
        b"The firewall is protecting this device.",
        b"El firewall protege este equipo.",
        b"Die Firewall sch\xFCtzt dieses Ger\xE4t.",
    ),
    (b" (actif)", b" (active)", b" (activa)", b" (aktiv)"),
    (
        b"Utiliser pour le r\xE9seau actuel",
        b"Use for the current network",
        b"Usar para la red actual",
        b"F\xFCr das aktuelle Netzwerk verwenden",
    ),
    (b"Entrant :", b"Inbound:", b"Entrada:", b"Eingehend:"),
    (b"Bloquer", b"Block", b"Bloquear", b"Blockieren"),
    (b"Autoriser", b"Allow", b"Permitir", b"Zulassen"),
    (b"Sortant :", b"Outbound:", b"Salida:", b"Ausgehend:"),
    (
        b"M'avertir lorsqu'une application commence \xE0 recevoir des connexions",
        b"Notify me when an app starts accepting incoming connections",
        b"Notificarme cuando una aplicaci\xF3n empiece a aceptar conexiones entrantes",
        b"Benachrichtigen, wenn eine App eingehende Verbindungen annimmt",
    ),
    (
        b"Nouvelle r\xE8gle",
        b"New rule",
        b"Nueva regla",
        b"Neue Regel",
    ),
    (
        b"D\xE9sactiver la r\xE8gle",
        b"Disable rule",
        b"Deshabilitar regla",
        b"Regel deaktivieren",
    ),
    (
        b"Activer la r\xE8gle",
        b"Enable rule",
        b"Habilitar regla",
        b"Regel aktivieren",
    ),
    (b"Profil", b"Profile", b"Perfil", b"Profil"),
    (b"Activ\xE9e", b"Enabled", b"Habilitada", b"Aktiviert"),
    (b"Action", b"Action", b"Acci\xF3n", b"Aktion"),
    (b"Protocole", b"Protocol", b"Protocolo", b"Protokoll"),
    (b"Ports", b"Ports", b"Puertos", b"Ports"),
    (b"Programme", b"Program", b"Programa", b"Programm"),
    (
        b"Aucune r\xE8gle. Le comportement par d\xE9faut du profil s'applique.",
        b"No rules. The profile's default behavior applies.",
        b"No hay reglas. Se aplica el comportamiento predeterminado del perfil.",
        b"Keine Regeln. Das Standardverhalten des Profils gilt.",
    ),
    (b"Tous", b"All", b"Todos", b"Alle"),
    (b"Oui", b"Yes", b"S\xED", b"Ja"),
    (b"Non", b"No", b"No", b"Nein"),
    (
        b"Tous les programmes",
        b"All programs",
        b"Todos los programas",
        b"Alle Programme",
    ),
    (
        b"Effacer le journal",
        b"Clear log",
        b"Borrar registro",
        b"Protokoll l\xF6schen",
    ),
    (b"Sens", b"Direction", b"Direcci\xF3n", b"Richtung"),
    (b"Source", b"Source", b"Origen", b"Quelle"),
    (b"Destination", b"Destination", b"Destino", b"Ziel"),
    (b"Port", b"Port", b"Puerto", b"Port"),
    (
        b"Aucune connexion bloqu\xE9e n'a \xE9t\xE9 enregistr\xE9e.",
        b"No blocked connections have been logged.",
        b"No se ha registrado ninguna conexi\xF3n bloqueada.",
        b"Es wurden keine blockierten Verbindungen protokolliert.",
    ),
    (b"Entrant", b"Inbound", b"Entrada", b"Eingehend"),
    (b"Sortant", b"Outbound", b"Salida", b"Ausgehend"),
    (
        b"Propri\xE9t\xE9s de la r\xE8gle",
        b"Rule properties",
        b"Propiedades de la regla",
        b"Regeleigenschaften",
    ),
    (
        b"Nouvelle r\xE8gle de trafic entrant",
        b"New inbound rule",
        b"Nueva regla de entrada",
        b"Neue eingehende Regel",
    ),
    (
        b"Nouvelle r\xE8gle de trafic sortant",
        b"New outbound rule",
        b"Nueva regla de salida",
        b"Neue ausgehende Regel",
    ),
    (
        b"Autoriser la connexion",
        b"Allow the connection",
        b"Permitir la conexi\xF3n",
        b"Verbindung zulassen",
    ),
    (
        b"Bloquer la connexion",
        b"Block the connection",
        b"Bloquear la conexi\xF3n",
        b"Verbindung blockieren",
    ),
    (
        b"Ports locaux (vide = tous ; ex. 80, 443, 8000-8100)",
        b"Local ports (empty = all; e.g. 80, 443, 8000-8100)",
        b"Puertos locales (vac\xEDo = todos; p. ej. 80, 443, 8000-8100)",
        b"Lokale Ports (leer = alle; z. B. 80, 443, 8000-8100)",
    ),
    (
        b"Ports distants (vide = tous ; ex. 80, 443)",
        b"Remote ports (empty = all; e.g. 80, 443)",
        b"Puertos remotos (vac\xEDo = todos; p. ej. 80, 443)",
        b"Remoteports (leer = alle; z. B. 80, 443)",
    ),
    (
        b"Tous les ports",
        b"All ports",
        b"Todos los puertos",
        b"Alle Ports",
    ),
    (
        b"Programme (chemin du .exe ou de l'ex\xE9cutable Linux ; vide = tous)",
        b"Program (path of the .exe or Linux executable; empty = all)",
        b"Programa (ruta del .exe o del ejecutable de Linux; vac\xEDo = todos)",
        b"Programm (Pfad der .exe oder Linux-Programmdatei; leer = alle)",
    ),
    (b"Profils", b"Profiles", b"Perfiles", b"Profile"),
    (
        b"R\xE8gle activ\xE9e",
        b"Rule enabled",
        b"Regla habilitada",
        b"Regel aktiviert",
    ),
    (
        b"Alerte de s\xE9curit\xE9 Winlux",
        b"Winlux Security Alert",
        b"Alerta de seguridad de Winlux",
        b"Winlux-Sicherheitswarnung",
    ),
    (
        b"Le Pare-feu Winlux a bloqu\xE9 certaines fonctionnalit\xE9s de cette application.",
        b"Winlux Firewall has blocked some features of this app.",
        b"El Firewall de Winlux bloque\xF3 algunas caracter\xEDsticas de esta aplicaci\xF3n.",
        b"Die Winlux-Firewall hat einige Funktionen dieser App blockiert.",
    ),
    (b"Nom :", b"Name:", b"Nombre:", b"Name:"),
    (b"Chemin :", b"Path:", b"Ruta:", b"Pfad:"),
    (b"Ports :", b"Ports:", b"Puertos:", b"Ports:"),
    (
        b"Autoriser cette application \xE0 recevoir des connexions",
        b"Allow this app to accept incoming connections",
        b"\xBFPermitir que esta aplicaci\xF3n acepte conexiones entrantes",
        b"Dieser App erlauben, eingehende Verbindungen anzunehmen,",
    ),
    (
        b"sur les r\xE9seaux de type \xAB ",
        b"on \"",
        b"en redes de tipo \xAB",
        b"in Netzwerken vom Typ \"",
    ),
    (b" \xBB ?", b"\" networks?", b"\xBB?", b"\"?"),
    (
        b"Autoriser l'acc\xE8s",
        b"Allow access",
        b"Permitir acceso",
        b"Zugriff zulassen",
    ),
    (
        b"L'application peut recevoir des connexions.",
        b"The app can now accept connections.",
        b"La aplicaci\xF3n ya puede aceptar conexiones.",
        b"Die App kann jetzt Verbindungen annehmen.",
    ),
    (
        b"L'application reste bloqu\xE9e.",
        b"The app stays blocked.",
        b"La aplicaci\xF3n sigue bloqueada.",
        b"Die App bleibt blockiert.",
    ),
];

// ---------- Zones cliquables ----------

const MAX_HITS: usize = 160;

/// Zones cliquables relevées pendant le dessin ; les clics sont résolus
/// sur la géométrie réellement affichée.
pub struct Hits {
    rects: [Rect; MAX_HITS],
    ids: [u16; MAX_HITS],
    n: usize,
    pub mx: i32,
    pub my: i32,
}

impl Hits {
    pub const fn new() -> Hits {
        Hits {
            rects: [rect(0, 0, 0, 0); MAX_HITS],
            ids: [0; MAX_HITS],
            n: 0,
            mx: -1,
            my: -1,
        }
    }
    pub fn clear(&mut self) {
        self.n = 0;
    }
    /// Enregistre une zone et indique si la souris la survole.
    pub fn add(&mut self, r: Rect, id: u16) -> bool {
        if self.n < MAX_HITS {
            self.rects[self.n] = r;
            self.ids[self.n] = id;
            self.n += 1;
        }
        inside(&r, self.mx, self.my)
    }
    /// Zone sous le point (la dernière dessinée l'emporte).
    pub fn find(&self, x: i32, y: i32) -> Option<u16> {
        (0..self.n)
            .rev()
            .find(|&i| inside(&self.rects[i], x, y))
            .map(|i| self.ids[i])
    }
}

// ---------- Texte ----------

/// Tampon de texte borné.
#[derive(Clone, Copy)]
pub struct Field<const N: usize> {
    pub buf: [u8; N],
    pub len: usize,
}

impl<const N: usize> Field<N> {
    pub const fn new() -> Self {
        Field {
            buf: [0; N],
            len: 0,
        }
    }
    pub fn get(&self) -> &[u8] {
        &self.buf[..self.len]
    }
    pub fn set(&mut self, s: &[u8]) {
        let n = s.len().min(N);
        self.buf[..n].copy_from_slice(&s[..n]);
        self.len = n;
    }
    pub fn push(&mut self, s: &[u8]) {
        for &c in s {
            if self.len < N {
                self.buf[self.len] = c;
                self.len += 1;
            }
        }
    }
    /// Saisie clavier : vrai si le texte a changé.
    pub fn key(&mut self, ch: u16) -> bool {
        match ch {
            8 => {
                self.len = self.len.saturating_sub(1);
                true
            }
            32..=126 | 160..=255 if self.len < N => {
                self.buf[self.len] = ch as u8;
                self.len += 1;
                true
            }
            _ => false,
        }
    }
}

/// Écrit un nombre décimal.
pub fn push_num<const N: usize>(f: &mut Field<N>, v: i64) {
    let mut tmp = [0u8; 20];
    let mut i = tmp.len();
    let neg = v < 0;
    let mut n = v.unsigned_abs();
    loop {
        i -= 1;
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if neg {
        f.push(b"-");
    }
    f.push(&tmp[i..]);
}

pub fn parse_num(s: &[u8]) -> Option<u32> {
    if s.is_empty() || s.len() > 9 || !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(s.iter().fold(0u32, |n, &c| n * 10 + (c - b'0') as u32))
}

pub fn trim(s: &[u8]) -> &[u8] {
    let a = s.iter().position(|&c| c != b' ').unwrap_or(s.len());
    let b = s.iter().rposition(|&c| c != b' ').map_or(a, |i| i + 1);
    &s[a..b]
}

/// Valeur de `clé=` dans un texte `clé=valeur` (première occurrence).
pub fn field<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() > key.len() && line.starts_with(key) && line[key.len()] == b'=' {
            return &line[key.len() + 1..];
        }
    }
    b""
}

/// Champ séparé par des tabulations.
pub fn column(line: &[u8], index: usize) -> &[u8] {
    line.split(|&c| c == b'\t').nth(index).unwrap_or(b"")
}

// ---------- Fichiers ----------

/// Chemin `Z:\…` terminé par un zéro.
pub struct Path {
    buf: [u8; 260],
    len: usize,
}

impl Path {
    pub fn new(base: &[u8]) -> Path {
        let mut p = Path {
            buf: [0; 260],
            len: 0,
        };
        p.push(base);
        p
    }
    pub fn push(&mut self, s: &[u8]) -> &mut Path {
        for &c in s {
            if self.len < 258 {
                self.buf[self.len] = c;
                self.len += 1;
            }
        }
        self.buf[self.len] = 0;
        self
    }
    pub fn ptr(&self) -> *const i8 {
        self.buf.as_ptr() as *const i8
    }
}

pub unsafe fn exists(p: *const i8) -> bool {
    GetFileAttributesA(p) != u32::MAX
}

/// Lit un fichier ; avec `tail`, garde la fin s'il dépasse le tampon.
pub unsafe fn read_file(path: *const i8, buf: &mut [u8], tail: bool) -> usize {
    let f = CreateFileA(
        path,
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return 0;
    }
    if tail {
        let size = GetFileSize(f, core::ptr::null_mut());
        if size != u32::MAX && size as usize > buf.len() {
            SetFilePointer(
                f,
                (size as usize - buf.len()) as i32,
                core::ptr::null_mut(),
                0,
            );
        }
    }
    let mut total = 0usize;
    while total < buf.len() {
        let mut read = 0u32;
        let ok = ReadFile(
            f,
            buf[total..].as_mut_ptr(),
            (buf.len() - total) as u32,
            &mut read,
            core::ptr::null_mut(),
        );
        if ok == 0 || read == 0 {
            break;
        }
        total += read as usize;
    }
    CloseHandle(f);
    total
}

/// Remplace le contenu d'un fichier (fichier temporaire puis renommage).
pub unsafe fn write_file(path: &Path, data: &[u8]) -> bool {
    let mut tmp = Path::new(&path.buf[..path.len]);
    tmp.push(b".new");
    let f = CreateFileA(
        tmp.ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return false;
    }
    let mut written = 0u32;
    let ok = WriteFile(
        f,
        data.as_ptr(),
        data.len() as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(f);
    if ok == 0 || written as usize != data.len() {
        DeleteFileA(tmp.ptr());
        return false;
    }
    DeleteFileA(path.ptr());
    MoveFileA(tmp.ptr(), path.ptr()) != 0
}

/// Lance `winlux ARGS` (services d'administration du runtime).
pub unsafe fn winlux(args: &[u8]) -> bool {
    let mut a = [0u8; 300];
    let n = args.len().min(298);
    a[..n].copy_from_slice(&args[..n]);
    ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux".as_ptr(),
        a.as_ptr() as *const i8,
        core::ptr::null(),
        0,
    ) as isize
        > 32
}

// ---------- Contrôles ----------

pub unsafe fn label(g: &mut Gfx, x: i32, y: i32, s: &[u8]) {
    text(g.hdc, g.f.caption, pal().text2, x, y, s);
}

/// Bouton de barre de commandes ou de boîte de dialogue.
pub unsafe fn button(
    g: &mut Gfx,
    h: &mut Hits,
    r: Rect,
    id: u16,
    s: &[u8],
    primary: bool,
    enabled: bool,
) {
    let p = pal();
    let hover = h.add(r, id) && enabled;
    if primary && enabled {
        let c = if hover {
            mix(p.accent, p.text, 1, 8)
        } else {
            p.accent
        };
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, c);
        text_centered(
            g.hdc,
            g.f.body,
            13,
            p.on_accent,
            (r.left + r.right) / 2,
            r.top + (r.bottom - r.top - 15) / 2,
            s,
        );
    } else {
        let bg = if hover { p.hover } else { p.surface };
        panel(
            g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, bg, p.border,
        );
        let c = if enabled { p.text } else { p.text3 };
        text_centered(
            g.hdc,
            g.f.body,
            13,
            c,
            (r.left + r.right) / 2,
            r.top + (r.bottom - r.top - 15) / 2,
            s,
        );
    }
}

/// Largeur d'un bouton pour son libellé.
pub fn button_w(s: &[u8]) -> i32 {
    text_width(s, 13) + 28
}

/// Puce sélectionnable (choix exclusif ou multiple).
pub unsafe fn chip(
    g: &mut Gfx,
    h: &mut Hits,
    x: i32,
    y: i32,
    id: u16,
    s: &[u8],
    selected: bool,
) -> i32 {
    let p = pal();
    let w = text_width(s, 13) + 24;
    let r = rect(x, y, x + w, y + 30);
    let hover = h.add(r, id);
    if selected {
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 15, p.accent);
        text(g.hdc, g.f.body, p.on_accent, x + 12, y + 7, s);
    } else {
        panel(
            g.hdc,
            g.b,
            r.left,
            r.top,
            r.right,
            r.bottom,
            15,
            if hover { p.hover } else { p.surface },
            p.border,
        );
        text(g.hdc, g.f.body, p.text, x + 12, y + 7, s);
    }
    w + 8
}

/// Interrupteur Windows 11 suivi de son libellé.
pub unsafe fn toggle(g: &mut Gfx, h: &mut Hits, x: i32, y: i32, id: u16, on: bool, s: &[u8]) {
    let p = pal();
    let w = 40 + 12 + text_width(s, 13);
    h.add(rect(x, y - 2, x + w, y + 22), id);
    if on {
        round_rect(g.hdc, g.b, x, y, x + 40, y + 20, 10, p.accent);
        ellipse(g.hdc, g.b, x + 24, y + 4, x + 36, y + 16, p.on_accent);
    } else {
        round_rect(g.hdc, g.b, x, y, x + 40, y + 20, 10, p.text2);
        round_rect(g.hdc, g.b, x + 1, y + 1, x + 39, y + 19, 9, p.surface);
        ellipse(g.hdc, g.b, x + 5, y + 5, x + 15, y + 15, p.text2);
    }
    text(g.hdc, g.f.body, p.text, x + 52, y + 2, s);
}

/// Champ de saisie ; le texte trop long montre sa fin.
pub unsafe fn input(
    g: &mut Gfx,
    h: &mut Hits,
    r: Rect,
    id: u16,
    value: &[u8],
    focused: bool,
    placeholder: &[u8],
) {
    let p = pal();
    h.add(r, id);
    panel(
        g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.surface, p.border,
    );
    if focused {
        fill(
            g.hdc,
            g.b,
            r.left + 2,
            r.bottom - 2,
            r.right - 2,
            r.bottom,
            p.accent,
        );
    }
    let max = r.right - r.left - 20;
    let mut start = 0;
    while start < value.len() && text_width(&value[start..], 13) > max {
        start += 1;
    }
    let ty = r.top + (r.bottom - r.top - 15) / 2;
    if value.is_empty() && !focused {
        text(g.hdc, g.f.body, p.text3, r.left + 10, ty, placeholder);
    } else {
        text(g.hdc, g.f.body, p.text, r.left + 10, ty, &value[start..]);
    }
    if focused {
        let cx = r.left + 10 + text_width(&value[start..], 13) + 1;
        fill(g.hdc, g.b, cx, ty, cx + 1, ty + 16, p.text);
    }
}

/// Cellule de tableau tronquée.
pub unsafe fn cell(g: &mut Gfx, x: i32, y: i32, w: i32, s: &[u8], color: u32) {
    text_fit(g.hdc, g.f.body, 13, color, x, y, s, w - 12);
}

/// En-tête de colonnes ; renvoie l'ordonnée de la première ligne.
pub unsafe fn header(g: &mut Gfx, x: i32, y: i32, right: i32, cols: &[(&[u8], i32)]) -> i32 {
    let p = pal();
    let mut cx = x;
    for (name, w) in cols {
        text_fit(g.hdc, g.f.caption, 12, p.text2, cx, y + 6, name, w - 10);
        cx += w;
    }
    fill(g.hdc, g.b, x - 8, y + 28, right, y + 29, p.border);
    y + 32
}

/// Navigation latérale ; renvoie vrai si l'entrée est survolée.
pub unsafe fn nav_item(
    g: &mut Gfx,
    h: &mut Hits,
    x: i32,
    y: i32,
    w: i32,
    id: u16,
    s: &[u8],
    glyph_kind: Glyph,
    active: bool,
) {
    let p = pal();
    let r = rect(x + 4, y, x + w - 4, y + 36);
    let hover = h.add(r, id);
    if active || hover {
        round_rect(
            g.hdc,
            g.b,
            r.left,
            r.top,
            r.right,
            r.bottom,
            4,
            if active { p.selected } else { p.hover },
        );
    }
    if active {
        round_rect(
            g.hdc,
            g.b,
            r.left,
            r.top + 9,
            r.left + 3,
            r.bottom - 9,
            2,
            p.accent,
        );
    }
    set_glyph_bg(Some(if active {
        p.selected
    } else if hover {
        p.hover
    } else {
        p.mica
    }));
    glyph(g.hdc, g.b, g.f, glyph_kind, x + 16, y + 9, 18);
    set_glyph_bg(None);
    text_fit(g.hdc, g.f.body, 13, p.text, x + 44, y + 10, s, w - 52);
}

/// Carte de boîte de dialogue centrée sur la zone cliente.
pub unsafe fn dialog(g: &mut Gfx, h: &mut Hits, c: &Rect, w: i32, ht: i32, title: &[u8]) -> Rect {
    let p = pal();
    // Voile : un clic hors de la carte ne traverse pas.
    h.add(*c, 0xFFFF);
    let veil = mix(p.window, p.shadow, 4, 10);
    fill(g.hdc, g.b, c.left, c.top, c.right, c.bottom, veil);
    let w = w.min(c.right - c.left - 24);
    let ht = ht.min(c.bottom - c.top - 24);
    let x = (c.left + c.right - w) / 2;
    let y = (c.top + c.bottom - ht) / 2;
    panel(g.hdc, g.b, x, y, x + w, y + ht, 8, p.window, p.border);
    h.add(rect(x, y, x + w, y + ht), 0xFFFE);
    text(g.hdc, g.f.large, p.text, x + 24, y + 20, title);
    rect(x, y, x + w, y + ht)
}
