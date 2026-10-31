#!/usr/bin/python3
"""Native settings: nonblocking control actions and live Linux status."""
import os
from pathlib import Path
import subprocess
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, GLib


# ── Langue de l'interface (Paramètres > Langue) : fr, en, es ou de ──────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    'Echec de la mise à jour : ': ('Update failed: ', 'Error en la actualización: ', 'Update fehlgeschlagen: '),
    'aucun détail dans le journal': ('no details in the log', 'ningún detalle en el registro', 'keine Details im Protokoll'),
    "Echec de l'operation. Le journal indique la cause.": ('The operation failed. The log shows the cause.', 'La operación ha fallado. El registro indica la causa.', 'Der Vorgang ist fehlgeschlagen. Die Ursache steht im Protokoll.'),
    'Operation en cours...': ('Operation in progress...', 'Operación en curso...', 'Vorgang läuft...'),
    'Operation terminee.': ('Operation complete.', 'Operación terminada.', 'Vorgang abgeschlossen.'),
    'Une operation est deja en cours. Consultez le journal.': ('An operation is already in progress. See the log.', 'Ya hay una operación en curso. Consulte el registro.', 'Es läuft bereits ein Vorgang. Siehe Protokoll.'),
    'Version installée : ': ('Installed version: ', 'Versión instalada: ', 'Installierte Version: '),
    'Version disponible : ': ('Available version: ', 'Versión disponible: ', 'Verfügbare Version: '),
    'Action en attente : ': ('Pending action: ', 'Acción pendiente: ', 'Ausstehende Aktion: '),
    'Portée : ': ('Scope: ', 'Alcance: ', 'Umfang: '),
    'Composants : ': ('Components: ', 'Componentes: ', 'Komponenten: '),
    'Taille à télécharger : ': ('Download size: ', 'Tamaño de descarga: ', 'Downloadgröße: '),
    'Taille décompressée : ': ('Unpacked size: ', 'Tamaño descomprimido: ', 'Entpackte Größe: '),
    'Action après installation : ': ('After installation: ', 'Tras la instalación: ', 'Nach der Installation: '),
    ' octets': (' bytes', ' bytes', ' Bytes'),
    ' Kio (hors sauvegarde et marge)': (' KiB (excluding backup and margin)', ' KiB (sin copia de seguridad ni margen)', ' KiB (ohne Sicherung und Reserve)'),
    'Winlux est à jour.': ('Winlux is up to date.', 'Winlux está actualizado.', 'Winlux ist auf dem neuesten Stand.'),
    'Aucune vérification disponible. Lancez une recherche de mises à jour (--check).': ('No check available yet. Search for updates (--check).', 'Todavía no hay ninguna comprobación. Busque actualizaciones (--check).', 'Noch keine Prüfung verfügbar. Suchen Sie nach Updates (--check).'),
    'Aucune vérification disponible. Lancez Rechercher.': ('No check available yet. Click Check.', 'Todavía no hay ninguna comprobación. Haga clic en Buscar.', 'Noch keine Prüfung verfügbar. Klicken Sie auf Suchen.'),
    'redémarrage du système': ('system restart', 'reinicio del sistema', 'Systemneustart'),
    'redémarrage de la session graphique': ('graphical session restart', 'reinicio de la sesión gráfica', 'Neustart der grafischen Sitzung'),
    'redémarrage conseillé': ('restart recommended', 'se recomienda reiniciar', 'Neustart empfohlen'),
    'bureau et runtime Winlux (APT)': ('Winlux desktop and runtime (APT)', 'escritorio y runtime de Winlux (APT)', 'Winlux-Desktop und -Runtime (APT)'),
    'Mio': ('MiB', 'MiB', 'MiB'),
    'Système': ('System', 'Sistema', 'System'),
    'Applications': ('Apps', 'Aplicaciones', 'Apps'),
    'Réseau': ('Network', 'Red', 'Netzwerk'),
    'Son': ('Sound', 'Sonido', 'Sound'),
    'Clavier': ('Keyboard', 'Teclado', 'Tastatur'),
    'Souris': ('Mouse', 'Mouse', 'Maus'),
    'Personnalisation': ('Personalization', 'Personalización', 'Personalisierung'),
    'Mises à jour': ('Updates', 'Actualizaciones', 'Updates'),
    'Écran': ('Display', 'Pantalla', 'Bildschirm'),
    'À propos': ('About', 'Acerca de', 'Info'),
    'Programmes Windows': ('Windows programs', 'Programas de Windows', 'Windows-Programme'),
    'Navigateurs': ('Browsers', 'Navegadores', 'Browser'),
    'Accessibilité': ('Accessibility', 'Accesibilidad', 'Barrierefreiheit'),
    'Effets visuels': ('Visual effects', 'Efectos visuales', 'Visuelle Effekte'),
    'Localisation': ('Language & region', 'Idioma y región', 'Sprache und Region'),
    'Gestionnaire de fichiers': ('File manager', 'Administrador de archivos', 'Dateimanager'),
    'Son avancé': ('Advanced sound', 'Sonido avanzado', 'Erweiterter Sound'),
    'Mises à jour système': ('System updates', 'Actualizaciones del sistema', 'Systemupdates'),
    'Confidentialité': ('Privacy', 'Privacidad', 'Datenschutz'),
    'Processeur': ('Processor', 'Procesador', 'Prozessor'),
    'Lecteurs réseau': ('Network drives', 'Unidades de red', 'Netzlaufwerke'),
    'Personnalisation du bureau': ('Desktop personalization', 'Personalización del escritorio', 'Desktop-Personalisierung'),
    'Bluetooth et appareils': ('Bluetooth & devices', 'Bluetooth y dispositivos', 'Bluetooth und Geräte'),
    'Alimentation et batterie': ('Power & battery', 'Energía y batería', 'Energie und Akku'),
    'Graphismes': ('Graphics', 'Gráficos', 'Grafik'),
    'Version': ('Version', 'Versión', 'Version'),
    'Noyau': ('Kernel', 'Kernel', 'Kernel'),
    'Connexion': ('Connection', 'Conexión', 'Verbindung'),
    'Volume': ('Volume', 'Volumen', 'Lautstärke'),
    'Batterie': ('Battery', 'Batería', 'Akku'),
    'Mode': ('Mode', 'Modo', 'Modus'),
    'Écran éteint après': ('Turn off screen after', 'Apagar pantalla tras', 'Bildschirm aus nach'),
    'Veille après': ('Sleep after', 'Suspender tras', 'Energiesparmodus nach'),
    'Capot': ('Lid', 'Tapa', 'Deckel'),
    "Bouton d'alimentation": ('Power button', 'Botón de encendido', 'Netzschalter'),
    'Luminosité (%)': ('Brightness (%)', 'Brillo (%)', 'Helligkeit (%)'),
    'Disposition': ('Layout', 'Distribución', 'Layout'),
    'Toucher-clic': ('Tap to click', 'Tocar para hacer clic', 'Tippen zum Klicken'),
    'Vitesse': ('Speed', 'Velocidad', 'Geschwindigkeit'),
    'Alignement': ('Alignment', 'Alineación', 'Ausrichtung'),
    'Définition': ('Resolution', 'Resolución', 'Auflösung'),
    'Échelle': ('Scale', 'Escala', 'Skalierung'),
    'Navigateur par défaut': ('Default browser', 'Navegador predeterminado', 'Standardbrowser'),
    'Relancer le bureau': ('Restart the desktop', 'Reiniciar el escritorio', 'Desktop neu starten'),
    'Redémarrer': ('Restart', 'Reiniciar', 'Neu starten'),
    'Éteindre': ('Shut down', 'Apagar', 'Herunterfahren'),
    'Signaler un problème': ('Report a problem', 'Informar de un problema', 'Problem melden'),
    'Renouveler la connexion': ('Renew the connection', 'Renovar la conexión', 'Verbindung erneuern'),
    'Volume −': ('Volume −', 'Volumen −', 'Lautstärke −'),
    'Muet / rétablir': ('Mute / unmute', 'Silenciar / restablecer', 'Stumm / Ton an'),
    'Volume +': ('Volume +', 'Volumen +', 'Lautstärke +'),
    'Français AZERTY': ('French AZERTY', 'Francés AZERTY', 'Französisch AZERTY'),
    'Anglais QWERTY': ('English QWERTY', 'Inglés QWERTY', 'Englisch QWERTY'),
    'Activer toucher-clic': ('Turn on tap to click', 'Activar tocar para hacer clic', 'Tippen zum Klicken aktivieren'),
    'Désactiver toucher-clic': ('Turn off tap to click', 'Desactivar tocar para hacer clic', 'Tippen zum Klicken deaktivieren'),
    'Lente': ('Slow', 'Lenta', 'Langsam'),
    'Normale': ('Normal', 'Normal', 'Normal'),
    'Rapide': ('Fast', 'Rápida', 'Schnell'),
    'À gauche': ('Left', 'A la izquierda', 'Links'),
    'Centrer': ('Center', 'Centrar', 'Zentriert'),
    'Ajouter Corbeille': ('Add Recycle Bin', 'Agregar Papelera de reciclaje', 'Papierkorb hinzufügen'),
    'Ajouter Explorateur': ('Add File Explorer', 'Agregar Explorador', 'Explorer hinzufügen'),
    'Ajouter Ce PC': ('Add This PC', 'Agregar Este equipo', 'Dieser PC hinzufügen'),
    'Ajouter Documents': ('Add Documents', 'Agregar Documentos', 'Dokumente hinzufügen'),
    'Rechercher': ('Check', 'Buscar', 'Suchen'),
    'Installer': ('Install', 'Instalar', 'Installieren'),
    'Restaurer': ('Restore', 'Restaurar', 'Wiederherstellen'),
    'Mises à jour auto : activer': ('Automatic updates: turn on', 'Actualizaciones automáticas: activar', 'Automatische Updates: aktivieren'),
    'Désactiver': ('Turn off', 'Desactivar', 'Deaktivieren'),
    'Performances maximales': ('Best performance', 'Máximo rendimiento', 'Höchstleistung'),
    'Équilibré': ('Balanced', 'Equilibrado', 'Ausbalanciert'),
    "Économie d'énergie": ('Power saver', 'Ahorro de energía', 'Energiesparmodus'),
    'Écran (secteur)': ('Screen (plugged in)', 'Pantalla (enchufado)', 'Bildschirm (Netzbetrieb)'),
    'Écran (batterie)': ('Screen (on battery)', 'Pantalla (batería)', 'Bildschirm (Akku)'),
    'Veille (secteur)': ('Sleep (plugged in)', 'Suspensión (enchufado)', 'Energiesparmodus (Netzbetrieb)'),
    'Veille (batterie)': ('Sleep (on battery)', 'Suspensión (batería)', 'Energiesparmodus (Akku)'),
    'Luminosité −': ('Brightness −', 'Brillo −', 'Helligkeit −'),
    'Luminosité +': ('Brightness +', 'Brillo +', 'Helligkeit +'),
    'Mettre en veille': ('Sleep', 'Suspender', 'Energie sparen'),
    'Veille prolongée': ('Hibernate', 'Hibernar', 'Ruhezustand'),
    'Suivre la taille de la VM': ('Follow the VM size', 'Seguir el tamaño de la VM', 'VM-Größe folgen'),
    'Définition fixe': ('Fixed resolution', 'Resolución fija', 'Feste Auflösung'),
    'Échelle −': ('Scale −', 'Escala −', 'Skalierung −'),
    'Automatique': ('Automatic', 'Automática', 'Automatisch'),
    'Échelle +': ('Scale +', 'Escala +', 'Skalierung +'),
    'Informations graphiques indisponibles (runtime Winlux introuvable).': ('Graphics information unavailable (Winlux runtime not found).', 'Información gráfica no disponible (no se encuentra el runtime de Winlux).', 'Grafikinformationen nicht verfügbar (Winlux-Runtime nicht gefunden).'),
    'Aucune carte graphique détectée (machine virtuelle ou affichage de secours).': ('No graphics card found (virtual machine or fallback display).', 'No se detectó ninguna tarjeta gráfica (máquina virtual o pantalla de emergencia).', 'Keine Grafikkarte erkannt (virtuelle Maschine oder Notfallanzeige).'),
    "GPU intégré (économie d'énergie)": ('Integrated GPU (power saving)', 'GPU integrada (ahorro de energía)', 'Integrierte GPU (Energiesparen)'),
    'GPU dédié (hautes performances)': ('Dedicated GPU (high performance)', 'GPU dedicada (alto rendimiento)', 'Dedizierte GPU (Hochleistung)'),
    'Carte graphique': ('Graphics card', 'Tarjeta gráfica', 'Grafikkarte'),
    "\nGPU par application : clic droit sur un .exe > Réglages de l'exécutable.": ('\nGPU per app: right-click an .exe > Executable settings.', '\nGPU por aplicación: clic derecho en un .exe > Configuración del ejecutable.', '\nGPU pro App: Rechtsklick auf eine .exe > Einstellungen der ausführbaren Datei.'),
    'Paramètres Winlux': ('Winlux Settings', 'Configuración de Winlux', 'Winlux-Einstellungen'),
    'WINLUX · PARAMÈTRES': ('WINLUX · SETTINGS', 'WINLUX · CONFIGURACIÓN', 'WINLUX · EINSTELLUNGEN'),
    'Rechercher un réglage': ('Find a setting', 'Buscar una configuración', 'Einstellung suchen'),
    'Vos préférences et l’état réel de votre ordinateur.': ('Your preferences and the real state of your computer.', 'Sus preferencias y el estado real de su equipo.', 'Ihre Einstellungen und der tatsächliche Zustand Ihres Computers.'),
    'Chargement…': ('Loading…', 'Cargando…', 'Wird geladen…'),
    'Installer, désinstaller et gérer le démarrage': ('Install, uninstall and manage startup', 'Instalar, desinstalar y administrar el inicio', 'Installieren, deinstallieren und Autostart verwalten'),
    'Wi-Fi et Bluetooth': ('Wi-Fi and Bluetooth', 'Wi-Fi y Bluetooth', 'WLAN und Bluetooth'),
    'Réglages rapides': ('Quick settings', 'Configuración rápida', 'Schnelleinstellungen'),
    'Thèmes, accent, fond et barre des tâches': ('Themes, accent, background and taskbar', 'Temas, color de énfasis, fondo y barra de tareas', 'Designs, Akzentfarbe, Hintergrund und Taskleiste'),
    'Appliquer la définition': ('Apply resolution', 'Aplicar resolución', 'Auflösung übernehmen'),
    'Ouvrir les réglages détaillés de cette catégorie :': ('Open the detailed settings for this category:', 'Abrir la configuración detallada de esta categoría:', 'Detaillierte Einstellungen dieser Kategorie öffnen:'),
    'Réglages avancés': ('Advanced settings', 'Configuración avanzada', 'Erweiterte Einstellungen'),
    'La préparation des mises à jour utilise le disque. Gardez la VM allumée pendant l’installation.': ('Preparing updates uses the disk. Keep the VM running during installation.', 'La preparación de las actualizaciones usa el disco. Mantenga la VM encendida durante la instalación.', 'Die Vorbereitung der Updates nutzt den Datenträger. Lassen Sie die VM während der Installation eingeschaltet.'),
    'Secours et restauration': ('Recovery and restore', 'Recuperación y restauración', 'Wiederherstellung'),
    'Redémarrer après installation': ('Restart after installation', 'Reiniciar tras la instalación', 'Nach der Installation neu starten'),
    'Confirmer cette opération ?': ('Confirm this operation?', '¿Confirmar esta operación?', 'Diesen Vorgang bestätigen?'),
    'Enregistrez vos documents avant de continuer.': ('Save your documents before continuing.', 'Guarde sus documentos antes de continuar.', 'Speichern Sie Ihre Dokumente, bevor Sie fortfahren.'),
    'Impossible de lancer l’opération': ('Unable to start the operation', 'No se puede iniciar la operación', 'Der Vorgang kann nicht gestartet werden'),
    'Opération en cours…': ('Operation in progress…', 'Operación en curso…', 'Vorgang läuft…'),
    'Aucun détail n’a été écrit dans le journal.': ('No details were written to the log.', 'No se escribió ningún detalle en el registro.', 'Es wurden keine Details ins Protokoll geschrieben.'),
    'L’opération a échoué': ('The operation failed', 'La operación ha fallado', 'Der Vorgang ist fehlgeschlagen'),
    'Indisponible': ('Unavailable', 'No disponible', 'Nicht verfügbar'),
    'Installation': ('Installing', 'Instalación', 'Installation'),
    'Sauvegarde': ('Backup', 'Copia de seguridad', 'Sicherung'),
    'Recherche et préparation': ('Checking and preparing', 'Búsqueda y preparación', 'Suche und Vorbereitung'),
    'Téléchargement': ('Downloading', 'Descarga', 'Download'),
    'Vérification': ('Verifying', 'Verificación', 'Überprüfung'),
    'Décompression': ('Unpacking', 'Descompresión', 'Entpacken'),
    'Finalisation': ('Finishing', 'Finalización', 'Abschluss'),
    'Restauration': ('Restore', 'Restauración', 'Wiederherstellung'),
    'Recherche': ('Check', 'Búsqueda', 'Suche'),
    'Mise à jour': ('Update', 'Actualización', 'Update'),
    'La mise à jour a échoué': ('The update failed', 'La actualización ha fallado', 'Das Update ist fehlgeschlagen'),
    'Installer {name}': ('Install {name}', 'Instalar {name}', '{name} installieren'),
    'Par défaut : {name}': ('Default: {name}', 'Predeterminado: {name}', 'Standard: {name}'),
    'Ouvrir {name}': ('Open {name}', 'Abrir {name}', '{name} öffnen'),
    '{label} : {value}': ('{label}: {value}', '{label}: {value}', '{label}: {value}'),
    'Pilote {driver}': ('Driver {driver}', 'Controlador {driver}', 'Treiber {driver}'),
    'inconnu': ('unknown', 'desconocido', 'unbekannt'),
    'Charge {load} %': ('Load {load}%', 'Carga {load} %', 'Last {load} %'),
    'VRAM {used} / {total} Mo': ('VRAM {used} / {total} MB', 'VRAM {used} / {total} MB', 'VRAM {used} / {total} MB'),
    'Fréquence {freq} MHz': ('Clock {freq} MHz', 'Frecuencia {freq} MHz', 'Takt {freq} MHz'),
    'Encodeur {value} %': ('Encoder {value}%', 'Codificador {value} %', 'Encoder {value} %'),
    'Décodeur {value} %': ('Decoder {value}%', 'Decodificador {value} %', 'Decoder {value} %'),
    'Code : {code}\n\n{detail}': ('Code: {code}\n\n{detail}', 'Código: {code}\n\n{detail}', 'Code: {code}\n\n{detail}'),
    'Étape : {stage} (code {code})\n\nDernières lignes du journal :\n{detail}': ('Step: {stage} (code {code})\n\nLast lines of the log:\n{detail}', 'Etapa: {stage} (código {code})\n\nÚltimas líneas del registro:\n{detail}', 'Schritt: {stage} (Code {code})\n\nLetzte Zeilen des Protokolls:\n{detail}'),
    '{done}/{total} fichiers': ('{done}/{total} files', '{done}/{total} archivos', '{done}/{total} Dateien'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


PAGES = [_t("Système"), _t("Applications"), _t("Réseau"), _t("Son"), _t("Clavier"),
         _t("Souris"), _t("Personnalisation"), _t("Mises à jour"), _t("Écran"), _t("À propos"),
         _t("Programmes Windows"), _t("Navigateurs"), _t("Accessibilité"), _t("Effets visuels"),
         _t("Localisation"), _t("Gestionnaire de fichiers"), _t("Son avancé"), _t("Mises à jour système"),
         _t("Confidentialité"), _t("Processeur"), _t("Lecteurs réseau"), _t("Personnalisation du bureau"),
         _t("Bluetooth et appareils"), _t("Alimentation et batterie"), _t("Graphismes")]
GRAPHICS_PAGE = 24
FIELDS = {
    0: [(_t("Version"), "version"), (_t("Noyau"), "kernel")],
    2: [(_t("Connexion"), "network")], 3: [(_t("Volume"), "audio")],
    23: [(_t("Batterie"), "battery"), (_t("Mode"), "power_profile"), (_t("Écran éteint après"), "power_screen_now"),
         (_t("Veille après"), "power_sleep_now"), (_t("Capot"), "power_lid"), (_t("Bouton d'alimentation"), "power_button"),
         (_t("Luminosité (%)"), "power_brightness")],
    4: [(_t("Disposition"), "layout"), (_t("Toucher-clic"), "tap")],
    5: [(_t("Vitesse"), "mouse_speed")], 6: [(_t("Alignement"), "align")],
    8: [(_t("Définition"), "display_current"), (_t("Échelle"), "display_scale")],
    9: [(_t("Version"), "version"), (_t("Noyau"), "kernel")],
    11: [(_t("Navigateur par défaut"), "browser_default")],
}
ACTIONS = {
    0: [(_t("Relancer le bureau"), "restart-session"), (_t("Redémarrer"), "reboot"), (_t("Éteindre"), "poweroff"),
        (_t("Signaler un problème"), "report-open")],
    2: [(_t("Renouveler la connexion"), "network-renew")],
    3: [(_t("Volume −"), "volume-down"), (_t("Muet / rétablir"), "volume-mute"), (_t("Volume +"), "volume-up")],
    4: [(_t("Français AZERTY"), "keyboard-fr"), (_t("Anglais QWERTY"), "keyboard-us"), (_t("Activer toucher-clic"), "tap-on"), (_t("Désactiver toucher-clic"), "tap-off")],
    5: [(_t("Lente"), "mouse-slow"), (_t("Normale"), "mouse-normal"), (_t("Rapide"), "mouse-fast")],
    6: [(_t("À gauche"), "align-left"), (_t("Centrer"), "align-center"),
        (_t("Ajouter Corbeille"), "desktop-icon-recycle"), (_t("Ajouter Explorateur"), "desktop-icon-explorer"),
        (_t("Ajouter Ce PC"), "desktop-icon-computer"), (_t("Ajouter Documents"), "desktop-icon-documents")],
    7: [(_t("Rechercher"), "update-check"), (_t("Installer"), "update-apply"), (_t("Restaurer"), "update-rollback"),
        (_t("Mises à jour auto : activer"), "update-auto-on"), (_t("Désactiver"), "update-auto-off")],
    23: [(_t("Performances maximales"), "power-profile-performance"), (_t("Équilibré"), "power-profile-balanced"),
         (_t("Économie d'énergie"), "power-profile-saver"), (_t("Écran (secteur)"), "power-screen-ac"),
         (_t("Écran (batterie)"), "power-screen-dc"), (_t("Veille (secteur)"), "power-sleep-ac"),
         (_t("Veille (batterie)"), "power-sleep-dc"), (_t("Capot"), "power-lid"), (_t("Bouton d'alimentation"), "power-button"),
         (_t("Luminosité −"), "power-brightness-down"), (_t("Luminosité +"), "power-brightness-up"),
         (_t("Mettre en veille"), "power-suspend"), (_t("Veille prolongée"), "power-hibernate")],
    8: [(_t("Suivre la taille de la VM"), "display-follow"), (_t("Définition fixe"), "display-fixed"),
        (_t("Échelle −"), "display-scale-down"), (_t("Automatique"), "display-scale-auto"), (_t("Échelle +"), "display-scale-up")],
}

def gpu_summary():
    """Résumé lisible du rapport GPU : seules les mesures exposées par le pilote."""
    try:
        out = subprocess.run(["winlux", "--gpu-status"], capture_output=True, text=True, timeout=3).stdout
    except (OSError, subprocess.SubprocessError):
        return _t("Informations graphiques indisponibles (runtime Winlux introuvable).")
    report = dict(line.split("=", 1) for line in out.splitlines() if "=" in line)
    try: count = min(8, int(report.get("gpu_count", "0")))
    except ValueError: count = 0
    if not count:
        return _t("Aucune carte graphique détectée (machine virtuelle ou affichage de secours).")
    roles = {"integrated": _t("GPU intégré (économie d'énergie)"), "dedicated": _t("GPU dédié (hautes performances)")}
    lines = []
    for i in range(count):
        get = lambda key: report.get(f"gpu{i}_{key}")
        lines.append(get("name") or _t("Carte graphique"))
        kind = [roles[get("role")]] if get("role") in roles else []
        lines.append("   " + " · ".join(kind + [_t("Pilote {driver}").format(driver=get("driver") or _t("inconnu"))]))
        metrics = []
        if get("load"): metrics.append(_t("Charge {load} %").format(load=get("load")))
        if get("vram_used_mb") and get("vram_total_mb"): metrics.append(_t("VRAM {used} / {total} Mo").format(used=get("vram_used_mb"), total=get("vram_total_mb")))
        if get("temp_c"): metrics.append(f"{get('temp_c')} °C")
        if get("freq_mhz"): metrics.append(_t("Fréquence {freq} MHz").format(freq=get("freq_mhz")))
        if get("power_w"): metrics.append(f"{get('power_w')} W")
        if get("encoder"): metrics.append(_t("Encodeur {value} %").format(value=get("encoder")))
        if get("decoder"): metrics.append(_t("Décodeur {value} %").format(value=get("decoder")))
        if metrics: lines.append("   " + " · ".join(metrics))
    if report.get("gpu_hybrid") == "1":
        lines.append(_t("\nGPU par application : clic droit sur un .exe > Réglages de l'exécutable."))
    return "\n".join(lines)

# Rapport de winlux-update (toujours écrit en français) : traduit à l'affichage.
UPDATE_PREFIXES = ("Version installée : ", "Version disponible : ", "Action en attente : ", "Portée : ",
                   "Composants : ", "Taille à télécharger : ", "Taille décompressée : ",
                   "Action après installation : ", "Echec de la mise à jour : ")
UPDATE_SUFFIXES = (" octets", " Kio (hors sauvegarde et marge)")


def localize_update_text(text):
    lines = []
    for line in text.split("\n"):
        for prefix in UPDATE_PREFIXES:
            if line.startswith(prefix):
                value = line[len(prefix):]
                for suffix in UPDATE_SUFFIXES:
                    if value.endswith(suffix):
                        value = value[:-len(suffix)] + _t(suffix)
                line = _t(prefix) + _t(value)
                break
        else:
            line = _t(line)
        lines.append(line)
    return "\n".join(lines)

def read(name):
    try: return Path(name).read_text(errors="replace")
    except OSError: return ""

class Settings(Gtk.Window):
    def __init__(self):
        super().__init__(title=_t("Paramètres Winlux"))
        self.set_icon_name("preferences-system")
        self.set_default_size(1020, 740)
        self.set_size_request(720, 500)
        self.connect("destroy", Gtk.main_quit)
        self.page = min(len(PAGES) - 1, max(0, int(os.environ.get("WINLUX_SETTINGS_PAGE", "0"))))
        self.process = None
        self.active_action = None
        css = Gtk.CssProvider()
        css.load_from_data(b"""
          window { background: #181b24; color: #ecedf5; }
          #sidebar { background: #202430; padding: 16px; }
          #sidebar list, #sidebar viewport { background: #202430; color: #ecedf5; }
          #sidebar row { padding: 10px; border-radius: 6px; }
          #sidebar row:hover { background: #30374a; }
          #sidebar row:selected { background: #423d66; }
          entry { background: #171b25; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 8px; }
          button { background: #30384b; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 10px 14px; box-shadow: none; }
          button:hover { background: #414b64; }
          button:disabled { color: #858da0; background: #252a36; }
          .heading { font-size: 24px; font-weight: bold; }
          .subtitle { color: #adb6c9; }
          .card { background: #242a38; border-radius: 8px; padding: 16px; }
        """)
        Gtk.StyleContext.add_provider_for_screen(self.get_screen(), css, 600)
        personal = Path.home() / ".config/gtk-3.0/gtk.css"
        if personal.is_file():
            override = Gtk.CssProvider()
            override.load_from_path(str(personal))
            Gtk.StyleContext.add_provider_for_screen(self.get_screen(), override, 800)
        outer = Gtk.Box()
        self.add(outer)
        side = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        side.set_name("sidebar")
        side.set_size_request(240, -1)
        outer.pack_start(side, False, False, 0)
        side.pack_start(Gtk.Label(label=_t("WINLUX · PARAMÈTRES"), xalign=0), False, False, 0)
        search = Gtk.SearchEntry(placeholder_text=_t("Rechercher un réglage"))
        side.pack_start(search, False, False, 0)
        nav_scroll = Gtk.ScrolledWindow()
        side.pack_start(nav_scroll, True, True, 0)
        self.nav = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
        nav_scroll.add(self.nav)
        for title in PAGES: self.nav.add(Gtk.Label(label=title, xalign=0))
        self.nav.set_filter_func(lambda row: search.get_text().casefold() in PAGES[row.get_index()].casefold())
        search.connect("search-changed", lambda *_: self.nav.invalidate_filter())
        self.nav.connect("row-selected", self.select)
        scroll = Gtk.ScrolledWindow()
        outer.pack_start(scroll, True, True, 0)
        self.body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=18, margin=28)
        scroll.add(self.body)
        self.nav.select_row(self.nav.get_row_at_index(self.page))
        GLib.timeout_add(1000, self.poll)
        self.show_all()
        self.progress.set_visible(self.page == 7)

    def label(self, text, style=None):
        label = Gtk.Label(label=text, xalign=0, wrap=True, selectable=True)
        if style: label.get_style_context().add_class(style)
        self.body.pack_start(label, False, False, 0)
        return label

    def select(self, _, row):
        if row is None: return
        self.page = row.get_index()
        for child in self.body.get_children(): child.destroy()
        self.label(PAGES[self.page], "heading")
        self.label(_t("Vos préférences et l’état réel de votre ordinateur."), "subtitle")
        self.state = self.label(_t("Chargement…"), "card")
        actions = list(ACTIONS.get(self.page, []))
        if self.page == 11:
            actions = [(_t(verb).format(name=name), f"browser-{action}-{name}") for name in ("edge", "chrome", "firefox")
                       for verb, action in (("Installer {name}", "install"), ("Par défaut : {name}", "default"), ("Ouvrir {name}", "launch"))]
        flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=3,
                           row_spacing=8, column_spacing=8)
        self.body.pack_start(flow, False, False, 0)
        self.install_button = None
        for title, action in actions:
            button = Gtk.Button(label=title)
            button.connect("clicked", lambda _, value=action: self.execute(value))
            flow.add(button)
            if action == "update-apply": self.install_button = button
        self.buttons = flow
        if self.page in (1, 2, 3, 6):
            tool = {1:"apps", 2:"quick", 3:"quick", 6:"appearance"}[self.page]
            shortcut = Gtk.Button(label={1:_t("Installer, désinstaller et gérer le démarrage"),2:_t("Wi-Fi et Bluetooth"),3:_t("Réglages rapides"),6:_t("Thèmes, accent, fond et barre des tâches")}[self.page])
            shortcut.connect("clicked", lambda *_: subprocess.Popen(["winlux-system-tools", tool]))
            self.body.pack_start(shortcut, False, False, 0)
        if self.page == 1:
            defaults = Gtk.Button(label="Applications par défaut")
            defaults.connect("clicked", lambda *_: subprocess.Popen(["winlux-default-apps"]))
            self.body.pack_start(defaults, False, False, 0)
        if self.page == 8:
            self.modes = Gtk.ComboBoxText()
            status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line)
            for mode in status.get("display_modes", "").split(","):
                if mode: self.modes.append_text(mode)
            self.modes.set_active(0)
            self.body.pack_start(self.modes, False, False, 0)
            button = Gtk.Button(label=_t("Appliquer la définition"))
            button.connect("clicked", lambda *_: self.execute("display-mode-" + self.modes.get_active_text()) if self.modes.get_active_text() else None)
            self.body.pack_start(button, False, False, 0)
        if self.page in (1, 10, 12, 13, GRAPHICS_PAGE):
            self.label(_t("Ouvrir les réglages détaillés de cette catégorie :"))
            button = Gtk.Button(label=_t("Réglages avancés"))
            legacy_page = self.page
            button.connect("clicked", lambda *_: subprocess.Popen(["winlux-shell-app", "settings", str(legacy_page)], env=dict(os.environ, WINLUX_LEGACY_SETTINGS="1")))
            self.body.pack_start(button, False, False, 0)
        self.notice = self.label("")
        self.progress = Gtk.ProgressBar(show_text=True)
        self.body.pack_start(self.progress, False, False, 0)
        self.details = self.label("")
        if self.page == 7:
            self.label(_t("La préparation des mises à jour utilise le disque. Gardez la VM allumée pendant l’installation."), "subtitle")
            recovery = Gtk.Button(label=_t("Secours et restauration"))
            recovery.connect("clicked", lambda *_: subprocess.Popen(["winlux-system-tools", "recovery"]))
            self.body.pack_start(recovery, False, False, 0)
            restart = Gtk.Button(label=_t("Redémarrer après installation"))
            restart.connect("clicked", lambda *_: self.execute("reboot"))
            self.body.pack_start(restart, False, False, 0)
        self.poll()
        self.body.show_all()
        self.progress.set_visible(self.page == 7)
        if self.install_button is not None:
            self.install_button.get_parent().set_visible("Winlux est à jour." not in read("/run/winlux/updates.txt"))

    def execute(self, action):
        if self.process is not None or self.busy: return
        if action in ("reboot", "poweroff", "restart-session", "update-rollback"):
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                       text=_t("Confirmer cette opération ?"), secondary_text=_t("Enregistrez vos documents avant de continuer."))
            answer = dialog.run()
            dialog.destroy()
            if answer != Gtk.ResponseType.OK: return
        try:
            self.process = subprocess.Popen(["winlux-control", action], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError as error:
            self.show_error(_t("Impossible de lancer l’opération"), str(error))
            return
        self.active_action = action
        self.notice.set_text(_t("Opération en cours…"))
        self.buttons.set_sensitive(False)

    def poll(self):
        status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line)
        self.busy = status.get("update_busy") == "1"
        if self.process is not None and self.process.poll() is not None:
            result = self.process.returncode
            action = self.active_action
            self.process = None
            self.active_action = None
            if result and action in ("update-check", "update-apply", "update-rollback"):
                self.show_update_error(action, result)
            elif result:
                detail = read("/run/winlux/action.log").strip()[-3000:] or _t("Aucun détail n’a été écrit dans le journal.")
                self.show_error(_t("L’opération a échoué"), _t("Code : {code}\n\n{detail}").format(code=result, detail=detail))
        self.buttons.set_sensitive(self.process is None and not self.busy)
        fields = FIELDS.get(self.page, [])
        text = "\n".join(_t("{label} : {value}").format(label=label, value=status.get(key) or _t("Indisponible")) for label, key in fields)
        if self.page == 7: text = read("/run/winlux/updates.txt")
        if self.install_button is not None:
            self.install_button.get_parent().set_visible("Winlux est à jour." not in text)
        if self.page == 7: text = localize_update_text(text)
        if self.page == 9: text += "\n" + read("/usr/share/winlux/BUILD-INFO")
        if self.page == GRAPHICS_PAGE: text = gpu_summary()
        if self.state.get_text() != text: self.state.set_text(text)
        if self.process is None: self.notice.set_text(localize_update_text(read("/run/winlux/action.txt").strip()))
        progress = dict(line.split("=", 1) for line in read("/run/winlux/update-progress").splitlines() if "=" in line)
        try: percent = int(progress.get("percent", "-1"))
        except ValueError: percent = -1
        try: fraction = min(1, max(0, percent / 100))
        except ValueError: fraction = 0
        if percent < 0: self.progress.pulse()
        else: self.progress.set_fraction(fraction)
        item = progress.get("item", "")
        if progress.get("unit") == "bytes":
            try:
                done, total = int(progress.get("done", "0")), int(progress.get("total", "0"))
                mib = _t("Mio")
                amount = f"{done / 1048576:.1f} {mib}" + (f" / {total / 1048576:.1f} {mib}" if total else "")
                item = f"{item} · {amount}" + (f" · {percent} %" if percent >= 0 else "")
            except ValueError: pass
        elif progress.get("phase") in ("backing-up", "installing"):
            try:
                done, total = int(progress.get("done", "0")), int(progress.get("total", "0"))
                if total: item = f"{progress.get('phase') == 'installing' and _t('Installation') or _t('Sauvegarde')} · {_t('{done}/{total} fichiers').format(done=done, total=total)} · {done * 100 // total} %\n{item}"
            except ValueError: pass
        self.progress.set_text(item)
        self.progress.set_visible(self.page == 7)
        if self.page == 7:
            journal = read("/run/winlux/update.log")[-12000:]
            if self.details.get_text() != journal: self.details.set_text(journal)
        return True

    def show_update_error(self, action, result):
        stages = {"checking": _t("Recherche et préparation"), "downloading": _t("Téléchargement"), "verifying": _t("Vérification"),
                  "extracting": _t("Décompression"), "backing-up": _t("Sauvegarde"),
                  "installing": _t("Installation"), "finalizing": _t("Finalisation"), "restoring": _t("Restauration")}
        phase = read("/run/winlux/update-error-phase").strip()
        stage = stages.get(phase, phase or {"update-check": _t("Recherche"), "update-apply": _t("Mise à jour"),
                                           "update-rollback": _t("Restauration")}[action])
        log = read("/run/winlux/update.log").strip()
        detail = log[-3000:] if log else _t("Aucun détail n’a été écrit dans le journal.")
        self.show_error(_t("La mise à jour a échoué"), _t("Étape : {stage} (code {code})\n\nDernières lignes du journal :\n{detail}").format(stage=stage, code=result, detail=detail))

    def show_error(self, title, detail):
        dialog = Gtk.MessageDialog(transient_for=self, modal=False, message_type=Gtk.MessageType.ERROR,
                                   buttons=Gtk.ButtonsType.CLOSE, text=title)
        dialog.format_secondary_text(detail)
        for child in dialog.get_message_area().get_children():
            if isinstance(child, Gtk.Label): child.set_selectable(True)
        dialog.connect("response", lambda window, _: window.destroy())
        dialog.show_all()

if __name__ == "__main__":
    Settings()
    Gtk.main()
