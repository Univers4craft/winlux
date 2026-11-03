#!/usr/bin/python3
"""Réinitialiser ce PC, clé de secours et réparation du démarrage (#283).

  winlux-reset status                      état (installé, réinitialisation en attente…)
  winlux-reset sources [--scan]            sources possibles : support Winlux branché, Internet
  winlux-reset prepare --mode keep|wipe --source SOURCE [--clean] [--user NOM --fullname NOM]
                                           prépare la réinitialisation (mot de passe du nouveau
                                           compte sur l'entrée standard pour « wipe »)
  winlux-reset cancel                      annule une réinitialisation préparée
  winlux-reset repair-boot                 réinstalle GRUB (BIOS et UEFI) et répare grub.cfg
  winlux-reset reboot-recovery             le prochain démarrage ouvre le menu de secours
  winlux-reset list-keys                   clés USB utilisables comme clé de secours
  winlux-reset make-key --device /dev/sdX --source SOURCE
                                           crée la clé de secours (tout son contenu est effacé)

SOURCE : « cloud » (dernière version publiée, téléchargée puis vérifiée),
« media:DOSSIER » (clé de secours, clé d'installation ou DVD Winlux monté) ou
« iso:FICHIER » (image ISO Winlux).

Deux temps, comme Windows. Pendant que Winlux tourne, `prepare` décompresse le
système d'origine dans /.winlux-reset/new sur la même partition, y reporte ce
qu'il faut garder (comptes, mots de passe, réglages de la machine) et pose le
noyau et l'initramfs de la nouvelle version dans /boot. Au redémarrage,
l'initramfs (scripts/winlux-reset-apply.sh) échange les dossiers par simples
renommages, avant que le système ne démarre : /home n'est jamais déplacé en
mode « Conserver mes fichiers », et une coupure de courant reprend l'échange
là où il s'était arrêté.

Sortie : lignes « PROGRESS pourcentage texte », puis « DONE texte » ou
« ERROR texte » ; textes en français, anglais, allemand ou espagnol.
"""
import argparse
import ctypes
import ctypes.util
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import string
import subprocess
import sys
import threading
import time

ROOT = Path(os.environ.get("WINLUX_RESET_ROOT", "/"))
STATE = ROOT / ".winlux-reset"
WORK = Path(os.environ.get("WINLUX_RESET_WORK", "/run/winlux/reset"))
DOWNLOADS = ROOT / "var/lib/winlux/reset-download"
REPOSITORY = "Univers4craft/winlux"
RELEASE_API = f"https://api.github.com/repos/{REPOSITORY}/releases/latest"
NAME = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
FULLNAME = re.compile(r"^[^:\n\r\0,]{0,64}$")
USER_MIN, USER_MAX = 1000, 60000
SKIP_USERS = {"winlux-web", "nobody"}
# Présent dans l'initramfs des versions qui savent terminer une réinitialisation.
SUPPORT_MARKER = b"reset-apply.sh"
KEY_LABEL = "WLXSECOURS"
KEY_MARKER = "boot/winlux-rescue-key"
NEXT_RECOVERY = "boot/winlux-next-recovery"
NEXT_RECOVERY_BLOCK = "# winlux-next-recovery"
ESP_TYPE = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"
BIOS_GRUB_TYPE = "21686148-6449-6e6f-744e-656564454649"

# Réglages de la machine, gardés dans les deux modes.
MACHINE_FILES = ("etc/hostname", "etc/hosts", "etc/fstab", "etc/machine-id", "etc/default/locale",
                 "etc/default/keyboard", "etc/locale.gen", "etc/timezone", "etc/localtime",
                 "etc/environment", "etc/apt/apt.conf.d/95winlux-proxy", "etc/winlux/proxy.env",
                 "etc/winlux/install-config.json", "etc/winlux/locale.conf", "etc/winlux/input.conf",
                 "etc/winlux/profile.json", "etc/winlux/benchmark.json", "var/lib/winlux/secureboot")
# « Conserver mes fichiers » : en plus, ce qui appartient aux comptes.
USER_FILES = ("etc/NetworkManager/system-connections", "etc/winlux/backup", "etc/winlux/users.conf")
# Restes du Live que l'installateur retire aussi (configure_first_boot).
LIVE_LEFTOVERS = ("root/Bureau/winlux-installer.desktop", "usr/share/applications/winlux-installer.desktop")
HOME_DIRS = ("Bureau", "Documents", "Téléchargements", "Images", "Musique", "Vidéos")
USER_GROUPS = ("sudo", "audio", "video", "plugdev", "netdev", "bluetooth", "input", "dialout", "cdrom", "floppy")

TEXT = {
    "Droits administrateur requis.": ("Administrator rights required.", "Se requieren derechos de administrador.", "Administratorrechte erforderlich."),
    "Winlux n’est pas installé sur ce disque : rien à réinitialiser.": ("Winlux is not installed on this disk: nothing to reset.", "Winlux no está instalado en este disco: no hay nada que restablecer.", "Winlux ist nicht auf diesem Datenträger installiert: nichts zurückzusetzen."),
    "Une réinitialisation est déjà en cours d’application : redémarrez pour la terminer.": ("A reset is already being applied: restart to finish it.", "Ya se está aplicando un restablecimiento: reinicie para terminarlo.", "Eine Zurücksetzung wird bereits angewendet: Starten Sie neu, um sie abzuschließen."),
    "Partition séparée pour {mount} : réinitialisez depuis la clé de secours en réinstallant Winlux.": ("Separate partition for {mount}: reinstall Winlux from the recovery drive instead.", "Partición separada para {mount}: reinstale Winlux desde la unidad de recuperación.", "Eigene Partition für {mount}: Installieren Sie Winlux stattdessen vom Wiederherstellungslaufwerk neu."),
    "La partition {mount} n’est pas en ext4 : « Tout supprimer » ne peut pas la vider.": ("The {mount} partition is not ext4: “Remove everything” cannot empty it.", "La partición {mount} no es ext4: «Quitar todo» no puede vaciarla.", "Die Partition {mount} ist nicht ext4: „Alles entfernen“ kann sie nicht leeren."),
    "Nom de compte invalide (lettres minuscules, chiffres, - et _).": ("Invalid account name (lowercase letters, digits, - and _).", "Nombre de cuenta no válido (minúsculas, cifras, - y _).", "Ungültiger Kontoname (Kleinbuchstaben, Ziffern, - und _)."),
    "Mot de passe vide : choisissez un mot de passe pour le nouveau compte.": ("Empty password: choose a password for the new account.", "Contraseña vacía: elija una contraseña para la nueva cuenta.", "Leeres Kennwort: Wählen Sie ein Kennwort für das neue Konto."),
    "Recherche de la dernière version de Winlux…": ("Looking for the latest Winlux version…", "Buscando la última versión de Winlux…", "Neueste Winlux-Version wird gesucht…"),
    "Téléchargement de Winlux {version}…": ("Downloading Winlux {version}…", "Descargando Winlux {version}…", "Winlux {version} wird heruntergeladen…"),
    "Vérification de l’image téléchargée…": ("Checking the downloaded image…", "Comprobando la imagen descargada…", "Heruntergeladenes Abbild wird geprüft…"),
    "Téléchargement impossible : {error}": ("Download failed: {error}", "No se pudo descargar: {error}", "Download fehlgeschlagen: {error}"),
    "L’image téléchargée est corrompue (somme SHA-256 différente).": ("The downloaded image is corrupted (SHA-256 mismatch).", "La imagen descargada está dañada (SHA-256 distinto).", "Das heruntergeladene Abbild ist beschädigt (SHA-256 stimmt nicht)."),
    "Aucun système Winlux dans {path}.": ("No Winlux system in {path}.", "No hay ningún sistema Winlux en {path}.", "Kein Winlux-System in {path}."),
    "Cette image de Winlux est trop ancienne pour réinitialiser un PC : utilisez la version {version} ou plus récente.": ("This Winlux image is too old to reset a PC: use version {version} or newer.", "Esta imagen de Winlux es demasiado antigua para restablecer un PC: use la versión {version} o posterior.", "Dieses Winlux-Abbild ist zu alt zum Zurücksetzen: Verwenden Sie Version {version} oder neuer."),
    "Espace disque insuffisant : {need} Go libres nécessaires, {free} Go disponibles.": ("Not enough disk space: {need} GB free needed, {free} GB available.", "Espacio insuficiente: se necesitan {need} GB libres, hay {free} GB.", "Nicht genug Speicherplatz: {need} GB frei benötigt, {free} GB verfügbar."),
    "Décompression du système d’origine…": ("Unpacking the original system…", "Descomprimiendo el sistema original…", "Ursprüngliches System wird entpackt…"),
    "Report des comptes et des réglages à garder…": ("Carrying over the accounts and settings to keep…", "Trasladando las cuentas y la configuración que se conservan…", "Zu behaltende Konten und Einstellungen werden übernommen…"),
    "Installation du noyau de démarrage…": ("Installing the boot kernel…", "Instalando el núcleo de arranque…", "Startkernel wird installiert…"),
    "Prêt : redémarrez pour terminer la réinitialisation. N’éteignez pas le PC pendant l’opération.": ("Ready: restart to finish the reset. Do not turn the PC off during the operation.", "Listo: reinicie para terminar el restablecimiento. No apague el PC durante la operación.", "Bereit: Starten Sie neu, um das Zurücksetzen abzuschließen. Schalten Sie den PC währenddessen nicht aus."),
    "Réinitialisation annulée : rien n’a été modifié.": ("Reset cancelled: nothing was changed.", "Restablecimiento cancelado: no se ha modificado nada.", "Zurücksetzen abgebrochen: Es wurde nichts geändert."),
    "Aucune réinitialisation en attente.": ("No pending reset.", "No hay ningún restablecimiento pendiente.", "Keine ausstehende Zurücksetzung."),
    "Échec : {error}": ("Failed: {error}", "Error: {error}", "Fehlgeschlagen: {error}"),
    "Partition système introuvable.": ("System partition not found.", "No se encuentra la partición del sistema.", "Systempartition nicht gefunden."),
    "Réinstallation de GRUB pour le démarrage BIOS…": ("Reinstalling GRUB for BIOS boot…", "Reinstalando GRUB para arranque BIOS…", "GRUB für BIOS-Start wird neu installiert…"),
    "Réinstallation de GRUB pour le démarrage UEFI…": ("Reinstalling GRUB for UEFI boot…", "Reinstalando GRUB para arranque UEFI…", "GRUB für UEFI-Start wird neu installiert…"),
    "Menu de démarrage réécrit (l’ancien était absent ou invalide).": ("Boot menu rewritten (the old one was missing or invalid).", "Menú de arranque reescrito (el anterior faltaba o no era válido).", "Startmenü neu geschrieben (das alte fehlte oder war ungültig)."),
    "Démarrage réparé.": ("Startup repaired.", "Arranque reparado.", "Start repariert."),
    "Démarrage réparé, avec des avertissements : {detail}": ("Startup repaired, with warnings: {detail}", "Arranque reparado, con advertencias: {detail}", "Start repariert, mit Warnungen: {detail}"),
    "grub-install est absent : démarrez la clé de secours pour réparer le démarrage.": ("grub-install is missing: boot the recovery drive to repair startup.", "Falta grub-install: arranque desde la unidad de recuperación para reparar el inicio.", "grub-install fehlt: Starten Sie vom Wiederherstellungslaufwerk, um den Start zu reparieren."),
    "Menu de démarrage Winlux introuvable.": ("Winlux boot menu not found.", "No se encuentra el menú de arranque de Winlux.", "Winlux-Startmenü nicht gefunden."),
    "Le prochain démarrage ouvrira le menu de secours.": ("The next startup will open the recovery menu.", "El próximo inicio abrirá el menú de recuperación.", "Der nächste Start öffnet das Wiederherstellungsmenü."),
    "Clé USB introuvable ou refusée : {device}": ("USB drive not found or refused: {device}", "Unidad USB no encontrada o rechazada: {device}", "USB-Laufwerk nicht gefunden oder abgelehnt: {device}"),
    "Clé trop petite : {need} Go nécessaires.": ("Drive too small: {need} GB needed.", "Unidad demasiado pequeña: se necesitan {need} GB.", "Laufwerk zu klein: {need} GB benötigt."),
    "Préparation de la clé (tout son contenu est effacé)…": ("Preparing the drive (all its content is erased)…", "Preparando la unidad (se borra todo su contenido)…", "Laufwerk wird vorbereitet (gesamter Inhalt wird gelöscht)…"),
    "Copie de Winlux sur la clé…": ("Copying Winlux to the drive…", "Copiando Winlux en la unidad…", "Winlux wird auf das Laufwerk kopiert…"),
    "Installation du démarrage de la clé…": ("Installing the drive’s boot loader…", "Instalando el arranque de la unidad…", "Bootloader des Laufwerks wird installiert…"),
    "Clé de secours prête : vous pouvez la retirer.": ("Recovery drive ready: you can remove it.", "Unidad de recuperación lista: puede retirarla.", "Wiederherstellungslaufwerk bereit: Sie können es entfernen."),
    "Menu de secours de ce PC": ("Recovery menu for this PC", "Menú de recuperación de este PC", "Wiederherstellungsmenü dieses PCs"),
    "Menu de secours (autre PC Winlux)": ("Recovery menu (another Winlux PC)", "Menú de recuperación (otro PC Winlux)", "Wiederherstellungsmenü (anderer Winlux-PC)"),
    "Démarrer Winlux depuis la clé (réinstaller)": ("Start Winlux from the drive (reinstall)", "Iniciar Winlux desde la unidad (reinstalar)", "Winlux vom Laufwerk starten (neu installieren)"),
    "Vérifier la clé puis démarrer": ("Check the drive, then start", "Comprobar la unidad y luego iniciar", "Laufwerk prüfen, dann starten"),
    "Aucune installation Winlux trouvée sur les disques.": ("No Winlux installation found on the disks.", "No se encontró ninguna instalación de Winlux en los discos.", "Keine Winlux-Installation auf den Datenträgern gefunden."),
    "Applications supprimées.txt": ("Removed apps.txt", "Aplicaciones eliminadas.txt", "Entfernte Apps.txt"),
    "Applications retirées lors de la réinitialisation du {date} :": ("Apps removed by the reset on {date}:", "Aplicaciones quitadas por el restablecimiento del {date}:", "Beim Zurücksetzen am {date} entfernte Apps:"),
    "Réinstallez-les depuis la Boutique si vous en avez besoin. Vos fichiers et vos programmes Windows de votre dossier personnel sont toujours là.": ("Reinstall them from the Store if you need them. Your files and the Windows programs in your personal folder are still there.", "Reinstálelas desde la Tienda si las necesita. Sus archivos y los programas de Windows de su carpeta personal siguen ahí.", "Installieren Sie sie bei Bedarf aus dem Store neu. Ihre Dateien und die Windows-Programme in Ihrem persönlichen Ordner sind noch da."),
    "Clé de secours ou support Winlux ({label})": ("Recovery drive or Winlux media ({label})", "Unidad de recuperación o soporte Winlux ({label})", "Wiederherstellungslaufwerk oder Winlux-Medium ({label})"),
    "Télécharger depuis Internet (environ 2 Go)": ("Download from the Internet (about 2 GB)", "Descargar de Internet (unos 2 GB)", "Aus dem Internet herunterladen (etwa 2 GB)"),
}
_COLUMN = {"en": 0, "es": 1, "de": 2}


EFI32_ARGS = ('# UEFI 32 bits (mode mixte) : sans efi=noruntime, le noyau 64 bits plante\n'
              '# dans les services UEFI sur les processeurs sans pages de 1 Go (Atom).\n'
              'set winlux_efi_args=\n'
              'if [ "${grub_platform}-${grub_cpu}" = "efi-i386" ]; then\n'
              '    set winlux_efi_args=efi=noruntime\n'
              'fi\n')


def efi_grub_targets(own_esp, grub_dir="/usr/lib/grub"):
    """Comme l'installateur : les deux chargeurs UEFI dans EFI/BOOT, sinon celui du micrologiciel."""
    ia32 = os.path.isdir(os.path.join(grub_dir, "i386-efi"))
    if not own_esp:
        try:
            bits = Path("/sys/firmware/efi/fw_platform_size").read_text().strip()
        except OSError:
            bits = "64"
        return ["i386-efi"] if ia32 and bits == "32" else ["x86_64-efi"]
    return ["x86_64-efi", "i386-efi"] if ia32 else ["x86_64-efi"]

def language():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in (ROOT / "etc/winlux/locale.conf").read_text(encoding="utf-8").splitlines():
                if line.startswith("LANG="):
                    code = line[5:].strip()
        except OSError:
            pass
    if not code:
        code = os.environ.get("LANG", "fr")
    return code[:2].lower() if code[:2].lower() in ("fr", "en", "es", "de") else "fr"


LANG = language()


def _(text, **values):
    row = TEXT.get(text)
    if row and LANG in _COLUMN:
        text = row[_COLUMN[LANG]]
    return text.format(**values) if values else text


class ResetError(Exception):
    pass


def emit(kind, text):
    print(f"{kind} {text}", flush=True)


def progress(percent, text):
    emit("PROGRESS", f"{int(percent)} {text}")


def run(argv, **kw):
    kw.setdefault("check", True)
    kw.setdefault("timeout", 600)
    return subprocess.run(argv, **kw)


def output(argv, timeout=60):
    try:
        return subprocess.run(argv, capture_output=True, text=True, timeout=timeout).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def have(tool):
    return shutil.which(tool) is not None


def load_fat_modules():
    """Menu de secours : pas d'udev pour charger FAT et ses jeux de caractères."""
    if have("modprobe"):
        for module in ("vfat", "nls_cp437", "nls_ascii", "nls_iso8859_1", "nls_utf8", "isofs"):
            run(["modprobe", "-q", module], check=False, capture_output=True, timeout=30)


def require_root():
    if os.geteuid() != 0 and "WINLUX_RESET_ROOT" not in os.environ:
        raise ResetError(_("Droits administrateur requis."))


def read_text(path, default=""):
    try:
        return Path(path).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return default


def gib(size):
    return f"{size / 1024 ** 3:.1f}".replace(".", "," if LANG in ("fr", "es", "de") else ".")


# ── Comptes ─────────────────────────────────────────────────────────────────

def read_table(path):
    return [line.split(":") for line in read_text(path).splitlines() if line and not line.startswith("#")]


def write_table(path, rows, mode=0o644, group=None):
    path = Path(path)
    tmp = path.with_name(path.name + ".winlux-reset")
    tmp.write_text("".join(":".join(row) + "\n" for row in rows), encoding="utf-8")
    os.chmod(tmp, mode)
    if group is not None and os.geteuid() == 0:
        os.chown(tmp, 0, group)
    os.replace(tmp, path)


def kept_users(old_etc):
    """Comptes des personnes (UID 1000 à 59999), pas les comptes de service."""
    return [row for row in read_table(old_etc / "passwd")
            if len(row) >= 7 and row[2].isdigit() and USER_MIN <= int(row[2]) < USER_MAX and row[0] not in SKIP_USERS]


def merge_accounts(old_etc, new_etc):
    """Reporte comptes, mots de passe et appartenances aux groupes dans le nouveau /etc."""
    users = kept_users(old_etc)
    names = {row[0] for row in users}
    passwd = [row for row in read_table(new_etc / "passwd") if row[0] not in names]
    taken = {row[2] for row in passwd if len(row) > 2}
    for row in users:
        if row[2] in taken:
            # Un compte de service de la nouvelle version a pris cet UID : le
            # compte de la personne garde le sien, celui du service cède.
            passwd = [other for other in passwd if other[2] != row[2]]
        passwd.append(row)
    old_groups = {row[0]: row for row in read_table(old_etc / "group") if len(row) >= 4}
    own_groups = [row for row in old_groups.values()
                  if row[2].isdigit() and USER_MIN <= int(row[2]) < USER_MAX and row[0] not in SKIP_USERS]
    own_names = {row[0] for row in own_groups}
    groups = []
    for row in read_table(new_etc / "group"):
        if len(row) < 4 or row[0] in own_names:
            continue
        members = [m for m in row[3].split(",") if m]
        before = old_groups.get(row[0])
        if before:
            members += [m for m in before[3].split(",") if m in names and m not in members]
        groups.append(row[:3] + [",".join(members)])
    gids = {row[2] for row in groups}
    for row in own_groups:
        if row[2] in gids:
            groups = [other for other in groups if other[2] != row[2]]
        groups.append(row)
    shadow_old = {row[0]: row for row in read_table(old_etc / "shadow")}
    shadow = [row for row in read_table(new_etc / "shadow") if row[0] not in names]
    shadow += [shadow_old.get(name, [name, "!", "19000", "0", "99999", "7", "", "", ""]) for name in
               [row[0] for row in users]]
    gshadow_old = {row[0]: row for row in read_table(old_etc / "gshadow") if len(row) >= 4}
    gshadow = []
    for row in read_table(new_etc / "gshadow"):
        if len(row) < 4 or row[0] in own_names:
            continue
        members = [m for m in row[3].split(",") if m]
        before = gshadow_old.get(row[0])
        if before:
            members += [m for m in before[3].split(",") if m in names and m not in members]
        gshadow.append(row[:3] + [",".join(members)])
    gshadow += [gshadow_old[name] for name in own_names if name in gshadow_old]
    shadow_gid = next((int(row[2]) for row in groups if row[0] == "shadow" and row[2].isdigit()), None)
    write_table(new_etc / "passwd", passwd)
    write_table(new_etc / "group", groups)
    write_table(new_etc / "shadow", shadow, 0o640, shadow_gid)
    if (new_etc / "gshadow").exists() or gshadow_old:
        write_table(new_etc / "gshadow", gshadow, 0o640, shadow_gid)
    for row in users:
        sudoers = old_etc / "sudoers.d" / row[0]
        if sudoers.is_file():
            (new_etc / "sudoers.d").mkdir(parents=True, exist_ok=True)
            shutil.copy2(sudoers, new_etc / "sudoers.d" / row[0])
    return users


def hash_password(password):
    salt = "".join(secrets.choice(string.ascii_letters + string.digits + "./") for _ in range(16))
    try:
        lib = ctypes.CDLL(ctypes.util.find_library("crypt") or "libcrypt.so.1")
        lib.crypt.restype = ctypes.c_char_p
        lib.crypt.argtypes = (ctypes.c_char_p, ctypes.c_char_p)
        hashed = lib.crypt(password.encode(), f"$6${salt}$".encode())
        if hashed and hashed.startswith(b"$6$"):
            return hashed.decode()
    except OSError:
        pass
    return run(["openssl", "passwd", "-6", "-stdin"], input=password + "\n", capture_output=True,
               text=True, timeout=10).stdout.strip()


def add_account(new_root, user, fullname, hashed):
    """« Tout supprimer » : le nouveau compte, comme à l'installation."""
    etc = new_root / "etc"
    passwd = [row for row in read_table(etc / "passwd") if row[0] != user]
    used = {int(row[2]) for row in passwd if len(row) > 2 and row[2].isdigit()}
    groups = read_table(etc / "group")
    used_gids = {int(row[2]) for row in groups if len(row) > 2 and row[2].isdigit()}
    uid = next(n for n in range(USER_MIN, USER_MAX) if n not in used and n not in used_gids)
    passwd.append([user, "x", str(uid), str(uid), fullname, f"/home/{user}", "/bin/bash"])
    out = []
    for row in groups:
        if len(row) >= 4 and row[0] in USER_GROUPS:
            members = [m for m in row[3].split(",") if m and m != user] + [user]
            row = row[:3] + [",".join(members)]
        if row[0] != user:
            out.append(row)
    out.append([user, "x", str(uid), ""])
    shadow = [row for row in read_table(etc / "shadow") if row[0] != user]
    shadow.append([user, hashed, str(int(time.time() // 86400)), "0", "99999", "7", "", "", ""])
    shadow_gid = next((int(row[2]) for row in out if row[0] == "shadow" and row[2].isdigit()), None)
    write_table(etc / "passwd", passwd)
    write_table(etc / "group", out)
    write_table(etc / "shadow", shadow, 0o640, shadow_gid)
    if (etc / "gshadow").exists():
        gshadow = [row for row in read_table(etc / "gshadow") if row[0] != user]
        for row in gshadow:
            if len(row) >= 4 and row[0] in USER_GROUPS:
                row[3] = ",".join([m for m in row[3].split(",") if m and m != user] + [user])
        gshadow.append([user, "!", "", ""])
        write_table(etc / "gshadow", gshadow, 0o640, shadow_gid)
    home = new_root / "home" / user
    for folder in (".config", ".local/share", *HOME_DIRS):
        (home / folder).mkdir(parents=True, exist_ok=True)
    if os.geteuid() == 0:
        for path in [home, *home.rglob("*")]:
            os.lchown(path, uid, uid)
    os.chmod(home, 0o750)
    (etc / "sudoers.d").mkdir(parents=True, exist_ok=True)
    sudoers = etc / "sudoers.d" / user
    sudoers.write_text(f"{user} ALL=(ALL:ALL) ALL\n")
    os.chmod(sudoers, 0o440)
    config = etc / "winlux/install-config.json"
    try:
        data = json.loads(read_text(config) or "{}")
    except ValueError:
        data = {}
    data.update({"username": user, "fullname": fullname, "packages": [],
                 "reset_date": time.strftime("%Y-%m-%d %H:%M:%S")})
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    os.chmod(config, 0o644)
    return uid


def copy_preserved(old_root, new_root, paths):
    for relative in paths:
        source, target = old_root / relative, new_root / relative
        if not (source.exists() or source.is_symlink()):
            continue
        if target.is_symlink() or target.is_file():
            target.unlink()
        elif target.is_dir():
            shutil.rmtree(target)
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir() and not source.is_symlink():
            shutil.copytree(source, target, symlinks=True)
        else:
            shutil.copy2(source, target, follow_symlinks=False)


def installed_packages(root):
    packages, name = set(), None
    for line in read_text(root / "var/lib/dpkg/status").splitlines():
        if line.startswith("Package: "):
            name = line[9:].strip()
        elif line.startswith("Status: ") and line.endswith(" installed") and name:
            packages.add(name)
    return packages


def desktop_dir(home):
    for line in read_text(home / ".config/user-dirs.dirs").splitlines():
        match = re.match(r'XDG_DESKTOP_DIR="\$HOME/([^"]+)"', line)
        if match:
            return home / match.group(1)
    for name in ("Bureau", "Desktop", "Escritorio", "Schreibtisch"):
        if (home / name).is_dir():
            return home / name
    return home / "Bureau"


# ── Disposition du disque ───────────────────────────────────────────────────

def fstab_entries(root):
    entries = []
    for line in read_text(root / "etc/fstab").splitlines():
        fields = line.split()
        if len(fields) >= 3 and not fields[0].startswith("#"):
            entries.append((fields[0], fields[1], fields[2]))
    return entries


def check_layout(root, mode):
    """Partitions séparées : /home (vidée en « Tout supprimer ») ou refus."""
    wipe, keep_names = [], []
    for spec, mount, fstype in fstab_entries(root):
        if fstype in ("swap", "tmpfs", "proc", "sysfs", "devpts") or mount in ("/", "/boot", "/boot/efi", "none"):
            if fstype == "swap" and spec.startswith("/") and spec.count("/") == 1:
                keep_names.append(spec[1:])
            continue
        if mount.startswith(("/mnt/", "/media/")) or mount in ("/mnt", "/media"):
            continue
        if mount == "/home" or mount.startswith("/home/"):
            if mode == "wipe":
                if fstype != "ext4":
                    raise ResetError(_("La partition {mount} n’est pas en ext4 : « Tout supprimer » ne peut pas la vider.", mount=mount))
                wipe.append(spec)
            continue
        raise ResetError(_("Partition séparée pour {mount} : réinitialisez depuis la clé de secours en réinstallant Winlux.", mount=mount))
    for name in ("swapfile", "swap.img"):
        if (root / name).is_file() and name not in keep_names:
            keep_names.append(name)
    return wipe, keep_names


def installed(root):
    return (root / "boot/vmlinuz").is_file() and ((root / "boot/winlux-split-root").is_file()
                                                  or (root / "etc/winlux/install-config.json").is_file())


def running_live():
    return Path("/run/winlux/live/media").is_dir() and os.path.ismount("/run/winlux/live/media")


# ── Sources du système d'origine ────────────────────────────────────────────

class Source:
    def __init__(self, label, kernel, initramfs, squashfs, offset=0, efi=None, themes=None, version=""):
        self.label, self.kernel, self.initramfs, self.squashfs = label, Path(kernel), Path(initramfs), Path(squashfs)
        self.offset, self.efi, self.themes, self.version = offset, efi, themes, version

    def unsquash_args(self):
        return (["-offset", str(self.offset)] if self.offset else []) + [str(self.squashfs)]


def source_from_dir(directory):
    directory = Path(directory)
    boot = directory / "boot"
    if not all((boot / name).is_file() for name in ("vmlinuz", "winlux-initramfs.cpio.gz", "winlux-rootfs.squashfs")):
        raise ResetError(_("Aucun système Winlux dans {path}.", path=directory))
    efi = directory / "EFI" if (directory / "EFI/BOOT").is_dir() else None
    themes = boot / "grub/themes" if (boot / "grub/themes").is_dir() else None
    source = Source(str(directory), boot / "vmlinuz", boot / "winlux-initramfs.cpio.gz",
                    boot / "winlux-rootfs.squashfs", efi=efi, themes=themes)
    source.version = squashfs_version(source)
    return source


def source_from_iso(iso, work):
    iso = Path(iso)
    if not iso.is_file():
        raise ResetError(_("Aucun système Winlux dans {path}.", path=iso))
    work.mkdir(parents=True, exist_ok=True)
    report = output(["xorriso", "-indev", str(iso), "-find", "/boot/winlux-rootfs.squashfs",
                     "-exec", "report_lba", "--"], timeout=60)
    extent = None
    for line in report.splitlines():
        if line.startswith("File data lba:"):
            fields = line.split(",")
            if len(fields) >= 2 and fields[1].strip().isdigit():
                extent = int(fields[1].strip())
            break
    if extent is None:
        raise ResetError(_("Aucun système Winlux dans {path}.", path=iso))
    boot = work / "iso-boot"
    if boot.exists():
        shutil.rmtree(boot)
    boot.mkdir(parents=True)
    for inside, outside in (("/boot/vmlinuz", "vmlinuz"), ("/boot/winlux-initramfs.cpio.gz", "winlux-initramfs.cpio.gz")):
        run(["xorriso", "-osirrox", "on", "-indev", str(iso), "-extract", inside, str(boot / outside)],
            capture_output=True, timeout=120)
    efi = themes = None
    if run(["xorriso", "-osirrox", "on", "-indev", str(iso), "-extract", "/EFI", str(boot / "EFI")],
           capture_output=True, check=False, timeout=120).returncode == 0 and (boot / "EFI/BOOT").is_dir():
        efi = boot / "EFI"
    if run(["xorriso", "-osirrox", "on", "-indev", str(iso), "-extract", "/boot/grub/themes", str(boot / "themes")],
           capture_output=True, check=False, timeout=120).returncode == 0 and (boot / "themes").is_dir():
        themes = boot / "themes"
    for path in boot.rglob("*"):
        os.chmod(path, 0o755 if path.is_dir() else 0o644)
    source = Source(str(iso), boot / "vmlinuz", boot / "winlux-initramfs.cpio.gz", iso, extent * 2048, efi, themes)
    source.version = squashfs_version(source)
    return source


def squashfs_version(source):
    text = output(["unsquashfs", "-cat", *source.unsquash_args(), "usr/share/winlux/version"], timeout=60)
    return text.strip().splitlines()[0] if text.strip() else ""


def squashfs_unpacked_size(source):
    total = 0
    for line in output(["unsquashfs", "-lls", *source.unsquash_args()], timeout=600).splitlines():
        fields = line.split()
        if len(fields) >= 3 and line.startswith("-") and fields[2].isdigit():
            total += int(fields[2])
    return total or source.squashfs.stat().st_size * 3


def initramfs_supports_reset(path):
    """L'initramfs de cette version termine-t-elle une réinitialisation ?"""
    tail = b""
    try:
        with gzip.open(path, "rb") as stream:
            while True:
                chunk = stream.read(1 << 20)
                if not chunk:
                    return False
                if SUPPORT_MARKER in tail + chunk:
                    return True
                tail = chunk[-64:]
    except (OSError, EOFError, gzip.BadGzipFile):
        return False


def mounted():
    """{point de montage: périphérique} de /proc/self/mounts."""
    out = {}
    for line in read_text("/proc/self/mounts").splitlines():
        fields = line.split()
        if len(fields) >= 2:
            out[fields[1].replace("\\040", " ")] = fields[0]
    return out


def find_media(scan=False):
    """Clé de secours, clé d'installation ou DVD Winlux, montés ou à monter."""
    found = []
    for mount in mounted():
        if (Path(mount) / "boot/winlux-rootfs.squashfs").is_file() and Path(mount) not in found:
            found.append(Path(mount))
    if scan and have("lsblk"):
        load_fat_modules()
        try:
            tree = json.loads(output(["lsblk", "-J", "-p", "-o", "NAME,FSTYPE,MOUNTPOINT"]) or "{}")
        except ValueError:
            tree = {}
        stack = list(tree.get("blockdevices", []))
        index = 0
        while stack:
            device = stack.pop(0)
            stack.extend(device.get("children") or [])
            fstype = device.get("fstype")
            if not fstype and not device.get("children"):
                # Sans base udev (menu de secours), lsblk ne connaît pas le type : sonder.
                fstype = output(["blkid", "-p", "-o", "value", "-s", "TYPE", device["name"]], timeout=20).strip()
            if fstype not in ("vfat", "iso9660") or device.get("mountpoint"):
                continue
            target = WORK / f"media-{index}"
            index += 1
            target.mkdir(parents=True, exist_ok=True)
            if run(["mount", "-o", "ro", device["name"], str(target)], check=False,
                   capture_output=True, timeout=30).returncode != 0:
                continue
            if (target / "boot/winlux-rootfs.squashfs").is_file():
                found.append(target)
            else:
                run(["umount", str(target)], check=False, capture_output=True)
    return found


def cloud_release():
    progress(2, _("Recherche de la dernière version de Winlux…"))
    try:
        data = json.loads(run(["curl", "--fail", "--location", "--silent", "--show-error", "--proto", "=https",
                               "--max-time", "60", "-H", "Accept: application/vnd.github+json", RELEASE_API],
                              capture_output=True, text=True, timeout=90).stdout)
    except (subprocess.SubprocessError, ValueError, OSError) as error:
        raise ResetError(_("Téléchargement impossible : {error}", error=getattr(error, "stderr", None) or error))
    tag = data.get("tag_name", "")
    assets = {asset.get("name"): asset for asset in data.get("assets", [])}
    iso = assets.get(f"winlux-live-{tag}.iso")
    checksum = assets.get(f"winlux-live-{tag}.iso.sha256")
    if not (re.fullmatch(r"v[0-9A-Za-z.+-]+", tag) and iso and checksum):
        raise ResetError(_("Téléchargement impossible : {error}", error="winlux-live-*.iso"))
    return tag, iso, checksum


def download(url, target, size, start, end, label):
    target.parent.mkdir(parents=True, exist_ok=True)
    process = subprocess.Popen(["curl", "--fail", "--location", "--silent", "--show-error", "--proto", "=https",
                                "--retry", "3", "--continue-at", "-", "-o", str(target), url],
                               stderr=subprocess.PIPE, text=True)
    while process.poll() is None:
        time.sleep(1)
        try:
            done = target.stat().st_size
        except OSError:
            done = 0
        if size:
            progress(start + (end - start) * min(done, size) // size, f"{label} {gib(done)} / {gib(size)} Go")
    if process.returncode != 0:
        raise ResetError(_("Téléchargement impossible : {error}", error=(process.stderr.read() or "").strip()[-300:]))


def cloud_source(work, start=3, end=40):
    tag, iso, checksum = cloud_release()
    free = shutil.disk_usage(DOWNLOADS.parent if DOWNLOADS.parent.exists() else ROOT).free
    if free < iso.get("size", 0) + (512 << 20):
        raise ResetError(_("Espace disque insuffisant : {need} Go libres nécessaires, {free} Go disponibles.",
                           need=gib(iso.get("size", 0) + (512 << 20)), free=gib(free)))
    target = DOWNLOADS / iso["name"]
    sums = DOWNLOADS / checksum["name"]
    download(checksum["browser_download_url"], sums, 0, start, start, "")
    expected = read_text(sums).split()[0] if read_text(sums).split() else ""
    download(iso["browser_download_url"], target, iso.get("size", 0), start, end - 3,
             _("Téléchargement de Winlux {version}…", version=tag))
    progress(end - 2, _("Vérification de l’image téléchargée…"))
    digest = hashlib.sha256()
    with target.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 22), b""):
            digest.update(block)
    if not re.fullmatch(r"[0-9a-f]{64}", expected) or digest.hexdigest() != expected:
        target.unlink(missing_ok=True)
        raise ResetError(_("L’image téléchargée est corrompue (somme SHA-256 différente)."))
    return source_from_iso(target, work)


def resolve_source(spec, work, start=3, end=40):
    if spec == "cloud":
        return cloud_source(work, start, end)
    kind, _sep, value = spec.partition(":")
    if kind == "media" and value.startswith("/"):
        return source_from_dir(value)
    if kind == "iso" and value.startswith("/"):
        return source_from_iso(value, work)
    if kind == "auto":
        media = find_media(scan=True)
        if media:
            return source_from_dir(media[0])
        return cloud_source(work, start, end)
    raise ResetError(_("Aucun système Winlux dans {path}.", path=spec))


def check_support(source):
    if not initramfs_supports_reset(source.initramfs):
        raise ResetError(_("Cette image de Winlux est trop ancienne pour réinitialiser un PC : utilisez la version {version} ou plus récente.",
                           version=read_text(ROOT / "usr/share/winlux/version").strip() or "v0.2.18"))


# ── Préparer, annuler ───────────────────────────────────────────────────────

def unsquash(source, target, start, end):
    command = ["unsquashfs", "-force", "-processors", "2", "-d", str(target), *source.unsquash_args()]
    process = subprocess.Popen(command[:1] + ["-percentage"] + command[1:], stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True)
    last = -1
    for line in process.stdout:
        value = line.strip()
        if value.isdigit() and int(value) != last:
            last = int(value)
            progress(start + (end - start) * last // 100, _("Décompression du système d’origine…"))
    error = process.stderr.read()
    if process.wait() != 0:
        if "percentage" in error:
            run(command[:1] + ["-no-progress"] + command[1:], timeout=7200)
            return
        raise ResetError(_("Échec : {error}", error=error.strip()[-400:]))


def stage_boot(source, root):
    """Noyau et initramfs de la nouvelle version ; les anciens restent à côté jusqu'à la fin."""
    boot = root / "boot"
    kmap = luks_keymap(boot / "winlux-initramfs.cpio.gz")
    for name, new in (("winlux-initramfs.cpio.gz", source.initramfs), ("vmlinuz", source.kernel)):
        current, backup = boot / name, boot / (name + ".before-reset")
        if current.is_file() and not backup.exists():
            shutil.copy2(current, backup)
        tmp = boot / (name + ".reset-new")
        shutil.copyfile(new, tmp)
        if name == "winlux-initramfs.cpio.gz" and kmap is not None:
            with tmp.open("ab") as stream:
                stream.write(gzip.compress(kmap))
        with tmp.open("rb") as stream:
            os.fsync(stream.fileno())
        os.chmod(tmp, 0o644)
        os.replace(tmp, current)


def restore_boot(root):
    boot = root / "boot"
    for name in ("winlux-initramfs.cpio.gz", "vmlinuz"):
        backup = boot / (name + ".before-reset")
        if backup.is_file():
            os.replace(backup, boot / name)
        (boot / (name + ".reset-new")).unlink(missing_ok=True)


def cpio_members(data):
    """(nom, données) de toutes les archives cpio newc concaténées."""
    offset = 0
    while offset + 110 <= len(data):
        if data[offset:offset + 6] != b"070701":
            offset += 1
            continue
        header = data[offset:offset + 110]
        size, name_size = int(header[54:62], 16), int(header[94:102], 16)
        name_end = offset + 110 + name_size
        name = data[offset + 110:name_end - 1].decode(errors="replace")
        start = (name_end + 3) & ~3
        yield name, header, data[start:start + size]
        offset = (start + size + 3) & ~3


def luks_keymap(initramfs):
    """Clavier de la phrase secrète ajouté par l'installateur (disque chiffré)."""
    try:
        with gzip.open(initramfs, "rb") as stream:
            data = stream.read()
    except (OSError, EOFError, gzip.BadGzipFile):
        return None
    for name, _header, content in cpio_members(data):
        if name == "etc/boot.kmap":
            return cpio_newc([("etc", 0o040755, b""), ("etc/boot.kmap", 0o100644, content)])
    return None


def cpio_newc(entries):
    out = bytearray()
    for index, (name, mode, data) in enumerate(entries + [("TRAILER!!!", 0, b"")]):
        encoded = name.encode() + b"\0"
        out += b"070701" + b"".join(f"{value:08X}".encode() for value in
                                    (index + 1, mode, 0, 0, 1, 0, len(data), 0, 0, 0, 0, len(encoded), 0))
        out += encoded + b"\0" * ((4 - (110 + len(encoded)) % 4) % 4)
        out += data + b"\0" * ((4 - len(data) % 4) % 4)
    return bytes(out)


def prepare(args, password=""):
    require_root()
    if not installed(ROOT):
        raise ResetError(_("Winlux n’est pas installé sur ce disque : rien à réinitialiser."))
    if (STATE / "state").exists():
        raise ResetError(_("Une réinitialisation est déjà en cours d’application : redémarrez pour la terminer."))
    if args.mode == "wipe":
        if not NAME.match(args.user or "") or args.user in SKIP_USERS or args.user == "root":
            raise ResetError(_("Nom de compte invalide (lettres minuscules, chiffres, - et _)."))
        if not FULLNAME.match(args.fullname or ""):
            raise ResetError(_("Nom de compte invalide (lettres minuscules, chiffres, - et _)."))
        if not password:
            raise ResetError(_("Mot de passe vide : choisissez un mot de passe pour le nouveau compte."))
    wipe_mounts, keep_names = check_layout(ROOT, args.mode)
    work = WORK / "prepare"
    source = resolve_source(args.source, work)
    check_support(source)
    need = squashfs_unpacked_size(source) + (512 << 20)
    if STATE.exists():
        shutil.rmtree(STATE)
    free = shutil.disk_usage(ROOT).free
    if free < need:
        raise ResetError(_("Espace disque insuffisant : {need} Go libres nécessaires, {free} Go disponibles.",
                           need=gib(need), free=gib(free)))
    STATE.mkdir(mode=0o700)
    new = STATE / "new"
    try:
        unsquash(source, new, 40, 85)
        progress(86, _("Report des comptes et des réglages à garder…"))
        configure_new_system(new, args, password, keep_names)
        progress(92, _("Installation du noyau de démarrage…"))
        names = sorted(entry.name for entry in new.iterdir())
        (STATE / "new-names").write_text("".join(f"{name}\n" for name in names))
        (STATE / "keep-names").write_text("".join(f"{name}\n" for name in keep_names))
        (STATE / "wipe-mounts").write_text("".join(f"{spec}\n" for spec in wipe_mounts))
        plan = {"mode": args.mode, "clean": "1" if args.clean and args.mode == "wipe" else "0", "lang": LANG,
                "version": source.version, "date": time.strftime("%Y-%m-%d %H:%M:%S"), "source": source.label}
        (STATE / "plan").write_text("".join(f"{key}={value}\n" for key, value in plan.items()))
        stage_boot(source, ROOT)
        os.sync()
        (STATE / "ready.new").write_text("1\n")
        os.replace(STATE / "ready.new", STATE / "ready")
        os.sync()
    except BaseException:
        shutil.rmtree(STATE, ignore_errors=True)
        restore_boot(ROOT)
        raise
    finally:
        if args.source == "cloud" or args.source == "auto":
            shutil.rmtree(DOWNLOADS, ignore_errors=True)
    emit("DONE", _("Prêt : redémarrez pour terminer la réinitialisation. N’éteignez pas le PC pendant l’opération."))


def configure_new_system(new, args, password, keep_names):
    old_etc, new_etc = ROOT / "etc", new / "etc"
    for leftover in LIVE_LEFTOVERS:
        (new / leftover).unlink(missing_ok=True)
    copy_preserved(ROOT, new, MACHINE_FILES)
    if args.mode == "keep":
        users = merge_accounts(old_etc, new_etc)
        copy_preserved(ROOT, new, USER_FILES)
        removed = sorted(installed_packages(ROOT) - installed_packages(new))
        removed = [name for name in removed if not name.startswith(("linux-", "winlux-"))]
        if removed:
            report = _("Applications retirées lors de la réinitialisation du {date} :", date=time.strftime("%d/%m/%Y")) + "\n\n"
            report += "".join(f"  - {name}\n" for name in removed) + "\n"
            report += _("Réinstallez-les depuis la Boutique si vous en avez besoin. Vos fichiers et vos programmes Windows de votre dossier personnel sont toujours là.") + "\n"
            (STATE / "removed-apps.txt").write_text(report, encoding="utf-8")
            lines = []
            for row in users:
                home = Path(row[5])
                if home.is_absolute() and (ROOT / home.relative_to("/")).is_dir():
                    target = desktop_dir(ROOT / home.relative_to("/"))
                    relative = "/" + str(target.relative_to(ROOT)) + "/" + _("Applications supprimées.txt")
                    lines.append(f"{row[2]}:{row[3]}:{relative}\n")
            (STATE / "removed-apps").write_text("".join(lines), encoding="utf-8")
    else:
        add_account(new, args.user, args.fullname or args.user, hash_password(password))
    for name in keep_names:
        (new / name).unlink(missing_ok=True)


def cancel():
    require_root()
    if (STATE / "state").exists():
        raise ResetError(_("Une réinitialisation est déjà en cours d’application : redémarrez pour la terminer."))
    if not STATE.exists():
        emit("DONE", _("Aucune réinitialisation en attente."))
        return
    (STATE / "ready").unlink(missing_ok=True)
    restore_boot(ROOT)
    shutil.rmtree(STATE, ignore_errors=True)
    os.sync()
    emit("DONE", _("Réinitialisation annulée : rien n’a été modifié."))


def status():
    plan = dict(line.split("=", 1) for line in read_text(STATE / "plan").splitlines() if "=" in line)
    values = {"installed": "1" if installed(ROOT) else "0", "live": "1" if running_live() else "0",
              "pending": plan.get("mode", "") if (STATE / "ready").exists() else "",
              "pending_version": plan.get("version", ""),
              "grub_install": "1" if have("grub-install") else "0",
              "key_tools": "1" if all(have(t) for t in ("parted", "mkfs.fat", "grub-install")) else "0",
              "next_recovery": "1" if (ROOT / NEXT_RECOVERY).exists() else "0"}
    for key, value in values.items():
        print(f"{key}={value}")


def sources(scan):
    for media in find_media(scan):
        label = media.name
        try:
            label = source_from_dir(media).version or label
        except ResetError:
            continue
        print(f"media:{media}|{_('Clé de secours ou support Winlux ({label})', label=label)}")
    print(f"cloud|{_('Télécharger depuis Internet (environ 2 Go)')}")


# ── Démarrage : réparer, menu de secours ────────────────────────────────────

def cmdline():
    text = os.environ.get("WINLUX_RESET_CMDLINE") or read_text("/proc/cmdline")
    return dict(token.split("=", 1) for token in text.split() if "=" in token)


def resolve_device(spec):
    if spec.startswith(("UUID=", "LABEL=", "PARTUUID=")):
        found = output(["blkid", "-o", "device", "-t", spec]).split()
        return found[0] if found else None
    return spec if spec and os.path.exists(spec) else None


def disk_of(device):
    name = os.path.basename(os.path.realpath(device))
    sys_dir = Path("/sys/class/block") / name
    slaves = sorted((sys_dir / "slaves").iterdir()) if (sys_dir / "slaves").is_dir() else []
    if slaves:
        return disk_of("/dev/" + slaves[0].name)
    if (sys_dir / "partition").exists():
        return "/dev/" + os.path.basename(os.path.realpath(sys_dir / ".."))
    return "/dev/" + name


def probe(device):
    """Champs de blkid -p (lu sur le disque, sans base udev : menu de secours)."""
    fields = {}
    for line in output(["blkid", "-p", "-o", "export", device], timeout=20).splitlines():
        key, _sep, value = line.partition("=")
        fields[key] = value
    return fields


def partitions(disk):
    """(table, [{name, fstype, parttype, mountpoint}]) lus directement sur le disque."""
    table = probe(disk).get("PTTYPE") or None
    mounts = {os.path.realpath(dev): mount for mount, dev in mounted().items() if dev.startswith("/dev/")}
    name = os.path.basename(os.path.realpath(disk))
    parts = []
    sys_dir = Path("/sys/class/block") / name
    children = sorted((p for p in sys_dir.iterdir() if (p / "partition").exists()),
                      key=lambda p: int(read_text(p / "partition").strip() or 0)) if sys_dir.is_dir() else []
    for child in children:
        device = "/dev/" + child.name
        fields = probe(device)
        parts.append({"name": device, "fstype": fields.get("TYPE"), "parttype": fields.get("PART_ENTRY_TYPE", "").lower(),
                      "mountpoint": mounts.get(os.path.realpath(device))})
    return table, parts


def boot_params():
    params = cmdline()
    root_spec = params.get("root", "")
    extra = "".join(f" {key}={params[key]}" for key in ("winlux.luks", "winlux.boot") if params.get(key))
    return root_spec, extra


def grub_template(search_uuid, root_arg, extra, windows_esp_uuid=None):
    windows = ""
    if windows_esp_uuid:
        windows = (f'menuentry "Windows" --hotkey=w {{\n    insmod part_gpt\n    insmod fat\n    insmod chain\n'
                   f'    search --no-floppy --fs-uuid {windows_esp_uuid} --set=root\n'
                   f'    chainloader /EFI/Microsoft/Boot/bootmgfw.efi\n}}\n')
    timeout, style = (5, "menu") if windows else (1, "hidden")
    return f"""set default=0
# UEFI 32 bits (mode mixte) : sans efi=noruntime, le noyau 64 bits plante
# dans les services UEFI sur les processeurs sans pages de 1 Go (Atom).
set winlux_efi_args=
if [ "${{grub_platform}}-${{grub_cpu}}" = "efi-i386" ]; then
    set winlux_efi_args=efi=noruntime
fi
set timeout={timeout}
set timeout_style={style}
# Terminal série seulement en machine virtuelle : sur un vrai PC il ralentit le clavier du menu.
set winlux_vendor=
smbios --type 1 --get-string 4 --set winlux_vendor
if [ "$winlux_vendor" = "QEMU" ]; then
    if serial --unit=0 --speed=115200; then
        terminal_input console serial
        terminal_output console serial
    fi
fi
menuentry "Winlux Desktop" {{
    search --no-floppy --fs-uuid {search_uuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 quiet loglevel=3 vt.global_cursor_default=0 winlux.interactive=1{extra} $winlux_efi_args
    initrd /boot/winlux-initramfs.cpio.gz
}}
{windows}menuentry "Winlux diagnostic" --hotkey=d {{
    search --no-floppy --fs-uuid {search_uuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 init=/init console=ttyS0,115200n8{extra} $winlux_efi_args
    initrd /boot/winlux-initramfs.cpio.gz
}}
menuentry "Winlux secours (restauration ou console)" --hotkey=s {{
    search --no-floppy --fs-uuid {search_uuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 console=tty1 console=ttyS0,115200n8 winlux.recovery=1{extra} $winlux_efi_args
    initrd /boot/winlux-initramfs.cpio.gz
}}
"""


def grub_cfg_valid(path):
    if not path.is_file() or "Winlux" not in read_text(path):
        return False
    if not have("grub-script-check"):
        return True
    return run(["grub-script-check", str(path)], check=False, capture_output=True).returncode == 0


def repair_boot():
    require_root()
    if not have("grub-install"):
        raise ResetError(_("grub-install est absent : démarrez la clé de secours pour réparer le démarrage."))
    root_spec, extra = boot_params()
    device = resolve_device(root_spec)
    if not device:
        raise ResetError(_("Partition système introuvable."))
    disk = disk_of(device)
    table, parts = partitions(disk)
    warnings = []
    boot = ROOT / "boot"
    if table == "dos" or any((p.get("parttype") or "").lower() == BIOS_GRUB_TYPE for p in parts):
        progress(20, _("Réinstallation de GRUB pour le démarrage BIOS…"))
        result = run(["grub-install", "--target=i386-pc", f"--boot-directory={boot}", "--recheck", disk],
                     check=False, capture_output=True, text=True)
        if result.returncode:
            warnings.append((result.stdout + result.stderr).strip()[-300:])
    esp = next((p for p in parts if (p.get("parttype") or "").lower() in (ESP_TYPE, "0xef")), None) or \
        next((p for p in parts if p.get("fstype") == "vfat"), None)
    windows_esp_uuid = None
    if esp:
        progress(55, _("Réinstallation de GRUB pour le démarrage UEFI…"))
        efi_dir = Path(esp.get("mountpoint") or boot / "efi")
        mounted_here = False
        if not esp.get("mountpoint"):
            load_fat_modules()
            efi_dir.mkdir(parents=True, exist_ok=True)
            run(["mount", "-t", "vfat", "-o", "iocharset=ascii", esp["name"], str(efi_dir)])
            mounted_here = True
        try:
            shared = (efi_dir / "EFI/Microsoft/Boot/bootmgfw.efi").is_file()
            if shared:
                windows_esp_uuid = output(["blkid", "-s", "UUID", "-o", "value", esp["name"]]).strip() or None
            # ESP partagée avec Windows : dossier et entrée à nous, jamais EFI/BOOT.
            efi_args = ["--bootloader-id=winlux"] if shared else ["--removable", "--no-nvram"]
            if shared and not any(Path("/sys/firmware/efi/efivars").glob("*")):
                efi_args.append("--no-nvram")  # efi=noruntime (UEFI 32 bits) : pas de variables
            failed = None
            for efi_target in efi_grub_targets(not shared):
                result = run(["grub-install", f"--target={efi_target}", f"--efi-directory={efi_dir}",
                              f"--boot-directory={boot}", "--recheck", *efi_args], check=False, capture_output=True, text=True)
                if result.returncode and failed is None:
                    failed = result
            if failed is not None:
                result = failed
                warnings.append((result.stdout + result.stderr).strip()[-300:])
            elif not shared and have("winlux-secureboot") and Path("/usr/lib/shim/shimx64.efi.signed").exists():
                uuid = output(["blkid", "-s", "UUID", "-o", "value", device]).strip()
                run(["winlux-secureboot", "install-esp", str(efi_dir), uuid], check=False, capture_output=True)
        finally:
            if mounted_here:
                run(["umount", str(efi_dir)], check=False)
    if not esp and not (table == "dos" or any((p.get("parttype") or "").lower() == BIOS_GRUB_TYPE for p in parts)):
        raise ResetError(_("Partition système introuvable."))
    cfg = boot / "grub/grub.cfg"
    if not grub_cfg_valid(cfg):
        boot_device = resolve_device(cmdline().get("winlux.boot", "")) or device
        search_uuid = output(["blkid", "-s", "UUID", "-o", "value", boot_device]).strip()
        if search_uuid:
            cfg.parent.mkdir(parents=True, exist_ok=True)
            if cfg.exists():
                shutil.copy2(cfg, cfg.with_name("grub.cfg.before-repair"))
            cfg.write_text(grub_template(search_uuid, root_spec, extra, windows_esp_uuid))
            progress(90, _("Menu de démarrage réécrit (l’ancien était absent ou invalide)."))
    os.sync()
    if warnings:
        emit("DONE", _("Démarrage réparé, avec des avertissements : {detail}", detail=" / ".join(warnings)))
    else:
        emit("DONE", _("Démarrage réparé."))


def recovery_title(text):
    match = re.search(r'^menuentry "([^"]*)"[^\n]*--hotkey=s', text, re.M)
    if not match:
        match = re.search(r'^menuentry "([^"]*(?:secours|Recovery)[^"]*)"', text, re.M)
    return match.group(1) if match else None


def enable_next_recovery(cfg):
    """Ajoute à grub.cfg le saut vers le menu de secours quand le témoin existe."""
    text = read_text(cfg)
    title = recovery_title(text)
    if not title:
        raise ResetError(_("Menu de démarrage Winlux introuvable."))
    if NEXT_RECOVERY_BLOCK in text:
        return
    block = (f'{NEXT_RECOVERY_BLOCK} : Paramètres > Récupération > Démarrage avancé\n'
             f'if [ -f /{NEXT_RECOVERY} ]; then\n    set default="{title}"\n    set timeout=3\n'
             f'    set timeout_style=menu\nfi\n')
    lines = text.splitlines(keepends=True)
    index = next((i + 1 for i, line in enumerate(lines) if line.startswith("set timeout_style=")),
                 next((i + 1 for i, line in enumerate(lines) if line.startswith("set timeout=")), 0))
    new = "".join(lines[:index]) + block + "".join(lines[index:])
    tmp = cfg.with_name("grub.cfg.winlux-reset")
    tmp.write_text(new)
    if have("grub-script-check") and run(["grub-script-check", str(tmp)], check=False, capture_output=True).returncode:
        tmp.unlink()
        raise ResetError(_("Menu de démarrage Winlux introuvable."))
    shutil.copymode(cfg, tmp)
    os.replace(tmp, cfg)


def reboot_recovery():
    require_root()
    enable_next_recovery(ROOT / "boot/grub/grub.cfg")
    (ROOT / NEXT_RECOVERY).write_text("1\n")
    os.sync()
    emit("DONE", _("Le prochain démarrage ouvrira le menu de secours."))


# ── Clé de secours ──────────────────────────────────────────────────────────

def system_disks():
    disks = set()
    for mount, device in mounted().items():
        if mount in ("/", "/boot", "/boot/efi", "/home", "/run/winlux/live/media") and device.startswith("/dev/"):
            try:
                disks.add(disk_of(device))
            except OSError:
                pass
    return disks


def list_keys():
    try:
        tree = json.loads(output(["lsblk", "-J", "-b", "-p", "-d", "-o", "NAME,SIZE,TYPE,TRAN,RM,HOTPLUG,MODEL,VENDOR"]) or "{}")
    except ValueError:
        return []
    busy = system_disks()
    keys = []
    for disk in tree.get("blockdevices", []):
        removable = disk.get("tran") == "usb" or str(disk.get("rm")) in ("1", "True", "true") or \
            str(disk.get("hotplug")) in ("1", "True", "true")
        if disk.get("type") != "disk" or not removable or disk["name"] in busy or not int(disk.get("size") or 0):
            continue
        model = " ".join(part for part in ((disk.get("vendor") or "").strip(), (disk.get("model") or "").strip()) if part)
        keys.append((disk["name"], int(disk["size"]), model or os.path.basename(disk["name"])))
    return keys


def partition_path(disk, number):
    return f"{disk}p{number}" if disk[-1].isdigit() else f"{disk}{number}"


def key_grub_cfg(root_spec, extra, theme):
    entries = []
    common = "rw rootwait rootfstype=ext4 console=tty1 console=ttyS0,115200n8 panic=0 loglevel=3 winlux.recovery=1"
    if root_spec and not root_spec.startswith("LABEL=WINLUX_ROOT"):
        entries.append(f'menuentry "{_("Menu de secours de ce PC")}" --hotkey=r {{\n'
                       f'    linux /boot/vmlinuz root={root_spec} {common}{extra} $winlux_efi_args\n'
                       f'    initrd /boot/winlux-initramfs.cpio.gz\n}}\n')
        title = _("Menu de secours (autre PC Winlux)")
    else:
        title = _("Menu de secours de ce PC")
    entries.append(f'menuentry "{title}" {{\n'
                   f'    if search --no-floppy --label WINLUX_ROOT --set=winlux_root; then\n'
                   f'        linux /boot/vmlinuz root=LABEL=WINLUX_ROOT {common} $winlux_efi_args\n'
                   f'        initrd /boot/winlux-initramfs.cpio.gz\n'
                   f'    else\n        echo "{_("Aucune installation Winlux trouvée sur les disques.")}"\n        sleep 6\n    fi\n}}\n')
    live = "console=tty1 console=ttyS0,115200n8 rootfstype=ramfs rdinit=/init panic=1 loglevel=3 vt.global_cursor_default=0"
    entries.append(f'menuentry "{_("Démarrer Winlux depuis la clé (réinstaller)")}" --hotkey=i {{\n'
                   f'    linux /boot/vmlinuz {live} quiet winlux.interactive=1 $winlux_efi_args\n'
                   f'    initrd /boot/winlux-initramfs.cpio.gz\n}}\n')
    entries.append(f'menuentry "{_("Vérifier la clé puis démarrer")}" --hotkey=v {{\n'
                   f'    linux /boot/vmlinuz {live} winlux.check-iso=1 winlux.interactive=1 $winlux_efi_args\n'
                   f'    initrd /boot/winlux-initramfs.cpio.gz\n}}\n')
    # Sans thème, la police Unicode de GRUB (posée par grub-install) garde les accents.
    themed = ("if [ \"${grub_platform}-${grub_cpu}\" = \"efi-i386\" ]; then\n    set winlux_term=console\nelif loadfont /boot/grub/fonts/unicode.pf2; then\n    insmod all_video\n    insmod gfxterm\n"
              "    set gfxmode=auto\n    if terminal_output gfxterm; then\n        set winlux_term=gfxterm\n    fi\nfi\n")
    if theme:
        themed = ("if [ \"${grub_platform}-${grub_cpu}\" = \"efi-i386\" ]; then\n    set winlux_term=console\nelif loadfont /boot/grub/themes/winlux/unicode.pf2; then\n    insmod all_video\n    insmod gfxterm\n"
                  "    insmod png\n    set gfxmode=auto\n    if terminal_output gfxterm; then\n"
                  "        set theme=/boot/grub/themes/winlux/theme.txt\n        set winlux_term=gfxterm\n    fi\nfi\n")
    return ("set default=0\nset timeout=10\nset timeout_style=menu\nset winlux_term=console\n" + EFI32_ARGS + themed +
            "set winlux_vendor=\nsmbios --type 1 --get-string 4 --set winlux_vendor\n"
            "if [ \"$winlux_vendor\" = \"QEMU\" ]; then\n"
            "    if serial --unit=0 --speed=115200; then\n        terminal_input serial console\n"
            "        terminal_output serial $winlux_term\n    fi\nfi\n" + "".join(entries))


def copy_with_progress(source, target, digests, relative, done, total, start, end):
    digest = hashlib.sha256()
    with open(source, "rb") as reader, open(target, "wb") as writer:
        while True:
            block = reader.read(1 << 22)
            if not block:
                break
            writer.write(block)
            digest.update(block)
            done += len(block)
            progress(start + (end - start) * done // max(total, 1), _("Copie de Winlux sur la clé…"))
        writer.flush()
        os.fsync(writer.fileno())
    digests[relative] = digest.hexdigest()
    return done


def copy_squashfs(source, target, digests, done, total, start, end):
    """La racine SquashFS, lue dans l'ISO à son décalage ou depuis un support monté."""
    digest = hashlib.sha256()
    remaining = None
    with open(source.squashfs, "rb") as reader, open(target, "wb") as writer:
        if source.offset:
            reader.seek(source.offset)
            superblock = reader.read(96)
            reader.seek(source.offset)
            # bytes_used : octets 40 à 48 du superbloc SquashFS 4.
            remaining = int.from_bytes(superblock[40:48], "little")
            remaining = ((remaining + 4095) // 4096) * 4096
        while remaining is None or remaining > 0:
            block = reader.read(1 << 22 if remaining is None else min(1 << 22, remaining))
            if not block:
                break
            if remaining is not None:
                remaining -= len(block)
            writer.write(block)
            digest.update(block)
            done += len(block)
            progress(start + (end - start) * done // max(total, 1), _("Copie de Winlux sur la clé…"))
        writer.flush()
        os.fsync(writer.fileno())
    digests["boot/winlux-rootfs.squashfs"] = digest.hexdigest()
    return done


def make_key(args):
    require_root()
    keys = {path: (size, model) for path, size, model in list_keys()}
    device = args.device
    test_device = os.environ.get("WINLUX_RESET_TEST_DEVICE")
    if device != test_device and device not in keys:
        raise ResetError(_("Clé USB introuvable ou refusée : {device}", device=device))
    work = WORK / "key"
    source = resolve_source(args.source, work, 2, 30)
    squash_size = source.squashfs.stat().st_size
    if source.offset:
        with open(source.squashfs, "rb") as reader:
            reader.seek(source.offset + 40)
            squash_size = int.from_bytes(reader.read(8), "little")
    need = squash_size + source.kernel.stat().st_size + source.initramfs.stat().st_size + (300 << 20)
    size = keys.get(device, (int(output(["blockdev", "--getsize64", device]).strip() or 0), ""))[0]
    if size < need:
        raise ResetError(_("Clé trop petite : {need} Go nécessaires.", need=gib(need)))
    progress(31, _("Préparation de la clé (tout son contenu est effacé)…"))
    for mount, dev in sorted(mounted().items(), key=lambda item: -len(item[0])):
        if dev == device or (dev.startswith(device) and dev[len(device):].lstrip("p").isdigit()):
            run(["umount", mount], timeout=60)
    run(["wipefs", "-a", device], capture_output=True)
    run(["parted", "-s", device, "--", "mklabel", "msdos", "mkpart", "primary", "fat32", "1MiB", "100%",
         "set", "1", "boot", "on"], capture_output=True)
    run(["partprobe", device], check=False, capture_output=True) if have("partprobe") else None
    run(["blockdev", "--rereadpt", device], check=False, capture_output=True)
    part = partition_path(device, 1)
    for _attempt in range(50):
        if os.path.exists(part):
            break
        time.sleep(0.2)
    run(["mkfs.fat", "-F", "32", "-n", KEY_LABEL, part], capture_output=True)
    load_fat_modules()
    target = work / "mnt"
    target.mkdir(parents=True, exist_ok=True)
    run(["mount", "-t", "vfat", part, str(target)])
    try:
        boot = target / "boot"
        (boot / "grub").mkdir(parents=True, exist_ok=True)
        digests = {}
        total = squash_size + source.kernel.stat().st_size + source.initramfs.stat().st_size
        done = copy_with_progress(source.kernel, boot / "vmlinuz", digests, "boot/vmlinuz", 0, total, 33, 85)
        done = copy_with_progress(source.initramfs, boot / "winlux-initramfs.cpio.gz", digests,
                                  "boot/winlux-initramfs.cpio.gz", done, total, 33, 85)
        copy_squashfs(source, boot / "winlux-rootfs.squashfs", digests, done, total, 33, 85)
        progress(86, _("Installation du démarrage de la clé…"))
        if source.themes:
            shutil.copytree(source.themes, boot / "grub/themes", dirs_exist_ok=True)
        if have("grub-install"):
            run(["grub-install", "--target=i386-pc", f"--boot-directory={boot}", "--recheck", device],
                check=False, capture_output=True)
            # Clé : BOOTX64.EFI et BOOTIA32.EFI, pour les PC à micrologiciel 32 bits.
            for efi_target in efi_grub_targets(True):
                run(["grub-install", f"--target={efi_target}", "--removable", "--no-nvram", f"--efi-directory={target}",
                     f"--boot-directory={boot}", "--recheck"], check=False, capture_output=True)
        if source.efi and (source.efi / "BOOT/BOOTX64.EFI").is_file():
            # Chaîne Secure Boot de l'ISO (shim signé -> GRUB signé) : ses
            # petits grub.cfg cherchent la clé par son fichier témoin.
            shutil.copytree(source.efi, target / "EFI", dirs_exist_ok=True)
            for stub in (target / "EFI").rglob("grub.cfg"):
                stub.write_text(f"search --no-floppy --set=root --file /{KEY_MARKER}\n"
                                "set prefix=($root)/boot/grub\nconfigfile $prefix/grub.cfg\n")
        root_spec, extra = boot_params() if installed(ROOT) and not running_live() else ("", "")
        (boot / "grub/grub.cfg").write_text(key_grub_cfg(root_spec, extra, (boot / "grub/themes/winlux").is_dir()))
        if have("grub-script-check"):
            run(["grub-script-check", str(boot / "grub/grub.cfg")], capture_output=True)
        (target / KEY_MARKER).write_text(f"{source.version or 'winlux'}\n")
        for path in sorted(target.rglob("*")):
            relative = str(path.relative_to(target))
            if path.is_file() and relative not in digests and relative != "winlux.sha256":
                digests[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
        (target / "winlux.sha256").write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(digests.items())))
        os.sync()
    finally:
        run(["umount", str(target)], check=False, timeout=300)
        if args.source in ("cloud", "auto"):
            shutil.rmtree(DOWNLOADS, ignore_errors=True)
    emit("DONE", _("Clé de secours prête : vous pouvez la retirer."))


# ── Point d'entrée ──────────────────────────────────────────────────────────

SOURCE = re.compile(r"^(cloud|auto|media:/[^\n\0]{1,400}|iso:/[^\n\0]{1,400})$")


def main(argv):
    parser = argparse.ArgumentParser(prog="winlux-reset", description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("status")
    found = sub.add_parser("sources")
    found.add_argument("--scan", action="store_true")
    prep = sub.add_parser("prepare")
    prep.add_argument("--mode", choices=("keep", "wipe"), required=True)
    prep.add_argument("--source", required=True)
    prep.add_argument("--clean", action="store_true")
    prep.add_argument("--user", default="")
    prep.add_argument("--fullname", default="")
    sub.add_parser("cancel")
    sub.add_parser("repair-boot")
    sub.add_parser("reboot-recovery")
    sub.add_parser("list-keys")
    key = sub.add_parser("make-key")
    key.add_argument("--device", required=True)
    key.add_argument("--source", required=True)
    args = parser.parse_args(argv)
    try:
        if getattr(args, "source", None) is not None and not SOURCE.match(args.source):
            raise ResetError(_("Aucun système Winlux dans {path}.", path=args.source))
        if args.command == "status":
            status()
        elif args.command == "sources":
            sources(args.scan)
        elif args.command == "prepare":
            password = sys.stdin.readline().rstrip("\n") if args.mode == "wipe" else ""
            prepare(args, password)
        elif args.command == "cancel":
            cancel()
        elif args.command == "repair-boot":
            repair_boot()
        elif args.command == "reboot-recovery":
            reboot_recovery()
        elif args.command == "list-keys":
            for path, size, model in list_keys():
                print(f"{path}|{size}|{model}")
        elif args.command == "make-key":
            if not re.fullmatch(r"/dev/[A-Za-z0-9/_-]{1,64}", args.device):
                raise ResetError(_("Clé USB introuvable ou refusée : {device}", device=args.device))
            make_key(args)
    except ResetError as error:
        emit("ERROR", str(error))
        return 1
    except subprocess.CalledProcessError as error:
        detail = (error.stderr if isinstance(error.stderr, str) else (error.stderr or b"").decode(errors="replace")) or ""
        emit("ERROR", _("Échec : {error}", error=f"{' '.join(map(str, error.cmd[:2]))} ({error.returncode}) {detail.strip()[-300:]}"))
        return 1
    except OSError as error:
        emit("ERROR", _("Échec : {error}", error=error))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
