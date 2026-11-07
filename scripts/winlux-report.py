#!/usr/bin/python3
"""Signaler un problème Winlux sur GitHub.

Rien ne part sans action de l'utilisateur : le rapport complet, modifiable,
est affiché avant l'envoi, et les données personnelles sont masquées par
défaut. Seule exception, à activer dans Paramètres (#438) : l'envoi
automatique des plantages, en arrière-plan, toujours anonymisé.

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
  winlux-report --system             arrêt anormal de la session précédente
                                     (une seule proposition par arrêt)
  winlux-report --open-saved         ouvre le dossier des rapports enregistrés
  winlux-report --component NOM      problème d'un composant Winlux
  winlux-report --print [--crash]    affiche le rapport (sans interface)
        [--attach network,sound,boot,update|all]  avec les journaux de diagnostic
  winlux-report --status             réglages, format clé=valeur
  winlux-report --set CLÉ=VALEUR     offer=on|off redact=on|off mode=auto|browser|file
                                     auto=on|off|default
  winlux-report --background [--once] envoi automatique différé (session)
  winlux-report --collect            met les plantages en file (sans réseau)
  winlux-report --set-token          jeton GitHub du système lu sur l'entrée (root)
  winlux-report --forget-token       oublie ce jeton (root)
  winlux-report --send-stdin         envoie un rapport de la file (root)
  winlux-report --auto-status        état de l'envoi automatique du compte
"""
import calendar
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

REPO = os.environ.get("WINLUX_REPORT_REPO", "Univers4craft/winlux")
API = os.environ.get("WINLUX_GITHUB_API", "https://api.github.com")
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
  "attach_logs": "Joindre les journaux de diagnostic :",
  "d_network": "Réseau et Wi-Fi", "d_sound": "Son", "d_boot": "Démarrage", "d_update": "Mises à jour",
  "h_diag": "Journaux de diagnostic", "d_empty": "vide (rien n'est encore écrit dans ce journal)",
  "d_unreadable": "illisible depuis ce compte", "d_none": "aucun journal trouvé pour ce domaine",
  "h_queue": "Rapports de plantage en attente d'envoi",
  "auto_desc": "Rapport envoyé automatiquement par Winlux au démarrage suivant, données personnelles masquées.",
  "auto_count": "Occurrences depuis le dernier envoi : {count}.",
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
  "acct_title": "Un compte GitHub est nécessaire pour l'instant",
  "acct_text": "Le service de signalement sans compte de Winlux n'est pas encore en ligne. Votre rapport va s'ouvrir sur le site GitHub, déjà rempli : il suffit de vous connecter (ou de créer un compte gratuit) puis de cliquer « Submit new issue ». Votre rapport est aussi enregistré sur cet ordinateur ({path}).",
  "acct_open": "Ouvrir GitHub", "acct_signup": "Créer un compte GitHub", "acct_later": "Enregistrer seulement",
  "via_browser": "Le service sans compte n'est pas encore en ligne : le rapport s'ouvrira, prérempli, sur le site GitHub, où un compte GitHub gratuit est nécessaire pour l'envoyer.",
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
  "h_program": "Programme", "h_stack": "Pile d'appels", "h_journal": "Dernières lignes du journal", "h_network": "Journal du réseau",
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
  "h_steps": "Étapes avant le problème",
  "h_checks": "Vérifications automatiques",
  "h_system": "Messages du système avant l'arrêt",
  "s_start": "Lancement de {exe}",
  "s_warn": "Avertissement de Winlux : {msg}",
  "s_error": "Erreur notée par Winlux : {msg}",
  "s_crash": "{exe} s'arrête : {exception} dans {module}",
  "s_sys": "Système : {msg}",
  "s_ran": "Le programme a fonctionné {duration} avant de s'arrêter.",
  "s_none": "Winlux n'a rien noté avant l'arrêt.",
  "s_more": "… et {count} autres messages semblables.",
  "k_disk_ok": "Espace disque suffisant : {free} libres sur {mount}.",
  "k_disk_low": "Disque presque plein : {free} libres sur {mount}. Libérez de la place, un programme peut planter faute d'espace.",
  "k_mem_ok": "Mémoire disponible : {free} sur {total}.",
  "k_mem_low": "Mémoire presque pleine : {free} libres sur {total}. Fermez des applications avant de réessayer.",
  "k_oom": "Le système a manqué de mémoire et a dû fermer un programme ({victim}).",
  "k_gpu": "La carte graphique a eu un problème au même moment : {msg}",
  "k_repeat": "Ce programme a déjà planté {count} fois avec la même erreur.",
  "k_first": "C'est le premier plantage de ce programme avec cette erreur.",
  "k_missing": "Fonctions Windows manquantes notées dans le journal : {funcs}.",
  "k_soft_gl": "Pas d'accélération graphique (rendu logiciel) : les jeux et applications 3D peuvent planter ou être très lents.",
  "k_prev": "La session précédente s'est arrêtée brutalement ({reason}).",
  "system_crashed": "Winlux s'est arrêté de façon inattendue lors de la dernière session.",
  "c_system": "Le noyau Linux ou un pilote a rencontré une erreur grave. Les derniers messages enregistrés avant l'arrêt sont joints au rapport.",
  "r_panic": "erreur grave du noyau Linux",
  "r_critical": "erreur critique enregistrée par le système",
  "nothing_system": "Aucun arrêt anormal du système à signaler.",
 },
 "en": {
  "attach_logs": "Attach diagnostic logs:",
  "d_network": "Network and Wi-Fi", "d_sound": "Sound", "d_boot": "Startup", "d_update": "Updates",
  "h_diag": "Diagnostic logs", "d_empty": "empty (nothing written to this log yet)",
  "d_unreadable": "not readable from this account", "d_none": "no log found for this area",
  "h_queue": "Crash reports waiting to be sent",
  "auto_desc": "Report sent automatically by Winlux at the next startup, personal data hidden.",
  "auto_count": "Occurrences since the last report: {count}.",
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
  "acct_title": "A GitHub account is needed for now",
  "acct_text": "Winlux's no-account reporting service is not online yet. Your report will open on the GitHub website, already filled in: just sign in (or create a free account) and click \"Submit new issue\". Your report is also saved on this computer ({path}).",
  "acct_open": "Open GitHub", "acct_signup": "Create a GitHub account", "acct_later": "Only save",
  "via_browser": "The no-account service is not online yet: the report will open, prefilled, on the GitHub website, where a free GitHub account is needed to submit it.",
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
  "h_program": "Program", "h_stack": "Call stack", "h_journal": "Last log lines", "h_network": "Network log",
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
  "h_steps": "Steps before the problem",
  "h_checks": "Automatic checks",
  "h_system": "System messages before the stop",
  "s_start": "{exe} started",
  "s_warn": "Winlux warning: {msg}",
  "s_error": "Error logged by Winlux: {msg}",
  "s_crash": "{exe} stops: {exception} in {module}",
  "s_sys": "System: {msg}",
  "s_ran": "The program ran for {duration} before stopping.",
  "s_none": "Winlux logged nothing before the stop.",
  "s_more": "… and {count} more similar messages.",
  "k_disk_ok": "Enough disk space: {free} free on {mount}.",
  "k_disk_low": "Disk almost full: {free} free on {mount}. Free some space, a program can crash when the disk is full.",
  "k_mem_ok": "Available memory: {free} of {total}.",
  "k_mem_low": "Memory almost full: {free} free of {total}. Close some applications before trying again.",
  "k_oom": "The system ran out of memory and had to close a program ({victim}).",
  "k_gpu": "The graphics card had a problem at the same time: {msg}",
  "k_repeat": "This program already crashed {count} times with the same error.",
  "k_first": "This is the first crash of this program with this error.",
  "k_missing": "Missing Windows functions noted in the log: {funcs}.",
  "k_soft_gl": "No graphics acceleration (software rendering): games and 3D applications may crash or be very slow.",
  "k_prev": "The previous session stopped abruptly ({reason}).",
  "system_crashed": "Winlux stopped unexpectedly during the last session.",
  "c_system": "The Linux kernel or a driver ran into a serious error. The last messages recorded before the stop are attached to the report.",
  "r_panic": "serious Linux kernel error",
  "r_critical": "critical error recorded by the system",
  "nothing_system": "No abnormal system stop to report.",
 },
 "es": {
  "attach_logs": "Adjuntar los registros de diagnóstico:",
  "d_network": "Red y Wi-Fi", "d_sound": "Sonido", "d_boot": "Arranque", "d_update": "Actualizaciones",
  "h_diag": "Registros de diagnóstico", "d_empty": "vacío (aún no se ha escrito nada en este registro)",
  "d_unreadable": "ilegible desde esta cuenta", "d_none": "no se encontró ningún registro para este ámbito",
  "h_queue": "Informes de bloqueo pendientes de envío",
  "auto_desc": "Informe enviado automáticamente por Winlux en el siguiente arranque, datos personales ocultos.",
  "auto_count": "Apariciones desde el último envío: {count}.",
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
  "acct_title": "Por ahora se necesita una cuenta de GitHub",
  "acct_text": "El servicio de informes sin cuenta de Winlux todavía no está en línea. Su informe se abrirá en el sitio de GitHub, ya rellenado: solo tiene que iniciar sesión (o crear una cuenta gratuita) y pulsar «Submit new issue». El informe también se guarda en este equipo ({path}).",
  "acct_open": "Abrir GitHub", "acct_signup": "Crear una cuenta de GitHub", "acct_later": "Solo guardar",
  "via_browser": "El servicio sin cuenta todavía no está en línea: el informe se abrirá, ya rellenado, en el sitio de GitHub, donde se necesita una cuenta gratuita de GitHub para enviarlo.",
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
  "h_program": "Programa", "h_stack": "Pila de llamadas", "h_journal": "Últimas líneas del registro", "h_network": "Registro de red",
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
  "h_steps": "Pasos antes del problema",
  "h_checks": "Comprobaciones automáticas",
  "h_system": "Mensajes del sistema antes de la parada",
  "s_start": "Inicio de {exe}",
  "s_warn": "Advertencia de Winlux: {msg}",
  "s_error": "Error registrado por Winlux: {msg}",
  "s_crash": "{exe} se detiene: {exception} en {module}",
  "s_sys": "Sistema: {msg}",
  "s_ran": "El programa funcionó {duration} antes de detenerse.",
  "s_none": "Winlux no registró nada antes de la parada.",
  "s_more": "… y {count} mensajes parecidos más.",
  "k_disk_ok": "Espacio en disco suficiente: {free} libres en {mount}.",
  "k_disk_low": "Disco casi lleno: {free} libres en {mount}. Libere espacio, un programa puede fallar si el disco está lleno.",
  "k_mem_ok": "Memoria disponible: {free} de {total}.",
  "k_mem_low": "Memoria casi llena: {free} libres de {total}. Cierre algunas aplicaciones antes de volver a intentarlo.",
  "k_oom": "El sistema se quedó sin memoria y tuvo que cerrar un programa ({victim}).",
  "k_gpu": "La tarjeta gráfica tuvo un problema al mismo tiempo: {msg}",
  "k_repeat": "Este programa ya falló {count} veces con el mismo error.",
  "k_first": "Es el primer fallo de este programa con este error.",
  "k_missing": "Funciones de Windows ausentes anotadas en el registro: {funcs}.",
  "k_soft_gl": "Sin aceleración gráfica (renderizado por software): los juegos y aplicaciones 3D pueden fallar o ir muy lentos.",
  "k_prev": "La sesión anterior se detuvo bruscamente ({reason}).",
  "system_crashed": "Winlux se detuvo de forma inesperada durante la última sesión.",
  "c_system": "El kernel Linux o un controlador sufrió un error grave. Los últimos mensajes registrados antes de la parada se adjuntan al informe.",
  "r_panic": "error grave del kernel Linux",
  "r_critical": "error crítico registrado por el sistema",
  "nothing_system": "No hay ninguna parada anómala del sistema que informar.",
 },
 "de": {
  "attach_logs": "Diagnoseprotokolle anhängen:",
  "d_network": "Netzwerk und WLAN", "d_sound": "Ton", "d_boot": "Start", "d_update": "Updates",
  "h_diag": "Diagnoseprotokolle", "d_empty": "leer (in dieses Protokoll wurde noch nichts geschrieben)",
  "d_unreadable": "von diesem Konto aus nicht lesbar", "d_none": "kein Protokoll für diesen Bereich gefunden",
  "h_queue": "Noch nicht gesendete Absturzberichte",
  "auto_desc": "Bericht automatisch von Winlux beim nächsten Start gesendet, persönliche Daten ausgeblendet.",
  "auto_count": "Vorkommen seit dem letzten Bericht: {count}.",
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
  "acct_title": "Vorerst ist ein GitHub-Konto nötig",
  "acct_text": "Der kontolose Meldedienst von Winlux ist noch nicht online. Ihr Bericht öffnet sich bereits ausgefüllt auf der GitHub-Website: Melden Sie sich an (oder erstellen Sie ein kostenloses Konto) und klicken Sie auf „Submit new issue“. Der Bericht ist außerdem auf diesem Computer gespeichert ({path}).",
  "acct_open": "GitHub öffnen", "acct_signup": "GitHub-Konto erstellen", "acct_later": "Nur speichern",
  "via_browser": "Der kontolose Dienst ist noch nicht online: Der Bericht öffnet sich vorausgefüllt auf der GitHub-Website, wo zum Absenden ein kostenloses GitHub-Konto nötig ist.",
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
  "h_program": "Programm", "h_stack": "Aufrufliste", "h_journal": "Letzte Protokollzeilen", "h_network": "Netzwerkprotokoll",
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
  "h_steps": "Schritte vor dem Problem",
  "h_checks": "Automatische Prüfungen",
  "h_system": "Systemmeldungen vor dem Halt",
  "s_start": "{exe} gestartet",
  "s_warn": "Winlux-Warnung: {msg}",
  "s_error": "Von Winlux protokollierter Fehler: {msg}",
  "s_crash": "{exe} hält an: {exception} in {module}",
  "s_sys": "System: {msg}",
  "s_ran": "Das Programm lief {duration}, bevor es anhielt.",
  "s_none": "Winlux hat vor dem Halt nichts protokolliert.",
  "s_more": "… und {count} weitere ähnliche Meldungen.",
  "k_disk_ok": "Genügend Speicherplatz: {free} frei auf {mount}.",
  "k_disk_low": "Datenträger fast voll: {free} frei auf {mount}. Schaffen Sie Platz, ein Programm kann bei vollem Datenträger abstürzen.",
  "k_mem_ok": "Verfügbarer Arbeitsspeicher: {free} von {total}.",
  "k_mem_low": "Arbeitsspeicher fast voll: {free} frei von {total}. Schließen Sie einige Anwendungen, bevor Sie es erneut versuchen.",
  "k_oom": "Dem System ging der Arbeitsspeicher aus und es musste ein Programm schließen ({victim}).",
  "k_gpu": "Die Grafikkarte hatte zur selben Zeit ein Problem: {msg}",
  "k_repeat": "Dieses Programm ist bereits {count}-mal mit demselben Fehler abgestürzt.",
  "k_first": "Dies ist der erste Absturz dieses Programms mit diesem Fehler.",
  "k_missing": "Im Protokoll vermerkte fehlende Windows-Funktionen: {funcs}.",
  "k_soft_gl": "Keine Grafikbeschleunigung (Software-Rendering): Spiele und 3D-Anwendungen können abstürzen oder sehr langsam sein.",
  "k_prev": "Die vorherige Sitzung wurde abrupt beendet ({reason}).",
  "system_crashed": "Winlux wurde in der letzten Sitzung unerwartet beendet.",
  "c_system": "Der Linux-Kernel oder ein Treiber hatte einen schweren Fehler. Die letzten vor dem Halt aufgezeichneten Meldungen sind dem Bericht beigefügt.",
  "r_panic": "schwerer Linux-Kernelfehler",
  "r_critical": "vom System aufgezeichneter kritischer Fehler",
  "nothing_system": "Kein ungewöhnlicher Systemhalt zu melden.",
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
            "mode": ("MODE", "auto", ("auto", "browser", "file")),
            # Envoi automatique des plantages : « default » = actif seulement
            # sur une machine qui a un jeton personnel (désactivé ailleurs).
            "auto": ("AUTO", "default", ("on", "off", "default"))}


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
        if token_path(conf):
            return "token"
        if SYSTEM_TOKEN.exists():
            return "system"
    return "browser"


# Jeton du système : root seul le lit (0600). Les comptes et les .exe qu'ils
# lancent ne le voient jamais ; ils remettent leurs rapports au relais root
# (winlux-root control report-send), qui les vérifie puis les envoie.
SYSTEM_TOKEN = Path(os.environ.get("WINLUX_REPORT_TOKEN", "/etc/winlux/report-token"))
SYSTEM_API = "https://api.github.com"
ROOT_STATE = Path(os.environ.get("WINLUX_REPORT_ROOT_STATE", "/var/lib/winlux/auto-report"))
ROOT_TOOL = os.environ.get("WINLUX_ROOT_TOOL", "winlux-root")
ROOT_MAX_PER_BOOT = 10
ROOT_MAX_PER_DAY = 30


def token_path(conf):
    """TOKEN_FILE du réglage (machine de développement, lisible par le
    compte). Le jeton normal est SYSTEM_TOKEN, réservé à root."""
    path = conf.get("TOKEN_FILE", "")
    if path and os.access(path, os.R_OK) and Path(path).stat().st_size > 0:
        return path
    return ""


def auto_enabled(conf=None):
    conf = conf or config()
    if conf["AUTO"] == "default":
        return bool(token_path(conf)) or SYSTEM_TOKEN.exists()
    return conf["AUTO"] == "on"


def status():
    conf = config()
    print(f"report_offer={conf['OFFER']}")
    print(f"report_redact={conf['REDACT']}")
    print(f"report_mode={conf['MODE']}")
    manual = transport(conf)
    # Transport d'un rapport manuel : le jeton du système ne sert qu'aux envois automatiques.
    print(f"report_transport={'browser' if manual == 'system' else manual}")
    print(f"report_relay={1 if conf.get('RELAY', '').startswith('https://') else 0}")
    print(f"report_auto={conf['AUTO']}")
    print(f"report_token={1 if SYSTEM_TOKEN.exists() else 0}")


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


NETWORK_LOGS = Path(os.environ.get("WINLUX_NETWORK_LOG_DIR", "/var/log/winlux"))
SSID_TEXT = re.compile(r"""(\b(?:SSID|ssid)\b\s*[=:]?\s*)(['"]).*?\2""")


def network_tail(limit=40):
    """Dernières lignes des journaux NetworkManager et wpa_supplicant (#457).

    Sans eux, un Wi-Fi qui refuse de se connecter ne laissait aucune trace.
    Le nom des réseaux cités est masqué ici ; adresses MAC et IP, et réseaux
    enregistrés, le sont ensuite par redact()."""
    sections = []
    for name, title in (("network-manager", "NetworkManager"), ("wpa-supplicant", "wpa_supplicant")):
        try:
            lines = (NETWORK_LOGS / f"{name}.log").read_text(errors="replace").splitlines()
        except OSError:
            continue
        lines = [SSID_TEXT.sub(r"\1\2<réseau-wifi>\2", line) for line in lines if line.strip()][-limit:]
        if lines:
            sections.append((title, lines))
    return sections


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


# ── Diagnostic : étapes avant le problème et vérifications ───────────────

JOURNAL_LINE = re.compile(r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(?:\.\d+)?Z? \[(\w+)\] (?:(\S+)\[(\d+)\] )?([\w.:-]+): (.*)$")
SYSTEM_PATTERNS = (
    ("oom", re.compile(r"out of memory|oom-kill|killed process", re.I)),
    ("gpu", re.compile(r"gpu hang|ring \w+ timeout|gpu reset|amdgpu.*(?:timeout|fault)|nvrm: xid|i915.*(?:hang|reset)|nouveau.*fault", re.I)),
    ("segv", re.compile(r"segfault at|general protection fault", re.I)),
    ("usb", re.compile(r"usb \S+: usb disconnect", re.I)),
)


def utc_seconds(text):
    """« 2026-10-31T17:06:44 » (UTC) → secondes depuis 1970."""
    try:
        return calendar.timegm(time.strptime(text[:19], "%Y-%m-%dT%H:%M:%S"))
    except ValueError:
        return 0


def winlux_lines(limit=2 * 1024 * 1024):
    """Lignes du journal du runtime (la fin s'il est très long)."""
    try:
        with open(JOURNAL / "winlux.log", "rb") as stream:
            stream.seek(0, os.SEEK_END)
            size = stream.tell()
            stream.seek(max(0, size - limit))
            return stream.read().decode("utf-8", "replace").splitlines()
    except OSError:
        return []


def program_events(crash, lines):
    """(horodatage, niveau, cible, message) du programme qui a planté, d'après
    l'origine « nom.exe[pid] » des lignes du journal."""
    pid = str((crash or {}).get("pid", ""))
    events = []
    for line in lines:
        m = JOURNAL_LINE.match(line)
        if m and m.group(4) == pid:
            events.append((utc_seconds(m.group(1)), m.group(2), m.group(5), m.group(6)))
    return events


def system_events(start, end):
    """Messages du noyau intéressants entre deux instants : manque de mémoire,
    carte graphique bloquée, autres plantages, périphérique débranché."""
    found = []
    text = run(["journalctl", "-k", "--since", f"@{int(start)}", "--until", f"@{int(end)}",
                "-o", "short-unix", "-q", "--no-pager"], timeout=5)
    for line in text.splitlines():
        stamp, _, rest = line.partition(" ")
        try:
            ts = float(stamp)
        except ValueError:
            continue
        message = rest.split(": ", 1)[-1]
        for kind, pattern in SYSTEM_PATTERNS:
            if pattern.search(message):
                found.append((int(ts), kind, message.strip()))
                break
    return found[-12:]


def duration(seconds, language=None):
    seconds = max(0, int(seconds))
    h, rest = divmod(seconds, 3600)
    m, s = divmod(rest, 60)
    if h:
        return f"{h} h {m:02d} min"
    if m:
        return f"{m} min {s:02d} s"
    return f"{s} s"


def steps(crash, lines, sys_events, language=None):
    """Chronologie lisible : lancement, avertissements et erreurs du programme,
    événements du système, puis l'arrêt."""
    if not crash:
        return []
    events = program_events(crash, lines)
    crash_ts = int(crash.get("ts", "0") or 0)
    exe = crash.get("exe", "?")
    out = []
    start = None
    for ts, level, target, message in events:
        if target == "session" and message.startswith("démarrage"):
            start = ts
            out.append((ts, tr("s_start", language, exe=exe)))
    seen = {}
    for ts, level, target, message in events:
        if level not in ("warn", "error") or target == "session":
            continue
        key = (level, target, re.sub(r"0x[0-9a-f]+|\d+", "#", message))
        seen[key] = seen.get(key, 0) + 1
        if seen[key] == 1:
            msg = f"{target}: {message}"[:200]
            out.append((ts, tr("s_error" if level == "error" else "s_warn", language, msg=msg)))
    for ts, kind, message in sys_events:
        out.append((ts, tr("s_sys", language, msg=message[:200])))
    out.sort(key=lambda item: item[0])
    repeated = sum(n - 1 for n in seen.values())
    if len(out) > 14:
        repeated += len(out) - 14
        head = [step for step in out if step[1] == tr("s_start", language, exe=exe)][:1]
        out = head + [step for step in out if step not in head][-13:]
    if repeated:
        out.append((None, tr("s_more", language, count=repeated)))
    module = crash.get("module", "?").split("+")[0]
    out.append((crash_ts, tr("s_crash", language, exe=exe, exception=crash.get("exception", "?"), module=module)))
    if start and crash_ts >= start:
        out.append((None, tr("s_ran", language, duration=duration(crash_ts - start, language))))
    elif len(out) == 1:
        out.insert(0, (None, tr("s_none", language)))
    return out


def human_size(value, language=None):
    """Taille lisible, unités et virgule de la langue (« 28,4 Go », « 28.4 GB »)."""
    language = language or lang()
    units = ("o", "Ko", "Mo", "Go", "To") if language == "fr" else ("B", "KB", "MB", "GB", "TB")
    for unit in units:
        if value < 1024 or unit == units[-1]:
            if unit == units[0]:
                return f"{int(value)} {unit}"
            text = f"{value:.1f}".removesuffix(".0")
            return f"{text.replace('.', ',') if language != 'en' else text} {unit}"
        value /= 1024
    return str(value)


def checks(crash, lines, sys_events, renderer="", language=None):
    """(ok, texte) : ce que Winlux a vérifié sur la machine au moment du
    rapport, avec un conseil quand quelque chose cloche."""
    out = []
    mounts = [Path("/")]
    home = Path.home()
    try:
        if home.exists() and os.stat(home).st_dev != os.stat("/").st_dev:
            mounts.append(home)
    except OSError:
        pass
    for mount in mounts:
        try:
            usage = shutil.disk_usage(mount)
        except OSError:
            continue
        low = usage.free < 2 * 1024 ** 3 or usage.free < usage.total * 0.05
        out.append((not low, tr("k_disk_low" if low else "k_disk_ok", language,
                                free=human_size(usage.free, language), mount=str(mount).replace(str(home), "~"))))
    mem = {}
    try:
        for line in Path("/proc/meminfo").read_text().splitlines():
            key, _, rest = line.partition(":")
            mem[key] = int(rest.split()[0]) * 1024
    except (OSError, ValueError, IndexError):
        pass
    if mem.get("MemTotal"):
        free, total = mem.get("MemAvailable", 0), mem["MemTotal"]
        low = free < total * 0.08
        out.append((not low, tr("k_mem_low" if low else "k_mem_ok", language,
                                free=human_size(free, language), total=human_size(total, language))))
    for ts, kind, message in sys_events:
        if kind == "oom":
            victim = re.search(r"\(([^)]+)\)", message)
            out.append((False, tr("k_oom", language, victim=victim.group(1) if victim else "?")))
            break
    for ts, kind, message in sys_events:
        if kind == "gpu":
            out.append((False, tr("k_gpu", language, msg=message[:160])))
            break
    if crash:
        sig = signature(crash)
        count = sum(1 for c in crashes() if signature(c) == sig)
        if count > 1:
            out.append((False, tr("k_repeat", language, count=count)))
        else:
            out.append((True, tr("k_first", language)))
        funcs = missing_functions(lines[-200:], crash)
        if funcs:
            out.append((False, tr("k_missing", language, funcs=", ".join(funcs))))
        module = crash.get("module", "").lower()
        graphics = any(k in module for k in ("d3d", "dxgi", "opengl", "vulkan", "ddraw"))
        if graphics and any(k in renderer.lower() for k in ("llvmpipe", "softpipe", "software")):
            out.append((False, tr("k_soft_gl", language)))
    return out


PSTORE = Path(os.environ.get("WINLUX_PSTORE", "/sys/fs/pstore"))


def previous_boot_problem():
    """Arrêt anormal de la session précédente : traces d'erreur grave du noyau
    gardées par pstore, sinon messages critiques du démarrage précédent."""
    lines, reason = [], ""
    try:
        dumps = sorted(p for p in PSTORE.iterdir() if p.is_file())
    except OSError:
        dumps = []
    for dump in dumps:
        if "dmesg" in dump.name or dump.name.startswith(("efi-", "console")):
            try:
                lines += dump.read_text(errors="replace").splitlines()
            except OSError:
                pass
    if lines:
        reason = "panic"
    else:
        text = run(["journalctl", "-b", "-1", "-p", "0..2", "-o", "short-iso", "-q", "--no-pager", "-n", "40"],
                   timeout=5)
        lines = [l for l in text.splitlines() if l.strip()]
        if lines:
            reason = "critical"
    if not lines:
        return None
    lines = [l for l in lines if l.strip()][-40:]
    key = hashlib.sha256("\n".join(lines).encode(errors="replace")).hexdigest()[:16]
    return {"reason": reason, "lines": lines, "key": key}


def system_signature(problem):
    """Empreinte d'un arrêt anormal : première ligne d'erreur sans dates,
    nombres ni adresses, pour retrouver le même défaut d'un démarrage à l'autre."""
    if not problem:
        return ""
    lines = problem.get("lines") or [""]
    first = next((l for l in lines if re.search(r"panic|bug|error|fault|oops", l, re.I)), lines[0])
    first = re.sub(r"^\S+ \S+ ", "", first.strip()).lower()
    first = re.sub(r"0x[0-9a-f]+|\b[0-9a-f]{6,}\b|\d+", "#", first)
    return "wlx-sys-" + hashlib.sha256(first.encode(errors="replace")).hexdigest()[:12]


def mark_system_offered(problem):
    """Une seule proposition par arrêt anormal, même au démarrage suivant."""
    marker = STATE / "system-crash-offered"
    try:
        if marker.read_text().strip() == problem["key"]:
            return False
    except OSError:
        pass
    try:
        STATE.mkdir(parents=True, exist_ok=True)
        marker.write_text(problem["key"] + "\n")
    except OSError:
        pass
    return True


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
SECRET = re.compile(r"(?i)\b(pass(?:word|wd|phrase)?|pwd|mot[ _-]de[ _-]passe|psk|wpa[_-]?psk|secret|"
                    r"api[_-]?key|token|authorization|bearer)(\s*[=:]\s*|\s+)(\"[^\"]*\"|'[^']*'|\S+)")
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
    text = SECRET.sub(lambda m: m.group(1) + m.group(2) + "<secret>", text)
    text = TOKEN.sub("<jeton>", text)
    text = EMAIL.sub("<e-mail>", text)
    text = MAC.sub("<adresse-mac>", text)
    text = IPV4.sub(lambda m: m.group(0) if m.group(0).startswith(("127.", "0.")) else "<adresse-ip>", text)
    for secret, label in sorted(values.items(), key=lambda kv: -len(kv[0])):
        text = re.sub(r"(?<![\w-])" + re.escape(secret) + r"(?![\w-])", label, text)
    for marker, value in protected.items():
        text = text.replace(marker, value)
    return text


# ── Journaux de diagnostic joints (#458) ─────────────────────────────────
#
# Joints seulement s'ils existent ; un fichier vide est signalé comme tel (sur
# un système installé, NetworkManager et wpa_supplicant n'écrivaient rien).
# Commandes en LC_ALL=C (pactl traduit ses en-têtes). Tout passe ensuite par
# le même masquage que le reste du rapport.

def _p(path):
    return Path(os.environ.get("WINLUX_DIAG_ROOT", "") + path)


DIAG_DOMAINS = {
    "network": {"files": ["/run/winlux/network.log", "/run/winlux/network-manager.log",
                          "/run/winlux/network-agent.log", "/run/winlux/network-fallback.log",
                          "/run/winlux/wpa-supplicant.log", "/run/winlux/network-probe.json",
                          "/run/winlux/time-sync.log", "/run/winlux/time-sync.json"],
                "commands": [["nmcli", "-t", "-f", "DEVICE,TYPE,STATE", "device"], ["rfkill", "list"],
                             ["ip", "-brief", "link"]]},
    "sound": {"files": ["/run/winlux/pipewire.log", "/run/winlux/wireplumber.log"],
              "commands": [["pactl", "info"], ["pactl", "list", "short", "sinks"],
                           ["pactl", "list", "short", "sources"], ["aplay", "-l"]]},
    "boot": {"files": ["/run/winlux/native-early.log", "/run/winlux/native-session.log",
                       "/run/winlux/boot-splash.log", "/run/winlux/system-services.log",
                       "/var/lib/winlux/native-boot-failure.log", "/run/winlux/hardware.log"],
             "commands": [["dmesg", "--level=emerg,alert,crit,err"]]},
    "update": {"files": ["/run/winlux/update.log", "/run/winlux/boot-update.log", "/run/winlux/sysupdate-auto.log",
                         "/run/winlux/updates.txt", "/run/winlux/sysupdates.txt",
                         "/var/log/winlux-first-boot-packages.log", "/var/log/winlux-drivers.log"],
               "commands": []},
}
DIAG_LINES = 80
DIAG_COMMANDS = os.environ.get("WINLUX_DIAG_COMMANDS", "1") == "1"


def diag_command(cmd):
    if not DIAG_COMMANDS or not shutil.which(cmd[0]):
        return None
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=5,
                             env={**os.environ, "LC_ALL": "C", "LANG": "C"})
    except (OSError, subprocess.SubprocessError):
        return None
    text = (out.stdout or "").strip()
    return text.splitlines()[-DIAG_LINES:] if text else None


def diagnostics(domain, language=None):
    """[(nom, lignes ou None, état)] pour un domaine : fichiers puis commandes."""
    found = []
    for name in DIAG_DOMAINS[domain]["files"]:
        path = _p(name)
        if not path.exists():
            continue
        try:
            text = path.read_text(errors="replace")
        except OSError:
            found.append((name, None, tr("d_unreadable", language)))
            continue
        lines = [SSID_TEXT.sub(r"\1\2<réseau-wifi>\2", l) for l in text.splitlines() if l.strip()][-DIAG_LINES:]
        found.append((name, lines or None, "" if lines else tr("d_empty", language)))
    for cmd in DIAG_DOMAINS[domain]["commands"]:
        lines = diag_command(cmd)
        if lines:
            found.append(("$ " + " ".join(cmd), lines, ""))
    return found


def queued_reports():
    titles = []
    for path in sorted(QUEUE.glob("*.json")) if QUEUE.exists() else []:
        item = load_json(path, {})
        if item.get("title"):
            titles.append(f"{item['title']} — `{item.get('signature', '')}`")
    return titles


def diagnostics_markdown(domains, language=None):
    fence = "`" * 3
    out = []
    for domain in domains:
        entries = diagnostics(domain, language)
        out += [f"<details><summary>{tr('h_diag', language)} — {tr('d_' + domain, language)}</summary>", ""]
        if not entries:
            out += [f"_{tr('d_none', language)}_", ""]
        for name, lines, note in entries:
            if lines is None:
                out += [f"- `{name}` : {note}"]
                continue
            out += [f"`{name}`", "", fence] + [l[:300].replace(fence, "'" * 3) for l in lines] + [fence, ""]
        out += ["", "</details>", ""]
    return out


# ── Construction du rapport ──────────────────────────────────────────────

class Report:
    def __init__(self, crash=None, component=None, language=None, system=None):
        self.language = language or lang()
        self.crash = crash
        self.component = component
        self.system = system
        self.journal = journal_tail()
        self.machine, flags = machine_info()
        self.cause = (tr("c_system", self.language) if system
                      else probable_cause(crash, self.journal, self.language, flags))
        lines = winlux_lines()
        sys_events = []
        if crash:
            crash_ts = int(crash.get("ts", "0") or 0)
            events = program_events(crash, lines)
            start = min([e[0] for e in events] + [crash_ts - 600])
            sys_events = system_events(start - 5, crash_ts + 5)
        renderer = dict(self.machine).get("OpenGL", "")
        self.steps = steps(crash, lines, sys_events, self.language)
        self.checks = checks(crash, lines, sys_events, renderer, self.language)
        if system:
            reason = tr("r_panic" if system["reason"] == "panic" else "r_critical", self.language)
            self.checks.insert(0, (False, tr("k_prev", self.language, reason=reason)))
        self.signature = signature(crash) or system_signature(system)
        self.attach = []
        path = (crash or {}).get("path", "")
        self.exe_name = (crash or {}).get("exe", "") or component or ""
        self.exe = pe_version(path) if path.lower().endswith((".exe", ".dll", ".com", ".scr")) else {}

    def kind(self):
        if self.system:
            return "system"
        if not self.crash:
            return "manual"
        exc = self.crash.get("exception", "")
        if exc == "ENTRYPOINT_NOT_FOUND":
            return "missing"
        if exc == "WINLUX_INTERNAL_ERROR" or self.component:
            return "internal"
        return "crash"

    def headline(self):
        if self.system:
            return tr("system_crashed", self.language)
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
        elif kind == "system":
            first = next((l for l in self.system["lines"] if re.search(r"panic|bug|error|fault|oops", l, re.I)),
                         self.system["lines"][0])
            title = "[Arrêt du système] " + re.sub(r"^\S+ \S+ ", "", first.strip())[:90]
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
                       "internal": "plantage", "manual": "à-trier", "system": "plantage"}[kind])
        if kind == "system":
            labels.append("runtime")
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
        if self.steps:
            out += [f"### {tr('h_steps', L)}", ""]
            number = 0
            for ts, text in self.steps:
                if ts is None:
                    out.append(f"_{text}_")
                else:
                    number += 1
                    out.append(f"{number}. {time.strftime('%H:%M:%S', time.localtime(ts))} — {text}")
            out.append("")
        if self.checks:
            out += [f"### {tr('h_checks', L)}", ""]
            out += [f"- {'✅' if ok else '⚠️'} {text}" for ok, text in self.checks] + [""]
        if self.system:
            out += [f"<details><summary>{tr('h_system', L)}</summary>", "", "```"]
            out += [l[:300].replace("```", "'''") for l in self.system["lines"]] + ["```", "", "</details>", ""]
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
        if self.kind() in ("manual", "system"):
            for title, lines in network_tail():
                out += [f"<details><summary>{tr('h_network', L)} ({title})</summary>", "", "```"]
                out += [l[:300].replace("```", "'''") for l in lines] + ["```", "", "</details>", ""]
        if self.attach and not comment:
            out += diagnostics_markdown(self.attach, L)
        waiting = queued_reports() if not comment else []
        if waiting:
            out += [f"<details><summary>{tr('h_queue', L)}</summary>", ""] + [f"- {t}" for t in waiting]
            out += ["", "</details>", ""]
        out += ["---", "_" + tr("footer", L, redact=tr("yes" if redacted else "no", L)) + "_"]
        text = "\n".join(out)
        if redacted:
            text = redact(text, keep_names=[self.exe_name], keep_strings=self.safe_strings())
        if len(text) > BODY_LIMIT and self.attach:
            # Trop long avec les journaux : on les raccourcit plutôt que de couper la fin.
            global DIAG_LINES
            if DIAG_LINES > 10:
                saved, DIAG_LINES = DIAG_LINES, max(10, DIAG_LINES // 2)
                try:
                    return self.body(description, redacted, comment)
                finally:
                    DIAG_LINES = saved
        return text[:BODY_LIMIT]


# ── GitHub ───────────────────────────────────────────────────────────────

class NoRedirect(urllib.request.HTTPRedirectHandler):
    """Un jeton ne suit jamais une redirection vers un autre serveur."""
    def redirect_request(self, *args, **kwargs):
        return None


def http(method, url, payload=None, token=None, timeout=20):
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("User-Agent", "winlux-report")
    if data is not None:
        req.add_header("Content-Type", "application/json")
    opener = urllib.request.build_opener()
    if token:
        req.add_header("Authorization", f"Bearer {token}")
        opener = urllib.request.build_opener(NoRedirect)
    with opener.open(req, timeout=timeout) as response:
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


# ── Envoi automatique des plantages (#438) ──────────────────────────────
#
# Lancé par l'ouverture de session, sans fenêtre : chaque plantage du journal
# (en mémoire vive, perdu à l'arrêt) entre dans une file persistante, puis part
# quand le réseau et l'horloge le permettent ; sinon il attend le démarrage
# connecté suivant. Toujours anonymisé, rédigé en français (règle du dépôt).

AUTO_DIR = STATE / "auto-report"
QUEUE = AUTO_DIR / "queue"
AUTO_MAX_QUEUE = 50
AUTO_MAX_SENDS = int(os.environ.get("WINLUX_REPORT_MAX_SENDS", "5"))
AUTO_COMMENT_GAP = 24 * 3600
BUILD_INFO = Path(os.environ.get("WINLUX_BUILD_INFO", "/usr/share/winlux/BUILD-INFO"))
TIME_CONF = Path(os.environ.get("WINLUX_TIME_CONF", "/etc/winlux/time.conf"))
_sent_this_run = [0]


def auto_log(message):
    try:
        AUTO_DIR.mkdir(parents=True, exist_ok=True)
        log = AUTO_DIR / "journal.log"
        lines = log.read_text(errors="replace").splitlines()[-199:] if log.exists() else []
        lines.append(time.strftime("%Y-%m-%dT%H:%M:%S ") + message)
        log.write_text("\n".join(lines) + "\n")
    except OSError:
        pass


def load_json(path, default):
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError):
        return default


def save_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + f".{os.getpid()}")
    tmp.write_text(json.dumps(value, ensure_ascii=False))
    tmp.chmod(0o600)
    tmp.replace(path)


class Lock:
    """Verrou de fichier : une seule collecte ou un seul envoi à la fois."""
    def __init__(self, name, wait=True):
        import fcntl
        AUTO_DIR.mkdir(parents=True, exist_ok=True)
        self.file = open(AUTO_DIR / name, "a")
        try:
            fcntl.flock(self.file, fcntl.LOCK_EX | (0 if wait else fcntl.LOCK_NB))
            self.held = True
        except OSError:
            self.held = False

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.file.close()


def enqueue(report, key):
    QUEUE.mkdir(parents=True, exist_ok=True)
    entries = sorted(QUEUE.glob("*.json"))
    if len(entries) >= AUTO_MAX_QUEUE:
        # File pleine (longtemps hors ligne) : on garde les plus récents.
        for old in entries[: len(entries) - AUTO_MAX_QUEUE + 1]:
            old.unlink(missing_ok=True)
    description = tr("auto_desc", "fr")
    item = {"key": key, "signature": report.signature, "kind": report.kind(), "created": int(time.time()),
            "title": report.title(), "labels": report.labels() + [auto_urgency(report)],
            "body": report.body(description, redacted=True),
            "comment": report.body(description, redacted=True, comment=True)}
    name = time.strftime("%Y%m%d-%H%M%S-") + hashlib.sha256(key.encode()).hexdigest()[:8] + ".json"
    save_json(QUEUE / name, item)
    auto_log(f"en file : {report.signature or key} ({item['kind']})")


def auto_urgency(report):
    return "urgence:1-quotidien" if report.kind() == "system" else "urgence:2-confort"


def collect():
    """Met en file les plantages pas encore vus ; aucun accès réseau."""
    with Lock("collect.lock"):
        seen = load_json(AUTO_DIR / "seen.json", [])
        known = set(seen)
        added = 0
        for crash in crashes():
            key = f"crash-{crash.get('ts')}-{crash.get('pid')}-{crash.get('exe', '')}"
            if key in known:
                continue
            try:
                enqueue(Report(crash, language="fr"), key)
                added += 1
            except Exception as error:  # un rapport illisible ne bloque pas les autres
                auto_log(f"rapport impossible ({key}) : {error}")
            seen.append(key)
            known.add(key)
        system = previous_boot_problem()
        if system and f"system-{system['key']}" not in known:
            key = f"system-{system['key']}"
            try:
                enqueue(Report(system=system, language="fr"), key)
                added += 1
            except Exception as error:
                auto_log(f"rapport impossible ({key}) : {error}")
            seen.append(key)
        save_json(AUTO_DIR / "seen.json", seen[-500:])
        return added


def clock_ready(now=None):
    """Pas d'HTTPS avec une horloge fausse (« certificate is not yet valid ») :
    jamais avant la date de construction, et, si l'heure automatique est
    active, pas avant la première synchronisation (sauf après 15 minutes)."""
    now = now if now is not None else time.time()
    floor = int(os.environ.get("WINLUX_CLOCK_FLOOR", "0") or 0)
    try:
        for line in BUILD_INFO.read_text().splitlines():
            if line.startswith("built_at_utc="):
                floor = max(floor, calendar.timegm(time.strptime(line.split("=", 1)[1].strip(), "%Y-%m-%dT%H:%M:%SZ")))
    except (OSError, ValueError):
        pass
    if now < max(floor, 1767225600):  # 2026-01-01
        return False
    if load_json(RUN / "time-sync.json", {}).get("last"):
        return True
    if read_conf(TIME_CONF).get("AUTO", "on").lower() in ("off", "0", "false", "no"):
        return True
    try:
        uptime = float(Path("/proc/uptime").read_text().split()[0])
    except (OSError, ValueError, IndexError):
        uptime = 0
    return uptime > 900


def search_signature(sig, token=None, api=None):
    query = urllib.parse.quote(f'repo:{REPO} is:issue "{sig}"')
    items = http("GET", f"{api or API}/search/issues?per_page=5&q={query}", token=token).get("items", [])
    issues = [i for i in items if "pull_request" not in i]
    issues.sort(key=lambda i: (i.get("state") != "open", i["number"]))
    return issues[0] if issues else None


def deliver(conf, item, sent, token=None, api=None):
    """Envoie un élément de la file ; vrai si GitHub l'a reçu (ou s'il est
    volontairement fondu dans un envoi récent)."""
    sig = item.get("signature") or item["key"]
    record = sent.get(sig, {})
    now = int(time.time())
    if record.get("issue") and now - record.get("last", 0) < AUTO_COMMENT_GAP:
        record["pending"] = record.get("pending", 0) + 1
        sent[sig] = record
        auto_log(f"déjà envoyé aujourd'hui : {sig} (#{record['issue']})")
        return True
    count = record.get("pending", 0) + 1
    comment = item["comment"]
    if count > 1:
        comment = tr("auto_count", "fr", count=count) + "\n\n" + comment
    kind = "token" if token else transport(conf)
    api = api or API
    if kind == "relay":
        payload = {"title": item["title"], "body": item["body"], "labels": item["labels"],
                   "issue": record.get("issue")}
        if record.get("issue"):
            payload["body"] = comment
        answer = http("POST", conf["RELAY"].rstrip("/") + "/report", payload)
        number = answer.get("issue") or record.get("issue")
    else:
        token = token or Path(token_path(conf)).read_text().strip()
        number = record.get("issue")
        if not number and item.get("signature"):
            found = search_signature(item["signature"], token, api)
            if found and found.get("state") == "open":
                number = found["number"]
            elif found:
                item["body"] += f"\n\nDéjà signalé dans #{found['number']} (fermée) : le défaut revient."
        if number:
            http("POST", f"{api}/repos/{REPO}/issues/{number}/comments", {"body": comment}, token)
        else:
            number = http("POST", f"{api}/repos/{REPO}/issues",
                          {"title": item["title"], "body": item["body"], "labels": item["labels"]}, token)["number"]
    sent[sig] = {"issue": number, "last": now, "pending": 0, "total": record.get("total", 0) + count}
    auto_log(f"envoyé : {sig} -> #{number}")
    return True


def flush(conf=None):
    """Vide la file vers GitHub. Rend le nombre d'envois, ou None si le
    réseau, l'horloge ou la cible manquent (la file attend)."""
    conf = conf or config()
    kind = transport(conf)
    if kind not in ("relay", "token", "system"):
        return None
    if not clock_ready():
        auto_log("horloge pas encore à l'heure : envoi reporté")
        return None
    lock = Lock("send.lock", wait=False)
    if not lock.held:
        return None
    with lock:
        sent = load_json(AUTO_DIR / "sent.json", {})
        done = 0
        try:
            for path in sorted(QUEUE.glob("*.json")):
                if _sent_this_run[0] >= AUTO_MAX_SENDS:
                    auto_log("limite d'envois atteinte : la suite partira au prochain démarrage")
                    break
                item = load_json(path, None)
                if not item or "title" not in item:
                    path.unlink(missing_ok=True)
                    continue
                try:
                    if kind == "system":
                        code = hand_to_root(item)
                        if code == 65:  # refusé par la vérification root : inutile de réessayer
                            auto_log("rapport refusé par le service système")
                            path.unlink(missing_ok=True)
                            continue
                        if code != 0:
                            auto_log(f"service système : envoi reporté ({code})")
                            return done or None
                    else:
                        deliver(conf, item, sent)
                except urllib.error.HTTPError as error:
                    auto_log(f"refus de GitHub ({error.code}) : envoi reporté")
                    if error.code == 422:  # rapport invalide : inutile de réessayer
                        path.unlink(missing_ok=True)
                        continue
                    return done or None
                except (OSError, ValueError, KeyError) as error:
                    auto_log(f"réseau indisponible ({error}) : envoi reporté")
                    return done or None
                path.unlink(missing_ok=True)
                _sent_this_run[0] += 1
                done += 1
        finally:
            save_json(AUTO_DIR / "sent.json", sent)
        return done


def background(once=False):
    """Ouverture de session : attend, puis collecte et envoie en tâche de fond
    de priorité minimale, sans fenêtre ni notification."""
    try:
        os.nice(19)
    except OSError:
        pass
    lock = Lock("background.lock", wait=False)
    if not lock.held:
        return 0
    with lock:
        time.sleep(float(os.environ.get("WINLUX_REPORT_DELAY", "120")))
        interval = float(os.environ.get("WINLUX_REPORT_INTERVAL", "300"))
        while True:
            conf = config()
            if auto_enabled(conf):
                try:
                    collect()
                    flush(conf)
                except Exception as error:  # jamais d'erreur visible
                    auto_log(f"erreur : {error}")
            if once:
                return 0
            time.sleep(interval)


def queue_size():
    try:
        return len(list(QUEUE.glob("*.json")))
    except OSError:
        return 0


def auto_status():
    conf = config()
    sent = load_json(AUTO_DIR / "sent.json", {})
    print(f"auto_setting={conf['AUTO']}")
    print(f"auto_enabled={1 if auto_enabled(conf) else 0}")
    print(f"auto_token={1 if (token_path(conf) or SYSTEM_TOKEN.exists()) else 0}")
    print(f"auto_transport={transport(conf)}")
    print(f"auto_queue={queue_size()}")
    print(f"auto_sent={sum(r.get('total', 0) for r in sent.values())}")


def set_token(text):
    """Jeton du système, écrit par root seul (Paramètres passe par le
    relais root avec confirmation administrateur) ; jamais sur la ligne de
    commande, jamais dans un journal."""
    token = text.strip()
    if not re.fullmatch(r"(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_\w{20,})", token):
        raise SystemExit("winlux-report: jeton GitHub invalide")
    if os.geteuid() != 0 and not os.environ.get("WINLUX_REPORT_TOKEN"):
        raise SystemExit("winlux-report: droits administrateur requis")
    SYSTEM_TOKEN.parent.mkdir(parents=True, exist_ok=True)
    tmp = SYSTEM_TOKEN.with_name(SYSTEM_TOKEN.name + f".{os.getpid()}")
    old = os.umask(0o077)
    try:
        fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w") as handle:
            handle.write(token + "\n")
        os.chmod(tmp, 0o600)
        tmp.replace(SYSTEM_TOKEN)
    finally:
        os.umask(old)


def hand_to_root(item):
    """Remet un rapport au relais root par l'entrée standard. Codes : 0 envoyé,
    65 refusé, 75 à réessayer plus tard."""
    if os.geteuid() == 0:
        return send_item(item)
    try:
        result = subprocess.run([ROOT_TOOL, "control", "report-send"], input=json.dumps(item),
                                capture_output=True, text=True, timeout=180)
    except (OSError, subprocess.SubprocessError):
        return 75
    return result.returncode


AUTO_TITLES = ("[Plantage] ", "[Fonction manquante] ", "[Erreur interne] ", "[Arrêt du système] ")
AUTO_LABELS = {"bug", "signalement-utilisateur", "plantage", "fonction-manquante", "compatibilité-windows",
               "runtime", "bureau", "urgence:1-quotidien", "urgence:2-confort"}


def check_item(item, token=""):
    """Vérification côté root d'un rapport remis par un compte (un .exe
    pourrait en fabriquer) : forme d'un rapport automatique, étiquettes
    connues, tailles bornées, mentions neutralisées, données masquées."""
    if not isinstance(item, dict):
        raise ValueError("rapport")
    sig, title = item.get("signature"), item.get("title")
    if not isinstance(sig, str) or not re.fullmatch(r"wlx-(?:sys-)?[0-9a-f]{12}", sig):
        raise ValueError("signature")
    if not isinstance(title, str) or not title.startswith(AUTO_TITLES) or len(title) > 256:
        raise ValueError("titre")
    clean = {"signature": sig, "key": str(item.get("key", sig))[:200]}
    for field in ("title", "body", "comment"):
        text = item.get(field)
        if not isinstance(text, str) or len(text) > BODY_LIMIT or (field != "title" and f"`{sig}`" not in text):
            raise ValueError(field)
        text = re.sub(r"@(?=[\w-])", "@​", text)
        if token:
            text = text.replace(token, "<jeton>")
        clean[field] = redact(text, keep_strings=re.findall(r"\b\d+(?:\.\d+){3}\b", text))
    labels = item.get("labels")
    if not isinstance(labels, list):
        raise ValueError("étiquettes")
    clean["labels"] = [l for l in labels if isinstance(l, str) and l in AUTO_LABELS][:8]
    return clean


def root_budget():
    """Au plus ROOT_MAX_PER_BOOT envois par démarrage et ROOT_MAX_PER_DAY par
    jour, quel que soit le compte ou le programme qui les demande."""
    state = load_json(ROOT_STATE / "budget.json", {})
    boot = read_text("/proc/sys/kernel/random/boot_id")
    now = int(time.time())
    if state.get("boot") != boot:
        state = {"boot": boot, "count": 0, "days": state.get("days", [])}
    state["days"] = [t for t in state.get("days", []) if now - t < 86400]
    if state["count"] >= ROOT_MAX_PER_BOOT or len(state["days"]) >= ROOT_MAX_PER_DAY:
        return None
    return state


def send_item(item):
    """Côté root : vérifie et envoie un rapport avec le jeton du système."""
    try:
        token = SYSTEM_TOKEN.read_text().strip()
    except OSError:
        return 75
    try:
        item = check_item(item, token)
    except ValueError:
        return 65
    if not clock_ready():
        return 75
    ROOT_STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    with Lock_at(ROOT_STATE / "send.lock"):
        budget = root_budget()
        if budget is None:
            return 75
        sent = load_json(ROOT_STATE / "sent.json", {})
        try:
            deliver({}, item, sent, token=token, api=SYSTEM_API)
        except urllib.error.HTTPError as error:
            return 65 if error.code == 422 else 75
        except (OSError, ValueError, KeyError):
            return 75
        budget["count"] += 1
        budget["days"].append(int(time.time()))
        save_json(ROOT_STATE / "budget.json", budget)
        save_json(ROOT_STATE / "sent.json", sent)
    return 0


def send_stdin():
    try:
        item = json.loads(sys.stdin.read(BODY_LIMIT * 3 + 4096))
    except ValueError:
        return 65
    return send_item(item)


class Lock_at(Lock):
    def __init__(self, path):
        import fcntl
        self.file = open(path, "a")
        fcntl.flock(self.file, fcntl.LOCK_EX)
        self.held = True


# ── Interface ────────────────────────────────────────────────────────────

def gui(report, conf):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gtk, Gdk, GLib

    L = report.language
    win = Gtk.Window(title=f"{tr('window', L)} — Winlux")
    win.set_icon_name("dialog-warning" if report.crash or report.system else "help-browser")
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
      #diag { padding: 6px 10px; }
      #headline { font-size: 15pt; font-weight: bold; }
      window box, window label { background-color: transparent; }
      label { color: #eceef6; }
      button { padding: 8px 14px; background: #30384b; background-image: none; color: #eceef6;
               border: 1px solid #48536b; box-shadow: none; text-shadow: none; }
      button label { color: #eceef6; }
      button:hover { background: #414b64; }
      button.suggested-action, button.suggested-action label { background: #3a56d4; color: #ffffff; border-color: #3a56d4; }
      button.suggested-action:hover { background: #4664e6; }
    """)
    # Above the user gtk.css (USER priority): this window always keeps its dark colors.
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 1)

    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=18)
    win.add(box)
    headline = Gtk.Label(label=report.headline(), xalign=0, wrap=True)
    headline.set_name("headline")
    box.pack_start(headline, False, False, 0)
    if report.crash or report.system:
        cause = Gtk.Label(label=f"{tr('cause', L)}{' :' if L == 'fr' else ':'} {report.cause}", xalign=0, wrap=True)
        cause.set_name("cause")
        box.pack_start(cause, False, False, 0)
    if report.checks or report.steps:
        diag = Gtk.Expander(label=tr("h_checks", L) + " · " + tr("h_steps", L))
        lines = [("✔ " if ok else "⚠ ") + text for ok, text in report.checks]
        if report.steps:
            lines += ["", tr("h_steps", L)]
            lines += [(time.strftime("%H:%M:%S", time.localtime(ts)) + "  " if ts else "") + text
                      for ts, text in report.steps]
        diag_label = Gtk.Label(label="\n".join(lines), xalign=0, wrap=True, selectable=True)
        diag_label.set_name("diag")
        diag.add(diag_label)
        diag.set_expanded(any(not ok for ok, _ in report.checks))
        box.pack_start(diag, False, False, 0)

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

    attach_row = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE)
    attach_row.add(Gtk.Label(label=tr("attach_logs", L), xalign=0))
    attach_checks = {}
    for domain in DIAG_DOMAINS:
        check = Gtk.CheckButton(label=tr("d_" + domain, L))
        # Problème du système ou libre : journaux joints d'office (décochables).
        check.set_active(report.crash is None)
        attach_checks[domain] = check
        attach_row.add(check)
    box.pack_start(attach_row, False, False, 0)
    report.attach = [d for d, c in attach_checks.items() if c.get_active()]

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
    if kind == "system":
        # Le jeton du système ne sert qu'aux rapports automatiques vérifiés par
        # root : un rapport libre passe par le navigateur.
        kind = "browser"
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

    def on_attach(*_):
        report.attach = [d for d, c in attach_checks.items() if c.get_active()]
        if not state["edited"]:
            regenerate()
    for check in attach_checks.values():
        check.connect("toggled", on_attach)
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
        elif kind == "browser" and not explain_account(short(path)):
            finish(tr("saved", L, path=path))
        else:
            fallback(title, body, labels, issue, path, None)

    def short(path):
        home = str(Path.home())
        text = str(path)
        return "~" + text[len(home):] if home != "/" and text.startswith(home) else text

    def explain_account(path):
        """Tant que le relais n'est pas en ligne, l'envoi passe par le site
        GitHub : on le dit clairement avant d'ouvrir le navigateur."""
        dialog = Gtk.MessageDialog(transient_for=win, modal=True, message_type=Gtk.MessageType.INFO,
                                   text=tr("acct_title", L))
        notice.set_text("")
        dialog.format_secondary_text(tr("acct_text", L, path=path))
        dialog.add_button(tr("acct_later", L), 0)
        dialog.add_button(tr("acct_signup", L), 2)
        dialog.add_button(tr("acct_open", L), 1).get_style_context().add_class("suggested-action")
        dialog.set_default_response(1)
        while True:
            answer = dialog.run()
            if answer == 2:
                open_url("https://github.com/signup")
                continue
            break
        dialog.destroy()
        return answer == 1

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
    if "--background" in args:
        return background("--once" in args)
    if "--collect" in args:
        if auto_enabled():
            collect()
        return 0
    if "--auto-status" in args:
        auto_status()
        return 0
    if "--set-token" in args:
        set_token(sys.stdin.read())
        return 0
    if "--forget-token" in args:
        SYSTEM_TOKEN.unlink(missing_ok=True)
        return 0
    if "--send-stdin" in args:
        return send_stdin()
    if "--open-saved" in args:
        directory = STATE / "reports"
        directory.mkdir(parents=True, exist_ok=True)
        return 0 if open_url(str(directory)) else 1
    conf = config()
    crash = None
    system = None
    if "--system" in args:
        system = previous_boot_problem()
        if not system:
            if "--print" in args:
                print(tr("nothing_system"))
            return 3
        if "--print" not in args and "--force" not in args:
            # Envoi automatique actif : l'arrêt part en arrière-plan, sans fenêtre.
            if auto_enabled(conf) and transport(conf) in ("relay", "token", "system"):
                return 0
            if conf["OFFER"] == "off":
                return 4
            if not mark_system_offered(system):
                return 0
    elif "--crash-at" in args:
        entries = crashes()
        index = int(option("--crash-at", "-1") or -1)
        crash = entries[index] if 0 <= index < len(entries) else None
    elif "--crash" in args:
        crash = last_crash(int(option("--since", "0") or 0))
        if not crash:
            if "--print" in args:
                print(tr("nothing"))
            return 3
        if "--print" not in args and auto_enabled(conf):
            # Gardé tout de suite : crash.log est perdu à l'arrêt.
            try:
                subprocess.Popen([sys.executable, os.path.abspath(sys.argv[0]), "--collect"],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            except OSError:
                pass
        if "--print" not in args and "--force" not in args:
            # 4 : proposition désactivée dans Paramètres, le lanceur affiche
            # alors son simple message d'erreur.
            if conf["OFFER"] == "off":
                return 4
            if not mark_offered(crash):
                return 0
    report = Report(crash, option("--component"), system=system)
    if "--attach" in args:
        wanted = (option("--attach", "") or "").split(",")
        report.attach = [d for d in DIAG_DOMAINS if d in wanted or "all" in wanted]
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
