#!/usr/bin/python3
"""Signaler un problème Winlux sur GitHub.

Rien ne part sans action de l'utilisateur : le rapport complet, modifiable,
est affiché avant l'envoi, et les données personnelles sont masquées par
défaut. Aucun envoi automatique, aucune télémétrie.

Chemins d'envoi, du plus simple au plus contraignant pour l'utilisateur :
  1. relais Winlux (RELAY=https://…) : aucun compte requis ; le relais détient
     le jeton GitHub, limite le débit et déduplique (integration/report-relay) ;
  2. jeton local (TOKEN_FILE=…) : réservé aux machines de développement ;
  3. navigateur : formulaire d'issue prérempli (compte GitHub requis) ;
  4. fichier : le rapport est seulement enregistré.

Usage :
  winlux-report                      formulaire « Signaler un problème »
  winlux-report --crash [--since T]  dernier plantage (rien si aucun après T)
  winlux-report --crash-at N         N-ième plantage du journal (Observateur)
  winlux-report --open-saved         ouvre le dossier des rapports enregistrés
  winlux-report --component NOM      problème d'un composant Winlux
  winlux-report --print [--crash]    affiche le rapport (sans interface)
  winlux-report --status             réglages, format clé=valeur
  winlux-report --set CLÉ=VALEUR     offer=on|off redact=on|off mode=auto|browser|file
"""
import functools
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

REPO = "Univers4craft/winlux"
API = "https://api.github.com"
JOURNAL = Path(os.environ.get("WINLUX_JOURNAL_DIR", "/run/winlux/journal"))
RUN = Path(os.environ.get("WINLUX_RUN_DIR", "/run/winlux"))
CONF = Path(os.environ.get("WINLUX_REPORT_CONF", "/etc/winlux/report.conf"))
DEFAULTS = Path(os.environ.get("WINLUX_REPORT_DEFAULTS", "/usr/share/winlux/report.conf"))
STATE = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "winlux"
URL_LIMIT = 7000
BODY_LIMIT = 60000

# ── Traductions ──────────────────────────────────────────────────────────

T = {
 "fr": {
  "window": "Signaler un problème", "crashed": "« {exe} » a cessé de fonctionner.",
  "component_failed": "Le composant Winlux « {exe} » a rencontré une erreur.",
  "manual": "Décrivez le problème : le rapport complet s'affiche ci-dessous et rien n'est envoyé sans votre accord.",
  "cause": "Cause probable", "summary": "Résumé en une ligne",
  "what": "Que faisiez-vous quand le problème est survenu ?",
  "redact": "Masquer les données personnelles (recommandé)",
  "preview": "Rapport complet qui sera envoyé (modifiable)",
  "edited": "Vous avez modifié le rapport : les champs ci-dessus ne s'y appliquent plus.",
  "regenerate": "Régénérer", "cancel": "Ne pas envoyer", "save": "Enregistrer", "send": "Envoyer le rapport",
  "via_relay": "Envoi via le service de signalement Winlux : aucun compte n'est nécessaire.",
  "via_token": "Envoi direct sur GitHub avec le jeton configuré sur cette machine.",
  "via_browser": "Le rapport s'ouvrira dans votre navigateur sur GitHub (un compte GitHub est nécessaire pour valider).",
  "via_file": "Le rapport sera seulement enregistré sur cet ordinateur.",
  "searching": "Recherche d'un signalement identique…", "sending": "Envoi en cours…",
  "sent": "Merci ! Signalement envoyé : {url}", "commented": "Merci ! Votre rapport a été ajouté au signalement existant : {url}",
  "opened": "Le formulaire GitHub est ouvert dans le navigateur. Rapport enregistré dans {path}.",
  "copied": "Le rapport complet est trop long pour l'adresse : il a été copié dans le presse-papiers, collez-le dans le formulaire.",
  "saved": "Rapport enregistré dans {path}.", "failed": "L'envoi a échoué : {error}. Le rapport est enregistré dans {path}.",
  "dup_title": "Ce problème a déjà été signalé",
  "dup_text": "Un signalement identique existe : #{number} « {title} ». Ajouter votre rapport à ce signalement plutôt que d'en créer un nouveau ?",
  "dup_similar": "Des signalements semblables existent. Ajouter votre rapport à l'un d'eux ?",
  "attach": "Ajouter à #{number}", "new": "Créer un nouveau signalement", "view": "Voir le signalement",
  "close": "Fermer", "nothing": "Aucun plantage récent à signaler.",
  "h_desc": "Description", "h_tech": "Détails techniques", "h_cause": "Cause probable", "h_machine": "Machine",
  "h_program": "Programme", "h_stack": "Pile d'appels", "h_journal": "Dernières lignes du journal",
  "h_comment": "Nouvelle occurrence", "no_desc": "(aucune description)",
  "footer": "Rapport créé avec « Signaler un problème » de Winlux. Données personnelles masquées : {redact}.",
  "yes": "oui", "no": "non",
  "c_missing": "Le programme a appelé la fonction Windows {func}, que Winlux ne fournit pas encore. Ce n'est pas une erreur de votre part : il faut l'ajouter à Winlux.",
  "c_internal": "Erreur interne de Winlux (et non du programme) : {detail}",
  "c_null": "Le programme a utilisé un pointeur vide (adresse 0x{addr}). Le plus souvent, une fonction Windows a renvoyé un résultat vide que le programme n'attendait pas.",
  "c_null_missing": "Juste avant, le journal signale des fonctions incomplètes : {funcs}. Ce sont les suspects les plus probables.",
  "c_access": "Le programme a lu ou écrit une zone mémoire interdite (adresse 0x{addr}).",
  "c_in_winlux": "La faute s'est produite dans le code de Winlux lui-même : c'est un bug de Winlux.",
  "c_in_dll": "La faute s'est produite dans la bibliothèque {module} chargée par le programme.",
  "c_in_exe": "La faute s'est produite dans le code du programme ({module}), souvent à cause d'une réponse inattendue de Winlux.",
  "c_stack": "Débordement de pile : récursion sans fin ou pile trop petite pour le programme.",
  "c_illegal": "Le processeur ne connaît pas une instruction utilisée par le programme.",
  "c_illegal_cpu": "Ce processeur ne gère pas {flags}, que le programme exige peut-être.",
  "c_break": "Le programme s'est arrêté volontairement (point d'arrêt, assertion ou protection anti-débogage).",
  "c_div": "Division par zéro dans le programme.",
  "c_unknown": "Cause non déterminée automatiquement ; les détails techniques ci-dessous aideront à l'analyse.",
  "c_manual": "Signalement manuel.",
 },
 "en": {
  "window": "Report a problem", "crashed": "\"{exe}\" stopped working.",
  "component_failed": "The Winlux component \"{exe}\" ran into an error.",
  "manual": "Describe the problem: the full report is shown below and nothing is sent without your consent.",
  "cause": "Probable cause", "summary": "One-line summary",
  "what": "What were you doing when the problem happened?",
  "redact": "Hide personal data (recommended)",
  "preview": "Full report that will be sent (editable)",
  "edited": "You edited the report: the fields above no longer apply to it.",
  "regenerate": "Regenerate", "cancel": "Don't send", "save": "Save", "send": "Send report",
  "via_relay": "Sent through the Winlux reporting service: no account needed.",
  "via_token": "Sent directly to GitHub with the token configured on this machine.",
  "via_browser": "The report will open in your browser on GitHub (a GitHub account is needed to submit it).",
  "via_file": "The report will only be saved on this computer.",
  "searching": "Looking for an identical report…", "sending": "Sending…",
  "sent": "Thank you! Report sent: {url}", "commented": "Thank you! Your report was added to the existing issue: {url}",
  "opened": "The GitHub form is open in your browser. Report saved to {path}.",
  "copied": "The full report is too long for the link: it was copied to the clipboard, paste it into the form.",
  "saved": "Report saved to {path}.", "failed": "Sending failed: {error}. The report is saved to {path}.",
  "dup_title": "This problem was already reported",
  "dup_text": "An identical report exists: #{number} \"{title}\". Add your report to it instead of creating a new one?",
  "dup_similar": "Similar reports exist. Add your report to one of them?",
  "attach": "Add to #{number}", "new": "Create a new report", "view": "View the report",
  "close": "Close", "nothing": "No recent crash to report.",
  "h_desc": "Description", "h_tech": "Technical details", "h_cause": "Probable cause", "h_machine": "Machine",
  "h_program": "Program", "h_stack": "Call stack", "h_journal": "Last log lines",
  "h_comment": "New occurrence", "no_desc": "(no description)",
  "footer": "Report created with Winlux \"Report a problem\". Personal data hidden: {redact}.",
  "yes": "yes", "no": "no",
  "c_missing": "The program called the Windows function {func}, which Winlux does not provide yet. This is not your fault: it needs to be added to Winlux.",
  "c_internal": "Internal Winlux error (not the program's): {detail}",
  "c_null": "The program used an empty pointer (address 0x{addr}). Most often a Windows function returned an empty result the program did not expect.",
  "c_null_missing": "Just before, the log reports incomplete functions: {funcs}. They are the most likely suspects.",
  "c_access": "The program read or wrote a forbidden memory area (address 0x{addr}).",
  "c_in_winlux": "The fault happened inside Winlux's own code: this is a Winlux bug.",
  "c_in_dll": "The fault happened in the library {module} loaded by the program.",
  "c_in_exe": "The fault happened in the program's code ({module}), often because of an unexpected answer from Winlux.",
  "c_stack": "Stack overflow: endless recursion or a stack too small for the program.",
  "c_illegal": "The processor does not know an instruction used by the program.",
  "c_illegal_cpu": "This processor lacks {flags}, which the program may require.",
  "c_break": "The program stopped itself on purpose (breakpoint, assertion or anti-debugging check).",
  "c_div": "Division by zero in the program.",
  "c_unknown": "The cause could not be determined automatically; the technical details below will help.",
  "c_manual": "Manual report.",
 },
 "es": {
  "window": "Informar de un problema", "crashed": "«{exe}» ha dejado de funcionar.",
  "component_failed": "El componente de Winlux «{exe}» ha sufrido un error.",
  "manual": "Describa el problema: el informe completo se muestra abajo y no se envía nada sin su permiso.",
  "cause": "Causa probable", "summary": "Resumen en una línea",
  "what": "¿Qué estaba haciendo cuando ocurrió el problema?",
  "redact": "Ocultar los datos personales (recomendado)",
  "preview": "Informe completo que se enviará (editable)",
  "edited": "Ha modificado el informe: los campos de arriba ya no se le aplican.",
  "regenerate": "Regenerar", "cancel": "No enviar", "save": "Guardar", "send": "Enviar el informe",
  "via_relay": "Envío mediante el servicio de informes de Winlux: no se necesita ninguna cuenta.",
  "via_token": "Envío directo a GitHub con el token configurado en este equipo.",
  "via_browser": "El informe se abrirá en su navegador en GitHub (se necesita una cuenta de GitHub para enviarlo).",
  "via_file": "El informe solo se guardará en este equipo.",
  "searching": "Buscando un informe idéntico…", "sending": "Enviando…",
  "sent": "¡Gracias! Informe enviado: {url}", "commented": "¡Gracias! Su informe se añadió al existente: {url}",
  "opened": "El formulario de GitHub está abierto en el navegador. Informe guardado en {path}.",
  "copied": "El informe completo es demasiado largo para el enlace: se ha copiado al portapapeles, péguelo en el formulario.",
  "saved": "Informe guardado en {path}.", "failed": "El envío ha fallado: {error}. El informe está guardado en {path}.",
  "dup_title": "Este problema ya se ha informado",
  "dup_text": "Existe un informe idéntico: #{number} «{title}». ¿Añadir su informe a este en lugar de crear uno nuevo?",
  "dup_similar": "Existen informes parecidos. ¿Añadir su informe a uno de ellos?",
  "attach": "Añadir a #{number}", "new": "Crear un informe nuevo", "view": "Ver el informe",
  "close": "Cerrar", "nothing": "No hay ningún fallo reciente que informar.",
  "h_desc": "Descripción", "h_tech": "Detalles técnicos", "h_cause": "Causa probable", "h_machine": "Equipo",
  "h_program": "Programa", "h_stack": "Pila de llamadas", "h_journal": "Últimas líneas del registro",
  "h_comment": "Nueva aparición", "no_desc": "(sin descripción)",
  "footer": "Informe creado con «Informar de un problema» de Winlux. Datos personales ocultos: {redact}.",
  "yes": "sí", "no": "no",
  "c_missing": "El programa llamó a la función de Windows {func}, que Winlux todavía no ofrece. No es culpa suya: hay que añadirla a Winlux.",
  "c_internal": "Error interno de Winlux (no del programa): {detail}",
  "c_null": "El programa usó un puntero vacío (dirección 0x{addr}). Casi siempre una función de Windows devolvió un resultado vacío que el programa no esperaba.",
  "c_null_missing": "Justo antes, el registro indica funciones incompletas: {funcs}. Son los sospechosos más probables.",
  "c_access": "El programa leyó o escribió una zona de memoria prohibida (dirección 0x{addr}).",
  "c_in_winlux": "El fallo se produjo en el propio código de Winlux: es un error de Winlux.",
  "c_in_dll": "El fallo se produjo en la biblioteca {module} cargada por el programa.",
  "c_in_exe": "El fallo se produjo en el código del programa ({module}), a menudo por una respuesta inesperada de Winlux.",
  "c_stack": "Desbordamiento de pila: recursión infinita o pila demasiado pequeña para el programa.",
  "c_illegal": "El procesador no conoce una instrucción usada por el programa.",
  "c_illegal_cpu": "Este procesador no admite {flags}, que el programa quizá necesite.",
  "c_break": "El programa se detuvo a propósito (punto de interrupción, aserción o protección antidepuración).",
  "c_div": "División por cero en el programa.",
  "c_unknown": "No se pudo determinar la causa automáticamente; los detalles técnicos de abajo ayudarán.",
  "c_manual": "Informe manual.",
 },
 "de": {
  "window": "Problem melden", "crashed": "„{exe}“ funktioniert nicht mehr.",
  "component_failed": "Die Winlux-Komponente „{exe}“ hat einen Fehler festgestellt.",
  "manual": "Beschreiben Sie das Problem: Der vollständige Bericht wird unten angezeigt, nichts wird ohne Ihre Zustimmung gesendet.",
  "cause": "Wahrscheinliche Ursache", "summary": "Zusammenfassung in einer Zeile",
  "what": "Was haben Sie gerade gemacht, als das Problem auftrat?",
  "redact": "Persönliche Daten ausblenden (empfohlen)",
  "preview": "Vollständiger Bericht, der gesendet wird (bearbeitbar)",
  "edited": "Sie haben den Bericht bearbeitet: Die Felder oben gelten nicht mehr dafür.",
  "regenerate": "Neu erstellen", "cancel": "Nicht senden", "save": "Speichern", "send": "Bericht senden",
  "via_relay": "Versand über den Winlux-Meldedienst: kein Konto erforderlich.",
  "via_token": "Direkter Versand an GitHub mit dem auf diesem Rechner eingerichteten Token.",
  "via_browser": "Der Bericht öffnet sich in Ihrem Browser auf GitHub (zum Absenden ist ein GitHub-Konto nötig).",
  "via_file": "Der Bericht wird nur auf diesem Computer gespeichert.",
  "searching": "Suche nach einer identischen Meldung…", "sending": "Wird gesendet…",
  "sent": "Danke! Meldung gesendet: {url}", "commented": "Danke! Ihr Bericht wurde der bestehenden Meldung hinzugefügt: {url}",
  "opened": "Das GitHub-Formular ist im Browser geöffnet. Bericht gespeichert unter {path}.",
  "copied": "Der vollständige Bericht ist für den Link zu lang: Er wurde in die Zwischenablage kopiert, fügen Sie ihn in das Formular ein.",
  "saved": "Bericht gespeichert unter {path}.", "failed": "Senden fehlgeschlagen: {error}. Der Bericht ist unter {path} gespeichert.",
  "dup_title": "Dieses Problem wurde bereits gemeldet",
  "dup_text": "Es gibt eine identische Meldung: #{number} „{title}“. Ihren Bericht dort hinzufügen, statt eine neue zu erstellen?",
  "dup_similar": "Es gibt ähnliche Meldungen. Ihren Bericht zu einer davon hinzufügen?",
  "attach": "Zu #{number} hinzufügen", "new": "Neue Meldung erstellen", "view": "Meldung ansehen",
  "close": "Schließen", "nothing": "Kein aktueller Absturz zu melden.",
  "h_desc": "Beschreibung", "h_tech": "Technische Details", "h_cause": "Wahrscheinliche Ursache", "h_machine": "Rechner",
  "h_program": "Programm", "h_stack": "Aufrufliste", "h_journal": "Letzte Protokollzeilen",
  "h_comment": "Erneutes Auftreten", "no_desc": "(keine Beschreibung)",
  "footer": "Bericht erstellt mit „Problem melden“ von Winlux. Persönliche Daten ausgeblendet: {redact}.",
  "yes": "ja", "no": "nein",
  "c_missing": "Das Programm hat die Windows-Funktion {func} aufgerufen, die Winlux noch nicht bereitstellt. Das ist nicht Ihr Fehler: Sie muss in Winlux ergänzt werden.",
  "c_internal": "Interner Winlux-Fehler (nicht des Programms): {detail}",
  "c_null": "Das Programm hat einen leeren Zeiger verwendet (Adresse 0x{addr}). Meist hat eine Windows-Funktion ein leeres Ergebnis geliefert, mit dem das Programm nicht rechnete.",
  "c_null_missing": "Kurz davor meldet das Protokoll unvollständige Funktionen: {funcs}. Sie sind die wahrscheinlichsten Verdächtigen.",
  "c_access": "Das Programm hat einen verbotenen Speicherbereich gelesen oder beschrieben (Adresse 0x{addr}).",
  "c_in_winlux": "Der Fehler trat im Code von Winlux selbst auf: Das ist ein Winlux-Fehler.",
  "c_in_dll": "Der Fehler trat in der vom Programm geladenen Bibliothek {module} auf.",
  "c_in_exe": "Der Fehler trat im Code des Programms auf ({module}), oft wegen einer unerwarteten Antwort von Winlux.",
  "c_stack": "Stapelüberlauf: endlose Rekursion oder zu kleiner Stapel für das Programm.",
  "c_illegal": "Der Prozessor kennt eine vom Programm verwendete Anweisung nicht.",
  "c_illegal_cpu": "Diesem Prozessor fehlt {flags}, was das Programm möglicherweise benötigt.",
  "c_break": "Das Programm hat sich absichtlich angehalten (Haltepunkt, Assertion oder Anti-Debugging-Prüfung).",
  "c_div": "Division durch null im Programm.",
  "c_unknown": "Die Ursache ließ sich nicht automatisch bestimmen; die technischen Details unten helfen bei der Analyse.",
  "c_manual": "Manuelle Meldung.",
 },
}


def lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in Path("/etc/winlux/locale.conf").read_text().splitlines():
                if line.startswith("LANG="):
                    code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in T else "fr"


def tr(key, language=None, **values):
    text = T[language or lang()].get(key) or T["fr"][key]
    return text.format(**values) if values else text


# ── Réglages ─────────────────────────────────────────────────────────────

SETTINGS = {"offer": ("OFFER", "on", ("on", "off")),
            "redact": ("REDACT", "on", ("on", "off")),
            "mode": ("MODE", "auto", ("auto", "browser", "file"))}


def read_conf(path):
    values = {}
    try:
        for line in Path(path).read_text().splitlines():
            if "=" in line and not line.lstrip().startswith("#"):
                key, value = line.split("=", 1)
                values[key.strip()] = value.strip()
    except OSError:
        pass
    return values


def config():
    values = read_conf(DEFAULTS)
    values.update(read_conf(CONF))
    for key, (name, default, allowed) in SETTINGS.items():
        if values.get(name) not in allowed:
            values[name] = default
    return values


def set_value(assignment):
    key, _, value = assignment.partition("=")
    if key not in SETTINGS or value not in SETTINGS[key][2]:
        raise SystemExit(f"winlux-report: réglage invalide : {assignment}")
    name = SETTINGS[key][0]
    lines = []
    try:
        lines = [l for l in CONF.read_text().splitlines() if not l.startswith(name + "=")]
    except OSError:
        pass
    lines.append(f"{name}={value}")
    CONF.parent.mkdir(parents=True, exist_ok=True)
    tmp = CONF.with_name(CONF.name + f".{os.getpid()}")
    tmp.write_text("\n".join(lines) + "\n")
    tmp.chmod(0o644)
    tmp.replace(CONF)


def transport(conf=None):
    conf = conf or config()
    if conf["MODE"] == "file":
        return "file"
    if conf["MODE"] == "auto":
        if conf.get("RELAY", "").startswith("https://"):
            return "relay"
        token = conf.get("TOKEN_FILE", "")
        if token and os.access(token, os.R_OK):
            return "token"
    return "browser"


def status():
    conf = config()
    print(f"report_offer={conf['OFFER']}")
    print(f"report_redact={conf['REDACT']}")
    print(f"report_mode={conf['MODE']}")
    print(f"report_transport={transport(conf)}")
    print(f"report_relay={1 if conf.get('RELAY', '').startswith('https://') else 0}")


# ── Lecture des plantages ────────────────────────────────────────────────

FIELD = re.compile(r"(\w+)=(\S+)")


def parse_crash_log(text):
    """Entrées [CRASH] de crash.log, avec leurs lignes indentées."""
    entries = []
    for line in text.splitlines():
        if line.startswith("[CRASH]"):
            entry = dict(FIELD.findall(line))
            entry.update(frames=[], details=[], regs={})
            entries.append(entry)
        elif entries and line.startswith("  "):
            entry, body = entries[-1], line.strip()
            if body.startswith("frame#"):
                entry["frames"].append(body.split(" ", 1)[1] if " " in body else body)
            elif body.startswith("detail="):
                entry["details"].append(body[7:])
            elif body.startswith("path="):
                entry["path"] = body[5:]
            else:
                entry["regs"].update(FIELD.findall(body))
    return entries


def crashes():
    try:
        return parse_crash_log((JOURNAL / "crash.log").read_text(errors="replace"))
    except OSError:
        return []


def last_crash(since=0):
    entries = [e for e in crashes() if int(e.get("ts", "0") or 0) >= since]
    return entries[-1] if entries else None


def journal_tail(limit=30):
    lines = []
    for path in (JOURNAL / "winlux.log", STATE / "last-launch.log"):
        try:
            lines += path.read_text(errors="replace").splitlines()[-limit:]
        except OSError:
            pass
    return [l for l in lines if l.strip()][-limit:]


MISSING = re.compile(r"(\w[\w.]*![\w@?$]+)")


def missing_functions(lines, crash=None):
    found = []
    for line in lines:
        low = line.lower()
        if "non implément" in low or "manquant" in low or "not implemented" in low or "stub" in low:
            found += [m for m in MISSING.findall(line) if m not in found]
    for detail in (crash or {}).get("details", []):
        if detail.startswith("fonction="):
            func = detail[9:]
            if func not in found:
                found.insert(0, func)
    return found[:5]


def probable_cause(crash, lines, language=None, cpu_flags=None):
    if not crash:
        return tr("c_manual", language)
    exc = crash.get("exception", "")
    module = crash.get("module", "inconnu")
    parts = []
    if exc == "ENTRYPOINT_NOT_FOUND":
        funcs = missing_functions(lines, crash) or ["?"]
        return tr("c_missing", language, func=funcs[0])
    if exc == "WINLUX_INTERNAL_ERROR":
        detail = (crash.get("details") or ["?"])[0]
        return tr("c_internal", language, detail=detail)
    addr = crash.get("regs", {}).get("addr", "0").removeprefix("0x").lstrip("0") or "0"
    if exc == "EXCEPTION_ACCESS_VIOLATION":
        if int(addr, 16) < 0x10000:
            parts.append(tr("c_null", language, addr=addr))
            funcs = missing_functions(lines)
            if funcs:
                parts.append(tr("c_null_missing", language, funcs=", ".join(funcs)))
        else:
            parts.append(tr("c_access", language, addr=addr))
    elif exc == "EXCEPTION_STACK_OVERFLOW":
        parts.append(tr("c_stack", language))
    elif exc == "EXCEPTION_ILLEGAL_INSTRUCTION":
        parts.append(tr("c_illegal", language))
        flags = cpu_flags if cpu_flags is not None else cpu_info().get("flags", set())
        lacking = [f for f in ("avx2", "avx512f", "sse4_2") if f not in flags]
        if flags and lacking:
            parts.append(tr("c_illegal_cpu", language, flags=", ".join(lacking).upper()))
    elif exc == "EXCEPTION_BREAKPOINT":
        parts.append(tr("c_break", language))
    elif exc == "EXCEPTION_INT_DIVIDE_BY_ZERO":
        parts.append(tr("c_div", language))
    else:
        parts.append(tr("c_unknown", language))
    name = module.split("+", 1)[0]
    if exc in ("EXCEPTION_ACCESS_VIOLATION", "EXCEPTION_ILLEGAL_INSTRUCTION", "EXCEPTION_INT_DIVIDE_BY_ZERO"):
        if name == "winlux":
            parts.append(tr("c_in_winlux", language))
        elif name.lower().endswith(".dll"):
            parts.append(tr("c_in_dll", language, module=name))
        elif name and name != "inconnu":
            parts.append(tr("c_in_exe", language, module=module))
    return " ".join(parts)


def signature(crash):
    """Empreinte stable d'un plantage, indépendante de la langue et des adresses
    de pile : même programme, même exception, même emplacement fautif."""
    if not crash:
        return ""
    module = crash.get("module", "")
    if "+0x" in module:
        name, offset = module.split("+0x", 1)
        try:
            module = f"{name.lower()}+0x{int(offset, 16) & ~0xF:x}"
        except ValueError:
            pass
    key = "|".join([crash.get("exe", "").lower(), crash.get("exception", ""), module]
                   + [d for d in crash.get("details", []) if d.startswith("fonction=")][:1])
    return "wlx-" + hashlib.sha256(key.encode()).hexdigest()[:12]


# ── Machine ──────────────────────────────────────────────────────────────

def run(cmd, timeout=3):
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return out.stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return ""


def cpu_info():
    info = {"model": platform.processor() or "?", "threads": os.cpu_count() or 0, "flags": set()}
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            key, _, value = line.partition(":")
            key = key.strip()
            if key == "model name":
                info["model"] = value.strip()
            elif key == "flags" and not info["flags"]:
                info["flags"] = set(value.split())
    except OSError:
        pass
    return info


def gpu_info():
    gpus = []
    text = run(["lspci", "-nnk"])
    current = None
    for line in text.splitlines():
        if re.search(r"VGA compatible|3D controller|Display controller", line):
            current = line.split(": ", 1)[-1]
            gpus.append(current)
        elif current and "Kernel driver in use" in line:
            gpus[-1] = f"{current} — pilote {line.split(':', 1)[1].strip()}"
            current = None
    if not gpus:
        for card in sorted(Path("/sys/class/drm").glob("card[0-9]")):
            dev = card / "device"
            try:
                vendor = (dev / "vendor").read_text().strip()
                device = (dev / "device").read_text().strip()
                driver = os.path.basename(os.readlink(dev / "driver"))
                gpus.append(f"{vendor}:{device} — pilote {driver}")
            except OSError:
                pass
    renderer = ""
    glx = run(["glxinfo", "-B"])
    for line in glx.splitlines():
        if "OpenGL renderer" in line or "OpenGL version" in line:
            renderer += line.split(":", 1)[1].strip() + " ; "
    return gpus or ["?"], renderer.strip(" ;")


def memory():
    values = {}
    try:
        for line in Path("/proc/meminfo").read_text().splitlines():
            key, _, rest = line.partition(":")
            if key in ("MemTotal", "MemAvailable", "SwapTotal"):
                values[key] = int(rest.split()[0]) // 1024
    except (OSError, ValueError, IndexError):
        pass
    return values


def winlux_version():
    info = read_conf("/usr/share/winlux/BUILD-INFO")
    version = info.get("version") or run(["winlux", "--version"]).removeprefix("winlux ").strip()
    if not version:
        try:
            version = (Path(__file__).resolve().parent.parent / "VERSION").read_text().strip()
        except OSError:
            version = "?"
    commit = info.get("commit", "")[:10]
    return f"{version}" + (f" ({commit})" if commit and commit != "unknown" else "")


def session_type():
    if os.environ.get("WINLUX_DESKTOP_DEMO") or Path("/run/winlux/desktop-shell").exists():
        desktop = "Bureau Winlux (shell)"
    else:
        desktop = os.environ.get("XDG_CURRENT_DESKTOP") or "?"
    display = "Wayland" if os.environ.get("WAYLAND_DISPLAY") else "X11" if os.environ.get("DISPLAY") else "console"
    try:
        live = "rootfstype=ramfs" in Path("/proc/cmdline").read_text()
    except OSError:
        live = False
    return f"{desktop}, {display}, {'Live' if live else 'installé'}"


def machine_info():
    cpu = cpu_info()
    gpus, renderer = gpu_info()
    mem = memory()
    os_release = read_conf("/etc/os-release").get("PRETTY_NAME", "").strip('"')
    virt = run(["systemd-detect-virt"]) or read_text("/sys/class/dmi/id/sys_vendor")
    resolution = ""
    for line in run(["xrandr", "--current"]).splitlines():
        if "*" in line:
            resolution = line.split()[0]
            break
    rows = [
        ("Winlux", winlux_version()),
        ("Noyau / Kernel", platform.release()),
        ("Base", os_release or "?"),
        ("Processeur / CPU", f"{cpu['model']} ({cpu['threads']} threads)"
         + (" AVX2" if "avx2" in cpu["flags"] else "") + (" AVX-512" if "avx512f" in cpu["flags"] else "")),
        ("GPU", " | ".join(gpus)),
        ("OpenGL", renderer or "?"),
        ("Mémoire / RAM", f"{mem.get('MemTotal', '?')} Mio, {mem.get('MemAvailable', '?')} Mio libres, swap {mem.get('SwapTotal', '?')} Mio"),
        ("Session", session_type()),
        ("Écran / Display", resolution or "?"),
        ("Virtualisation", virt.strip() or "none"),
        ("Langue / Language", lang()),
    ]
    return rows, cpu["flags"]


def read_text(path):
    try:
        return Path(path).read_text(errors="replace").strip()
    except OSError:
        return ""


# ── Informations sur le .exe ─────────────────────────────────────────────

def pe_version(path):
    """Version de fichier (VS_FIXEDFILEINFO) et nom de produit d'un PE."""
    try:
        data = Path(path).read_bytes()[:64 * 1024 * 1024]
    except OSError:
        return {}
    info = {"size": len(data), "sha256": hashlib.sha256(data).hexdigest()[:16]}
    at = data.find(b"\xbd\x04\xef\xfe")
    if at >= 0 and at + 16 <= len(data):
        ms, ls = int.from_bytes(data[at + 8:at + 12], "little"), int.from_bytes(data[at + 12:at + 16], "little")
        info["version"] = f"{ms >> 16}.{ms & 0xFFFF}.{ls >> 16}.{ls & 0xFFFF}"
    for key in ("ProductName", "CompanyName", "FileDescription"):
        marker = key.encode("utf-16-le") + b"\0\0"
        pos = data.find(marker)
        if pos < 0:
            continue
        pos += len(marker)
        pos += pos % 4  # alignement DWORD de la valeur
        end = data.find(b"\0\0", pos)
        while end > 0 and (end - pos) % 2:
            end = data.find(b"\0\0", end + 1)
        if end > pos:
            value = data[pos:end].decode("utf-16-le", "replace").strip("\0 ")
            if value and len(value) < 120:
                info[key] = value
    if data[:2] == b"MZ" and len(data) > 0x40:
        pe = int.from_bytes(data[0x3C:0x40], "little")
        if data[pe:pe + 4] == b"PE\0\0":
            machine = int.from_bytes(data[pe + 4:pe + 6], "little")
            info["machine"] = {0x8664: "x86-64", 0x14C: "x86 (32 bits)", 0xAA64: "ARM64"}.get(machine, hex(machine))
    kind = run(["winlux", "--inspect", str(path)])
    if kind:
        info["kind"] = kind
    return info


# ── Masquage des données personnelles ────────────────────────────────────

@functools.lru_cache(maxsize=1)
def private_values():
    """Valeurs propres à cette machine et à son utilisateur."""
    values = {}
    home = str(Path.home())
    user = os.environ.get("USER") or os.environ.get("LOGNAME") or Path.home().name
    host = socket.gethostname()
    if user and user not in ("root",) and len(user) > 1:
        values[user] = "<utilisateur>"
    if host and host not in ("localhost", "winlux") and len(host) > 2:
        values[host] = "<machine>"
    for name in ("product_serial", "board_serial", "chassis_serial", "product_uuid"):
        serial = read_text(f"/sys/class/dmi/id/{name}")
        if len(serial) > 3 and serial.lower() not in ("none", "default string", "to be filled by o.e.m."):
            values[serial] = "<numéro-de-série>"
    machine_id = read_text("/etc/machine-id")
    if machine_id:
        values[machine_id] = "<identifiant-machine>"
    ssids = run(["nmcli", "-t", "-f", "NAME", "connection", "show"]).splitlines()
    ssids += [p.stem for p in Path("/etc/NetworkManager/system-connections").glob("*.nmconnection")] \
        if os.access("/etc/NetworkManager/system-connections", os.R_OK) else []
    ssids.append(run(["iwgetid", "-r"]))
    for ssid in ssids:
        ssid = ssid.strip()
        if len(ssid) > 2 and ssid.lower() not in ("lo", "wired connection 1", "connexion filaire 1"):
            values[ssid] = "<réseau-wifi>"
    return home, tuple(values.items())


EMAIL = re.compile(r"[\w.+-]+@[\w-]+\.[\w.-]+")
MAC = re.compile(r"\b(?:[0-9a-fA-F]{2}[:-]){5}[0-9a-fA-F]{2}\b")
IPV4 = re.compile(r"\b(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b")
TOKEN = re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_\w{20,}|[A-Za-z0-9_-]{40,})")
HOME_PATH = re.compile(r"(?:/home/[^/\s`]+|/root|/media/[^/\s`]+/[^/\s`]+|/run/media/[^/\s`]+/[^/\s`]+|"
                       r"[A-Za-z]:\\\\?(?:[Uu]sers|home)\\\\?[^\\\s`]+)((?:[/\\][^\s\"'`<>|]*"
                       r"(?:\s(?=[^\s\"'`<>|/\\]*\.\w{1,6}\b)[^\s\"'`<>|]*)*)?)")


def redact(text, keep_names=(), home=None, values=None, keep_strings=()):
    """Masque chemins personnels, noms de fichiers, identifiants, SSID,
    numéros de série, adresses e-mail/MAC/IP et jetons. `keep_names` : noms de
    fichiers à conserver (le .exe concerné, utile au diagnostic)."""
    if home is None or values is None:
        home, found = private_values()
        values = dict(found)
    keep = {n.lower() for n in keep_names if n}
    # Valeurs utiles au diagnostic qui ressemblent à des données masquées
    # (une version 1.0.0.0 a la forme d'une adresse IP).
    protected = {}
    for i, value in enumerate(v for v in keep_strings if v):
        marker = f"\x00{i}\x00"
        protected[marker] = value
        text = text.replace(value, marker)

    def path(match):
        rest = match.group(1) or ""
        parts = [p for p in re.split(r"[/\\]", rest) if p]
        if not parts:
            return "~"
        last = parts[-1]
        if last.lower() in keep:
            shown = last
        else:
            ext = os.path.splitext(last)[1]
            shown = f"<fichier>{ext}" if len(ext) <= 6 else "<fichier>"
        return "~/" + ("…/" if len(parts) > 1 else "") + shown

    if home and home not in ("/", "/root"):
        text = text.replace(home, "/home/<utilisateur>")
    text = HOME_PATH.sub(path, text)
    text = TOKEN.sub("<jeton>", text)
    text = EMAIL.sub("<e-mail>", text)
    text = MAC.sub("<adresse-mac>", text)
    text = IPV4.sub(lambda m: m.group(0) if m.group(0).startswith(("127.", "0.")) else "<adresse-ip>", text)
    for secret, label in sorted(values.items(), key=lambda kv: -len(kv[0])):
        text = re.sub(r"(?<![\w-])" + re.escape(secret) + r"(?![\w-])", label, text)
    for marker, value in protected.items():
        text = text.replace(marker, value)
    return text


# ── Construction du rapport ──────────────────────────────────────────────

class Report:
    def __init__(self, crash=None, component=None, language=None):
        self.language = language or lang()
        self.crash = crash
        self.component = component
        self.journal = journal_tail()
        self.machine, flags = machine_info()
        self.cause = probable_cause(crash, self.journal, self.language, flags)
        self.signature = signature(crash)
        path = (crash or {}).get("path", "")
        self.exe_name = (crash or {}).get("exe", "") or component or ""
        self.exe = pe_version(path) if path.lower().endswith((".exe", ".dll", ".com", ".scr")) else {}

    def kind(self):
        if not self.crash:
            return "manual"
        exc = self.crash.get("exception", "")
        if exc == "ENTRYPOINT_NOT_FOUND":
            return "missing"
        if exc == "WINLUX_INTERNAL_ERROR" or self.component:
            return "internal"
        return "crash"

    def headline(self):
        if self.component:
            return tr("component_failed", self.language, exe=self.component)
        if self.crash:
            return tr("crashed", self.language, exe=self.exe_name or "?")
        return tr("manual", self.language)

    def title(self, summary=""):
        c = self.crash or {}
        version = self.exe.get("version", "")
        exe = f"{self.exe_name} {version}".strip()
        kind = self.kind()
        if kind == "missing":
            funcs = missing_functions(self.journal, c) or ["?"]
            title = f"[Fonction manquante] {exe} : {funcs[0]}"
        elif kind == "internal":
            title = f"[Erreur interne] {self.component or 'winlux'} : {(c.get('details') or [c.get('exception', '?')])[0][:80]}"
        elif kind == "crash":
            title = f"[Plantage] {exe} : {c.get('exception', '?')} dans {c.get('module', '?').split('+')[0]}"
        else:
            title = "[Problème] " + (summary.strip().splitlines()[0][:100] if summary.strip() else "…")
        if summary.strip() and kind != "manual":
            title += f" — {summary.strip().splitlines()[0][:60]}"
        return title

    def labels(self):
        kind = self.kind()
        labels = ["bug", "signalement-utilisateur"]
        labels.append({"crash": "plantage", "missing": "fonction-manquante",
                       "internal": "plantage", "manual": "à-trier"}[kind])
        if kind in ("crash", "missing"):
            labels.append("compatibilité-windows")
        if kind == "internal" or self.component:
            labels.append("runtime" if not self.component else "bureau")
        return labels

    def technical(self):
        c = self.crash or {}
        lines = []
        if c:
            rows = [("Exception", c.get("exception", "?")), ("Code", c.get("code", "?")),
                    ("Signal", c.get("sig", "?")), ("Stop code", c.get("stop", "?")),
                    ("Module", c.get("module", "?")),
                    ("Date (UTC)", time.strftime("%Y-%m-%d %H:%M:%S", time.gmtime(int(c.get("ts", "0") or 0))))]
            rows += [(k, v) for k, v in c.get("regs", {}).items()]
            rows += [("Détail", d) for d in c.get("details", [])]
            lines += [f"| {k} | `{v}` |" for k, v in rows]
        return lines

    def safe_strings(self):
        return [self.exe.get("version", ""), winlux_version(), platform.release()]

    def body(self, description="", redacted=True, comment=False):
        L = self.language
        out = []
        if comment:
            out.append(f"### {tr('h_comment', L)}")
        out += [f"### {tr('h_desc', L)}", "", description.strip() or tr("no_desc", L), ""]
        out += [f"### {tr('h_cause', L)}", "", self.cause, ""]
        if self.crash:
            out += [f"### {tr('h_tech', L)}", "", "| | |", "|---|---|"] + self.technical() + [""]
            if self.signature:
                out += [f"Signature : `{self.signature}`", ""]
        if self.exe_name and self.crash:
            names = {"size": "Taille / Size", "sha256": "SHA-256 (16)", "version": "Version",
                     "machine": "Architecture", "kind": "Type", "ProductName": "Produit / Product",
                     "CompanyName": "Éditeur / Publisher", "FileDescription": "Description"}
            out += [f"### {tr('h_program', L)}", "", "| | |", "|---|---|", f"| Fichier / File | `{self.exe_name}` |"]
            out += [f"| {names.get(k, k)} | {v} |" for k, v in self.exe.items()]
            if self.crash.get("path"):
                out.append(f"| Chemin / Path | `{self.crash['path']}` |")
            out.append("")
        if not comment:
            out += [f"### {tr('h_machine', L)}", "", "| | |", "|---|---|"]
            out += [f"| {k} | {v} |" for k, v in self.machine] + [""]
        if self.crash and self.crash.get("frames"):
            out += [f"<details><summary>{tr('h_stack', L)}</summary>", "", "```"]
            out += [f"#{i} {f}" for i, f in enumerate(self.crash["frames"])] + ["```", "", "</details>", ""]
        if self.journal:
            out += [f"<details><summary>{tr('h_journal', L)}</summary>", "", "```"]
            out += [l[:300].replace("```", "'''") for l in self.journal] + ["```", "", "</details>", ""]
        out += ["---", "_" + tr("footer", L, redact=tr("yes" if redacted else "no", L)) + "_"]
        text = "\n".join(out)
        if redacted:
            text = redact(text, keep_names=[self.exe_name], keep_strings=self.safe_strings())
        return text[:BODY_LIMIT]


# ── GitHub ───────────────────────────────────────────────────────────────

def http(method, url, payload=None, token=None, timeout=20):
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("User-Agent", "winlux-report")
    if data is not None:
        req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=timeout) as response:
        return json.loads(response.read().decode() or "{}")


def find_duplicates(report, summary="", token=None):
    """Signalement identique (même signature) ou, à défaut, semblables par titre.
    Recherche publique, non authentifiée : seule la signature ou les mots du
    résumé quittent la machine, au moment où l'utilisateur clique Envoyer."""
    if report.signature:
        query = f'repo:{REPO} is:issue "{report.signature}"'
    else:
        words = [w for w in re.findall(r"\w{4,}", summary.lower())][:6]
        if not words:
            return "none", []
        query = f"repo:{REPO} is:issue is:open in:title " + " ".join(words)
    url = f"{API}/search/issues?per_page=5&q=" + urllib.parse.quote(query)
    try:
        items = http("GET", url, token=token).get("items", [])
    except (OSError, ValueError, urllib.error.URLError):
        return "none", []
    issues = [{"number": i["number"], "title": i["title"], "url": i["html_url"], "state": i["state"]}
              for i in items if "pull_request" not in i]
    if not issues:
        return "none", []
    return ("identical" if report.signature else "similar"), issues


def send_api(conf, title, body, labels, issue=None):
    kind = transport(conf)
    if kind == "relay":
        payload = {"title": title, "body": body, "labels": labels, "issue": issue}
        return http("POST", conf["RELAY"].rstrip("/") + "/report", payload).get("url", "")
    token = Path(conf["TOKEN_FILE"]).read_text().strip()
    if issue:
        return http("POST", f"{API}/repos/{REPO}/issues/{issue}/comments", {"body": body}, token)["html_url"]
    return http("POST", f"{API}/repos/{REPO}/issues", {"title": title, "body": body, "labels": labels}, token)["html_url"]


def browser_url(title, body, labels, issue=None):
    """Formulaire prérempli. Les étiquettes du lien ne s'appliquent que pour les
    membres du dépôt ; le modèle rapport-winlux.md les pose pour les autres."""
    if issue:
        return f"https://github.com/{REPO}/issues/{issue}#new_comment_field", False
    base = f"https://github.com/{REPO}/issues/new?template=rapport-winlux.md&labels=" + \
        urllib.parse.quote(",".join(labels)) + "&title=" + urllib.parse.quote(title) + "&body="
    full = base + urllib.parse.quote(body)
    if len(full) <= URL_LIMIT:
        return full, False
    cut = body
    while len(base + urllib.parse.quote(cut)) > URL_LIMIT - 300 and cut:
        cut = cut[: int(len(cut) * 0.8)]
    note = "\n\n…\n\n> Rapport complet copié dans le presse-papiers : collez-le ici."
    return base + urllib.parse.quote(cut + note), True


def open_url(url):
    for opener in (["xdg-open", url], ["winlux-browser", url]):
        if shutil.which(opener[0]):
            try:
                subprocess.Popen(opener, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
                return True
            except OSError:
                continue
    return False


def save_report(title, body):
    directory = STATE / "reports"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / time.strftime("rapport-%Y%m%d-%H%M%S.md")
    path.write_text(f"# {title}\n\n{body}\n")
    return path


def mark_offered(crash):
    """Une seule proposition par plantage, même si le bureau et le lanceur
    réagissent tous les deux."""
    if not crash:
        return True
    marker = RUN / "report-offered"
    key = f"{crash.get('ts')}-{crash.get('pid')}"
    try:
        if marker.read_text().strip() == key:
            return False
    except OSError:
        pass
    try:
        marker.write_text(key + "\n")
    except OSError:
        pass
    try:
        (RUN / "crash-offer").unlink()
    except OSError:
        pass
    return True


# ── Interface ────────────────────────────────────────────────────────────

def gui(report, conf):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gtk, Gdk, GLib

    L = report.language
    win = Gtk.Window(title=f"{tr('window', L)} — Winlux")
    win.set_icon_name("dialog-warning" if report.crash else "help-browser")
    display = Gdk.Display.get_default()
    monitor = display.get_primary_monitor() or display.get_monitor(0) if display else None
    area = monitor.get_workarea() if monitor else None
    width, height = (area.width, area.height) if area else (1280, 800)
    win.set_default_size(min(900, int(width * 0.8)), min(760, int(height * 0.85)))
    win.set_position(Gtk.WindowPosition.CENTER)
    win.set_keep_above(True)
    win.connect("destroy", Gtk.main_quit)
    css = Gtk.CssProvider()
    css.load_from_data(b"""
      window { background: #1f2230; color: #eceef6; }
      textview, textview text, entry { background: #161924; color: #eceef6; }
      #cause { background: #2a2f42; border-radius: 6px; padding: 10px; }
      #headline { font-size: 15pt; font-weight: bold; }
      button { padding: 8px 14px; }
      button.suggested-action { background: #4a6cf7; color: white; }
    """)
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=18)
    win.add(box)
    headline = Gtk.Label(label=report.headline(), xalign=0, wrap=True)
    headline.set_name("headline")
    box.pack_start(headline, False, False, 0)
    if report.crash:
        cause = Gtk.Label(label=f"{tr('cause', L)}{' :' if L == 'fr' else ':'} {report.cause}", xalign=0, wrap=True)
        cause.set_name("cause")
        box.pack_start(cause, False, False, 0)

    box.pack_start(Gtk.Label(label=tr("summary", L), xalign=0), False, False, 0)
    summary = Gtk.Entry()
    box.pack_start(summary, False, False, 0)
    box.pack_start(Gtk.Label(label=tr("what", L), xalign=0), False, False, 0)
    desc_view = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR)
    desc_view.set_size_request(-1, 90)
    desc_scroll = Gtk.ScrolledWindow()
    desc_scroll.add(desc_view)
    box.pack_start(desc_scroll, False, True, 0)

    mask = Gtk.CheckButton(label=tr("redact", L))
    mask.set_active(conf["REDACT"] == "on")
    box.pack_start(mask, False, False, 0)

    expander = Gtk.Expander(label=tr("preview", L))
    preview_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
    title_entry = Gtk.Entry()
    preview_box.pack_start(title_entry, False, False, 0)
    body_view = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR, monospace=True)
    body_scroll = Gtk.ScrolledWindow()
    body_scroll.set_size_request(-1, 200)
    body_scroll.set_vexpand(True)
    body_scroll.add(body_view)
    preview_box.pack_start(body_scroll, True, True, 0)
    edited_row = Gtk.Box(spacing=8)
    edited_label = Gtk.Label(label=tr("edited", L), xalign=0, wrap=True)
    regen = Gtk.Button(label=tr("regenerate", L))
    edited_row.pack_start(edited_label, True, True, 0)
    edited_row.pack_end(regen, False, False, 0)
    preview_box.pack_start(edited_row, False, False, 0)
    expander.add(preview_box)
    expander.set_expanded(True)
    box.pack_start(expander, True, True, 0)

    kind = transport(conf)
    via = Gtk.Label(label=tr("via_" + kind, L), xalign=0, wrap=True)
    box.pack_start(via, False, False, 0)
    notice = Gtk.Label(label="", xalign=0, wrap=True, selectable=True)
    box.pack_start(notice, False, False, 0)
    link = Gtk.LinkButton(uri="https://github.com", label=tr("view", L))
    link.set_no_show_all(True)
    box.pack_start(link, False, False, 0)

    buttons = Gtk.Box(spacing=8)
    cancel = Gtk.Button(label=tr("cancel", L))
    save = Gtk.Button(label=tr("save", L))
    send = Gtk.Button(label=tr("send", L))
    send.get_style_context().add_class("suggested-action")
    buttons.pack_end(send, False, False, 0)
    buttons.pack_end(save, False, False, 0)
    buttons.pack_start(cancel, False, False, 0)
    box.pack_end(buttons, False, False, 0)

    state = {"edited": False, "updating": False}

    def description():
        buf = desc_view.get_buffer()
        return buf.get_text(buf.get_start_iter(), buf.get_end_iter(), True)

    def body_text():
        buf = body_view.get_buffer()
        return buf.get_text(buf.get_start_iter(), buf.get_end_iter(), True)

    def regenerate(*_):
        state["updating"] = True
        title = report.title(summary.get_text())
        title_entry.set_text(redact(title, [report.exe_name], keep_strings=report.safe_strings())
                             if mask.get_active() else title)
        body_view.get_buffer().set_text(report.body(description(), mask.get_active()))
        state["updating"] = False
        state["edited"] = False
        edited_row.hide()

    def on_body_changed(*_):
        if not state["updating"]:
            state["edited"] = True
            edited_row.show_all()

    def on_inputs_changed(*_):
        if not state["edited"]:
            regenerate()

    body_view.get_buffer().connect("changed", on_body_changed)
    desc_view.get_buffer().connect("changed", on_inputs_changed)
    summary.connect("changed", on_inputs_changed)
    mask.connect("toggled", lambda *_: regenerate() if not state["edited"] else on_masked_edit())
    regen.connect("clicked", regenerate)

    def on_masked_edit():
        # Le texte a été modifié : on masque ce qui est affiché sans le réécrire.
        if mask.get_active():
            state["updating"] = True
            body_view.get_buffer().set_text(redact(body_text(), [report.exe_name],
                                                   keep_strings=report.safe_strings()))
            state["updating"] = False

    def finish(message, url=None):
        notice.set_text(message)
        for widget in (send, save, summary, desc_view, body_view, mask):
            widget.set_sensitive(False)
        cancel.set_label(tr("close", L))
        if url:
            link.set_uri(url)
            link.show()

    def busy(message):
        notice.set_text(message)
        send.set_sensitive(False)
        save.set_sensitive(False)

    def on_save(*_):
        path = save_report(title_entry.get_text(), body_text())
        notice.set_text(tr("saved", L, path=path))

    def deliver(issue=None):
        title, body, labels = title_entry.get_text(), body_text(), report.labels()
        if issue:
            body = f"### {tr('h_comment', L)}\n\n" + body
        path = save_report(title, body)
        if kind in ("relay", "token"):
            busy(tr("sending", L))

            def work():
                try:
                    url = send_api(conf, title, body, labels, issue)
                    GLib.idle_add(finish, tr("commented" if issue else "sent", L, url=url), url)
                except (OSError, ValueError, KeyError, urllib.error.URLError) as error:
                    GLib.idle_add(fallback, title, body, labels, issue, path, str(error))
            threading.Thread(target=work, daemon=True).start()
        else:
            fallback(title, body, labels, issue, path, None)

    def fallback(title, body, labels, issue, path, error):
        url, truncated = browser_url(title, body, labels, issue)
        if truncated or issue:
            Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text(body, -1)
        opened = open_url(url)
        message = tr("opened", L, path=path) if opened else tr("saved", L, path=path)
        if error:
            message = tr("failed", L, error=error, path=path) + " " + message
        if truncated or issue:
            message += " " + tr("copied", L)
        finish(message, url)

    def ask_duplicate(found, issues):
        dialog = Gtk.MessageDialog(transient_for=win, modal=True, message_type=Gtk.MessageType.QUESTION,
                                   text=tr("dup_title", L))
        if found == "identical":
            dialog.format_secondary_text(tr("dup_text", L, number=issues[0]["number"], title=issues[0]["title"]))
            choices = issues[:1]
        else:
            dialog.format_secondary_text(tr("dup_similar", L) + "\n\n" +
                                         "\n".join(f"#{i['number']} {i['title']}" for i in issues[:3]))
            choices = issues[:3]
        for index, issue in enumerate(choices):
            dialog.add_button(tr("attach", L, number=issue["number"]), 100 + index)
        dialog.add_button(tr("new", L), 1)
        dialog.add_button(tr("cancel", L), Gtk.ResponseType.CANCEL)
        answer = dialog.run()
        dialog.destroy()
        notice.set_text("")
        send.set_sensitive(True)
        save.set_sensitive(True)
        if answer >= 100:
            deliver(choices[answer - 100]["number"])
        elif answer == 1:
            deliver()

    def on_send(*_):
        if kind == "file":
            on_save()
            finish(notice.get_text())
            return
        busy(tr("searching", L))

        def work():
            found, issues = find_duplicates(report, summary.get_text())
            GLib.idle_add(lambda: ask_duplicate(found, issues) if issues else deliver())
        threading.Thread(target=work, daemon=True).start()

    cancel.connect("clicked", lambda *_: win.destroy())
    save.connect("clicked", on_save)
    send.connect("clicked", on_send)
    regenerate()
    win.show_all()
    edited_row.hide()
    summary.grab_focus()
    Gtk.main()


# ── Point d'entrée ───────────────────────────────────────────────────────

def main(argv):
    args = argv[1:]

    def option(name, default=None):
        if name in args:
            index = args.index(name)
            return args[index + 1] if index + 1 < len(args) else default
        return default

    if "--status" in args:
        status()
        return 0
    if "--set" in args:
        set_value(option("--set", ""))
        return 0
    if "--open-saved" in args:
        directory = STATE / "reports"
        directory.mkdir(parents=True, exist_ok=True)
        return 0 if open_url(str(directory)) else 1
    conf = config()
    crash = None
    if "--crash-at" in args:
        entries = crashes()
        index = int(option("--crash-at", "-1") or -1)
        crash = entries[index] if 0 <= index < len(entries) else None
    elif "--crash" in args:
        crash = last_crash(int(option("--since", "0") or 0))
        if not crash:
            if "--print" in args:
                print(tr("nothing"))
            return 3
        if "--print" not in args and "--force" not in args:
            # 4 : proposition désactivée dans Paramètres, le lanceur affiche
            # alors son simple message d'erreur.
            if conf["OFFER"] == "off":
                return 4
            if not mark_offered(crash):
                return 0
    report = Report(crash, option("--component"))
    if "--print" in args:
        redacted = conf["REDACT"] == "on" and "--no-redact" not in args
        print(report.title())
        print(" ".join(report.labels()))
        print()
        print(report.body("", redacted))
        return 0
    gui(report, conf)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
