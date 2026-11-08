#!/usr/bin/python3
"""Paramètres > Mises à jour > Mise à jour OS : la page refaite (#875 à #882).

Une carte d'état avec l'icône Prisme et un seul bouton qui suit la situation
(rechercher, télécharger et installer, redémarrer, réessayer, reprendre), la
progression en étapes, les nouveautés de la version, puis « Plus d'options » :
suspendre, installation automatique, heures d'activité, historique, revenir à
la version précédente, logiciels et pilotes Linux.

Le moteur (winlux-update-apt, en root) publie son état en clé=valeur dans
/run/winlux/update-state.txt (winlux-update --state, recopié par
winlux-control), l'historique et les notes dans /var/lib/winlux. Toutes les
actions passent par winlux-control et le relais root, comme avant.

Chargé par winlux-settings (page 7). « winlux-update-page notify » envoie la
notification du bureau quand une version arrive ou attend un redémarrage
(appelé par la boucle de session, seulement quand l'avis change).
"""
import html
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time


def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in Path("/etc/winlux/locale.conf").read_text(encoding="utf-8").splitlines():
                if line.startswith("LANG="):
                    code = line[5:].strip()
        except OSError:
            pass
    if not code:
        code = os.environ.get("LANG", "fr")
    return code[:2].lower()


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
# Texte français -> (anglais, espagnol, allemand). Jamais une langue de moins.
_TR = {
    "Winlux est à jour": ("Winlux is up to date", "Winlux está actualizado", "Winlux ist auf dem neuesten Stand"),
    "Recherchez les mises à jour": ("Check for updates", "Busque actualizaciones", "Nach Updates suchen"),
    "Aucune recherche pour l’instant": ("No check yet", "Todavía no se ha buscado", "Noch nicht gesucht"),
    "Dernière recherche : {when}": ("Last checked: {when}", "Última búsqueda: {when}", "Zuletzt gesucht: {when}"),
    "Rechercher des mises à jour": ("Check for updates", "Buscar actualizaciones", "Nach Updates suchen"),
    "Recherche de mises à jour…": ("Checking for updates…", "Buscando actualizaciones…", "Suche nach Updates…"),
    "Connexion à GitHub, où Winlux est publié": ("Contacting GitHub, where Winlux is published", "Conectando con GitHub, donde se publica Winlux", "Verbindung zu GitHub, wo Winlux veröffentlicht wird"),
    "Mise à jour disponible": ("Update available", "Actualización disponible", "Update verfügbar"),
    "{size} à télécharger": ("{size} to download", "{size} para descargar", "{size} herunterzuladen"),
    "redémarrage nécessaire": ("restart required", "requiere reinicio", "Neustart erforderlich"),
    "Télécharger et installer": ("Download and install", "Descargar e instalar", "Herunterladen und installieren"),
    "Installation de Winlux {version}": ("Installing Winlux {version}", "Instalando Winlux {version}", "Winlux {version} wird installiert"),
    "Retour à la version précédente…": ("Going back to the previous version…", "Volviendo a la versión anterior…", "Rückkehr zur vorherigen Version…"),
    "Vous pouvez continuer à travailler. L’écran reste allumé jusqu’à la fin.": ("You can keep working. The screen stays on until it finishes.", "Puede seguir trabajando. La pantalla permanece encendida hasta el final.", "Sie können weiterarbeiten. Der Bildschirm bleibt bis zum Ende eingeschaltet."),
    "Téléchargement": ("Download", "Descarga", "Download"),
    "Point de restauration": ("Restore point", "Punto de restauración", "Wiederherstellungspunkt"),
    "Installation": ("Installation", "Instalación", "Installation"),
    "Vérification": ("Verification", "Verificación", "Überprüfung"),
    "Redémarrage": ("Restart", "Reinicio", "Neustart"),
    "{done} sur {total}": ("{done} of {total}", "{done} de {total}", "{done} von {total}"),
    "Redémarrage nécessaire": ("Restart required", "Reinicio necesario", "Neustart erforderlich"),
    "Winlux {version} est installé. Redémarrez pour terminer ; si le bureau ne démarre pas, la version {previous} revient toute seule.": ("Winlux {version} is installed. Restart to finish; if the desktop does not start, version {previous} comes back on its own.", "Winlux {version} está instalado. Reinicie para terminar; si el escritorio no arranca, la versión {previous} vuelve sola.", "Winlux {version} ist installiert. Starten Sie neu, um abzuschließen; startet der Desktop nicht, kehrt Version {previous} von selbst zurück."),
    "Winlux {version} est installé. Redémarrez pour terminer.": ("Winlux {version} is installed. Restart to finish.", "Winlux {version} está instalado. Reinicie para terminar.", "Winlux {version} ist installiert. Starten Sie neu, um abzuschließen."),
    "Redémarrer maintenant": ("Restart now", "Reiniciar ahora", "Jetzt neu starten"),
    "La mise à jour a échoué": ("The update failed", "La actualización ha fallado", "Das Update ist fehlgeschlagen"),
    "Étape : {stage}. Rien n’a été changé sur ce PC si l’échec a eu lieu avant l’installation.": ("Step: {stage}. Nothing was changed on this PC if it failed before installation.", "Paso: {stage}. No se cambió nada en este PC si falló antes de la instalación.", "Schritt: {stage}. Auf diesem PC wurde nichts geändert, wenn der Fehler vor der Installation auftrat."),
    "Réessayer": ("Try again", "Reintentar", "Erneut versuchen"),
    "Afficher les détails": ("Show details", "Mostrar detalles", "Details anzeigen"),
    "Mises à jour suspendues": ("Updates paused", "Actualizaciones en pausa", "Updates angehalten"),
    "Reprise automatique le {date}.": ("They resume automatically on {date}.", "Se reanudan automáticamente el {date}.", "Sie werden am {date} automatisch fortgesetzt."),
    "Winlux {version} attend.": ("Winlux {version} is waiting.", "Winlux {version} está en espera.", "Winlux {version} wartet."),
    "Reprendre les mises à jour": ("Resume updates", "Reanudar actualizaciones", "Updates fortsetzen"),
    "Winlux {failed} a été retiré : le bureau n’a pas démarré. Il ne s’installera plus tout seul.": ("Winlux {failed} was removed: the desktop did not start. It will not install itself again.", "Winlux {failed} se retiró: el escritorio no arrancó. No volverá a instalarse solo.", "Winlux {failed} wurde entfernt: Der Desktop startete nicht. Es wird nicht mehr automatisch installiert."),
    "Nouveautés de la version {version}": ("What’s new in version {version}", "Novedades de la versión {version}", "Neu in Version {version}"),
    "Nouveautés de votre version ({version})": ("What’s new in your version ({version})", "Novedades de su versión ({version})", "Neu in Ihrer Version ({version})"),
    "Lire toutes les notes de version": ("Read the full release notes", "Leer todas las notas de la versión", "Alle Versionshinweise lesen"),
    "Réduire": ("Show less", "Mostrar menos", "Weniger anzeigen"),
    "Les notes de version sont rédigées en français.": ("Release notes are written in French.", "Las notas de la versión están escritas en francés.", "Die Versionshinweise sind auf Französisch verfasst."),
    "Afficher le journal": ("Show the log", "Mostrar el registro", "Protokoll anzeigen"),
    "Plus d’options": ("More options", "Más opciones", "Weitere Optionen"),
    "Suspendre les mises à jour": ("Pause updates", "Pausar actualizaciones", "Updates anhalten"),
    "Rien ne s’installe tout seul pendant la pause ; la recherche reste possible.": ("Nothing installs by itself during the pause; you can still check.", "Nada se instala solo durante la pausa; puede seguir buscando.", "Während der Pause wird nichts automatisch installiert; Suchen bleibt möglich."),
    "Suspendues jusqu’au {date}": ("Paused until {date}", "En pausa hasta el {date}", "Angehalten bis {date}"),
    "Reprendre": ("Resume", "Reanudar", "Fortsetzen"),
    "Pendant {count} semaine(s)": ("For {count} week(s)", "Durante {count} semana(s)", "Für {count} Woche(n)"),
    "Choisir une durée": ("Choose a duration", "Elegir una duración", "Dauer wählen"),
    "Installer automatiquement les nouvelles versions": ("Install new versions automatically", "Instalar automáticamente las nuevas versiones", "Neue Versionen automatisch installieren"),
    "En dehors des heures d’activité, jamais pendant un jeu. Le redémarrage reste votre choix.": ("Outside active hours, never during a game. Restarting is always your choice.", "Fuera del horario de actividad, nunca durante un juego. El reinicio siempre lo decide usted.", "Außerhalb der Nutzungszeit, nie während eines Spiels. Den Neustart entscheiden Sie."),
    "Heures d’activité": ("Active hours", "Horas activas", "Nutzungszeit"),
    "Aucune installation automatique de {start} à {end}.": ("No automatic installation from {start} to {end}.", "Ninguna instalación automática de {start} a {end}.", "Keine automatische Installation von {start} bis {end}."),
    "de": ("from", "de", "von"),
    "à": ("to", "a", "bis"),
    "Historique des mises à jour": ("Update history", "Historial de actualizaciones", "Updateverlauf"),
    "Aucune mise à jour enregistrée pour l’instant.": ("No updates recorded yet.", "Todavía no hay actualizaciones registradas.", "Noch keine Updates aufgezeichnet."),
    "{count} entrée(s)": ("{count} entry(ies)", "{count} entrada(s)", "{count} Eintrag/Einträge"),
    "Afficher": ("Show", "Mostrar", "Anzeigen"),
    "Revenir à la version précédente": ("Go back to the previous version", "Volver a la versión anterior", "Zur vorherigen Version zurückkehren"),
    "{version} est conservée et peut être remise en place.": ("{version} is kept and can be put back.", "{version} se conserva y puede restaurarse.", "{version} wird aufbewahrt und kann wiederhergestellt werden."),
    "Aucune version précédente conservée.": ("No previous version kept.", "No se conserva ninguna versión anterior.", "Keine vorherige Version aufbewahrt."),
    "Revenir": ("Go back", "Volver", "Zurückkehren"),
    "Logiciels et pilotes Linux": ("Linux software and drivers", "Software y controladores de Linux", "Linux-Software und -Treiber"),
    "Applications, pilotes et bibliothèques installés avec APT.": ("Apps, drivers and libraries installed with APT.", "Aplicaciones, controladores y bibliotecas instalados con APT.", "Mit APT installierte Apps, Treiber und Bibliotheken."),
    "Ouvrir": ("Open", "Abrir", "Öffnen"),
    "‹ Retour": ("‹ Back", "‹ Volver", "‹ Zurück"),
    "Installée": ("Installed", "Instalada", "Installiert"),
    "Échec": ("Failed", "Error", "Fehlgeschlagen"),
    "Restaurée à la main": ("Restored by hand", "Restaurada manualmente", "Manuell wiederhergestellt"),
    "Restaurée automatiquement": ("Restored automatically", "Restaurada automáticamente", "Automatisch wiederhergestellt"),
    "depuis {version}": ("from {version}", "desde {version}", "von {version}"),
    "le bureau n’a pas démarré": ("the desktop did not start", "el escritorio no arrancó", "der Desktop startete nicht"),
    "Nouveautés": ("What’s new", "Novedades", "Neuerungen"),
    "aujourd’hui à {time}": ("today at {time}", "hoy a las {time}", "heute um {time}"),
    "hier à {time}": ("yesterday at {time}", "ayer a las {time}", "gestern um {time}"),
    "le {date} à {time}": ("on {date} at {time}", "el {date} a las {time}", "am {date} um {time}"),
    "Mo": ("MB", "MB", "MB"),
    "Go": ("GB", "GB", "GB"),
    "Recherche": ("Search", "Búsqueda", "Suche"),
    "Décompression": ("Unpacking", "Descompresión", "Entpacken"),
    "Sauvegarde": ("Backup", "Copia de seguridad", "Sicherung"),
    "Finalisation": ("Finishing", "Finalización", "Abschluss"),
    "Restauration": ("Restore", "Restauración", "Wiederherstellung"),
    # Notification du bureau
    "Mise à jour de Winlux disponible": ("Winlux update available", "Actualización de Winlux disponible", "Winlux-Update verfügbar"),
    "Winlux {version} est prête. Ouvrez Paramètres pour voir les nouveautés et l’installer.": ("Winlux {version} is ready. Open Settings to see what’s new and install it.", "Winlux {version} está lista. Abra Configuración para ver las novedades e instalarla.", "Winlux {version} ist bereit. Öffnen Sie die Einstellungen, um die Neuerungen zu sehen und es zu installieren."),
    "Redémarrez pour terminer la mise à jour": ("Restart to finish the update", "Reinicie para terminar la actualización", "Neu starten, um das Update abzuschließen"),
    "Winlux {version} est installé. Enregistrez votre travail puis redémarrez.": ("Winlux {version} is installed. Save your work, then restart.", "Winlux {version} está instalado. Guarde su trabajo y reinicie.", "Winlux {version} ist installiert. Speichern Sie Ihre Arbeit und starten Sie neu."),
    "Voir": ("View", "Ver", "Anzeigen"),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


RUN = Path(os.environ.get("WINLUX_RUN_DIR", "/run/winlux"))
STATE_DIR = Path(os.environ.get("WINLUX_UPDATE_STATE_DIR", "/var/lib/winlux"))
PAUSE_WEEKS = (1, 2, 3, 4, 5)
# Étapes affichées, dans l'ordre du moteur (download → point → apt → dpkg --verify → ready).
STEPS = ("Téléchargement", "Point de restauration", "Installation", "Vérification", "Redémarrage")
PHASE_STEP = {"downloading": 0, "extracting": 0, "restore-point": 1, "backing-up": 1,
              "installing": 2, "verifying": 3, "finalizing": 3, "ready": 4}
PHASE_NAMES = {"checking": "Recherche", "downloading": "Téléchargement", "verifying": "Vérification",
               "extracting": "Décompression", "backing-up": "Sauvegarde", "restore-point": "Point de restauration",
               "installing": "Installation", "finalizing": "Finalisation", "restoring": "Restauration"}


def read(path):
    try:
        return Path(path).read_text(errors="replace")
    except OSError:
        return ""


def keyvals(text):
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


# ── Données ──────────────────────────────────────────────────────────────

def legacy_state(listing):
    """État déduit de l'ancien rapport (moteur sans --state)."""
    values = {}
    for line in listing.splitlines():
        key, _, value = line.partition(" : ")
        if key == "Version installée": values["installed"] = value
        elif key == "Version disponible": values["available"] = value
        elif key == "Taille à télécharger": values["download_bytes"] = value.split()[0]
        elif key == "Version précédente conservée": values["previous"] = value
    return values


def load_state(run=None):
    run = Path(run or RUN)
    values = keyvals(read(run / "update-state.txt"))
    if not values.get("installed"):
        values = {**legacy_state(read(run / "updates.txt")), **{k: v for k, v in values.items() if v}}
    return values


def load_history(directory=None):
    """Historique, le plus récent d'abord ; lignes abîmées ignorées."""
    entries = []
    for line in read(Path(directory or STATE_DIR) / "update-history").splitlines():
        try:
            entry = json.loads(line)
        except ValueError:
            continue
        if isinstance(entry, dict) and isinstance(entry.get("date"), int) and entry.get("result"):
            entries.append(entry)
    return entries[::-1]


def load_notes(version, directory=None):
    if not version or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._+-]*", version):
        return ""
    return read(Path(directory or STATE_DIR) / "notes" / f"{version}.md").strip()


def number(value, decimals):
    text = f"{value:.{decimals}f}"
    return text if _LANG == "en" else text.replace(".", ",")


def human_size(value):
    """« 463 Mo », « 1,4 Go » (unités de 1 048 576 octets, comme le reste des Paramètres)."""
    try:
        size = int(value)
    except (TypeError, ValueError):
        return ""
    if size >= 1024 ** 3:
        return f"{number(size / 1024 ** 3, 1)} {_t('Go')}"
    return f"{max(1, round(size / 1024 ** 2))} {_t('Mo')}"


def short_date(stamp):
    day = time.localtime(stamp)
    if _LANG == "en": return time.strftime("%m/%d/%Y", day)
    if _LANG == "de": return time.strftime("%d.%m.%Y", day)
    return time.strftime("%d/%m/%Y", day)


def clock(stamp):
    return time.strftime("%H:%M", time.localtime(stamp))


def when(stamp, now=None):
    """« aujourd’hui à 09:12 », « hier à 18:40 », « le 02/11/2026 à 21:02 »."""
    now = time.time() if now is None else now
    today = time.localtime(now)
    day = time.localtime(stamp)
    if day[:3] == today[:3]:
        return _t("aujourd’hui à {time}").format(time=clock(stamp))
    if day[:3] == time.localtime(now - 86400)[:3]:
        return _t("hier à {time}").format(time=clock(stamp))
    return _t("le {date} à {time}").format(date=short_date(stamp), time=clock(stamp))


def notes_blocks(text):
    """Blocs des notes Markdown : ("h", titre), ("li", puce), ("p", paragraphe).
    Les lignes de suite (puce sur plusieurs lignes, paragraphe) sont recollées."""
    blocks = []
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line.strip() or line.strip() in ("---", "***"):
            blocks.append(None)
            continue
        heading = re.match(r"#{1,6}\s+(.*)", line)
        bullet = re.match(r"\s*[-*+]\s+(.*)", line)
        if heading: blocks.append(["h", heading.group(1).strip()])
        elif bullet: blocks.append(["li", bullet.group(1).strip()])
        elif blocks and blocks[-1] is not None and blocks[-1][0] in ("li", "p"):
            blocks[-1][1] += " " + line.strip()
        else: blocks.append(["p", line.strip()])
    return [tuple(block) for block in blocks if block is not None]


def inline_markup(text):
    # Les listes de numéros d'issues « (#232, #533) » ne disent rien à l'utilisateur.
    text = re.sub(r"\s*\((?:#\d+[,;\s]*)+\)", "", text)
    text = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", text)
    text = html.escape(text, quote=False)
    text = re.sub(r"\*\*(.+?)\*\*", r"<b>\1</b>", text)
    return re.sub(r"`([^`]+)`", r"<tt>\1</tt>", text)


def notes_markup(text, limit=None):
    """Markdown des notes de version → balisage Pango (titres en gras, puces,
    gras, code, liens réduits à leur texte). limit : seulement les N premières puces."""
    blocks = notes_blocks(text)
    if limit is not None:
        # Aperçu : la première phrase de chaque puce.
        items = [inline_markup(re.split(r"(?<=[.!?])\s", body, maxsplit=1)[0]) for kind, body in blocks if kind == "li"][:limit]
        if items: return "\n".join("•  " + item for item in items)
        blocks = [block for block in blocks if block[0] == "p"][:1]
    lines = []
    for index, (kind, body) in enumerate(blocks):
        if kind == "h" and index == 0 and body.lower().startswith("winlux"): continue
        if kind == "h":
            if lines: lines.append("")
            lines.append(f"<b>{inline_markup(body)}</b>")
        elif kind == "li": lines.append("•  " + inline_markup(body))
        else: lines.append(inline_markup(body))
    return "\n".join(lines).strip("\n")


def situation(state, progress, busy, failed=None, now=None):
    """L'état que montre la carte du haut : (genre, titre, sous-titre, bouton)."""
    now = time.time() if now is None else now
    installed = state.get("installed", "")
    available = state.get("available", "")
    phase = progress.get("phase", "")
    if busy:
        if phase in ("checking", "idle", ""):
            return "checking", _t("Recherche de mises à jour…"), _t("Connexion à GitHub, où Winlux est publié"), None
        if phase == "restoring":
            return "running", _t("Retour à la version précédente…"), "", None
        return "running", _t("Installation de Winlux {version}").format(version=available or "?"), "", None
    if failed:
        stage = _t(PHASE_NAMES.get(failed, "Installation"))
        return ("error", _t("La mise à jour a échoué"),
                _t("Étape : {stage}. Rien n’a été changé sur ce PC si l’échec a eu lieu avant l’installation.").format(stage=stage),
                "retry")
    if state.get("reboot") == "1" or (state.get("trial") and state.get("trial") == installed and phase == "ready"):
        previous = state.get("previous", "")
        text = (_t("Winlux {version} est installé. Redémarrez pour terminer ; si le bureau ne démarre pas, la version {previous} revient toute seule.")
                if previous else _t("Winlux {version} est installé. Redémarrez pour terminer.")).format(version=installed, previous=previous)
        return "reboot", _t("Redémarrage nécessaire"), text, "reboot"
    try: paused = int(state.get("paused_until", "0") or 0)
    except ValueError: paused = 0
    if paused > now:
        text = _t("Reprise automatique le {date}.").format(date=short_date(paused))
        if available: text += " " + _t("Winlux {version} attend.").format(version=available)
        return "paused", _t("Mises à jour suspendues"), text, "resume"
    if available:
        parts = ["Winlux " + available]
        size = human_size(state.get("download_bytes"))
        if size: parts.append(_t("{size} à télécharger").format(size=size))
        parts.append(_t("redémarrage nécessaire"))
        return "available", _t("Mise à jour disponible"), " · ".join(parts), "install"
    try: checked = int(state.get("last_check", "0") or 0)
    except ValueError: checked = 0
    version = ("Winlux " + installed) if installed else ""
    if not checked:
        return "never", _t("Recherchez les mises à jour"), " · ".join(p for p in (_t("Aucune recherche pour l’instant"), version) if p), "check"
    return "uptodate", _t("Winlux est à jour"), " · ".join(p for p in (_t("Dernière recherche : {when}").format(when=when(checked, now)), version) if p), "check"


def progress_text(progress):
    """« Téléchargement · 171,7 Mo sur 463 Mo » ou l'étape seule."""
    phase = progress.get("phase", "")
    name = _t(PHASE_NAMES.get(phase, "Installation"))
    try:
        done, total = int(progress.get("done", "0")), int(progress.get("total", "0"))
    except ValueError:
        done = total = 0
    if progress.get("unit") == "bytes" and total:
        mib = _t("Mo")
        amount = _t("{done} sur {total}").format(done=f"{number(done / 1048576, 1)} {mib}", total=f"{number(total / 1048576, 0)} {mib}")
        return f"{name} · {amount}"
    if total:
        return f"{name} · {_t('{done} sur {total}').format(done=done, total=total)}"
    return name


# ── Icônes Prisme (même dessin que les icônes d'applications) ────────────

GLYPHS = {
    "sync": '<path d="M44 26a13 13 0 0 0-23-5l-3 3"/><path d="M18 16v8h8"/><path d="M20 38a13 13 0 0 0 23 5l3-3"/><path d="M46 48v-8h-8"/>',
    "download": '<path d="M32 17v20M24 30l8 8 8-8"/><path d="M18 42v5h28v-5"/>',
    "restart": '<path d="M44 30a12 12 0 1 1-6-10"/><path d="M38 12v8h8"/>',
    "pause": '<path d="M26 20v24M38 20v24"/>',
    "error": '<path d="M32 18v16"/><path d="M32 44v1"/>',
}
TILES = {"accent": ("#9B7BFF", "#6A3DF0"), "amber": ("#F2A33A", "#C46A00"), "red": ("#F06B6B", "#C62828")}
KIND_ICON = {"uptodate": ("accent", "sync", True), "never": ("accent", "sync", False), "checking": ("accent", "sync", False),
             "available": ("accent", "download", False), "running": ("accent", "sync", False),
             "reboot": ("amber", "restart", False), "paused": ("amber", "pause", False), "error": ("red", "error", False)}


def hero_svg(kind):
    tile, glyph, check = KIND_ICON.get(kind, KIND_ICON["never"])
    top, bottom = TILES[tile]
    badge = ('<circle cx="47" cy="47" r="10" fill="#16924F" stroke="#fff" stroke-width="2.5"/>'
             '<path d="M42.5 47l3 3 5.5-6" stroke-width="2.6"/>') if check else ""
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><defs>'
            f'<linearGradient id="t" x1="0" y1="0" x2="0" y2="1"><stop stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>'
            '<linearGradient id="h" x1="0" y1="0" x2="0" y2="1"><stop stop-color="#fff" stop-opacity=".28"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>'
            '<rect x="7" y="10" width="50" height="50" rx="12" fill="#000" opacity=".16"/>'
            '<rect x="6" y="6" width="52" height="52" rx="12" fill="url(#t)"/><rect x="6" y="6" width="52" height="52" rx="12" fill="url(#h)"/>'
            f'<g fill="none" stroke="#fff" stroke-width="3.6" stroke-linecap="round" stroke-linejoin="round">{GLYPHS[glyph]}{badge}</g></svg>')


ROW_ICONS = {
    "pause": '<rect x="6" y="5" width="4" height="14" rx="1"/><rect x="14" y="5" width="4" height="14" rx="1"/>',
    "auto": '<path d="M12 4v10M8 10l4 4 4-4"/><path d="M5 18h14"/>',
    "clock": '<circle cx="12" cy="12" r="8"/><path d="M12 7v5h4"/>',
    "history": '<path d="M4 12a8 8 0 1 0 3-6.2"/><path d="M4 4v4h4"/><path d="M12 8v4l3 2"/>',
    "undo": '<path d="M9 14L4 9l5-5"/><path d="M4 9h10a6 6 0 0 1 0 12h-3"/>',
    "package": '<path d="M4 8l8-4 8 4v8l-8 4-8-4z"/><path d="M4 8l8 4 8-4M12 12v8"/>',
}


def row_svg(name, color):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="{color}" '
            f'stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{ROW_ICONS[name]}</svg>')


# ── Page GTK ─────────────────────────────────────────────────────────────

def _gtk():
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf, GLib, Gtk, Pango
    return GdkPixbuf, GLib, Gtk, Pango


def svg_image(svg, size):
    GdkPixbuf, _, Gtk, _ = _gtk()
    loader = GdkPixbuf.PixbufLoader.new_with_type("svg")
    loader.set_size(size, size)
    loader.write(svg.encode())
    loader.close()
    return Gtk.Image.new_from_pixbuf(loader.get_pixbuf())


PAGE_CSS = """
  #update-page .hero {{ background: {card}; border: 1px solid {border}; border-radius: 8px; padding: 20px 24px; }}
  #update-page .option {{ background: {card}; border: 1px solid {border}; border-radius: 8px; padding: 12px 20px; }}
  #update-page .hero-title {{ font-size: 20px; font-weight: 600; }}
  #update-page .row-title {{ font-weight: 500; }}
  #update-page .dim {{ color: {dim}; }}
  #update-page .step {{ color: {dim}; font-size: 12px; border-top: 3px solid {border}; padding-top: 6px; }}
  #update-page .step.done {{ color: {fg}; border-top-color: {accent_line}; }}
  #update-page .step.current {{ color: {accent_text}; font-weight: 600; border-top-color: {accent_line}; }}
  #update-page button.primary {{ background: {accent}; border-color: {accent}; padding: 8px 18px; }}
  #update-page button.primary label {{ color: {on_accent}; }}
  #update-page button.primary:hover {{ background: {accent_hover}; }}
  #update-page button.secondary {{ border-color: {border}; padding: 8px 18px; }}
  #update-page button.link {{ background: transparent; border: none; padding: 2px 0; }}
  #update-page button.link label {{ color: {accent_text}; }}
  #update-page button.link:hover label {{ text-decoration: underline; }}
  #update-page progressbar trough {{ background: {border}; border: none; min-height: 4px; border-radius: 2px; }}
  #update-page progressbar progress {{ background: {accent_line}; border: none; min-height: 4px; border-radius: 2px; }}
  #update-page .history-ok {{ color: {ok}; }}
  #update-page .history-warn {{ color: {warn}; }}
  #update-page .history-error {{ color: {error}; }}
  #update-page .notes tt {{ font-family: monospace; }}
  #update-page switch {{ border-color: {border}; }}
  #update-page switch:checked {{ background: {accent}; border-color: {accent}; }}
  #update-page switch slider {{ background: #ffffff; border-color: {border}; }}
"""


def page_palette(base, colors):
    """Couleurs propres à la page, toujours lisibles (4,5:1) sur la carte."""
    readable, text_on, mix = colors["readable"], colors["text_on"], colors["mix"]
    p = dict(base)
    accent = p.get("accent", "#6a3df0")
    p["on_accent"] = text_on(accent)
    p["accent_hover"] = mix(accent, "#000000" if p["on_accent"] == "#ffffff" else "#ffffff", 0.12)
    p["accent_text"] = readable(accent, p["card"])
    p["accent_line"] = readable(accent, p["card"], 3.0)
    for key, light, dark in (("ok", "#16924F", "#2FB872"), ("warn", "#B86E00", "#C98A12"), ("error", "#D32F2F", "#E5484D")):
        p[key] = readable(light, p["card"])
    return p


class UpdatePanel:
    """Contenu de la page 7, posé dans window.body ; refresh() à chaque relevé."""

    def __init__(self, window, journal=None, colors=None):
        GdkPixbuf, GLib, Gtk, Pango = _gtk()
        self.Gtk, self.Pango = Gtk, Pango
        self.window = window
        self.journal_path = journal
        # WINLUX_UPDATE_VIEW=history ouvre l'historique d'emblée (captures, tests).
        self.view = window.update_view = getattr(window, "update_view", None) or os.environ.get("WINLUX_UPDATE_VIEW", "main")
        self.failed_phase = getattr(window, "update_failed", None)
        self.kind = None
        self.notes_open = False
        self.css = Gtk.CssProvider()
        if colors:
            try:
                self.css.load_from_data(PAGE_CSS.format(**page_palette(colors["palette"], colors)).encode())
                Gtk.StyleContext.add_provider_for_screen(window.get_screen(), self.css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 2)
            except Exception as error:
                print(f"Styles de la page Mises à jour : {error}", file=sys.stderr)
        self.dim_color = (colors or {}).get("palette", {}).get("fg", "#1f2330")
        self.root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.root.set_name("update-page")
        window.body.pack_start(self.root, False, False, 0)
        if self.view == "history":
            self.build_history()
        else:
            self.build_main()
        self.root.show_all()
        self.refresh()

    # Briques

    def label(self, text="", classes=(), parent=None, wrap=True, markup=False):
        Gtk, Pango = self.Gtk, self.Pango
        widget = Gtk.Label(xalign=0, wrap=wrap)
        if wrap: widget.set_line_wrap_mode(Pango.WrapMode.WORD_CHAR)
        if markup: widget.set_markup(text)
        else: widget.set_text(text)
        for name in classes: widget.get_style_context().add_class(name)
        if parent is not None: parent.pack_start(widget, False, False, 0)
        return widget

    def button(self, text, callback, style="secondary"):
        widget = self.Gtk.Button(label=text, valign=self.Gtk.Align.CENTER)
        widget.get_style_context().add_class(style)
        widget.connect("clicked", lambda *_: callback())
        return widget

    def option(self, icon, title, subtitle, widget=None):
        Gtk = self.Gtk
        box = Gtk.Box(spacing=16)
        box.get_style_context().add_class("option")
        box.pack_start(svg_image(row_svg(icon, self.dim_color), 22), False, False, 0)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, valign=Gtk.Align.CENTER)
        self.label(title, ("row-title",), texts)
        sub = self.label(subtitle or "", ("dim",), texts)
        sub.set_no_show_all(not subtitle)
        box.pack_start(texts, True, True, 0)
        if widget is not None:
            widget.set_valign(Gtk.Align.CENTER)
            box.pack_start(widget, False, False, 0)
        self.root.pack_start(box, False, False, 0)
        return box, sub

    # Page principale

    def build_main(self):
        Gtk = self.Gtk
        hero = Gtk.Box(spacing=20)
        hero.get_style_context().add_class("hero")
        self.icon_box = Gtk.Box(valign=Gtk.Align.START)
        hero.pack_start(self.icon_box, False, False, 0)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4, valign=Gtk.Align.CENTER)
        self.title = self.label("", ("hero-title",), texts)
        self.subtitle = self.label("", ("dim",), texts)
        self.bar = Gtk.ProgressBar()
        self.bar.set_margin_top(8)
        texts.pack_start(self.bar, False, False, 0)
        self.steps = Gtk.Box(spacing=6, homogeneous=True)
        self.step_labels = []
        for name in STEPS:
            step = self.label(_t(name), ("step",), self.steps, wrap=True)
            step.set_width_chars(12)
            self.step_labels.append(step)
        texts.pack_start(self.steps, False, False, 0)
        hero.pack_start(texts, True, True, 0)
        buttons = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, valign=Gtk.Align.CENTER)
        self.primary = Gtk.Button(valign=Gtk.Align.CENTER)
        self.primary.get_style_context().add_class("primary")
        self.primary.connect("clicked", lambda *_: self.primary_clicked())
        buttons.pack_start(self.primary, False, False, 0)
        self.details_button = self.button(_t("Afficher les détails"), self.show_details)
        buttons.pack_start(self.details_button, False, False, 0)
        hero.pack_start(buttons, False, False, 0)
        self.root.pack_start(hero, False, False, 0)
        self.install_button = self.primary
        # Sous la carte : avis, nouveautés, journal.
        self.note = self.label("", ("dim",), self.root)
        self.note.set_margin_start(4)
        self.notes_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.notes_box.get_style_context().add_class("option")
        self.notes_title = self.label("", ("row-title",), self.notes_box)
        self.notes_label = self.label("", ("notes",), self.notes_box, markup=True)
        self.notes_label.set_selectable(True)
        self.notes_language = self.label(_t("Les notes de version sont rédigées en français."), ("dim",), self.notes_box)
        self.notes_toggle = self.button(_t("Lire toutes les notes de version"), self.toggle_notes, "link")
        self.notes_toggle.set_halign(Gtk.Align.START)
        self.notes_box.pack_start(self.notes_toggle, False, False, 0)
        self.root.pack_start(self.notes_box, False, False, 0)
        self.journal = Gtk.Expander(label=_t("Afficher le journal"))
        self.journal_text = self.label("", ("dim",), None)
        self.journal_text.set_selectable(True)
        self.journal.add(self.journal_text)
        self.root.pack_start(self.journal, False, False, 0)
        for widget in (self.note, self.notes_box, self.journal, self.bar, self.steps, self.details_button):
            widget.set_no_show_all(True)
        self.notes_label.show(); self.notes_title.show(); self.notes_toggle.show(); self.journal_text.show()
        self.notes_language.set_no_show_all(True)
        if _LANG != "fr": self.notes_language.show()
        for step in self.step_labels: step.show()
        # Plus d'options
        section = self.label(_t("Plus d’options"), ("section",), self.root)
        section.set_margin_top(12)
        state = load_state()
        status = getattr(window_status(self.window), "copy", dict)()
        self.pause_combo = Gtk.ComboBoxText()
        self.pause_combo.append("", _t("Choisir une durée"))
        for weeks in PAUSE_WEEKS:
            self.pause_combo.append(str(weeks), _t("Pendant {count} semaine(s)").format(count=weeks))
        self.pause_combo.set_active_id("")
        self.pause_combo.connect("changed", lambda combo: combo.get_active_id() and self.run("update-pause-" + combo.get_active_id()))
        self.resume_button = self.button(_t("Reprendre"), lambda: self.run("update-resume"))
        pause_widgets = Gtk.Box(spacing=8)
        pause_widgets.pack_start(self.pause_combo, False, False, 0)
        pause_widgets.pack_start(self.resume_button, False, False, 0)
        self.resume_button.set_no_show_all(True)
        self.pause_row, self.pause_sub = self.option("pause", _t("Suspendre les mises à jour"),
                                                     _t("Rien ne s’installe tout seul pendant la pause ; la recherche reste possible."), pause_widgets)
        auto = state.get("auto") or status.get("update_auto", "disabled")
        self.auto_switch = Gtk.Switch(active=auto == "enabled")
        self.auto_switch.connect("notify::active", lambda switch, _: self.run("update-auto-on" if switch.get_active() else "update-auto-off"))
        self.option("auto", _t("Installer automatiquement les nouvelles versions"),
                    _t("En dehors des heures d’activité, jamais pendant un jeu. Le redémarrage reste votre choix."), self.auto_switch)
        start, end = active_hours(state)
        hours = Gtk.Box(spacing=6)
        self.start_combo, self.end_combo = Gtk.ComboBoxText(), Gtk.ComboBoxText()
        for combo, value in ((self.start_combo, start), (self.end_combo, end)):
            for hour in range(24): combo.append(str(hour), f"{hour:02d}:00")
            combo.set_active_id(str(value))
        hours.pack_start(self.label(_t("de"), ("dim",), None, wrap=False), False, False, 0)
        hours.pack_start(self.start_combo, False, False, 0)
        hours.pack_start(self.label(_t("à"), ("dim",), None, wrap=False), False, False, 0)
        hours.pack_start(self.end_combo, False, False, 0)
        for combo in (self.start_combo, self.end_combo): combo.connect("changed", lambda *_: self.hours_changed())
        _, self.hours_sub = self.option("clock", _t("Heures d’activité"), hours_text(start, end), hours)
        history = load_history()
        self.option("history", _t("Historique des mises à jour"),
                    _t("{count} entrée(s)").format(count=len(history)) if history else _t("Aucune mise à jour enregistrée pour l’instant."),
                    self.button(_t("Afficher"), self.open_history))
        self.rollback_button = self.button(_t("Revenir"), lambda: self.run("update-rollback"))
        self.rollback_row, self.recovery_label = self.option("undo", _t("Revenir à la version précédente"), " ", self.rollback_button)
        self.option("package", _t("Logiciels et pilotes Linux"), _t("Applications, pilotes et bibliothèques installés avec APT."),
                    self.button(_t("Ouvrir"), lambda: self.window.open_page(17)))

    def build_history(self):
        Gtk = self.Gtk
        back = self.button(_t("‹ Retour"), self.close_history, "link")
        back.set_halign(Gtk.Align.START)
        self.root.pack_start(back, False, False, 0)
        self.label(_t("Historique des mises à jour"), ("section",), self.root)
        entries = load_history()
        if not entries:
            box = Gtk.Box()
            box.get_style_context().add_class("option")
            self.label(_t("Aucune mise à jour enregistrée pour l’instant."), ("dim",), box)
            self.root.pack_start(box, False, False, 0)
        names = {"installed": ("Installée", "history-ok", "✔"), "failed": ("Échec", "history-error", "✖"),
                 "restored": ("Restaurée à la main", "history-warn", "↺"), "auto-restored": ("Restaurée automatiquement", "history-warn", "↺")}
        for entry in entries:
            name, style, mark = names.get(entry["result"], (entry["result"], "dim", "•"))
            box = Gtk.Box(spacing=16)
            box.get_style_context().add_class("option")
            texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
            self.label("Winlux " + str(entry.get("version", "?")), ("row-title",), texts)
            detail = when(entry["date"])
            if entry.get("from"): detail += " · " + _t("depuis {version}").format(version=entry["from"])
            self.label(detail, ("dim",), texts)
            box.pack_start(texts, True, True, 0)
            reason = entry.get("detail", "")
            if entry["result"] == "auto-restored" and reason in ("boot", "session"): reason = _t("le bureau n’a pas démarré")
            elif entry["result"] == "failed":
                stage = reason.split(" : ", 1)[0]
                reason = _t(PHASE_NAMES.get(stage, stage)) if stage else ""
            result = self.label(f"{mark} {_t(name)}" + (f" ({reason})" if reason else ""), (style,), None)
            result.set_max_width_chars(48)
            box.pack_start(result, False, False, 0)
            if entry["result"] == "installed" and load_notes(entry.get("version")):
                box.pack_start(self.button(_t("Nouveautés"), lambda version=entry["version"]: self.show_notes(version)), False, False, 0)
            self.root.pack_start(box, False, False, 0)

    # Actions

    def run(self, action):
        self.window.execute(action)

    def primary_clicked(self):
        action = {"check": "update-check", "install": "update-apply", "retry": "update-apply",
                  "reboot": "reboot", "resume": "update-resume"}.get(self.action)
        if self.action == "retry" and not load_state().get("available"): action = "update-check"
        self.window.update_failed = self.failed_phase = None
        if action: self.run(action)

    def hours_changed(self):
        start, end = self.start_combo.get_active_id(), self.end_combo.get_active_id()
        if start is None or end is None: return
        self.hours_sub.set_text(hours_text(int(start), int(end)))
        if start != end: self.run(f"update-hours-{start}-{end}")

    def toggle_notes(self):
        self.notes_open = not self.notes_open
        self.notes_shown = None
        self.refresh()

    def open_history(self):
        self.window.update_view = "history"
        self.window.open_page(7) if self.window.current != 7 else self.window.select(self.window.nav, None)

    def close_history(self):
        self.window.update_view = "main"
        self.window.select(self.window.nav, None)

    def show_notes(self, version):
        self.window.show_info(_t("Nouveautés de la version {version}").format(version=version),
                              re.sub(r"<[^>]+>", "", html.unescape(notes_markup(load_notes(version)))))

    def show_details(self):
        self.window.show_update_error(getattr(self.window, "update_failed_action", None) or "update-apply", 1)

    def failed(self, action, phase):
        """Échec d'une recherche ou d'une installation : la carte le dit (plus de fenêtre surgissante)."""
        self.window.update_failed = self.failed_phase = phase or {"update-check": "checking"}.get(action, "installing")
        self.window.update_failed_action = action
        self.refresh()

    # Relevé

    def refresh(self, status=None):
        if self.view != "main": return
        status = status if status is not None else window_status(self.window)
        state = load_state()
        progress = keyvals(read(RUN / "update-progress"))
        running = getattr(self.window, "active_action", None) in ("update-check", "update-apply", "update-rollback")
        busy = status.get("update_busy") == "1" or running
        if busy and running and getattr(self.window, "active_action", None) == "update-check" and progress.get("phase") not in ("checking",):
            progress = dict(progress, phase="checking")
        kind, title, subtitle, action = situation(state, progress, busy, self.failed_phase)
        self.action = action
        if kind != self.kind:
            for child in self.icon_box.get_children(): child.destroy()
            image = svg_image(hero_svg(kind), 72)
            self.icon_box.pack_start(image, False, False, 0)
            image.show()
            self.kind = kind
        if kind == "running":
            subtitle = progress_text(progress)
        set_text(self.title, title)
        set_text(self.subtitle, subtitle)
        labels = {"check": _t("Rechercher des mises à jour"), "install": _t("Télécharger et installer"),
                  "retry": _t("Réessayer"), "reboot": _t("Redémarrer maintenant"), "resume": _t("Reprendre les mises à jour")}
        self.primary.set_visible(action is not None)
        if action: set_text(self.primary, labels[action], button=True)
        self.primary.set_sensitive(not busy)
        self.details_button.set_visible(kind == "error")
        # Progression : seulement pendant une opération (#877).
        moving = kind in ("running", "checking")
        self.bar.set_visible(moving)
        self.steps.set_visible(kind == "running" and progress.get("phase") != "restoring")
        if moving:
            try: percent = int(progress.get("percent", "-1"))
            except ValueError: percent = -1
            if percent < 0 or kind == "checking": self.bar.pulse()
            else: self.bar.set_fraction(min(1, percent / 100))
            current = PHASE_STEP.get(progress.get("phase", ""), 0)
            for index, step in enumerate(self.step_labels):
                context = step.get_style_context()
                for name, wanted in (("done", index < current), ("current", index == current)):
                    if wanted: context.add_class(name)
                    else: context.remove_class(name)
        # Avis sous la carte
        note = ""
        if kind == "running": note = _t("Vous pouvez continuer à travailler. L’écran reste allumé jusqu’à la fin.")
        elif state.get("rolled_failed") and kind in ("uptodate", "never", "available"):
            note = _t("Winlux {failed} a été retiré : le bureau n’a pas démarré. Il ne s’installera plus tout seul.").format(failed=state["rolled_failed"])
        set_text(self.note, note)
        self.note.set_visible(bool(note))
        # Nouveautés : celles de la version proposée, sinon de la version installée.
        version, title_key = state.get("notes") or "", "Nouveautés de la version {version}"
        if not version and kind in ("uptodate", "reboot", "never") and state.get("installed_notes"):
            version, title_key = state["installed_notes"], "Nouveautés de votre version ({version})"
        text = load_notes(version)
        self.notes_box.set_visible(bool(text))
        if text:
            set_text(self.notes_title, _t(title_key).format(version=version))
            short = notes_markup(text, limit=5)
            full = notes_markup(text)
            wanted = full if self.notes_open or short == full else short
            if self.notes_label.get_label() != wanted: self.notes_label.set_markup(wanted)
            self.notes_toggle.set_visible(short != full)
            set_text(self.notes_toggle, _t("Réduire") if self.notes_open else _t("Lire toutes les notes de version"), button=True)
        # Journal replié (#878)
        log = read(self.journal_path).strip()[-6000:] if self.journal_path else ""
        self.journal.set_visible(bool(log))
        if log and self.journal.get_expanded(): set_text(self.journal_text, log)
        # Options
        try: paused = int(state.get("paused_until", "0") or 0) > time.time()
        except ValueError: paused = False
        self.pause_combo.set_visible(not paused)
        self.resume_button.set_visible(paused)
        set_text(self.pause_sub, _t("Suspendues jusqu’au {date}").format(date=short_date(int(state["paused_until"]))) if paused
                 else _t("Rien ne s’installe tout seul pendant la pause ; la recherche reste possible."))
        previous = state.get("previous", "")
        self.rollback_row.set_visible(bool(previous))
        set_text(self.recovery_label, _t("{version} est conservée et peut être remise en place.").format(version=previous)
                 if previous else _t("Aucune version précédente conservée."))
        for widget in (self.pause_combo, self.resume_button, self.auto_switch, self.start_combo, self.end_combo, self.rollback_button):
            widget.set_sensitive(not busy)


def set_text(widget, text, button=False):
    current = widget.get_label() if button else widget.get_text()
    if current != text:
        if button: widget.set_label(text)
        else: widget.set_text(text)


def window_status(window):
    status = getattr(window, "status", None)
    if isinstance(status, dict) and status: return status
    return keyvals(read(RUN / "control.txt"))


def active_hours(state):
    match = re.fullmatch(r"([0-9]{1,2})-([0-9]{1,2})", state.get("active_hours", ""))
    if match and int(match.group(1)) < 24 and int(match.group(2)) < 24:
        return int(match.group(1)), int(match.group(2))
    return 8, 18


def hours_text(start, end):
    return _t("Aucune installation automatique de {start} à {end}.").format(start=f"{start:02d}:00", end=f"{end:02d}:00")


# ── Notification du bureau (#881) ────────────────────────────────────────

def notification(notice):
    """(titre, texte) de la notification pour l'avis du moteur, None sinon."""
    version = notice.get("version", "")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._+-]*", version): return None
    if notice.get("kind") == "available":
        return (_t("Mise à jour de Winlux disponible"),
                _t("Winlux {version} est prête. Ouvrez Paramètres pour voir les nouveautés et l’installer.").format(version=version))
    if notice.get("kind") == "reboot":
        return (_t("Redémarrez pour terminer la mise à jour"),
                _t("Winlux {version} est installé. Enregistrez votre travail puis redémarrez.").format(version=version))
    return None


def notify(seen_file=None):
    """Une notification par avis (version + genre), jamais deux fois."""
    notice = keyvals(read(RUN / "update-notice"))
    message = notification(notice)
    seen_file = Path(seen_file or Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "winlux/update-notice-seen")
    key = f"{notice.get('kind')}:{notice.get('version')}"
    already = read(seen_file).strip() == key
    # Toujours réécrit : la session ne relance Python que si l'avis est plus récent.
    seen_file.parent.mkdir(parents=True, exist_ok=True)
    seen_file.write_text(key + "\n")
    if message is None or already: return 0
    _, GLib, _, _ = _gtk()
    from gi.repository import Gio
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        reply = bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications",
                              "Notify", GLib.Variant("(susssasa{sv}i)", ("Paramètres", 0, "system-software-install", message[0], message[1],
                                                                          ["default", _t("Voir")], {}, 0)),
                              GLib.VariantType("(u)"), Gio.DBusCallFlags.NONE, 5000, None)
    except GLib.Error as error:
        print(f"Notification impossible : {error.message}", file=sys.stderr)
        return 1
    identity = reply.unpack()[0]
    loop = GLib.MainLoop()

    def invoked(_connection, _sender, _path, _interface, signal, parameters):
        values = parameters.unpack()
        if values[0] != identity: return
        if signal == "ActionInvoked":
            subprocess.Popen(["winlux-settings", "7"], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, start_new_session=True)
        loop.quit()
    for signal in ("ActionInvoked", "NotificationClosed"):
        bus.signal_subscribe("org.freedesktop.Notifications", "org.freedesktop.Notifications", signal,
                             "/org/freedesktop/Notifications", None, Gio.DBusSignalFlags.NONE, invoked)
    # La notification reste dans le centre de notifications : on attend un clic une heure au plus.
    GLib.timeout_add_seconds(3600, loop.quit)
    loop.run()
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["notify"]:
        raise SystemExit(notify())
    print("Usage : winlux-update-page notify", file=sys.stderr)
    raise SystemExit(2)
