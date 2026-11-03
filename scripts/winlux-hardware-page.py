#!/usr/bin/python3
"""Paramètres > Système > Matériel et pilotes (page 34), et les sections
« Pilote de la carte graphique » (Graphismes) et « Microcode et fréquence »
(Processeur).

Comme le Gestionnaire de périphériques et les pilotes facultatifs de Windows
Update : ce qui est reconnu, ce qui ne l'est pas, et ce qui peut s'installer
depuis les dépôts Ubuntu. L'état vient de winlux-drivers (relevé dans
/run/winlux/drivers.txt par winlux-control) ; les installations passent par
winlux-control drivers-* et le Contrôle de compte d'utilisateur. Une section
sans objet sur cette machine (pas de carte NVIDIA, machine virtuelle, session
live) n'apparaît pas.

Chargé par winlux-settings-pages ; aucune fenêtre propre.
"""
import os
from pathlib import Path
import shutil
import subprocess


def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in Path("/etc/winlux/locale.conf").read_text(encoding="utf-8").splitlines():
                if line.startswith("LANG="):
                    code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
_TR = {
    "Diagnostic du matériel": ("Hardware check", "Diagnóstico de hardware", "Hardwarediagnose"),
    "Dit quels appareils de ce PC n’ont pas de pilote et pourquoi ; le rapport s’ouvre et peut être joint à un signalement.": ("Lists the devices of this PC that have no driver and why; the report opens and can be attached to a bug report.", "Indica qué dispositivos de este PC no tienen controlador y por qué; el informe se abre y puede adjuntarse a un aviso.", "Zeigt, welche Geräte dieses PCs keinen Treiber haben und warum; der Bericht öffnet sich und kann einer Meldung beigefügt werden."),
    "Lancer le diagnostic": ("Run the check", "Iniciar el diagnóstico", "Diagnose starten"),
    "Matériel et pilotes": ("Hardware and drivers", "Hardware y controladores", "Hardware und Treiber"),
    "Pilote de la carte graphique": ("Graphics card driver", "Controlador de la tarjeta gráfica", "Grafikkartentreiber"),
    "Pilote : {driver}": ("Driver: {driver}", "Controlador: {driver}", "Treiber: {driver}"),
    "aucun pilote": ("no driver", "ningún controlador", "kein Treiber"),
    "carte principale": ("main card", "tarjeta principal", "Hauptkarte"),
    "Pilote NVIDIA propriétaire": ("Proprietary NVIDIA driver", "Controlador propietario de NVIDIA", "Proprietärer NVIDIA-Treiber"),
    "Recommandé pour les jeux, CUDA et l’encodage vidéo NVENC. Version {version}, depuis les dépôts Ubuntu.": ("Recommended for games, CUDA and NVENC video encoding. Version {version}, from the Ubuntu repositories.", "Recomendado para juegos, CUDA y la codificación de vídeo NVENC. Versión {version}, desde los repositorios de Ubuntu.", "Empfohlen für Spiele, CUDA und NVENC-Videokodierung. Version {version}, aus den Ubuntu-Paketquellen."),
    "Le module est compilé pour ce noyau pendant l’installation (quelques minutes).": ("The module is built for this kernel during installation (a few minutes).", "El módulo se compila para este núcleo durante la instalación (unos minutos).", "Das Modul wird während der Installation für diesen Kernel kompiliert (einige Minuten)."),
    "Installer": ("Install", "Instalar", "Installieren"),
    "Installé : {name}": ("Installed: {name}", "Instalado: {name}", "Installiert: {name}"),
    "Revenir au pilote libre": ("Go back to the open driver", "Volver al controlador libre", "Zum freien Treiber zurückkehren"),
    "Réinstaller pour ce noyau": ("Reinstall for this kernel", "Reinstalar para este núcleo", "Für diesen Kernel neu installieren"),
    "Le pilote NVIDIA a été installé pour un autre noyau : il doit être refait.": ("The NVIDIA driver was installed for another kernel: it must be redone.", "El controlador de NVIDIA se instaló para otro núcleo: hay que volver a instalarlo.", "Der NVIDIA-Treiber wurde für einen anderen Kernel installiert und muss neu installiert werden."),
    "Aucun pilote NVIDIA des dépôts ne prend en charge cette carte : le pilote libre nouveau reste utilisé.": ("No NVIDIA driver in the repositories supports this card: the open nouveau driver stays in use.", "Ningún controlador de NVIDIA de los repositorios admite esta tarjeta: se sigue usando el controlador libre nouveau.", "Kein NVIDIA-Treiber aus den Paketquellen unterstützt diese Karte: der freie Treiber nouveau bleibt aktiv."),
    "Rechercher les pilotes": ("Look for drivers", "Buscar controladores", "Nach Treibern suchen"),
    "Vidéo matérielle (VA-API)": ("Hardware video (VA-API)", "Vídeo por hardware (VA-API)", "Hardware-Video (VA-API)"),
    "Décode et encode les vidéos sur la carte graphique : moins de processeur, plus d’autonomie.": ("Decodes and encodes videos on the graphics card: less CPU, longer battery life.", "Decodifica y codifica los vídeos en la tarjeta gráfica: menos procesador, más autonomía.", "Dekodiert und kodiert Videos auf der Grafikkarte: weniger CPU-Last, längere Akkulaufzeit."),
    "Actif pour : {list}": ("Active for: {list}", "Activo para: {list}", "Aktiv für: {list}"),
    "Encodage NVENC disponible": ("NVENC encoding available", "Codificación NVENC disponible", "NVENC-Kodierung verfügbar"),
    "Mesa {version} (OpenGL, Vulkan)": ("Mesa {version} (OpenGL, Vulkan)", "Mesa {version} (OpenGL, Vulkan)", "Mesa {version} (OpenGL, Vulkan)"),
    "Mettre à jour Mesa": ("Update Mesa", "Actualizar Mesa", "Mesa aktualisieren"),
    "Carte utilisée par un programme : clic droit sur un .exe > Réglages de l’exécutable > GPU.": ("Card used by a program: right-click an .exe > Executable settings > GPU.", "Tarjeta usada por un programa: clic derecho en un .exe > Ajustes del ejecutable > GPU.", "Von einem Programm verwendete Karte: Rechtsklick auf eine .exe > Einstellungen der ausführbaren Datei > GPU."),
    "Microcode et fréquence": ("Microcode and frequency", "Microcódigo y frecuencia", "Microcode und Taktfrequenz"),
    "Pilote de fréquence": ("Frequency driver", "Controlador de frecuencia", "Frequenztreiber"),
    "Gouverneur : {value}": ("Governor: {value}", "Gobernador: {value}", "Governor: {value}"),
    "État : {value}": ("Status: {value}", "Estado: {value}", "Status: {value}"),
    "Le mode d’alimentation (Alimentation et batterie) règle ce pilote.": ("The power mode (Power & battery) drives this driver.", "El modo de energía (Energía y batería) ajusta este controlador.", "Der Energiemodus (Energie und Akku) steuert diesen Treiber."),
    "Activer amd_pstate": ("Turn on amd_pstate", "Activar amd_pstate", "amd_pstate aktivieren"),
    "Pilote de fréquence moderne des Ryzen : plus réactif et plus économe. Pris en compte au prochain démarrage.": ("Modern frequency driver for Ryzen: more responsive and more efficient. Applied at next startup.", "Controlador de frecuencia moderno de los Ryzen: más reactivo y más eficiente. Se aplica en el próximo inicio.", "Moderner Frequenztreiber für Ryzen: reaktionsschneller und sparsamer. Wird beim nächsten Start übernommen."),
    "Microcode du processeur": ("Processor microcode", "Microcódigo del procesador", "Prozessor-Microcode"),
    "Révision {value}": ("Revision {value}", "Revisión {value}", "Revision {value}"),
    "Corrige des failles et des erreurs du processeur, chargé à chaque démarrage.": ("Fixes processor flaws and errata, loaded at every startup.", "Corrige fallos y errores del procesador; se carga en cada inicio.", "Behebt Sicherheitslücken und Fehler des Prozessors, wird bei jedem Start geladen."),
    "À jour, chargé au démarrage": ("Up to date, loaded at startup", "Actualizado, se carga al inicio", "Aktuell, wird beim Start geladen"),
    "Températures": ("Temperatures", "Temperaturas", "Temperaturen"),
    "Processeur {value} °C": ("Processor {value} °C", "Procesador {value} °C", "Prozessor {value} °C"),
    "Carte graphique {value} °C": ("Graphics card {value} °C", "Tarjeta gráfica {value} °C", "Grafikkarte {value} °C"),
    "Disque {value} °C": ("Drive {value} °C", "Disco {value} °C", "Laufwerk {value} °C"),
    "État du matériel": ("Hardware status", "Estado del hardware", "Hardwarestatus"),
    "Tout le matériel détecté a un pilote.": ("All detected hardware has a driver.", "Todo el hardware detectado tiene controlador.", "Für die gesamte erkannte Hardware ist ein Treiber vorhanden."),
    "{count} appareil(s) sans pilote.": ("{count} device(s) without a driver.", "{count} dispositivo(s) sin controlador.", "{count} Gerät(e) ohne Treiber."),
    "Redémarrez pour utiliser les pilotes installés.": ("Restart to use the installed drivers.", "Reinicie para usar los controladores instalados.", "Starten Sie neu, um die installierten Treiber zu verwenden."),
    "Redémarrer": ("Restart", "Reiniciar", "Neu starten"),
    "Installation de pilotes en cours…": ("Installing drivers…", "Instalando controladores…", "Treiber werden installiert…"),
    "Machine virtuelle : les pilotes du matériel réel ne s’appliquent pas ici.": ("Virtual machine: real hardware drivers do not apply here.", "Máquina virtual: los controladores de hardware real no se aplican aquí.", "Virtuelle Maschine: Treiber für echte Hardware gelten hier nicht."),
    "Session live : les pilotes s’installeront après l’installation de Winlux.": ("Live session: drivers will be installed after Winlux is installed.", "Sesión live: los controladores se instalarán después de instalar Winlux.", "Live-Sitzung: Die Treiber werden nach der Installation von Winlux installiert."),
    "Pilotes recommandés": ("Recommended drivers", "Controladores recomendados", "Empfohlene Treiber"),
    "Depuis les dépôts Ubuntu signés, comme les pilotes facultatifs de Windows Update.": ("From the signed Ubuntu repositories, like the optional drivers of Windows Update.", "Desde los repositorios firmados de Ubuntu, como los controladores opcionales de Windows Update.", "Aus den signierten Ubuntu-Paketquellen, wie die optionalen Treiber von Windows Update."),
    "Tous les pilotes recommandés sont installés.": ("All recommended drivers are installed.", "Todos los controladores recomendados están instalados.", "Alle empfohlenen Treiber sind installiert."),
    "Installer automatiquement les pilotes recommandés": ("Install recommended drivers automatically", "Instalar automáticamente los controladores recomendados", "Empfohlene Treiber automatisch installieren"),
    "Au premier démarrage et après une mise à jour du noyau.": ("At first startup and after a kernel update.", "En el primer inicio y después de una actualización del núcleo.", "Beim ersten Start und nach einem Kernel-Update."),
    "Pilotes complets du noyau": ("Complete kernel drivers", "Controladores completos del núcleo", "Vollständige Kernel-Treiber"),
    "Tous les pilotes Ubuntu pour ce noyau (environ 160 Mo) : cartes Wi-Fi, son, capteurs et appareils peu courants.": ("Every Ubuntu driver for this kernel (about 160 MB): Wi-Fi cards, sound, sensors and less common devices.", "Todos los controladores de Ubuntu para este núcleo (unos 160 MB): tarjetas Wi-Fi, sonido, sensores y dispositivos poco comunes.", "Alle Ubuntu-Treiber für diesen Kernel (etwa 160 MB): WLAN-Karten, Sound, Sensoren und seltenere Geräte."),
    "Pilote NVIDIA {version}": ("NVIDIA driver {version}", "Controlador NVIDIA {version}", "NVIDIA-Treiber {version}"),
    "Microcode {vendor}": ("{vendor} microcode", "Microcódigo {vendor}", "{vendor}-Microcode"),
    "Mises à jour du micrologiciel (fwupd)": ("Firmware updates (fwupd)", "Actualizaciones del firmware (fwupd)", "Firmware-Updates (fwupd)"),
    "BIOS/UEFI, SSD, stations d’accueil, souris et claviers, depuis le service LVFS des fabricants.": ("BIOS/UEFI, SSDs, docks, mice and keyboards, from the manufacturers’ LVFS service.", "BIOS/UEFI, SSD, bases de conexión, ratones y teclados, desde el servicio LVFS de los fabricantes.", "BIOS/UEFI, SSDs, Dockingstationen, Mäuse und Tastaturen, über den LVFS-Dienst der Hersteller."),
    "Lecteur d’empreintes digitales": ("Fingerprint reader", "Lector de huellas digitales", "Fingerabdruckleser"),
    "Lecteur détecté : installe la prise en charge (fprintd).": ("Reader detected: installs support (fprintd).", "Lector detectado: instala la compatibilidad (fprintd).", "Leser erkannt: installiert die Unterstützung (fprintd)."),
    "Scanners": ("Scanners", "Escáneres", "Scanner"),
    "Scanners USB et réseau (SANE, eSCL/AirScan).": ("USB and network scanners (SANE, eSCL/AirScan).", "Escáneres USB y de red (SANE, eSCL/AirScan).", "USB- und Netzwerkscanner (SANE, eSCL/AirScan)."),
    "Pilotes d’imprimantes": ("Printer drivers", "Controladores de impresora", "Druckertreiber"),
    "Gutenprint, HP, Brother et Samsung, pour les imprimantes qui ne parlent pas IPP Everywhere.": ("Gutenprint, HP, Brother and Samsung, for printers that do not speak IPP Everywhere.", "Gutenprint, HP, Brother y Samsung, para impresoras que no hablan IPP Everywhere.", "Gutenprint, HP, Brother und Samsung, für Drucker ohne IPP Everywhere."),
    "Appareils": ("Devices", "Dispositivos", "Geräte"),
    "Reconnu": ("Recognized", "Reconocido", "Erkannt"),
    "Non reconnu": ("Not recognized", "No reconocido", "Nicht erkannt"),
    "Carte graphique": ("Graphics card", "Tarjeta gráfica", "Grafikkarte"),
    "Processeur": ("Processor", "Procesador", "Prozessor"),
    "Wi-Fi": ("Wi-Fi", "Wi-Fi", "WLAN"),
    "Carte Wi-Fi présente, mais aucun réseau sans fil disponible.": ("Wi-Fi card present, but no wireless network interface.", "Tarjeta Wi-Fi presente, pero sin interfaz de red inalámbrica.", "WLAN-Karte vorhanden, aber keine Funknetzwerkschnittstelle."),
    "Bluetooth": ("Bluetooth", "Bluetooth", "Bluetooth"),
    "Son": ("Sound", "Sonido", "Sound"),
    "{count} carte(s) son": ("{count} sound card(s)", "{count} tarjeta(s) de sonido", "{count} Soundkarte(n)"),
    "Carte son présente, mais aucune sortie audio.": ("Sound card present, but no audio output.", "Tarjeta de sonido presente, pero sin salida de audio.", "Soundkarte vorhanden, aber keine Audioausgabe."),
    "Webcam": ("Webcam", "Cámara web", "Webcam"),
    "{count} caméra(s)": ("{count} camera(s)", "{count} cámara(s)", "{count} Kamera(s)"),
    "Pavé tactile": ("Touchpad", "Panel táctil", "Touchpad"),
    "Écran tactile": ("Touchscreen", "Pantalla táctil", "Touchscreen"),
    "Lecteur détecté, prise en charge non installée.": ("Reader detected, support not installed.", "Lector detectado, compatibilidad no instalada.", "Leser erkannt, Unterstützung nicht installiert."),
    "Appareils sans pilote": ("Devices without a driver", "Dispositivos sin controlador", "Geräte ohne Treiber"),
    "Les pilotes complets du noyau en reconnaissent souvent une partie.": ("The complete kernel drivers often recognize some of them.", "Los controladores completos del núcleo suelen reconocer parte de ellos.", "Die vollständigen Kernel-Treiber erkennen oft einen Teil davon."),
    "Stockage": ("Storage", "Almacenamiento", "Speicher"),
    "Réseau": ("Network", "Red", "Netzwerk"),
    "Affichage": ("Display", "Pantalla", "Anzeige"),
    "Multimédia": ("Multimedia", "Multimedia", "Multimedia"),
    "Sans fil": ("Wireless", "Inalámbrico", "Drahtlos"),
    "Micrologiciel": ("Firmware", "Firmware", "Firmware"),
    "Rechercher des mises à jour": ("Check for updates", "Buscar actualizaciones", "Nach Updates suchen"),
    "Jamais vérifié.": ("Never checked.", "Nunca comprobado.", "Nie geprüft."),
    "Aucune mise à jour de micrologiciel.": ("No firmware update.", "Ninguna actualización de firmware.", "Kein Firmware-Update."),
    "{count} mise(s) à jour disponible(s) : {names}": ("{count} update(s) available: {names}", "{count} actualización(es) disponible(s): {names}", "{count} Update(s) verfügbar: {names}"),
    "Mettre à jour le micrologiciel": ("Update firmware", "Actualizar el firmware", "Firmware aktualisieren"),
    "Mettre à jour le micrologiciel maintenant ? Gardez l’ordinateur branché sur le secteur ; certains appareils redémarrent pour terminer.": ("Update the firmware now? Keep the computer plugged in; some devices restart to finish.", "¿Actualizar el firmware ahora? Mantenga el equipo enchufado; algunos dispositivos se reinician para terminar.", "Firmware jetzt aktualisieren? Lassen Sie den Computer am Netz; manche Geräte starten zum Abschluss neu."),
    "Installer le pilote NVIDIA propriétaire ? Le téléchargement fait plusieurs centaines de Mo ; un redémarrage sera nécessaire.": ("Install the proprietary NVIDIA driver? The download is several hundred MB; a restart will be needed.", "¿Instalar el controlador propietario de NVIDIA? La descarga ocupa varios cientos de MB; será necesario reiniciar.", "Den proprietären NVIDIA-Treiber installieren? Der Download umfasst mehrere hundert MB; ein Neustart ist erforderlich."),
    "Retirer le pilote NVIDIA et revenir au pilote libre nouveau ?": ("Remove the NVIDIA driver and go back to the open nouveau driver?", "¿Quitar el controlador de NVIDIA y volver al controlador libre nouveau?", "NVIDIA-Treiber entfernen und zum freien Treiber nouveau zurückkehren?"),
    "Microcode, pilote de fréquence et températures : Processeur. Pilote NVIDIA et vidéo matérielle : Graphismes.": ("Microcode, frequency driver and temperatures: Processor. NVIDIA driver and hardware video: Graphics.", "Microcódigo, controlador de frecuencia y temperaturas: Procesador. Controlador NVIDIA y vídeo por hardware: Gráficos.", "Microcode, Frequenztreiber und Temperaturen: Prozessor. NVIDIA-Treiber und Hardware-Video: Grafik."),
    "Ouvrir Processeur": ("Open Processor", "Abrir Procesador", "Prozessor öffnen"),
    "Ouvrir Graphismes": ("Open Graphics", "Abrir Gráficos", "Grafik öffnen"),
    # Votre PC (À propos)
    "Votre PC": ("Your PC", "Su PC", "Ihr PC"),
    "Tout ce que Winlux a détecté sur cette machine.": ("Everything Winlux detected on this machine.", "Todo lo que Winlux ha detectado en este equipo.", "Alles, was Winlux auf diesem Gerät erkannt hat."),
    "Fabricant": ("Manufacturer", "Fabricante", "Hersteller"),
    "Modèle": ("Model", "Modelo", "Modell"),
    "{cores} cœurs, {threads} threads": ("{cores} cores, {threads} threads", "{cores} núcleos, {threads} hilos", "{cores} Kerne, {threads} Threads"),
    "Mémoire (RAM)": ("Memory (RAM)", "Memoria (RAM)", "Arbeitsspeicher (RAM)"),
    "Disque": ("Drive", "Disco", "Laufwerk"),
    "SSD NVMe": ("NVMe SSD", "SSD NVMe", "NVMe-SSD"),
    "SSD": ("SSD", "SSD", "SSD"),
    "Disque dur": ("Hard drive", "Disco duro", "Festplatte"),
    "Mémoire eMMC": ("eMMC storage", "Memoria eMMC", "eMMC-Speicher"),
    "Carte SD": ("SD card", "Tarjeta SD", "SD-Karte"),
    "Amovible (USB)": ("Removable (USB)", "Extraíble (USB)", "Wechseldatenträger (USB)"),
    "{size} libres": ("{size} free", "{size} libres", "{size} frei"),
    "Écran": ("Display", "Pantalla", "Bildschirm"),
    "écran intégré": ("built-in display", "pantalla integrada", "integrierter Bildschirm"),
    "{width} × {height} à {rate} Hz": ("{width} × {height} at {rate} Hz", "{width} × {height} a {rate} Hz", "{width} × {height} bei {rate} Hz"),
    "Ethernet": ("Ethernet", "Ethernet", "Ethernet"),
    "connecté": ("connected", "conectado", "verbunden"),
    "déconnecté": ("disconnected", "desconectado", "getrennt"),
    "Batterie": ("Battery", "Batería", "Akku"),
    "en charge": ("charging", "cargando", "wird geladen"),
    "sur batterie": ("on battery", "con batería", "Akkubetrieb"),
    "chargée": ("fully charged", "cargada", "voll geladen"),
    "branchée, ne charge pas": ("plugged in, not charging", "enchufada, sin cargar", "angeschlossen, wird nicht geladen"),
    "santé {value} %": ("health {value} %", "salud {value} %", "Zustand {value} %"),
    "UEFI {bits} bits": ("UEFI {bits}-bit", "UEFI de {bits} bits", "UEFI {bits} Bit"),
    "BIOS hérité": ("Legacy BIOS", "BIOS heredado", "Legacy-BIOS"),
    "Architecture": ("Architecture", "Arquitectura", "Architektur"),
    "{arch} (64 bits)": ("{arch} (64-bit)", "{arch} (64 bits)", "{arch} (64 Bit)"),
    "{arch} (32 bits)": ("{arch} (32-bit)", "{arch} (32 bits)", "{arch} (32 Bit)"),
    "processeur 64 bits": ("64-bit processor", "procesador de 64 bits", "64-Bit-Prozessor"),
    "Secure Boot": ("Secure Boot", "Arranque seguro", "Secure Boot"),
    "Activé": ("On", "Activado", "Aktiviert"),
    "Désactivé": ("Off", "Desactivado", "Deaktiviert"),
    "Adaptateur détecté": ("Adapter detected", "Adaptador detectado", "Adapter erkannt"),
    "Détecté": ("Detected", "Detectado", "Erkannt"),
    "Machine virtuelle": ("Virtual machine", "Máquina virtual", "Virtuelle Maschine"),
    "o": ("B", "B", "B"),
    "Ko": ("KB", "KB", "KB"),
    "Mo": ("MB", "MB", "MB"),
    "Go": ("GB", "GB", "GB"),
    "To": ("TB", "TB", "TB"),
}

PAGE_HARDWARE = 34
PAGE_CPU = 19
PAGE_GRAPHICS = 24
REPORT = Path(os.environ.get("WINLUX_DRIVERS_REPORT", "/run/winlux/drivers.txt"))
VENDOR_NAMES = {"intel": "Intel", "amd": "AMD", "nvidia": "NVIDIA"}
CLASS_NAMES = {"0x01": "Stockage", "0x02": "Réseau", "0x03": "Affichage", "0x04": "Multimédia", "0x0d": "Sans fil"}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def tool():
    here = Path(__file__).resolve()
    for candidate in (here.with_name("winlux-drivers.py"), here.with_name("winlux-drivers")):
        if candidate.is_file():
            return ["/usr/bin/python3", str(candidate)]
    found = shutil.which("winlux-drivers")
    return [found] if found else None


def available():
    return tool() is not None


def report():
    """Relevé de winlux-drivers : celui de winlux-control, sinon lu directement."""
    try:
        text = REPORT.read_text(errors="replace")
    except OSError:
        text = ""
    if not text and tool():
        try:
            text = subprocess.run(tool() + ["status"], capture_output=True, text=True, timeout=20).stdout
        except (OSError, subprocess.SubprocessError):
            text = ""
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


def _int(value):
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0


def can_install(r):
    return r.get("drivers_installable") == "1" and not r.get("drivers_virtual")


def gpus(r):
    return [{key: r.get(f"gpu{i}_{key}", "") for key in ("name", "vendor", "pci", "driver", "boot")}
            for i in range(min(4, _int(r.get("gpu_count"))))]


def busy_or_reboot(w, r, box):
    if r.get("busy") == "1":
        w.hint(_t("Installation de pilotes en cours…"), box)
    elif r.get("reboot_needed") == "1":
        w.hint(_t("Redémarrez pour utiliser les pilotes installés."), box)
        w.actions(box, [(_t("Redémarrer"), lambda: w.execute("reboot"))])


def install(w, item, question=None):
    if question and not w.confirm(_t(question)):
        return
    w.execute("drivers-install-" + item)


# ── Graphismes : pilote de la carte graphique ──────────────────────────────

def gpu_section(w):
    r = report()
    cards = gpus(r)
    if not cards:
        return
    w.section(_t("Pilote de la carte graphique"))
    box = w.card()
    for card in cards:
        detail = _t("Pilote : {driver}").format(driver=card["driver"] if card["driver"] not in ("", "none") else _t("aucun pilote"))
        if card["boot"] == "1" and len(cards) > 1:
            detail += " · " + _t("carte principale")
        w.row(box, card["name"] or _t("Carte graphique"), None, detail)
    if r.get("mesa_version") and any(c["vendor"] in ("intel", "amd") for c in cards):
        w.row(box, _t("Mesa {version} (OpenGL, Vulkan)").format(version=r["mesa_version"]),
              None if not can_install(r) else _button(_t("Mettre à jour Mesa"), lambda: install(w, "mesa")))
    busy_or_reboot(w, r, box)
    installable = can_install(r) and r.get("busy") != "1"
    if any(c["vendor"] == "nvidia" for c in cards) and can_install(r):
        w.section(_t("Pilote NVIDIA propriétaire"))
        box = w.card()
        if r.get("nvidia_installed"):
            w.row(box, _t("Installé : {name}").format(name=r["nvidia_installed"]), None)
            if r.get("nvidia_reinstall") == "1":
                w.hint(_t("Le pilote NVIDIA a été installé pour un autre noyau : il doit être refait."), box)
                w.actions(box, [(_t("Réinstaller pour ce noyau"), lambda: install(w, "nvidia"), installable)])
            w.actions(box, [(_t("Revenir au pilote libre"),
                             lambda: w.confirm(_t("Retirer le pilote NVIDIA et revenir au pilote libre nouveau ?"))
                             and w.execute("drivers-remove-nvidia"), installable)])
        elif r.get("nvidia_candidate"):
            subtitle = _t("Recommandé pour les jeux, CUDA et l’encodage vidéo NVENC. Version {version}, depuis les dépôts Ubuntu.").format(
                version=r["nvidia_candidate"])
            if r.get("nvidia_method") == "dkms":
                subtitle += " " + _t("Le module est compilé pour ce noyau pendant l’installation (quelques minutes).")
            w.row(box, _t("Pilote NVIDIA {version}").format(version=r["nvidia_candidate"]),
                  _button(_t("Installer"), lambda: install(w, "nvidia", "Installer le pilote NVIDIA propriétaire ? Le téléchargement fait plusieurs centaines de Mo ; un redémarrage sera nécessaire."), installable),
                  subtitle)
        elif r.get("candidates_known") == "1":
            w.hint(_t("Aucun pilote NVIDIA des dépôts ne prend en charge cette carte : le pilote libre nouveau reste utilisé."), box)
        else:
            w.actions(box, [(_t("Rechercher les pilotes"), lambda: w.execute("drivers-refresh"), installable)])
    # Vidéo matérielle
    missing = [v for v in r.get("vaapi_missing", "").split(",") if v]
    ok = [v for v in r.get("vaapi_ok", "").split(",") if v]
    if missing or ok or r.get("nvenc") == "1":
        w.section(_t("Vidéo matérielle (VA-API)"), _t("Décode et encode les vidéos sur la carte graphique : moins de processeur, plus d’autonomie."))
        box = w.card()
        if ok:
            w.row(box, _t("Actif pour : {list}").format(list=", ".join(VENDOR_NAMES.get(v, v) for v in ok)), None)
        if r.get("nvenc") == "1":
            w.row(box, _t("Encodage NVENC disponible"), None)
        if missing and can_install(r):
            w.row(box, ", ".join(VENDOR_NAMES.get(v, v) for v in missing),
                  _button(_t("Installer"), lambda: install(w, "vaapi"), installable))
    if len(cards) > 1:
        w.hint(_t("Carte utilisée par un programme : clic droit sur un .exe > Réglages de l’exécutable > GPU."))


def _button(label, callback, sensitive=True):
    from gi.repository import Gtk
    button = Gtk.Button(label=label)
    button.set_sensitive(sensitive)
    button.connect("clicked", lambda *_: callback())
    return button


# ── Processeur : microcode, pilote de fréquence, températures ──────────────

def cpu_section(w):
    r = report()
    if not r:
        return
    installable = can_install(r) and r.get("busy") != "1"
    shown = r.get("freq_driver") or (r.get("cpu_vendor") and not r.get("drivers_virtual"))
    if shown:
        w.section(_t("Microcode et fréquence"))
        box = w.card()
        if r.get("freq_driver"):
            parts = []
            if r.get("freq_governor"): parts.append(_t("Gouverneur : {value}").format(value=r["freq_governor"]))
            if r.get("freq_status"): parts.append(_t("État : {value}").format(value=r["freq_status"]))
            w.row(box, _t("Pilote de fréquence") + " : " + r["freq_driver"], None,
                  " · ".join(parts + [_t("Le mode d’alimentation (Alimentation et batterie) règle ce pilote.")]))
        if r.get("amd_pstate_possible") == "1" and can_install(r):
            w.row(box, _t("Activer amd_pstate"), _button(_t("Installer"), lambda: install(w, "amd-pstate"), installable),
                  _t("Pilote de fréquence moderne des Ryzen : plus réactif et plus économe. Pris en compte au prochain démarrage."))
        if r.get("cpu_vendor") and not r.get("drivers_virtual"):
            subtitle = _t("Corrige des failles et des erreurs du processeur, chargé à chaque démarrage.")
            if r.get("microcode_revision"):
                subtitle = _t("Révision {value}").format(value=r["microcode_revision"]) + " · " + subtitle
            done = r.get("microcode_package") == "1" and r.get("microcode_early") == "1"
            widget = None
            if not done and can_install(r):
                widget = _button(_t("Installer"), lambda: install(w, "microcode"), installable)
            w.row(box, _t("Microcode du processeur") + (" — " + _t("À jour, chargé au démarrage") if done else ""), widget, subtitle)
        busy_or_reboot(w, r, box)
    temps = [(_t("Processeur {value} °C"), r.get("temp_cpu")), (_t("Carte graphique {value} °C"), r.get("temp_gpu")),
             (_t("Disque {value} °C"), r.get("temp_disk"))]
    temps = [label.format(value=value) for label, value in temps if value]
    if temps:
        w.section(_t("Températures"))
        box = w.card()
        w.row(box, " · ".join(temps), None)


# ── Votre PC (Paramètres > À propos) ───────────────────────────────────────

VM_NAMES = {"qemu": "QEMU/KVM", "virtualbox": "VirtualBox", "vmware": "VMware", "hyperv": "Hyper-V",
            "xen": "Xen", "parallels": "Parallels"}
DISK_KINDS = {"nvme": "SSD NVMe", "ssd": "SSD", "hdd": "Disque dur", "emmc": "Mémoire eMMC", "sd": "Carte SD", "usb": "Amovible (USB)"}
BATTERY_STATES = {"charging": "en charge", "discharging": "sur batterie", "full": "chargée", "not-charging": "branchée, ne charge pas"}


def size_text(value, binary=False):
    """Taille lisible : 1 To, 512 Go, 15,5 Go (virgule sauf en anglais)."""
    try:
        value = float(value)
    except (TypeError, ValueError):
        return ""
    step = 1024 if binary else 1000
    unit = "o"
    for name in ("Ko", "Mo", "Go", "To"):
        if value < step:
            break
        value /= step
        unit = name
    text = f"{value:.0f}" if value >= 100 or unit == "o" else f"{value:.1f}".rstrip("0").rstrip(".")
    if _LANG != "en":
        text = text.replace(".", ",")
    return f"{text} {_t(unit)}"


def _split(value):
    return [part for part in (value or "").split("|") if part]


def pc_rows(r, status=None):
    """(titre, valeur) de chaque élément détecté ; rien n'est inventé : ce qui
    n'a pas été détecté (batterie d'un PC de bureau, Secure Boot en BIOS
    hérité…) n'a pas de ligne."""
    status = status or {}
    rows = []
    vendor, model = r.get("pc_vendor", ""), r.get("pc_model", "")
    virtual = r.get("drivers_virtual", "")
    if vendor:
        rows.append((_t("Fabricant"), vendor))
    if model:
        if r.get("pc_family"):
            model += f" ({r['pc_family']})"
        rows.append((_t("Modèle"), model))
    if virtual:
        rows.append((_t("Machine virtuelle"), VM_NAMES.get(virtual, virtual)))
    if r.get("pc_cpu_model"):
        detail = r["pc_cpu_model"]
        if _int(r.get("pc_cpu_threads")):
            detail += " · " + _t("{cores} cœurs, {threads} threads").format(
                cores=_int(r.get("pc_cpu_cores")) or r["pc_cpu_threads"], threads=r["pc_cpu_threads"])
        rows.append((_t("Processeur"), detail))
    if _int(r.get("pc_memory_kb")):
        rows.append((_t("Mémoire (RAM)"), size_text(_int(r["pc_memory_kb"]) * 1024, binary=True)))
    for name in _split(r.get("pc_gpus")):
        rows.append((_t("Carte graphique"), name))
    for i in range(min(8, _int(r.get("pc_disk_count")))):
        get = lambda key: r.get(f"pc_disk{i}_{key}", "")
        parts = [get("model")] if get("model") else []
        if get("kind") in DISK_KINDS:
            parts.append(_t(DISK_KINDS[get("kind")]))
        if _int(get("size")):
            parts.append(size_text(get("size")))
        if _int(get("mounted")) and get("free") != "":
            parts.append(_t("{size} libres").format(size=size_text(get("free"))))
        rows.append((_t("Disque") + f" ({get('name')})" if get("name") else _t("Disque"), " · ".join(parts)))
    try:
        screens = min(4, int(status.get("display_count", "0")))
    except ValueError:
        screens = 0
    for n in range(1, screens + 1):
        get = lambda key: status.get(f"display_{n}_{key}", "")
        if not get("name"):
            continue
        parts = [get("name")]
        mode = get("mode")
        if "x" in mode:
            width, _, height = mode.partition("x")
            rate = get("rate").split(".")[0] if get("rate") and get("rate") != "auto" else ""
            parts.append(_t("{width} × {height} à {rate} Hz").format(width=width, height=height, rate=rate) if rate
                         else f"{width} × {height}")
        if get("internal") == "1":
            parts.append(_t("écran intégré"))
        rows.append((_t("Écran"), " · ".join(parts)))
    if not screens and status.get("display_current"):
        rows.append((_t("Écran"), status["display_current"].replace("x", " × ")))
    address = ""
    if status.get("network_connected") == "1":
        iface, _, address = status.get("network", "").partition(" ")
    else:
        iface = ""
    for i in range(min(8, _int(r.get("pc_net_count")))):
        get = lambda key: r.get(f"pc_net{i}_{key}", "")
        parts = [_t("Wi-Fi") if get("kind") == "wifi" else _t("Ethernet"), get("name"),
                 _t("connecté") if get("up") == "1" else _t("déconnecté")]
        if address and get("name") == iface:
            parts.append(address.split("/")[0])
        rows.append((_t("Réseau"), " · ".join(p for p in parts if p)))
    for name in _split(r.get("pc_sound")):
        rows.append((_t("Son"), name))
    if r.get("bluetooth") == "1":
        rows.append((_t("Bluetooth"), _t("Adaptateur détecté")))
    if _int(r.get("webcam_count")):
        rows.append((_t("Webcam"), _t("{count} caméra(s)").format(count=r["webcam_count"])))
    if r.get("touchscreen") == "1":
        rows.append((_t("Écran tactile"), _t("Détecté")))
    if r.get("touchpad") == "1":
        rows.append((_t("Pavé tactile"), _t("Détecté")))
    for i in range(min(2, _int(r.get("pc_battery_count")))):
        get = lambda key: r.get(f"pc_battery{i}_{key}", "")
        parts = [f"{get('capacity')} %"] if get("capacity") else []
        if get("status") in BATTERY_STATES:
            parts.append(_t(BATTERY_STATES[get("status")]))
        if get("health"):
            parts.append(_t("santé {value} %").format(value=get("health")))
        if get("model"):
            parts.append(get("model"))
        rows.append((_t("Batterie"), " · ".join(parts)))
    if r.get("pc_firmware") == "uefi":
        firmware = _t("UEFI {bits} bits").format(bits=r["pc_firmware_bits"]) if r.get("pc_firmware_bits") in ("32", "64") else "UEFI"
    elif r.get("pc_firmware") == "bios":
        firmware = _t("BIOS hérité")
    else:
        firmware = ""
    if firmware:
        bios = " ".join(part for part in (r.get("pc_bios_vendor", ""), r.get("pc_bios_version", "")) if part)
        if r.get("pc_bios_date"):
            bios += f" ({r['pc_bios_date']})" if bios else r["pc_bios_date"]
        rows.append((_t("Micrologiciel"), firmware + (f" · {bios}" if bios else "")))
    if r.get("pc_arch"):
        arch = r["pc_arch"]
        bits = "64" if arch in ("x86_64", "aarch64", "ppc64le", "riscv64", "s390x") else "32"
        text = _t("{arch} (%s bits)" % bits).format(arch=arch)
        if bits == "32" and r.get("pc_cpu_64bit") == "1":
            text += " · " + _t("processeur 64 bits")
        rows.append((_t("Architecture"), text))
    if r.get("pc_firmware") == "uefi" and r.get("pc_secure_boot") in ("0", "1"):
        rows.append((_t("Secure Boot"), _t("Activé") if r["pc_secure_boot"] == "1" else _t("Désactivé")))
    return [(title, value) for title, value in rows if value]


def pc_section(w, status=None):
    """Section « Votre PC », sous la version et le noyau de la page À propos."""
    rows = pc_rows(report(), status)
    if not rows:
        return
    w.section(_t("Votre PC"), _t("Tout ce que Winlux a détecté sur cette machine."))
    box = w.card()
    for title, value in rows:
        w.row(box, title, None, value)


# ── Page Matériel et pilotes ───────────────────────────────────────────────

def recommended(r):
    """(titre, sous-titre, élément, question) de ce qui manque à cette machine."""
    items = []
    if r.get("modules_full") != "1" and r.get("modules_candidate"):
        items.append((_t("Pilotes complets du noyau"),
                      _t("Tous les pilotes Ubuntu pour ce noyau (environ 160 Mo) : cartes Wi-Fi, son, capteurs et appareils peu courants."),
                      "modules", None))
    if r.get("nvidia_candidate") and not r.get("nvidia_installed"):
        items.append((_t("Pilote NVIDIA {version}").format(version=r["nvidia_candidate"]),
                      _t("Recommandé pour les jeux, CUDA et l’encodage vidéo NVENC. Version {version}, depuis les dépôts Ubuntu.").format(version=r["nvidia_candidate"]),
                      "nvidia", "Installer le pilote NVIDIA propriétaire ? Le téléchargement fait plusieurs centaines de Mo ; un redémarrage sera nécessaire."))
    missing = [v for v in r.get("vaapi_missing", "").split(",") if v and (v != "nvidia" or r.get("nvidia_installed"))]
    if missing:
        items.append((_t("Vidéo matérielle (VA-API)"), _t("Décode et encode les vidéos sur la carte graphique : moins de processeur, plus d’autonomie."), "vaapi", None))
    if r.get("cpu_vendor") and not (r.get("microcode_package") == "1" and r.get("microcode_early") == "1"):
        items.append((_t("Microcode {vendor}").format(vendor=VENDOR_NAMES.get(r["cpu_vendor"], r["cpu_vendor"])),
                      _t("Corrige des failles et des erreurs du processeur, chargé à chaque démarrage."), "microcode", None))
    if r.get("amd_pstate_possible") == "1":
        items.append((_t("Activer amd_pstate"), _t("Pilote de fréquence moderne des Ryzen : plus réactif et plus économe. Pris en compte au prochain démarrage."), "amd-pstate", None))
    if r.get("fwupd") != "1":
        items.append((_t("Mises à jour du micrologiciel (fwupd)"), _t("BIOS/UEFI, SSD, stations d’accueil, souris et claviers, depuis le service LVFS des fabricants."), "fwupd", None))
    if r.get("fingerprint_device") == "1" and r.get("fprintd") != "1":
        items.append((_t("Lecteur d’empreintes digitales"), _t("Lecteur détecté : installe la prise en charge (fprintd)."), "fingerprint", None))
    if r.get("cups") == "1" and r.get("scanner_tools") != "1":
        items.append((_t("Scanners"), _t("Scanners USB et réseau (SANE, eSCL/AirScan)."), "scanner", None))
    if r.get("cups") == "1" and r.get("printer_drivers") != "1":
        items.append((_t("Pilotes d’imprimantes"), _t("Gutenprint, HP, Brother et Samsung, pour les imprimantes qui ne parlent pas IPP Everywhere."), "printer-drivers", None))
    return items


def devices(r):
    """(appareil, détail, reconnu) pour le matériel présent seulement."""
    rows = []
    for card in gpus(r):
        bound = card["driver"] not in ("", "none")
        rows.append((card["name"] or _t("Carte graphique"),
                     _t("Pilote : {driver}").format(driver=card["driver"] if bound else _t("aucun pilote")), bound))
    if r.get("cpu_vendor") or r.get("freq_driver"):
        detail = r.get("freq_driver") and _t("Pilote de fréquence") + " : " + r["freq_driver"] or ""
        rows.append((_t("Processeur"), detail, True))
    if r.get("wifi_card") == "1" or r.get("wifi") == "1":
        rows.append((_t("Wi-Fi"), "" if r.get("wifi") == "1" else _t("Carte Wi-Fi présente, mais aucun réseau sans fil disponible."), r.get("wifi") == "1"))
    if r.get("bluetooth") == "1":
        rows.append((_t("Bluetooth"), "", True))
    if _int(r.get("audio_cards")) or r.get("audio_card_pci") == "1":
        count = _int(r.get("audio_cards"))
        rows.append((_t("Son"), _t("{count} carte(s) son").format(count=count) if count else _t("Carte son présente, mais aucune sortie audio."), bool(count)))
    if _int(r.get("webcam_count")):
        rows.append((_t("Webcam"), _t("{count} caméra(s)").format(count=r["webcam_count"]), True))
    if r.get("touchpad") == "1":
        rows.append((_t("Pavé tactile"), "", True))
    if r.get("touchscreen") == "1":
        rows.append((_t("Écran tactile"), "", True))
    if r.get("fingerprint_device") == "1":
        rows.append((_t("Lecteur d’empreintes digitales"), "" if r.get("fprintd") == "1" else _t("Lecteur détecté, prise en charge non installée."), r.get("fprintd") == "1"))
    return rows


def unbound(r):
    out = []
    for entry in [e for e in r.get("unbound", "").split(",") if e.count(":") >= 3]:
        parts = entry.split(":")
        slot = ":".join(parts[:-3])
        pci_class, vendor, device = parts[-3:]
        kind = _t(CLASS_NAMES.get(pci_class[:4], "Appareils"))
        out.append((f"{kind} — {vendor}:{device}", slot))
    return out


def check_tool():
    here = Path(__file__).resolve()
    for candidate in (here.with_name("winlux-hardware-check.py"), here.with_name("winlux-hardware-check")):
        if candidate.is_file():
            return ["/usr/bin/python3", str(candidate)]
    found = shutil.which("winlux-hardware-check")
    return [found] if found else None


def run_check():
    """winlux-hardware-check écrit son rapport dans le dossier personnel, puis
    l'éditeur de texte l'ouvre (même rapport qu'en terminal)."""
    command = check_tool()
    if not command:
        return
    report_file = Path.home() / "winlux-diagnostic-materiel.txt"
    subprocess.run(command + ["--save", str(report_file)], capture_output=True, timeout=120, check=False)
    if report_file.is_file():
        subprocess.Popen(["xdg-open", str(report_file)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def page(w):
    r = report()
    installable = can_install(r) and r.get("busy") != "1"
    w.section(_t("État du matériel"))
    box = w.card()
    lost = _int(r.get("unbound_count"))
    w.row(box, _t("Tout le matériel détecté a un pilote.") if not lost else _t("{count} appareil(s) sans pilote.").format(count=lost), None)
    if r.get("drivers_virtual"):
        w.hint(_t("Machine virtuelle : les pilotes du matériel réel ne s’appliquent pas ici."), box)
    elif r.get("drivers_live") == "1":
        w.hint(_t("Session live : les pilotes s’installeront après l’installation de Winlux."), box)
    busy_or_reboot(w, r, box)
    if can_install(r):
        w.section(_t("Pilotes recommandés"), _t("Depuis les dépôts Ubuntu signés, comme les pilotes facultatifs de Windows Update."))
        box = w.card()
        items = recommended(r)
        if not items:
            w.row(box, _t("Tous les pilotes recommandés sont installés."), None)
        for title, subtitle, item, question in items:
            w.row(box, title, _button(_t("Installer"), lambda i=item, q=question: install(w, i, q), installable), subtitle)
        w.actions(box, [(_t("Rechercher les pilotes"), lambda: w.execute("drivers-refresh"), installable)])
        w.switch(box, _t("Installer automatiquement les pilotes recommandés"), r.get("drivers_auto", "1") != "0",
                 lambda on: w.execute("drivers-auto-on" if on else "drivers-auto-off"),
                 _t("Au premier démarrage et après une mise à jour du noyau."))
    rows = devices(r)
    if rows:
        w.section(_t("Appareils"))
        box = w.card()
        for name, detail, ok in rows:
            mark = "✓ " + _t("Reconnu") if ok else "⚠ " + _t("Non reconnu")
            w.row(box, name, None, " · ".join(part for part in (mark, detail) if part))
    lost_rows = unbound(r)
    if lost_rows:
        w.section(_t("Appareils sans pilote"), _t("Les pilotes complets du noyau en reconnaissent souvent une partie."))
        box = w.card()
        for name, slot in lost_rows:
            w.row(box, name, None, slot)
    if r.get("fwupd") == "1" and not r.get("drivers_virtual"):
        w.section(_t("Micrologiciel"), _t("BIOS/UEFI, SSD, stations d’accueil, souris et claviers, depuis le service LVFS des fabricants."))
        box = w.card()
        count = _int(r.get("firmware_updates"))
        if not r.get("firmware_checked"):
            text = _t("Jamais vérifié.")
        elif not count:
            text = _t("Aucune mise à jour de micrologiciel.")
        else:
            text = _t("{count} mise(s) à jour disponible(s) : {names}").format(count=count, names=r.get("firmware_names", "").replace("|", ", "))
        w.row(box, text, None)
        buttons = [(_t("Rechercher des mises à jour"), lambda: w.execute("drivers-firmware-check"), installable)]
        if count:
            buttons.append((_t("Mettre à jour le micrologiciel"),
                            lambda: w.confirm(_t("Mettre à jour le micrologiciel maintenant ? Gardez l’ordinateur branché sur le secteur ; certains appareils redémarrent pour terminer."))
                            and w.execute("drivers-firmware-update"), installable))
        w.actions(box, buttons)
    w.section(_t("Diagnostic du matériel"), _t("Dit quels appareils de ce PC n’ont pas de pilote et pourquoi ; le rapport s’ouvre et peut être joint à un signalement."))
    box = w.card()
    w.actions(box, [(_t("Lancer le diagnostic"), run_check, True)])
    w.hint(_t("Microcode, pilote de fréquence et températures : Processeur. Pilote NVIDIA et vidéo matérielle : Graphismes."))
    w.actions(w.body, [(_t("Ouvrir Processeur"), lambda: w.open_page(PAGE_CPU)),
                       (_t("Ouvrir Graphismes"), lambda: w.open_page(PAGE_GRAPHICS))])
