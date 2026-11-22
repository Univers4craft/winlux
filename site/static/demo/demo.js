/* Démo de Winlux dans le navigateur (#2926).
 *
 * Une simulation du bureau, écrite à la main, sans bibliothèque : rien du
 * runtime ne tourne ici et rien n'est téléchargé ni envoyé. Ce qui est imité
 * vient du dépôt :
 *   - écran de démarrage Prisme : runtime/src/bootsplash.rs ;
 *   - menu Démarrer (recherche sans casse ni accents, Épinglé, Toutes les
 *     applications, volet « Pour vous ») : scripts/winlux-start-menu.py ;
 *   - ancrage (bords et coins, dispositions, flèches, Assistant d'ancrage) :
 *     scripts/winlux-snap.py, mêmes zones et mêmes dispositions ;
 *   - icônes : jeu maison Prisme, assets/icons/explorateur/.
 */
"use strict";
(() => {
  // ── Langues ────────────────────────────────────────────────────────────
  const LANGS = ["fr", "en", "de", "es"];
  const T = {
    fr: {
      site: "Site de Winlux", demo: "Démonstration", langue: "Langue",
      avis: "Une simulation du bureau dans votre navigateur, pas Winlux en train de tourner. Rien n'est téléchargé ni envoyé.",
      passer: "Cliquez ou appuyez sur une touche pour passer", deverrouiller: "Cliquez pour ouvrir la session",
      recherche: "Tapez pour rechercher…", epingle: "Épinglé", toutes: "Toutes les applications  ›", retour: "‹  Retour",
      resultats: "Résultats", aucune: "Aucune application trouvée", fichier_de: "Fichier · ",
      pour_vous: "Pour vous", masquer: "Masquer", reprendre: "Reprendre où vous en étiez", souvent: "Souvent ouvert à cette heure-ci",
      habitudes: "D’après vos habitudes {jours}, {moment}", semaine: "en semaine", weekend: "le week-end",
      matin: "le matin", midi: "l’après-midi", soir: "le soir",
      vie_privee: "Calculé sur ce PC à partir de votre usage, rien n’est envoyé. Désactivable dans Paramètres.",
      il_y_a_min: "il y a {n} min", hier: "hier", il_y_a_j: "il y a {n} jours",
      visiteur: "Visiteur", alim: "Options d’alimentation", verrouiller: "Verrouiller", veille: "Mettre en veille",
      redemarrer: "Redémarrer", arreter: "Arrêter", rallumer: "Rallumer",
      explorateur: "Explorateur de fichiers", blocnotes: "Bloc-notes", terminal: "Terminal", parametres: "Paramètres",
      apropos: "À propos de cette démo", bonjour: "Bonjour",
      ce_pc: "Ce PC", corbeille: "Corbeille", accueil: "Accueil", bureau: "Bureau", documents: "Documents", images: "Images",
      musique: "Musique", telechargements: "Téléchargements", videos: "Vidéos",
      acces_rapide: "Accès rapide", lecteurs: "Lecteurs", recents: "Récents", dossiers: "Dossiers",
      perso: "Dossier personnel (C:)", linux: "Système Linux (Z:)", libre: "{n} Go libres sur {t} Go",
      elements: "{n} élément(s)", vide: "Ce dossier est vide.",
      precedent: "Précédent", suivant: "Suivant", parent: "Dossier parent",
      reduire: "Réduire", agrandir: "Agrandir", restaurer: "Restaurer", fermer: "Fermer",
      assist_autre: "Choisissez une fenêtre pour l’autre côté", assist_place: "Choisissez une fenêtre pour cet emplacement",
      nouveau: "Nouveau", enregistrer: "Enregistrer", enregistre: "Enregistré dans ce navigateur seulement.",
      ligne_col: "Ligne {l}, colonne {c}", caracteres: "{n} caractères",
      notes_txt: "Notes.txt", bienvenue_txt: "Bienvenue.txt", rapport: "Rapport.pdf", vacances: "Vacances.png",
      capture: "Capture.png", morceau: "Morceau.ogg", guide: "Guide.pdf",
      notes: "Liste de courses\n- pain\n- lait\n- piles pour la souris\n\nIdée : essayer l'ancrage à quatre fenêtres.\n",
      bienvenue: "Bienvenue dans la démo de Winlux !\n\nCe bureau est une simulation qui tourne dans votre navigateur.\nOuvrez des fenêtres, glissez-les vers un bord ou un coin de l'écran,\net laissez la souris sur le bouton Agrandir pour voir les dispositions.\n",
      apercu: "Aperçu indisponible dans la démo", apercu_txt: "Dans Winlux, ce fichier s'ouvrirait dans l'application associée. La démo ne contient pas de vrai fichier.",
      exe_titre: "Un programme .exe", exe_txt: "Dans Winlux, un double-clic sur un fichier .exe lance le programme, sans réglage. Ici, c'est une imitation : rien n'est exécuté.",
      ok: "OK",
      perso_titre: "Personnalisation", mode: "Mode", sombre: "Sombre", clair: "Clair", fond: "Fond d’écran",
      aurore: "Aurore", nuit: "Nuit", aube: "Aube",
      ancrage: "Ancrage des fenêtres", assistant: "Assistant d’ancrage",
      assistant_txt: "Après un ancrage, proposer les autres fenêtres pour remplir la place libre.",
      survol: "Dispositions au survol du bouton Agrandir", oui: "Activé", non: "Désactivé",
      term_bienvenue: "Terminal de démonstration : seules quelques commandes sont imitées. Tapez « aide ».",
      term_aide: "Commandes : aide, ls, pwd, whoami, uname -a, date, echo, clear, exit",
      term_inconnu: "{c} : commande inconnue dans la démo (tapez « aide »)",
      apropos_titre: "Une démonstration, pas le système",
      apropos_1: "Ce bureau est une simulation en HTML, CSS et JavaScript. Il reprend l'allure de Winlux, mais aucun programme ne s'exécute : le vrai système ne tourne pas dans votre navigateur.",
      apropos_2: "À essayer :",
      astuce_1: "Glissez une fenêtre par sa barre de titre vers un bord de l'écran (moitié), un coin (quart) ou le haut (plein écran).",
      astuce_2: "Laissez la souris sur le bouton Agrandir : la grille des dispositions apparaît.",
      astuce_3: "Touche Super (ou Ctrl + Alt) avec les flèches : ancrer la fenêtre active. Super + Z : dispositions.",
      astuce_4: "La recherche du menu Démarrer ignore les majuscules et les accents.",
      apropos_3: "Pour essayer le vrai Winlux, téléchargez l'image et démarrez-la dans une machine virtuelle.",
      telecharger: "Télécharger Winlux",
      reseau: "Réseau", volume: "Volume", demarrer: "Démarrer",
    },
    en: {
      site: "Winlux website", demo: "Demo", langue: "Language",
      avis: "A simulation of the desktop in your browser, not Winlux actually running. Nothing is downloaded or sent.",
      passer: "Click or press a key to skip", deverrouiller: "Click to sign in",
      recherche: "Type to search…", epingle: "Pinned", toutes: "All apps  ›", retour: "‹  Back",
      resultats: "Results", aucune: "No apps found", fichier_de: "File · ",
      pour_vous: "For you", masquer: "Hide", reprendre: "Pick up where you left off", souvent: "Often opened around this time",
      habitudes: "Based on your habits {jours}, {moment}", semaine: "on weekdays", weekend: "at weekends",
      matin: "in the morning", midi: "in the afternoon", soir: "in the evening",
      vie_privee: "Worked out on this PC from your usage, nothing is sent. Can be turned off in Settings.",
      il_y_a_min: "{n} min ago", hier: "yesterday", il_y_a_j: "{n} days ago",
      visiteur: "Visitor", alim: "Power options", verrouiller: "Lock", veille: "Sleep",
      redemarrer: "Restart", arreter: "Shut down", rallumer: "Turn on",
      explorateur: "File Explorer", blocnotes: "Notepad", terminal: "Terminal", parametres: "Settings",
      apropos: "About this demo", bonjour: "Hello",
      ce_pc: "This PC", corbeille: "Recycle Bin", accueil: "Home", bureau: "Desktop", documents: "Documents", images: "Pictures",
      musique: "Music", telechargements: "Downloads", videos: "Videos",
      acces_rapide: "Quick access", lecteurs: "Drives", recents: "Recent", dossiers: "Folders",
      perso: "Home folder (C:)", linux: "Linux system (Z:)", libre: "{n} GB free of {t} GB",
      elements: "{n} item(s)", vide: "This folder is empty.",
      precedent: "Back", suivant: "Forward", parent: "Up",
      reduire: "Minimize", agrandir: "Maximize", restaurer: "Restore", fermer: "Close",
      assist_autre: "Choose a window for the other side", assist_place: "Choose a window for this space",
      nouveau: "New", enregistrer: "Save", enregistre: "Saved in this browser only.",
      ligne_col: "Line {l}, column {c}", caracteres: "{n} characters",
      notes_txt: "Notes.txt", bienvenue_txt: "Welcome.txt", rapport: "Report.pdf", vacances: "Holidays.png",
      capture: "Screenshot.png", morceau: "Song.ogg", guide: "Guide.pdf",
      notes: "Shopping list\n- bread\n- milk\n- batteries for the mouse\n\nIdea: try snapping four windows.\n",
      bienvenue: "Welcome to the Winlux demo!\n\nThis desktop is a simulation running in your browser.\nOpen windows, drag them to an edge or a corner of the screen,\nand rest the mouse on the Maximize button to see the layouts.\n",
      apercu: "No preview in the demo", apercu_txt: "In Winlux, this file would open in its associated app. The demo holds no real file.",
      exe_titre: "An .exe program", exe_txt: "In Winlux, double-clicking an .exe file starts the program, with no setup. Here it is an imitation: nothing is run.",
      ok: "OK",
      perso_titre: "Personalization", mode: "Mode", sombre: "Dark", clair: "Light", fond: "Background",
      aurore: "Aurora", nuit: "Night", aube: "Dawn",
      ancrage: "Window snapping", assistant: "Snap assist",
      assistant_txt: "After snapping, offer the other windows to fill the free space.",
      survol: "Layouts when hovering the Maximize button", oui: "On", non: "Off",
      term_bienvenue: "Demo terminal: only a few commands are imitated. Type “help”.",
      term_aide: "Commands: help, ls, pwd, whoami, uname -a, date, echo, clear, exit",
      term_inconnu: "{c}: unknown command in the demo (type “help”)",
      apropos_titre: "A demonstration, not the system",
      apropos_1: "This desktop is a simulation in HTML, CSS and JavaScript. It takes on the look of Winlux, but no program runs: the real system is not running in your browser.",
      apropos_2: "Things to try:",
      astuce_1: "Drag a window by its title bar to a screen edge (half), a corner (quarter) or the top (full screen).",
      astuce_2: "Rest the mouse on the Maximize button: the layout grid appears.",
      astuce_3: "Super key (or Ctrl + Alt) with the arrows: snap the active window. Super + Z: layouts.",
      astuce_4: "The Start menu search ignores capitals and accents.",
      apropos_3: "To try the real Winlux, download the image and start it in a virtual machine.",
      telecharger: "Download Winlux",
      reseau: "Network", volume: "Volume", demarrer: "Start",
    },
    de: {
      site: "Winlux-Website", demo: "Demo", langue: "Sprache",
      avis: "Eine Simulation des Desktops in Ihrem Browser, nicht das laufende Winlux. Nichts wird heruntergeladen oder gesendet.",
      passer: "Klicken oder eine Taste drücken zum Überspringen", deverrouiller: "Zum Anmelden klicken",
      recherche: "Zum Suchen tippen…", epingle: "Angeheftet", toutes: "Alle Apps  ›", retour: "‹  Zurück",
      resultats: "Ergebnisse", aucune: "Keine Apps gefunden", fichier_de: "Datei · ",
      pour_vous: "Für Sie", masquer: "Ausblenden", reprendre: "Dort weitermachen, wo Sie aufgehört haben", souvent: "Oft um diese Uhrzeit geöffnet",
      habitudes: "Nach Ihren Gewohnheiten {jours}, {moment}", semaine: "unter der Woche", weekend: "am Wochenende",
      matin: "morgens", midi: "nachmittags", soir: "abends",
      vie_privee: "Auf diesem PC aus Ihrer Nutzung berechnet, nichts wird gesendet. In den Einstellungen abschaltbar.",
      il_y_a_min: "vor {n} Min.", hier: "gestern", il_y_a_j: "vor {n} Tagen",
      visiteur: "Besucher", alim: "Energieoptionen", verrouiller: "Sperren", veille: "Energie sparen",
      redemarrer: "Neu starten", arreter: "Herunterfahren", rallumer: "Einschalten",
      explorateur: "Datei-Explorer", blocnotes: "Editor", terminal: "Terminal", parametres: "Einstellungen",
      apropos: "Über diese Demo", bonjour: "Hallo",
      ce_pc: "Dieser PC", corbeille: "Papierkorb", accueil: "Start", bureau: "Desktop", documents: "Dokumente", images: "Bilder",
      musique: "Musik", telechargements: "Downloads", videos: "Videos",
      acces_rapide: "Schnellzugriff", lecteurs: "Laufwerke", recents: "Zuletzt verwendet", dossiers: "Ordner",
      perso: "Persönlicher Ordner (C:)", linux: "Linux-System (Z:)", libre: "{n} GB frei von {t} GB",
      elements: "{n} Element(e)", vide: "Dieser Ordner ist leer.",
      precedent: "Zurück", suivant: "Vorwärts", parent: "Übergeordneter Ordner",
      reduire: "Minimieren", agrandir: "Maximieren", restaurer: "Wiederherstellen", fermer: "Schließen",
      assist_autre: "Fenster für die andere Seite auswählen", assist_place: "Fenster für diesen Bereich auswählen",
      nouveau: "Neu", enregistrer: "Speichern", enregistre: "Nur in diesem Browser gespeichert.",
      ligne_col: "Zeile {l}, Spalte {c}", caracteres: "{n} Zeichen",
      notes_txt: "Notizen.txt", bienvenue_txt: "Willkommen.txt", rapport: "Bericht.pdf", vacances: "Urlaub.png",
      capture: "Bildschirmfoto.png", morceau: "Lied.ogg", guide: "Anleitung.pdf",
      notes: "Einkaufsliste\n- Brot\n- Milch\n- Batterien für die Maus\n\nIdee: vier Fenster andocken.\n",
      bienvenue: "Willkommen in der Winlux-Demo!\n\nDieser Desktop ist eine Simulation in Ihrem Browser.\nÖffnen Sie Fenster, ziehen Sie sie an einen Rand oder in eine Ecke,\nund lassen Sie die Maus auf der Schaltfläche Maximieren ruhen, um die Layouts zu sehen.\n",
      apercu: "Keine Vorschau in der Demo", apercu_txt: "In Winlux würde sich diese Datei in der zugehörigen App öffnen. Die Demo enthält keine echte Datei.",
      exe_titre: "Ein .exe-Programm", exe_txt: "In Winlux startet ein Doppelklick auf eine .exe-Datei das Programm, ohne Einrichtung. Hier ist es eine Nachahmung: nichts wird ausgeführt.",
      ok: "OK",
      perso_titre: "Personalisierung", mode: "Modus", sombre: "Dunkel", clair: "Hell", fond: "Hintergrund",
      aurore: "Polarlicht", nuit: "Nacht", aube: "Morgengrauen",
      ancrage: "Fenster andocken", assistant: "Andock-Assistent",
      assistant_txt: "Nach dem Andocken die anderen Fenster für den freien Platz anbieten.",
      survol: "Layouts beim Zeigen auf Maximieren", oui: "Ein", non: "Aus",
      term_bienvenue: "Demo-Terminal: nur wenige Befehle werden nachgeahmt. Geben Sie „hilfe“ ein.",
      term_aide: "Befehle: hilfe, ls, pwd, whoami, uname -a, date, echo, clear, exit",
      term_inconnu: "{c}: in der Demo unbekannter Befehl (geben Sie „hilfe“ ein)",
      apropos_titre: "Eine Demonstration, nicht das System",
      apropos_1: "Dieser Desktop ist eine Simulation in HTML, CSS und JavaScript. Er zeigt das Aussehen von Winlux, aber kein Programm läuft: das echte System läuft nicht in Ihrem Browser.",
      apropos_2: "Zum Ausprobieren:",
      astuce_1: "Ziehen Sie ein Fenster an der Titelleiste an einen Bildschirmrand (Hälfte), in eine Ecke (Viertel) oder nach oben (Vollbild).",
      astuce_2: "Lassen Sie die Maus auf Maximieren ruhen: das Layout-Raster erscheint.",
      astuce_3: "Super-Taste (oder Strg + Alt) mit den Pfeilen: aktives Fenster andocken. Super + Z: Layouts.",
      astuce_4: "Die Suche im Startmenü ignoriert Groß- und Kleinschreibung sowie Akzente.",
      apropos_3: "Um das echte Winlux auszuprobieren, laden Sie das Abbild herunter und starten Sie es in einer virtuellen Maschine.",
      telecharger: "Winlux herunterladen",
      reseau: "Netzwerk", volume: "Lautstärke", demarrer: "Start",
    },
    es: {
      site: "Sitio de Winlux", demo: "Demostración", langue: "Idioma",
      avis: "Una simulación del escritorio en su navegador, no Winlux funcionando de verdad. No se descarga ni se envía nada.",
      passer: "Haga clic o pulse una tecla para saltar", deverrouiller: "Haga clic para iniciar sesión",
      recherche: "Escriba para buscar…", epingle: "Anclado", toutes: "Todas las aplicaciones  ›", retour: "‹  Volver",
      resultats: "Resultados", aucune: "No se encontró ninguna aplicación", fichier_de: "Archivo · ",
      pour_vous: "Para ti", masquer: "Ocultar", reprendre: "Continuar donde lo dejó", souvent: "Abierto a menudo a esta hora",
      habitudes: "Según sus costumbres {jours}, {moment}", semaine: "entre semana", weekend: "el fin de semana",
      matin: "por la mañana", midi: "por la tarde", soir: "por la noche",
      vie_privee: "Calculado en este PC a partir de su uso, no se envía nada. Se puede desactivar en Configuración.",
      il_y_a_min: "hace {n} min", hier: "ayer", il_y_a_j: "hace {n} días",
      visiteur: "Visitante", alim: "Opciones de energía", verrouiller: "Bloquear", veille: "Suspender",
      redemarrer: "Reiniciar", arreter: "Apagar", rallumer: "Encender",
      explorateur: "Explorador de archivos", blocnotes: "Bloc de notas", terminal: "Terminal", parametres: "Configuración",
      apropos: "Acerca de esta demostración", bonjour: "Hola",
      ce_pc: "Este equipo", corbeille: "Papelera", accueil: "Inicio", bureau: "Escritorio", documents: "Documentos", images: "Imágenes",
      musique: "Música", telechargements: "Descargas", videos: "Vídeos",
      acces_rapide: "Acceso rápido", lecteurs: "Unidades", recents: "Recientes", dossiers: "Carpetas",
      perso: "Carpeta personal (C:)", linux: "Sistema Linux (Z:)", libre: "{n} GB libres de {t} GB",
      elements: "{n} elemento(s)", vide: "Esta carpeta está vacía.",
      precedent: "Atrás", suivant: "Adelante", parent: "Carpeta superior",
      reduire: "Minimizar", agrandir: "Maximizar", restaurer: "Restaurar", fermer: "Cerrar",
      assist_autre: "Elija una ventana para el otro lado", assist_place: "Elija una ventana para este espacio",
      nouveau: "Nuevo", enregistrer: "Guardar", enregistre: "Guardado solo en este navegador.",
      ligne_col: "Línea {l}, columna {c}", caracteres: "{n} caracteres",
      notes_txt: "Notas.txt", bienvenue_txt: "Bienvenida.txt", rapport: "Informe.pdf", vacances: "Vacaciones.png",
      capture: "Captura.png", morceau: "Cancion.ogg", guide: "Guia.pdf",
      notes: "Lista de la compra\n- pan\n- leche\n- pilas para el ratón\n\nIdea: probar a anclar cuatro ventanas.\n",
      bienvenue: "¡Bienvenido a la demostración de Winlux!\n\nEste escritorio es una simulación que funciona en su navegador.\nAbra ventanas, arrástrelas a un borde o a una esquina de la pantalla\ny deje el ratón sobre el botón Maximizar para ver las disposiciones.\n",
      apercu: "Sin vista previa en la demostración", apercu_txt: "En Winlux, este archivo se abriría en la aplicación asociada. La demostración no contiene ningún archivo real.",
      exe_titre: "Un programa .exe", exe_txt: "En Winlux, un doble clic en un archivo .exe inicia el programa, sin configurar nada. Aquí es una imitación: no se ejecuta nada.",
      ok: "Aceptar",
      perso_titre: "Personalización", mode: "Modo", sombre: "Oscuro", clair: "Claro", fond: "Fondo",
      aurore: "Aurora", nuit: "Noche", aube: "Amanecer",
      ancrage: "Anclaje de ventanas", assistant: "Asistente de anclaje",
      assistant_txt: "Después de anclar, ofrecer las otras ventanas para llenar el espacio libre.",
      survol: "Disposiciones al pasar sobre Maximizar", oui: "Activado", non: "Desactivado",
      term_bienvenue: "Terminal de demostración: solo se imitan algunos comandos. Escriba «ayuda».",
      term_aide: "Comandos: ayuda, ls, pwd, whoami, uname -a, date, echo, clear, exit",
      term_inconnu: "{c}: comando desconocido en la demostración (escriba «ayuda»)",
      apropos_titre: "Una demostración, no el sistema",
      apropos_1: "Este escritorio es una simulación en HTML, CSS y JavaScript. Tiene el aspecto de Winlux, pero no se ejecuta ningún programa: el sistema real no funciona en su navegador.",
      apropos_2: "Para probar:",
      astuce_1: "Arrastre una ventana por su barra de título hasta un borde de la pantalla (mitad), una esquina (cuarto) o arriba (pantalla completa).",
      astuce_2: "Deje el ratón sobre el botón Maximizar: aparece la cuadrícula de disposiciones.",
      astuce_3: "Tecla Super (o Ctrl + Alt) con las flechas: anclar la ventana activa. Super + Z: disposiciones.",
      astuce_4: "La búsqueda del menú Inicio ignora mayúsculas y acentos.",
      apropos_3: "Para probar el Winlux real, descargue la imagen e iníciela en una máquina virtual.",
      telecharger: "Descargar Winlux",
      reseau: "Red", volume: "Volumen", demarrer: "Inicio",
    },
  };

  const stock = {
    lire(k, d) { try { const v = localStorage.getItem("winlux-demo-" + k); return v === null ? d : v; } catch (_) { return d; } },
    ecrire(k, v) { try { localStorage.setItem("winlux-demo-" + k, v); } catch (_) { /* navigation privée */ } },
  };
  const choisirLangue = () => {
    const q = new URLSearchParams(location.search).get("lang");
    if (LANGS.includes(q)) return q;
    for (const l of navigator.languages || [navigator.language || "fr"]) {
      const c = String(l).slice(0, 2).toLowerCase();
      if (LANGS.includes(c)) return c;
    }
    return "en";
  };
  const lang = choisirLangue();
  const t = (k, v) => {
    let s = (T[lang] && T[lang][k]) ?? T.fr[k] ?? k;
    if (v) for (const [a, b] of Object.entries(v)) s = s.replace("{" + a + "}", b);
    return s;
  };
  // Recherche : ni casse ni accents, comme fold() du menu Démarrer.
  const plier = (s) => s.toLocaleLowerCase().replace(/œ/g, "oe").replace(/æ/g, "ae").replace(/ß/g, "ss")
    .normalize("NFKD").replace(/[̀-ͯ]/g, "");

  // ── Petits outils DOM ──────────────────────────────────────────────────
  function h(tag, attrs, ...enfants) {
    const el = document.createElement(tag);
    if (attrs) for (const [k, v] of Object.entries(attrs)) {
      if (v === undefined || v === null || v === false) continue;
      if (k === "class") el.className = v;
      else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
      else if (k === "html") el.innerHTML = v; // uniquement nos propres SVG
      else el.setAttribute(k, v === true ? "" : v);
    }
    for (const e of enfants.flat()) if (e !== null && e !== undefined && e !== false) el.append(e);
    return el;
  }
  const svg = (chemin, vb = "0 0 16 16") =>
    `<svg viewBox="${vb}" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${chemin}</svg>`;
  const S = {
    loupe: svg('<circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5 14 14"/>'),
    alim: svg('<path d="M8 2v6"/><path d="M4.4 4.6a5 5 0 1 0 7.2 0"/>'),
    reseau: svg('<rect x="2" y="9" width="3" height="5" rx=".5"/><rect x="6.5" y="6" width="3" height="8" rx=".5"/><rect x="11" y="3" width="3" height="11" rx=".5"/>'),
    volume: svg('<path d="M2.5 6h2.5l3.5-3v10L5 10H2.5z"/><path d="M11 5.5a3.5 3.5 0 0 1 0 5"/><path d="M12.8 3.6a6 6 0 0 1 0 8.8"/>'),
    gauche: svg('<path d="M10 3 5 8l5 5"/>'), droite: svg('<path d="m6 3 5 5-5 5"/>'), haut: svg('<path d="m3 10 5-5 5 5"/>'),
    reduire: svg('<path d="M1 8h14"/>'), agrandir: svg('<rect x="1.5" y="1.5" width="13" height="13" rx="1.5"/>'),
    restaurer: svg('<rect x="1.5" y="4.5" width="10" height="10" rx="1.5"/><path d="M4.5 4.5V3a1.5 1.5 0 0 1 1.5-1.5h7A1.5 1.5 0 0 1 14.5 3v7a1.5 1.5 0 0 1-1.5 1.5h-1.5"/>'),
    fermer: svg('<path d="m2 2 12 12M14 2 2 14"/>'),
    cadenas: svg('<rect x="3" y="7" width="10" height="7" rx="1.5"/><path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2"/>'),
    engrenage: svg('<circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/><circle cx="8" cy="8" r="4.6"/>'),
  };
  // Jeu Prisme : une variante par thème (« -clair »), échangée au changement de mode.
  const srcIco = (nom, p) => "icones/" + nom + (document.documentElement.dataset.theme === "clair" ? "-clair" : "") + (p ? "-p" : "") + ".png";
  const ico = (nom, p) => h("img", { src: srcIco(nom, p), "data-ico": nom, "data-p": p ? "1" : null, alt: "", width: p ? 20 : 48, height: p ? 20 : 48, draggable: "false" });
  const logo = (taille) => h("img", { src: "../img/logo.svg", alt: "", width: taille, height: taille, draggable: "false" });
  const icoParametres = () => h("span", { class: "ico-svg", html: S.engrenage.replace("currentColor", "#b69cff") });

  // ── Bandeau, langue, mention ───────────────────────────────────────────
  document.documentElement.lang = lang;
  document.title = "Winlux — " + t("demo");
  document.querySelectorAll("[data-t]").forEach((el) => { el.textContent = t(el.dataset.t); });
  document.getElementById("retour").href = "../../" + lang + "/index.html";
  document.querySelectorAll(".langues a").forEach((a) => { if (a.dataset.lang === lang) a.setAttribute("aria-current", "true"); });
  document.querySelector(".langues").setAttribute("aria-label", t("langue"));
  document.querySelectorAll(".mention [lang]").forEach((el) => { el.hidden = el.lang !== lang; });

  const ecran = document.getElementById("ecran");
  ecran.textContent = "";
  const HAUTEUR_BARRE = 48;
  const aire = () => ({ w: ecran.clientWidth, h: ecran.clientHeight - HAUTEUR_BARRE });
  const petit = () => ecran.clientWidth < 720;

  // ── Réglages (gardés dans ce navigateur seulement) ─────────────────────
  const reglages = {
    theme: stock.lire("theme", "sombre"),
    fond: stock.lire("fond", "aurore"),
    assistant: stock.lire("assistant", "1") === "1",
    survol: stock.lire("survol", "1") === "1",
    pourVous: stock.lire("pour-vous", "1") === "1",
  };
  function appliquerTheme() {
    document.documentElement.dataset.theme = reglages.theme;
    const fonds = { aurore: reglages.theme === "clair" ? "aurore-clair.jpg" : "aurore.jpg", nuit: "../img/fond-nuit.jpg", aube: "../img/fond-aube.jpg" };
    document.documentElement.style.setProperty("--fond", `url("${fonds[reglages.fond] || fonds.aurore}")`);
    document.querySelectorAll("img[data-ico]").forEach((img) => { img.src = srcIco(img.dataset.ico, img.dataset.p); });
  }
  appliquerTheme();

  // ── Fichiers de la démo ────────────────────────────────────────────────
  const DOSSIERS = ["bureau", "documents", "images", "musique", "telechargements", "videos"];
  const ICONE_TYPE = { txt: "type-texte", pdf: "type-pdf", png: "type-image", ogg: "type-audio", exe: "type-exe" };
  const FICHIERS = {
    bureau: [{ n: t("bienvenue_txt"), texte: t("bienvenue") }],
    documents: [{ n: t("notes_txt"), texte: t("notes") }, { n: t("rapport") }],
    images: [{ n: t("vacances") }, { n: t("capture") }],
    musique: [{ n: t("morceau") }],
    telechargements: [{ n: "bonjour.exe" }, { n: t("guide") }],
    videos: [],
    linux: ["bin", "etc", "home", "opt", "usr", "var"].map((n) => ({ n, dossier: true })),
  };
  const extension = (n) => n.split(".").pop().toLowerCase();
  const icoFichier = (f) => (f.dossier ? "dossier" : ICONE_TYPE[extension(f.n)] || "type-texte");
  function ouvrirFichier(f, lieu) {
    if (f.dossier) return;
    const ext = extension(f.n);
    if (ext === "txt") ouvrir("blocnotes", f);
    else if (ext === "exe") ouvrir("bonjour");
    else ouvrir("apercu", f);
  }

  // ── Applications ───────────────────────────────────────────────────────
  const APPS = {
    explorateur: { nom: () => t("explorateur"), icone: () => ico("dossier"), taille: [860, 540], creer: creerExplorateur, epinglee: true },
    blocnotes: { nom: () => t("blocnotes"), icone: () => ico("type-texte"), taille: [640, 460], creer: creerBlocNotes, epinglee: true },
    terminal: { nom: () => t("terminal"), icone: () => ico("terminal"), taille: [680, 420], creer: creerTerminal, epinglee: true },
    parametres: { nom: () => t("parametres"), icone: icoParametres, taille: [760, 520], creer: creerParametres, epinglee: true, unique: true },
    apropos: { nom: () => t("apropos"), icone: () => logo(32), taille: [600, 520], creer: creerAPropos, unique: true },
    bonjour: { nom: () => "bonjour.exe", icone: () => ico("type-exe"), taille: [420, 250], creer: creerBonjour },
    apercu: { nom: () => t("apercu"), icone: () => ico("type-image"), taille: [440, 260], creer: creerApercu },
  };

  // ── Écran de démarrage Prisme ──────────────────────────────────────────
  let demarrage = null;
  function lancerDemarrage(ensuite) {
    if (demarrage) demarrage.remove();
    const el = h("div", { class: "demarrage", role: "img", "aria-label": "Winlux" },
      h("div", { class: "halo" }), h("img", { class: "logo", src: "../img/logo.svg", alt: "" }),
      h("div", { class: "points" }, h("i"), h("i"), h("i"), h("i"), h("i")),
      h("p", { class: "passer" }, t("passer")));
    demarrage = el;
    ecran.append(el);
    const reduit = matchMedia("(prefers-reduced-motion: reduce)").matches;
    let fini = false;
    const finir = () => {
      if (fini) return;
      fini = true;
      document.removeEventListener("keydown", finir);
      el.classList.add("fini");
      setTimeout(() => { el.remove(); if (demarrage === el) demarrage = null; }, 360);
      if (ensuite) ensuite();
    };
    el.addEventListener("pointerdown", finir);
    document.addEventListener("keydown", finir);
    setTimeout(finir, reduit ? 600 : 2600);
  }

  // ── Bureau ─────────────────────────────────────────────────────────────
  const bureau = h("div", { class: "bureau" });
  const icBureau = [
    { nom: t("ce_pc"), ico: "ce-pc", ouvrir: () => ouvrir("explorateur", "cepc") },
    { nom: t("corbeille"), ico: "corbeille-vide", ouvrir: () => ouvrir("explorateur", "corbeille") },
    { nom: t("documents"), ico: "documents", ouvrir: () => ouvrir("explorateur", "documents") },
    ...FICHIERS.bureau.map((f) => ({ nom: f.n, ico: icoFichier(f), ouvrir: () => ouvrirFichier(f) })),
    { nom: t("apropos"), ico: null, ouvrir: () => ouvrir("apropos") },
  ];
  for (const ic of icBureau) {
    const b = h("button", { class: "icone-bureau", type: "button" }, ic.ico ? ico(ic.ico) : logo(40), h("span", null, ic.nom));
    activerOuverture(b, ic.ouvrir, () => { bureau.querySelectorAll(".choisie").forEach((x) => x.classList.remove("choisie")); b.classList.add("choisie"); });
    bureau.append(b);
  }
  bureau.addEventListener("pointerdown", (e) => { if (e.target === bureau) bureau.querySelectorAll(".choisie").forEach((x) => x.classList.remove("choisie")); });
  /** Double-clic à la souris, simple toucher au doigt, Entrée au clavier. */
  function activerOuverture(el, ouvrirFn, choisir) {
    let doigt = false;
    el.addEventListener("pointerdown", (e) => { doigt = e.pointerType === "touch"; });
    el.addEventListener("click", () => { if (choisir) choisir(); if (doigt) ouvrirFn(); });
    el.addEventListener("dblclick", () => { if (!doigt) ouvrirFn(); });
    el.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); ouvrirFn(); } });
  }
  ecran.append(bureau);

  // ── Barre des tâches ───────────────────────────────────────────────────
  const barre = h("div", { class: "barre", role: "toolbar", "aria-label": "Winlux" });
  const centre = h("div", { class: "centre" });
  const boutonDemarrer = h("button", { class: "bouton-barre", type: "button", title: t("demarrer"), "aria-label": t("demarrer"), "aria-expanded": "false" }, logo(24));
  const horloge = h("div", { class: "horloge" });
  barre.append(centre, h("div", { class: "droite" },
    h("button", { class: "zone", type: "button", title: t("reseau") + " · " + t("volume") }, h("span", { html: S.reseau }), h("span", { html: S.volume }), h("span", null, lang.toUpperCase())),
    h("button", { class: "zone", type: "button" }, horloge)));
  ecran.append(barre);
  function majHorloge() {
    const d = new Date();
    horloge.replaceChildren(
      h("div", null, d.toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" })),
      h("div", null, d.toLocaleDateString(lang, { day: "2-digit", month: "2-digit", year: "numeric" })));
  }
  majHorloge();
  setInterval(majHorloge, 15000);

  // ── Fenêtres ───────────────────────────────────────────────────────────
  /** @type {Array<object>} */
  const fenetres = [];
  let active = null;
  let pile = 10;
  let compteur = 0;

  function ouvrir(appId, arg) {
    fermerMenu();
    const app = APPS[appId];
    if (app.unique) {
      const deja = fenetres.find((f) => f.app === appId);
      if (deja) { if (deja.naviguer && arg) deja.naviguer(arg); montrer(deja); return deja; }
    }
    const A = aire();
    const f = { id: ++compteur, app: appId, etat: "normal", zone: null, reduite: false };
    const [lw, lh] = app.taille;
    f.w = Math.min(lw, A.w - 24); f.h = Math.min(lh, A.h - 24);
    const decal = (fenetres.length % 6) * 28;
    f.x = Math.max(0, Math.round((A.w - f.w) / 2) - 70 + decal);
    f.y = Math.max(0, Math.round((A.h - f.h) / 2) - 50 + decal);
    f.nom = h("span", { class: "nom" }, app.nom());
    const bMax = h("button", { type: "button", class: "agrandir", title: t("agrandir"), "aria-label": t("agrandir"), html: S.agrandir });
    const bar = h("div", { class: "titre" },
      h("span", { class: "ico" }, app.icone()), f.nom,
      h("button", { type: "button", title: t("reduire"), "aria-label": t("reduire"), html: S.reduire, onclick: () => reduire(f) }),
      bMax,
      h("button", { type: "button", class: "fermer", title: t("fermer"), "aria-label": t("fermer"), html: S.fermer, onclick: () => fermer(f) }));
    f.bMax = bMax;
    f.contenu = h("div", { class: "contenu" });
    f.el = h("div", { class: "fenetre", role: "dialog", "aria-label": app.nom() }, bar, f.contenu, h("div", { class: "poignee" }));
    f.titre = (texte) => { f.nom.textContent = texte; f.el.setAttribute("aria-label", texte); };
    app.creer(f, arg);
    ecran.insertBefore(f.el, barre);
    fenetres.push(f);
    f.el.addEventListener("pointerdown", () => focaliser(f), true);
    bMax.addEventListener("click", () => basculerAgrandi(f));
    survolAgrandir(f, bMax);
    glisser(f, bar);
    redimensionner(f, f.el.querySelector(".poignee"));
    bar.addEventListener("dblclick", (e) => { if (!e.target.closest("button")) basculerAgrandi(f); });
    if (petit()) agrandir(f, false); else appliquer(f, false);
    focaliser(f);
    if (f.focusInitial) f.focusInitial();
    return f;
  }

  function appliquer(f, anime = true) {
    f.el.classList.toggle("anime", anime);
    Object.assign(f.el.style, { left: f.x + "px", top: f.y + "px", width: f.w + "px", height: f.h + "px" });
    f.el.classList.toggle("agrandie", f.etat === "max");
    f.el.classList.toggle("ancree", f.etat === "snap");
    const max = f.etat === "max";
    f.bMax.innerHTML = max ? S.restaurer : S.agrandir;
    f.bMax.title = max ? t("restaurer") : t("agrandir");
    f.bMax.setAttribute("aria-label", f.bMax.title);
    if (anime) setTimeout(() => f.el.classList.remove("anime"), 200);
  }
  function retenir(f) { if (f.etat === "normal") f.avant = { x: f.x, y: f.y, w: f.w, h: f.h }; }
  function agrandir(f, anime = true) {
    retenir(f);
    const A = aire();
    Object.assign(f, { etat: "max", zone: null, x: 0, y: 0, w: A.w, h: A.h });
    appliquer(f, anime);
  }
  function restaurer(f, anime = true) {
    const r = f.avant || { x: 60, y: 40, w: 640, h: 440 };
    Object.assign(f, { etat: "normal", zone: null }, r);
    appliquer(f, anime);
  }
  function basculerAgrandi(f) { if (f.etat === "max") restaurer(f); else agrandir(f); fermerDispositions(); }
  function reduire(f) {
    f.reduite = true;
    f.el.classList.add("reduite");
    if (active === f) { active = null; const autre = [...fenetres].reverse().find((g) => !g.reduite); if (autre) focaliser(autre); }
    majBarre();
  }
  function montrer(f) { f.reduite = false; f.el.classList.remove("reduite"); focaliser(f); }
  function fermer(f) {
    if (f.quitter) f.quitter();
    f.el.remove();
    fenetres.splice(fenetres.indexOf(f), 1);
    fermerAssistant();
    if (active === f) { active = null; const autre = [...fenetres].reverse().find((g) => !g.reduite); if (autre) focaliser(autre); }
    majBarre();
  }
  function focaliser(f) {
    if (active !== f) {
      if (active) active.el.classList.remove("active");
      active = f;
      f.el.style.zIndex = ++pile;
      // Les fenêtres gardent l'ordre de la pile pour la recherche de la suivante.
      fenetres.splice(fenetres.indexOf(f), 1); fenetres.push(f);
    }
    f.el.classList.add("active");
    majBarre();
  }

  // Ancrage : mêmes zones et mêmes dispositions que scripts/winlux-snap.py.
  const ZONES = {
    "half:left": [0, 0, 1 / 2, 1], "half:right": [1 / 2, 0, 1 / 2, 1],
    "wide:left": [0, 0, 2 / 3, 1], "wide:right": [2 / 3, 0, 1 / 3, 1],
    "narrow:left": [0, 0, 1 / 3, 1], "narrow:right": [1 / 3, 0, 2 / 3, 1],
    "third:1": [0, 0, 1 / 3, 1], "third:2": [1 / 3, 0, 1 / 3, 1], "third:3": [2 / 3, 0, 1 / 3, 1],
    "quarter:tl": [0, 0, 1 / 2, 1 / 2], "quarter:tr": [1 / 2, 0, 1 / 2, 1 / 2],
    "quarter:bl": [0, 1 / 2, 1 / 2, 1 / 2], "quarter:br": [1 / 2, 1 / 2, 1 / 2, 1 / 2],
  };
  const LAYOUTS = [["half:left", "half:right"], ["wide:left", "wide:right"], ["third:1", "third:2", "third:3"],
    ["narrow:left", "narrow:right"], ["quarter:tl", "quarter:tr", "quarter:bl", "quarter:br"]];
  const OPPOSE = { "half:left": "half:right", "half:right": "half:left", "wide:left": "wide:right", "wide:right": "wide:left", "narrow:left": "narrow:right", "narrow:right": "narrow:left" };
  const QUARTS = ["quarter:tl", "quarter:tr", "quarter:bl", "quarter:br"];
  function rectZone(zone) {
    const A = aire();
    if (zone === "max") return { x: 0, y: 0, w: A.w, h: A.h };
    const [fx, fy, fw, fh] = ZONES[zone];
    const x = Math.round(fx * A.w), y = Math.round(fy * A.h);
    return { x, y, w: Math.round((fx + fw) * A.w) - x, h: Math.round((fy + fh) * A.h) - y };
  }
  function suivantes(zone) {
    if (OPPOSE[zone]) return [OPPOSE[zone]];
    if (QUARTS.includes(zone)) return QUARTS.filter((q) => q !== zone);
    const l = LAYOUTS.find((d) => d.includes(zone)) || [];
    return l.filter((c) => c !== zone);
  }
  function recouvre(a, b) {
    const w = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x), hh = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
    return w > 0 && hh > 0 ? w * hh : 0;
  }
  /** Case encore libre : pas couverte aux quatre cinquièmes par une fenêtre ancrée. */
  const libre = (zone) => {
    const r = rectZone(zone);
    return !fenetres.some((g) => !g.reduite && g.etat === "snap" && recouvre(r, g) >= 0.8 * r.w * r.h);
  };
  function ancrer(f, zone, assistant = true) {
    fermerDispositions();
    if (zone === "max") { agrandir(f); return; }
    retenir(f);
    Object.assign(f, { etat: "snap", zone, reduite: false }, rectZone(zone));
    f.el.classList.remove("reduite");
    appliquer(f);
    focaliser(f);
    if (assistant && reglages.assistant) lancerAssistant(suivantes(zone), zone);
  }

  // Touche Super (ou Ctrl + Alt) + flèches.
  function fleche(f, dir) {
    const z = f.etat === "snap" ? f.zone : null;
    const cote = z && z.endsWith("left") ? "left" : z && z.endsWith("right") ? "right" : null;
    if (dir === "left" || dir === "right") {
      if (z && z.startsWith("half:") && cote !== dir) restaurer(f);
      else ancrer(f, "half:" + dir);
    } else if (dir === "up") {
      if (z && z.startsWith("half:")) ancrer(f, cote === "left" ? "quarter:tl" : "quarter:tr");
      else if (z === "quarter:bl" || z === "quarter:br") ancrer(f, z === "quarter:bl" ? "half:left" : "half:right");
      else agrandir(f);
    } else {
      if (z && z.startsWith("half:")) ancrer(f, cote === "left" ? "quarter:bl" : "quarter:br");
      else if (z === "quarter:tl" || z === "quarter:tr") ancrer(f, z === "quarter:tl" ? "half:left" : "half:right");
      else if (f.etat === "max") restaurer(f);
      else reduire(f);
    }
  }

  // Aperçu pendant le glisser.
  const apercu = h("div", { class: "apercu", hidden: true });
  ecran.append(apercu);
  function montrerApercu(zone) {
    if (!zone) { apercu.hidden = true; return; }
    const r = rectZone(zone);
    if (apercu.hidden) { apercu.style.transition = "none"; requestAnimationFrame(() => { apercu.style.transition = ""; }); }
    apercu.hidden = false;
    Object.assign(apercu.style, { left: r.x + "px", top: r.y + "px", width: r.w - 12 + "px", height: r.h - 12 + "px", zIndex: pile + 1 });
  }

  // Grille des dispositions (survol du bouton Agrandir, Super + Z, haut de l'écran).
  let dispositions = null;
  function creerDispositions(choisir, classe) {
    fermerDispositions();
    const el = h("div", { class: "dispositions " + (classe || ""), role: "menu" });
    for (const d of LAYOUTS) {
      const bloc = h("div", { class: "disposition" });
      for (const zone of d) {
        const [fx, fy, fw, fh] = ZONES[zone];
        const c = h("button", { type: "button", class: "case", "data-zone": zone, "aria-label": zone, role: "menuitem" });
        Object.assign(c.style, { left: `calc(${fx * 100}% + 3px)`, top: `calc(${fy * 100}% + 3px)`, width: `calc(${fw * 100}% - 6px)`, height: `calc(${fh * 100}% - 6px)` });
        c.addEventListener("click", () => choisir(zone));
        bloc.append(c);
      }
      el.append(bloc);
    }
    el.style.zIndex = pile + 5;
    ecran.append(el);
    dispositions = el;
    return el;
  }
  function fermerDispositions() { if (dispositions) { dispositions.remove(); dispositions = null; } }
  function dispositionsSous(f) {
    const el = creerDispositions((zone) => ancrer(f, zone));
    const b = f.bMax.getBoundingClientRect(), R = ecran.getBoundingClientRect();
    const larg = el.offsetWidth;
    el.style.left = Math.max(4, Math.min(R.width - larg - 4, b.left - R.left + b.width / 2 - larg / 2)) + "px";
    el.style.top = Math.min(b.bottom - R.top + 2, R.height - HAUTEUR_BARRE - el.offsetHeight - 4) + "px";
    let minuterie = null;
    el.addEventListener("pointerleave", () => { minuterie = setTimeout(() => { if (dispositions === el) fermerDispositions(); }, 450); });
    el.addEventListener("pointerenter", () => clearTimeout(minuterie));
    return el;
  }
  function survolAgrandir(f, b) {
    let minuterie = null;
    b.addEventListener("pointerenter", (e) => {
      if (e.pointerType !== "mouse" || !reglages.survol || petit()) return;
      minuterie = setTimeout(() => { const el = dispositionsSous(f); b.addEventListener("pointerleave", () => setTimeout(() => { if (dispositions === el && !el.matches(":hover")) fermerDispositions(); }, 450), { once: true }); }, 550);
    });
    b.addEventListener("pointerleave", () => clearTimeout(minuterie));
    b.addEventListener("contextmenu", (e) => { e.preventDefault(); if (!petit()) dispositionsSous(f); });
  }

  // Assistant d'ancrage : la place libre propose les autres fenêtres.
  let assistant = null;
  function fermerAssistant() { if (assistant) { assistant.remove(); assistant = null; } }
  function lancerAssistant(zones, origine) {
    fermerAssistant();
    const libres = zones.filter(libre);
    const autres = [...fenetres].reverse().filter((g) => g !== active && g.etat !== "snap");
    if (!libres.length || !autres.length) return;
    const zone = libres[0];
    const r = rectZone(zone);
    const el = h("div", { class: "assistant", role: "dialog" },
      h("p", null, OPPOSE[origine] === zone && libres.length === 1 ? t("assist_autre") : t("assist_place")));
    for (const g of autres) {
      const carte = h("button", { type: "button" }, h("span", { class: "mini" }, APPS[g.app].icone()), h("span", null, g.nom.textContent));
      carte.addEventListener("click", () => { ancrer(g, zone, false); lancerAssistant(libres.slice(1), origine); });
      el.append(carte);
    }
    Object.assign(el.style, { left: r.x + "px", top: r.y + "px", width: r.w - 12 + "px", height: r.h - 12 + "px", zIndex: pile + 2 });
    ecran.append(el);
    assistant = el;
    const premier = el.querySelector("button");
    if (premier) premier.focus({ preventScroll: true });
  }
  ecran.addEventListener("pointerdown", (e) => {
    if (assistant && !assistant.contains(e.target)) fermerAssistant();
    if (dispositions && !dispositions.contains(e.target) && !e.target.closest(".agrandir")) fermerDispositions();
  }, true);

  function glisser(f, bar) {
    bar.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || e.target.closest("button") || petit()) return;
      const R = ecran.getBoundingClientRect();
      let sx = e.clientX, sy = e.clientY, ox = f.x, oy = f.y, bouge = false, zone = null, enHaut = null;
      bar.setPointerCapture(e.pointerId);
      const deplacer = (ev) => {
        const dx = ev.clientX - sx, dy = ev.clientY - sy;
        if (!bouge && Math.hypot(dx, dy) < 5) return;
        if (!bouge) { bouge = true; fermerAssistant(); fermerDispositions(); }
        const px = ev.clientX - R.left, py = ev.clientY - R.top;
        if (f.etat !== "normal") {
          // Décrochée : la fenêtre reprend sa taille d'avant sous le pointeur.
          const ratio = (sx - R.left - f.x) / f.w;
          const r = f.avant || { w: 640, h: 440 };
          Object.assign(f, { etat: "normal", zone: null, w: r.w, h: r.h });
          f.x = Math.round(px - ratio * r.w); f.y = Math.max(0, py - 18);
          ox = f.x; oy = f.y; sx = ev.clientX; sy = ev.clientY;
          appliquer(f, false);
          return;
        }
        const A = aire();
        f.x = ox + (ev.clientX - sx); f.y = Math.max(0, Math.min(A.h - 36, oy + (ev.clientY - sy)));
        appliquer(f, false);
        // Près du haut, au centre : la barre des dispositions descend.
        if (py < 8 && px > A.w * 0.3 && px < A.w * 0.7 && !enHaut) enHaut = creerDispositions(() => {}, "en-haut");
        if (enHaut && py > 110) { fermerDispositions(); enHaut = null; }
        const sous = enHaut ? document.elementsFromPoint(ev.clientX, ev.clientY).find((n) => n.classList && n.classList.contains("case")) : null;
        if (enHaut) enHaut.querySelectorAll(".vise").forEach((c) => { if (c !== sous) c.classList.remove("vise"); });
        const bord = 10;
        if (sous) { sous.classList.add("vise"); zone = sous.dataset.zone; }
        else if (py <= 6 && !enHaut) zone = "max";
        else if (px <= bord) zone = py < A.h * 0.15 ? "quarter:tl" : py > A.h * 0.85 ? "quarter:bl" : "half:left";
        else if (px >= A.w - bord) zone = py < A.h * 0.15 ? "quarter:tr" : py > A.h * 0.85 ? "quarter:br" : "half:right";
        else zone = null;
        montrerApercu(zone);
      };
      const lacher = () => {
        bar.removeEventListener("pointermove", deplacer);
        bar.removeEventListener("pointerup", lacher);
        bar.removeEventListener("pointercancel", lacher);
        montrerApercu(null);
        if (enHaut) { fermerDispositions(); enHaut = null; }
        if (bouge && zone) {
          // Taille d'avant : la place du début du glisser, pas celle du lâcher.
          f.x = ox; f.y = oy;
          if (zone === "max") { f.etat = "normal"; agrandir(f); } else ancrer(f, zone);
        }
      };
      bar.addEventListener("pointermove", deplacer);
      bar.addEventListener("pointerup", lacher);
      bar.addEventListener("pointercancel", lacher);
    });
  }
  function redimensionner(f, p) {
    p.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      e.stopPropagation();
      p.setPointerCapture(e.pointerId);
      const sx = e.clientX, sy = e.clientY, w0 = f.w, h0 = f.h;
      const bouger = (ev) => { f.w = Math.max(280, w0 + ev.clientX - sx); f.h = Math.max(180, h0 + ev.clientY - sy); appliquer(f, false); };
      const fin = () => { p.removeEventListener("pointermove", bouger); p.removeEventListener("pointerup", fin); };
      p.addEventListener("pointermove", bouger);
      p.addEventListener("pointerup", fin);
    });
  }
  window.addEventListener("resize", () => {
    for (const f of fenetres) {
      if (f.etat === "max") agrandir(f, false);
      else if (f.etat === "snap") { Object.assign(f, rectZone(f.zone)); appliquer(f, false); }
    }
    fermerAssistant(); fermerDispositions();
  });

  // Boutons de la barre : applications épinglées, puis celles ouvertes.
  function majBarre() {
    const ids = Object.keys(APPS).filter((id) => APPS[id].epinglee);
    for (const f of fenetres) if (!ids.includes(f.app)) ids.push(f.app);
    const boutons = [boutonDemarrer];
    for (const id of ids) {
      const siennes = fenetres.filter((f) => f.app === id);
      const b = h("button", { type: "button", class: "bouton-barre", title: APPS[id].nom(), "aria-label": APPS[id].nom() }, APPS[id].icone());
      if (siennes.length) b.classList.add("lance");
      if (active && active.app === id && !active.reduite) b.classList.add("actif");
      b.addEventListener("click", () => {
        if (!siennes.length) { ouvrir(id); return; }
        const derniere = siennes[siennes.length - 1];
        if (derniere === active && !derniere.reduite) reduire(derniere); else montrer(derniere);
      });
      boutons.push(b);
    }
    centre.replaceChildren(...boutons);
  }

  // ── Menu Démarrer ──────────────────────────────────────────────────────
  const ENTREES = [
    { id: "explorateur", nom: () => t("explorateur"), icone: () => ico("dossier"), lancer: () => ouvrir("explorateur", "accueil") },
    { id: "parametres", nom: () => t("parametres"), icone: icoParametres, lancer: () => ouvrir("parametres") },
    { id: "blocnotes", nom: () => t("blocnotes"), icone: () => ico("type-texte"), lancer: () => ouvrir("blocnotes") },
    { id: "terminal", nom: () => t("terminal"), icone: () => ico("terminal"), lancer: () => ouvrir("terminal") },
    { id: "documents", nom: () => t("documents"), icone: () => ico("documents"), lancer: () => ouvrir("explorateur", "documents") },
    { id: "cepc", nom: () => t("ce_pc"), icone: () => ico("ce-pc"), lancer: () => ouvrir("explorateur", "cepc") },
    { id: "images", nom: () => t("images"), icone: () => ico("images"), lancer: () => ouvrir("explorateur", "images") },
    { id: "apropos", nom: () => t("apropos"), icone: () => logo(32), lancer: () => ouvrir("apropos") },
    { id: "telechargements", nom: () => t("telechargements"), icone: () => ico("telechargements"), lancer: () => ouvrir("explorateur", "telechargements"), cachee: true },
  ];
  const champ = h("input", { type: "search", placeholder: t("recherche"), "aria-label": t("recherche"), autocomplete: "off", spellcheck: "false" });
  const titreListe = h("h2");
  const bascule = h("button", { type: "button" });
  const liste = h("div", { class: "liste" });
  const volet = h("aside", { class: "pour-vous" });
  const boutonAlim = h("button", { type: "button", class: "alim", title: t("alim"), "aria-label": t("alim"), html: S.alim });
  const menu = h("div", { class: "menu-demarrer", role: "dialog", "aria-label": t("demarrer"), "aria-hidden": "true", inert: true },
    h("div", { class: "principal" },
      h("div", { class: "recherche" }, h("span", { html: S.loupe }), champ),
      h("div", { class: "entete" }, titreListe, bascule),
      liste,
      h("div", { class: "pied" },
        h("button", { type: "button", class: "compte" }, h("span", { class: "avatar" }, t("visiteur").slice(0, 1)), h("span", null, t("visiteur"))),
        boutonAlim)),
    volet);
  ecran.append(menu);
  let toutes = false;
  let menuAlim = null;

  function rendreListe() {
    const q = plier(champ.value.trim());
    liste.replaceChildren();
    if (q) {
      titreListe.textContent = t("resultats");
      bascule.hidden = true;
      const apps = ENTREES.filter((e) => plier(e.nom()).includes(q));
      const fichiers = [];
      for (const lieu of DOSSIERS) for (const f of FICHIERS[lieu]) if (plier(f.n).includes(q)) fichiers.push([f, lieu]);
      if (!apps.length && !fichiers.length) liste.append(h("p", { class: "vide" }, t("aucune")));
      apps.forEach((e, i) => liste.append(ligne(e.icone(), e.nom(), null, e.lancer, i === 0)));
      for (const [f, lieu] of fichiers) liste.append(ligne(ico(icoFichier(f)), f.n, t("fichier_de") + t(lieu), () => { fermerMenu(); ouvrirFichier(f, lieu); }, !apps.length && fichiers[0][0] === f));
      return;
    }
    bascule.hidden = false;
    titreListe.textContent = toutes ? t("toutes").replace(/\s*›$/, "") : t("epingle");
    bascule.textContent = toutes ? t("retour") : t("toutes");
    if (!toutes) {
      const g = h("div", { class: "grille" });
      for (const e of ENTREES.filter((x) => !x.cachee)) g.append(tuile(e));
      liste.append(g);
      return;
    }
    const triees = [...ENTREES].sort((a, b) => a.nom().localeCompare(b.nom(), lang));
    let lettre = "";
    for (const e of triees) {
      const l = plier(e.nom()).charAt(0).toUpperCase();
      if (l !== lettre) { lettre = l; liste.append(h("div", { class: "lettre" }, l)); }
      liste.append(ligne(e.icone(), e.nom(), null, e.lancer));
    }
  }
  function tuile(e) {
    return h("button", { type: "button", class: "appli", title: e.nom(), onclick: e.lancer }, h("span", { class: "ico" }, e.icone()), h("span", null, e.nom()));
  }
  function ligne(icone, nom, detail, action, premier) {
    return h("button", { type: "button", class: "ligne" + (premier ? " premier" : ""), onclick: action },
      h("span", { class: "ico" }, icone), h("span", { class: "txt" }, h("b", null, nom), detail ? h("span", { class: "dim" }, detail) : null));
  }
  function rendreVolet() {
    volet.hidden = !reglages.pourVous;
    if (!reglages.pourVous) return;
    const maintenant = new Date();
    const heure = maintenant.getHours();
    const jours = maintenant.getDay() % 6 === 0 ? t("weekend") : t("semaine");
    const moment = heure < 12 ? t("matin") : heure < 18 ? t("midi") : t("soir");
    const corps = h("div", { class: "corps" },
      h("div", { class: "section" }, t("reprendre")),
      ligne(ico("type-texte"), FICHIERS.documents[0].n, t("blocnotes") + " · " + t("il_y_a_min", { n: 5 }), () => { fermerMenu(); ouvrirFichier(FICHIERS.documents[0]); }),
      ligne(ico("type-pdf"), FICHIERS.documents[1].n, t("explorateur") + " · " + t("hier"), () => { fermerMenu(); ouvrirFichier(FICHIERS.documents[1]); }),
      ligne(ico("type-image"), FICHIERS.images[0].n, t("il_y_a_j", { n: 3 }), () => { fermerMenu(); ouvrirFichier(FICHIERS.images[0]); }),
      h("div", { class: "section" }, t("souvent")),
      h("div", { class: "tuiles" }, ...["explorateur", "terminal", "blocnotes"].map((id) => {
        const b = tuile(ENTREES.find((e) => e.id === id)); b.classList.add("carte"); return b;
      })),
      h("p", { class: "dim pourquoi" }, t("habitudes", { jours, moment })));
    volet.replaceChildren(
      h("div", { class: "haut" }, h("span", { class: "etincelle" }, "✦"), h("h2", null, t("pour_vous")),
        h("button", { type: "button", onclick: () => { reglages.pourVous = false; stock.ecrire("pour-vous", "0"); rendreVolet(); } }, t("masquer"))),
      corps,
      h("div", { class: "vie-privee" }, h("span", { html: S.cadenas }), h("span", { class: "dim" }, t("vie_privee"))));
  }
  function ouvrirMenu() {
    fermerAssistant(); fermerDispositions();
    toutes = false; champ.value = "";
    rendreListe(); rendreVolet();
    menu.style.zIndex = pile + 10;
    menu.classList.add("ouvert");
    menu.removeAttribute("inert");
    menu.setAttribute("aria-hidden", "false");
    boutonDemarrer.classList.add("ouvert-menu");
    boutonDemarrer.setAttribute("aria-expanded", "true");
    setTimeout(() => champ.focus({ preventScroll: true }), 30);
  }
  function fermerMenu() {
    if (!menu.classList.contains("ouvert")) return;
    menu.classList.remove("ouvert");
    menu.setAttribute("inert", "");
    menu.setAttribute("aria-hidden", "true");
    boutonDemarrer.classList.remove("ouvert-menu");
    boutonDemarrer.setAttribute("aria-expanded", "false");
    fermerAlim();
  }
  const basculerMenu = () => (menu.classList.contains("ouvert") ? fermerMenu() : ouvrirMenu());
  boutonDemarrer.addEventListener("click", basculerMenu);
  bascule.addEventListener("click", () => { toutes = !toutes; rendreListe(); });
  champ.addEventListener("input", rendreListe);
  champ.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { const p = liste.querySelector(".premier") || liste.querySelector("button"); if (p) p.click(); }
  });
  ecran.addEventListener("pointerdown", (e) => {
    if (menu.classList.contains("ouvert") && !menu.contains(e.target) && !boutonDemarrer.contains(e.target) && !(menuAlim && menuAlim.contains(e.target))) fermerMenu();
  });

  function fermerAlim() { if (menuAlim) { menuAlim.remove(); menuAlim = null; } }
  boutonAlim.addEventListener("click", () => {
    if (menuAlim) { fermerAlim(); return; }
    const actions = [["verrouiller", verrouiller], ["veille", veille], ["redemarrer", redemarrer], ["arreter", arreter]];
    menuAlim = h("div", { class: "menu-alim", role: "menu" }, ...actions.map(([k, fn]) =>
      h("button", { type: "button", role: "menuitem", onclick: () => { fermerMenu(); fn(); } }, t(k))));
    menuAlim.style.zIndex = pile + 20;
    ecran.append(menuAlim);
    const b = boutonAlim.getBoundingClientRect(), R = ecran.getBoundingClientRect();
    menuAlim.style.left = Math.min(R.width - menuAlim.offsetWidth - 6, b.right - R.left - menuAlim.offsetWidth / 2) + "px";
    menuAlim.style.top = b.top - R.top - menuAlim.offsetHeight - 6 + "px";
    menuAlim.querySelector("button").focus();
  });

  // ── Alimentation ───────────────────────────────────────────────────────
  function toutFermer() { for (const f of [...fenetres]) fermer(f); }
  function verrouiller() {
    const d = new Date();
    const el = h("div", { class: "verrou", tabindex: "0", role: "button", "aria-label": t("deverrouiller") },
      h("div", { class: "heure" }, d.toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" })),
      h("div", { class: "date" }, d.toLocaleDateString(lang, { weekday: "long", day: "numeric", month: "long" })),
      h("div", { class: "indice" }, t("deverrouiller")));
    const ouvrirSession = () => { el.remove(); document.removeEventListener("keydown", ouvrirSession); };
    el.addEventListener("click", ouvrirSession);
    document.addEventListener("keydown", ouvrirSession);
    ecran.append(el);
    el.focus();
  }
  function veille() {
    const el = h("div", { class: "eteint" });
    el.style.cursor = "pointer";
    el.addEventListener("click", () => { el.remove(); verrouiller(); });
    ecran.append(el);
  }
  function redemarrer() { toutFermer(); lancerDemarrage(); }
  function arreter() {
    toutFermer();
    const el = h("div", { class: "eteint" }, h("button", { type: "button", onclick: () => { el.remove(); lancerDemarrage(); } }, t("rallumer")));
    ecran.append(el);
    el.querySelector("button").focus();
  }

  // ── Clavier ────────────────────────────────────────────────────────────
  let superSeul = false;
  document.addEventListener("keydown", (e) => {
    if (demarrage) return;
    const mod = e.metaKey || (e.ctrlKey && e.altKey);
    if (e.key === "Meta" || e.key === "OS") { superSeul = true; return; }
    superSeul = false;
    if (e.key === "Escape") {
      if (menuAlim) fermerAlim(); else if (menu.classList.contains("ouvert")) fermerMenu();
      fermerAssistant(); fermerDispositions();
      return;
    }
    if (!mod || !active || active.reduite || petit()) return;
    const dirs = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };
    if (dirs[e.key]) { e.preventDefault(); fleche(active, dirs[e.key]); }
    else if (e.key.toLowerCase() === "z") { e.preventDefault(); dispositionsSous(active); }
  });
  document.addEventListener("keyup", (e) => {
    if ((e.key === "Meta" || e.key === "OS") && superSeul && !demarrage) basculerMenu();
    superSeul = false;
  });

  // ── Explorateur ────────────────────────────────────────────────────────
  function creerExplorateur(f, lieu) {
    let historique = [], suite = [];
    let ici = lieu || "accueil";
    const bPrec = h("button", { type: "button", title: t("precedent"), "aria-label": t("precedent"), html: S.gauche });
    const bSuiv = h("button", { type: "button", title: t("suivant"), "aria-label": t("suivant"), html: S.droite });
    const bHaut = h("button", { type: "button", title: t("parent"), "aria-label": t("parent"), html: S.haut });
    const adresse = h("div", { class: "adresse" });
    const vue = h("div", { class: "vue" });
    const etat = h("div", { class: "etat" });
    const nav = h("nav", { class: "volet" });
    const NOMS = { accueil: t("accueil"), cepc: t("ce_pc"), perso: t("perso"), linux: t("linux"), corbeille: t("corbeille") };
    const nom = (l) => NOMS[l] || t(l);
    const icone = (l) => ({ accueil: "accueil", cepc: "ce-pc", perso: "disque-c", linux: "disque-z", corbeille: "corbeille-vide" })[l] || l;
    const parent = (l) => (DOSSIERS.includes(l) ? "perso" : l === "perso" || l === "linux" ? "cepc" : null);
    const entreesVolet = ["accueil", null, ...DOSSIERS, null, "cepc", "perso", "linux"];
    function aller(l, memoriser = true) {
      if (memoriser && l !== ici) { historique.push(ici); suite = []; }
      ici = l;
      rendre();
    }
    f.naviguer = (l) => aller(l);
    bPrec.addEventListener("click", () => { if (historique.length) { suite.push(ici); ici = historique.pop(); rendre(); } });
    bSuiv.addEventListener("click", () => { if (suite.length) { historique.push(ici); ici = suite.pop(); rendre(); } });
    bHaut.addEventListener("click", () => { const p = parent(ici); if (p) aller(p); });
    function element(icn, libelle, ouvrirFn) {
      const b = h("button", { type: "button", class: "element" }, ico(icn), h("span", null, libelle));
      activerOuverture(b, ouvrirFn, () => { vue.querySelectorAll(".choisi").forEach((x) => x.classList.remove("choisi")); b.classList.add("choisi"); });
      return b;
    }
    function lecteur(l, utilise, total) {
      const b = h("button", { type: "button", class: "lecteur" }, ico(icone(l)),
        h("span", { class: "txt" }, h("b", null, nom(l)), h("div", { class: "jauge" }, h("i", { style: `width:${Math.round(utilise / total * 100)}%` })),
          h("span", { class: "dim" }, t("libre", { n: total - utilise, t: total }))));
      activerOuverture(b, () => aller(l));
      return b;
    }
    function rendre() {
      f.titre(nom(ici) + " – " + t("explorateur"));
      bPrec.disabled = !historique.length; bSuiv.disabled = !suite.length; bHaut.disabled = !parent(ici);
      const chemin = [];
      for (let l = ici; l; l = parent(l)) chemin.unshift(nom(l));
      adresse.replaceChildren(ico(icone(ici), true), h("span", null, chemin.join("  ›  ")));
      nav.replaceChildren(...entreesVolet.map((l) => l === null ? h("hr") :
        h("button", { type: "button", class: l === ici ? "ici" : "", onclick: () => aller(l) },
          ico(icone(l), true), nom(l))));
      vue.replaceChildren();
      let n = 0;
      if (ici === "accueil") {
        const m = h("div", { class: "mosaique" }, ...DOSSIERS.map((d) => element(d, t(d), () => aller(d))));
        const recents = h("div", null, ...[[FICHIERS.documents[0], "documents"], [FICHIERS.documents[1], "documents"], [FICHIERS.images[0], "images"]].map(([fi, l]) =>
          ligne(ico(icoFichier(fi)), fi.n, t("fichier_de") + t(l), () => ouvrirFichier(fi, l))));
        vue.append(h("h3", null, t("acces_rapide")), m, h("h3", null, t("recents")), recents);
        n = DOSSIERS.length;
      } else if (ici === "cepc") {
        vue.append(h("h3", null, t("lecteurs")), h("div", { class: "lecteurs" }, lecteur("perso", 128, 476), lecteur("linux", 22, 64)),
          h("h3", null, t("dossiers")), h("div", { class: "mosaique" }, ...DOSSIERS.map((d) => element(d, t(d), () => aller(d)))));
        n = 2 + DOSSIERS.length;
      } else if (ici === "perso") {
        vue.append(h("div", { class: "mosaique" }, ...DOSSIERS.map((d) => element(d, t(d), () => aller(d)))));
        n = DOSSIERS.length;
      } else {
        const contenu = FICHIERS[ici] || [];
        if (!contenu.length) vue.append(h("p", { class: "vide" }, t("vide")));
        else vue.append(h("div", { class: "mosaique" }, ...contenu.map((fi) => element(icoFichier(fi), fi.n, () => ouvrirFichier(fi, ici)))));
        n = contenu.length;
      }
      etat.textContent = t("elements", { n });
    }
    f.contenu.append(h("div", { class: "barre-outils" }, bPrec, bSuiv, bHaut, adresse),
      h("div", { class: "explo" }, nav, vue), etat);
    rendre();
  }

  // ── Bloc-notes ─────────────────────────────────────────────────────────
  function creerBlocNotes(f, fichier) {
    const nomFichier = fichier ? fichier.n : t("notes_txt");
    const cle = "texte-" + nomFichier;
    const zone = h("textarea", { class: "texte", spellcheck: "false", "aria-label": nomFichier });
    zone.value = stock.lire(cle, fichier && fichier.texte !== undefined ? fichier.texte : "");
    const etat = h("div", { class: "etat" });
    const maj = (msg) => {
      const avant = zone.value.slice(0, zone.selectionStart).split("\n");
      etat.textContent = msg || t("ligne_col", { l: avant.length, c: avant[avant.length - 1].length + 1 }) + "   ·   " + t("caracteres", { n: zone.value.length });
    };
    ["input", "click", "keyup"].forEach((ev) => zone.addEventListener(ev, () => maj()));
    f.titre(nomFichier + " – " + t("blocnotes"));
    f.contenu.append(h("div", { class: "menus" },
      h("button", { type: "button", onclick: () => { zone.value = ""; maj(); zone.focus(); } }, t("nouveau")),
      h("button", { type: "button", onclick: () => { stock.ecrire(cle, zone.value); maj(t("enregistre")); } }, t("enregistrer"))),
      zone, etat);
    maj();
    f.focusInitial = () => zone.focus({ preventScroll: true });
  }

  // ── Terminal ───────────────────────────────────────────────────────────
  function creerTerminal(f) {
    const sortie = h("div", { class: "terminal" });
    const invite = () => h("span", { class: "invite" }, "visiteur@winlux:~$ ");
    const entree = h("input", { "aria-label": t("terminal"), autocomplete: "off", spellcheck: "false", autocapitalize: "off" });
    const ligneSaisie = h("div", null, invite(), entree);
    const ecrire = (texte, classe) => sortie.insertBefore(h("div", { class: classe || null }, texte), ligneSaisie);
    sortie.append(ligneSaisie);
    ecrire(t("term_bienvenue"), "ok");
    sortie.addEventListener("click", () => { if (!getSelection().toString()) entree.focus(); });
    const lieux = () => DOSSIERS.map((d) => t(d)).join("  ");
    entree.addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      const cmd = entree.value.trim();
      sortie.insertBefore(h("div", null, invite(), cmd), ligneSaisie);
      entree.value = "";
      const [c, ...args] = cmd.split(/\s+/);
      if (!c) { /* ligne vide */ }
      else if (["aide", "help", "hilfe", "ayuda"].includes(c)) ecrire(t("term_aide"));
      else if (c === "ls") ecrire(lieux());
      else if (c === "pwd") ecrire("/home/visiteur");
      else if (c === "whoami") ecrire("visiteur");
      else if (c === "uname") ecrire(args.includes("-a") ? "Linux winlux x86_64 GNU/Linux" : "Linux");
      else if (c === "date") ecrire(new Date().toLocaleString(lang));
      else if (c === "echo") ecrire(args.join(" "));
      else if (c === "clear") { while (sortie.firstChild !== ligneSaisie) sortie.firstChild.remove(); }
      else if (c === "exit") { fermer(f); return; }
      else ecrire(t("term_inconnu", { c }));
      sortie.scrollTop = sortie.scrollHeight;
    });
    f.contenu.append(sortie);
    f.focusInitial = () => entree.focus({ preventScroll: true });
  }

  // ── Paramètres ─────────────────────────────────────────────────────────
  function creerParametres(f) {
    let page = "perso";
    const nav = h("nav");
    const corps = h("div", { class: "page" });
    const choix = (valeurs, actuelle, changer) => h("div", { class: "choix" }, ...valeurs.map(([v, libelle]) =>
      h("button", { type: "button", "aria-pressed": String(v === actuelle), onclick: () => { changer(v); rendre(); } }, libelle)));
    function rendre() {
      nav.replaceChildren(...[["perso", t("perso_titre")], ["ancrage", t("ancrage")]].map(([p, libelle]) =>
        h("button", { type: "button", class: p === page ? "ici" : "", onclick: () => { page = p; rendre(); } }, libelle)));
      if (page === "perso") {
        const fonds = h("div", { class: "fonds" }, ...[["aurore", reglages.theme === "clair" ? "aurore-clair.jpg" : "aurore.jpg"], ["nuit", "../img/fond-nuit.jpg"], ["aube", "../img/fond-aube.jpg"]].map(([v, img]) =>
          h("button", { type: "button", title: t(v), "aria-label": t(v), "aria-pressed": String(reglages.fond === v), style: `background-image:url("${img}")`,
            onclick: () => { reglages.fond = v; stock.ecrire("fond", v); appliquerTheme(); rendre(); } })));
        corps.replaceChildren(h("h3", null, t("perso_titre")),
          h("div", { class: "reglage" }, h("span", null, t("mode")), choix([["sombre", t("sombre")], ["clair", t("clair")]], reglages.theme,
            (v) => { reglages.theme = v; stock.ecrire("theme", v); appliquerTheme(); })),
          h("div", { class: "reglage" }, h("span", null, t("fond")), fonds));
      } else {
        corps.replaceChildren(h("h3", null, t("ancrage")),
          h("div", { class: "reglage" }, h("span", null, h("b", null, t("assistant")), h("br"), h("span", { class: "dim" }, t("assistant_txt"))),
            choix([[true, t("oui")], [false, t("non")]], reglages.assistant, (v) => { reglages.assistant = v; stock.ecrire("assistant", v ? "1" : "0"); })),
          h("div", { class: "reglage" }, h("span", null, t("survol")),
            choix([[true, t("oui")], [false, t("non")]], reglages.survol, (v) => { reglages.survol = v; stock.ecrire("survol", v ? "1" : "0"); })));
      }
    }
    f.contenu.append(h("div", { class: "parametres" }, nav, corps));
    rendre();
  }

  // ── À propos, programme .exe, aperçu ───────────────────────────────────
  function creerAPropos(f) {
    f.contenu.append(h("div", { class: "apropos" }, logo(56), h("h3", null, t("apropos_titre")),
      h("p", null, t("apropos_1")), h("p", null, h("b", null, t("apropos_2"))),
      h("ul", null, ...["astuce_1", "astuce_2", "astuce_3", "astuce_4"].map((k) => h("li", null, t(k)))),
      h("p", null, t("apropos_3")),
      h("p", null, h("a", { href: "../../" + lang + "/download.html" }, t("telecharger") + " ›"))));
  }
  function creerBonjour(f) {
    f.titre(t("bonjour") + " – bonjour.exe");
    f.contenu.append(h("div", { class: "programme" }, ico("type-exe"), h("div", { class: "grand" }, t("exe_titre")),
      h("div", { class: "dim" }, t("exe_txt")), h("button", { type: "button", onclick: () => fermer(f) }, t("ok"))));
    f.focusInitial = () => f.contenu.querySelector("button").focus({ preventScroll: true });
  }
  function creerApercu(f, fichier) {
    f.titre(fichier.n);
    f.contenu.append(h("div", { class: "programme" }, ico(icoFichier(fichier)), h("div", { class: "grand" }, fichier.n),
      h("div", { class: "dim" }, t("apercu_txt")), h("button", { type: "button", onclick: () => fermer(f) }, t("ok"))));
  }

  // ── Démarrage de la démo ───────────────────────────────────────────────
  majBarre();
  lancerDemarrage(() => {
    if (!petit()) { ouvrir("apropos"); }
  });
})();
