#!/usr/bin/python3
"""Installateur graphique Winlux — style Windows 11.

Lance l'installation de Winlux sur un disque physique depuis le Live USB/DVD.
Interface GTK3 avec étapes : langue, licence, partitionnement guidé, chiffrement
LUKS, compte local, progression.
"""
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk, GLib, Pango
import cairo
import gzip
import json
import math
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT_DIR = os.path.dirname(SCRIPT_DIR)
HW_DETECT = os.path.join(SCRIPT_DIR, "winlux-hw-detect.sh")
INSTALL_SH = os.path.join(SCRIPT_DIR, "install.sh")

CSS = b"""
@define-color bg_primary #0078D4;
@define-color bg_dark #1a1a2e;
@define-color bg_surface #2d2d44;
@define-color bg_card #363654;
@define-color fg_primary #ffffff;
@define-color fg_secondary #b4b4cc;
@define-color accent #0078D4;
@define-color accent_hover #1a8aff;
@define-color danger #e74856;

window.installer-window {
    background-color: @bg_dark;
}
.installer-sidebar {
    background-color: @bg_surface;
    padding: 24px 16px;
}
.sidebar-step {
    padding: 8px 12px;
    border-radius: 6px;
    color: @fg_secondary;
    font-size: 13px;
}
.sidebar-step.active {
    background-color: @accent;
    color: @fg_primary;
    font-weight: bold;
}
.sidebar-step.done {
    color: @fg_primary;
}
.installer-content {
    background-color: @bg_dark;
    padding: 40px;
}
.page-title {
    color: @fg_primary;
    font-size: 24px;
    font-weight: bold;
    margin-bottom: 8px;
}
.page-subtitle {
    color: @fg_secondary;
    font-size: 14px;
    margin-bottom: 24px;
}
.card {
    background-color: @bg_card;
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 12px;
}
.card:hover {
    background-color: shade(@bg_card, 1.1);
}
.card.selected {
    border: 2px solid @accent;
}
.card-title {
    color: @fg_primary;
    font-size: 14px;
    font-weight: bold;
}
.card-detail {
    color: @fg_secondary;
    font-size: 12px;
}
.btn-primary {
    background-color: @accent;
    color: @fg_primary;
    border: none;
    border-radius: 6px;
    padding: 10px 32px;
    font-size: 14px;
    font-weight: bold;
    min-height: 36px;
}
.btn-primary:hover {
    background-color: @accent_hover;
}
.btn-primary:disabled {
    background-color: alpha(@accent, 0.4);
}
.btn-secondary {
    background-color: @bg_surface;
    color: @fg_primary;
    border: 1px solid @fg_secondary;
    border-radius: 6px;
    padding: 10px 24px;
    font-size: 14px;
    min-height: 36px;
}
.btn-danger {
    background-color: @danger;
    color: @fg_primary;
    border: none;
    border-radius: 6px;
    padding: 10px 32px;
    font-size: 14px;
    font-weight: bold;
}
.input-field {
    background-color: @bg_surface;
    color: @fg_primary;
    border: 1px solid @fg_secondary;
    border-radius: 4px;
    padding: 8px 12px;
    font-size: 14px;
    min-height: 32px;
}
.input-field:focus {
    border-color: @accent;
}
.input-label {
    color: @fg_primary;
    font-size: 13px;
    margin-bottom: 4px;
}
.progress-bar trough {
    background-color: @bg_surface;
    border-radius: 4px;
    min-height: 8px;
}
.progress-bar progress {
    background-color: @accent;
    border-radius: 4px;
    min-height: 8px;
}
.progress-text {
    color: @fg_secondary;
    font-size: 13px;
}
.warning-text {
    color: @danger;
    font-size: 12px;
}
.success-icon {
    color: #2ecc71;
    font-size: 64px;
}
.license-text {
    color: @fg_secondary;
    font-size: 12px;
    padding: 16px;
    background-color: @bg_surface;
    border-radius: 6px;
}
.disk-warn {
    color: @danger;
    font-size: 12px;
    font-weight: bold;
}
.hw-info {
    color: @fg_secondary;
    font-size: 12px;
    font-family: monospace;
}
.section-label {
    color: @fg_primary;
    font-size: 13px;
    font-weight: bold;
    margin-top: 12px;
    margin-bottom: 4px;
}
.info-text {
    color: @fg_secondary;
    font-size: 12px;
    margin-top: 4px;
}
.strength-weak {
    color: @danger;
    font-size: 12px;
    font-weight: bold;
}
.strength-medium {
    color: #f0ad4e;
    font-size: 12px;
    font-weight: bold;
}
.strength-strong {
    color: #2ecc71;
    font-size: 12px;
    font-weight: bold;
}
.luks-info-box {
    background-color: @bg_surface;
    border-radius: 8px;
    padding: 16px;
    margin-top: 8px;
}
/* Keep the dark look whatever GTK theme the live session ships. */
.installer-content label, .installer-content radiobutton label, .installer-content checkbutton label {
    color: @fg_primary;
}
.installer-content .card-detail, .installer-content .page-subtitle, .installer-content .progress-text,
.installer-content .hw-info {
    color: @fg_secondary;
}
.installer-content .warning-text, .installer-content .disk-warn {
    color: @danger;
}
radiobutton:disabled label, checkbutton:disabled label {
    color: alpha(@fg_secondary, 0.5);
}
button.btn-primary, button.btn-secondary, button.btn-danger {
    background-image: none;
    box-shadow: none;
    text-shadow: none;
}
button.btn-primary label, button.btn-secondary label, button.btn-danger label {
    color: @fg_primary;
}
scale value {
    color: @fg_primary;
}
entry.input-field, combobox button {
    background-image: none;
    background-color: @bg_surface;
    color: @fg_primary;
}

"""

LANGUAGES = [
    ("fr_FR", "Français", "AZERTY (fr)"),
    ("en_US", "English", "QWERTY (us)"),
    ("de_DE", "Deutsch", "QWERTZ (de)"),
    ("es_ES", "Español", "QWERTY (es)"),
    ("it_IT", "Italiano", "QWERTY (it)"),
    ("pt_BR", "Português (Brasil)", "QWERTY (br)"),
]

KEYBOARDS = [
    ("fr", "AZERTY — Français"),
    ("us", "QWERTY — Anglais (US)"),
    ("gb", "QWERTY — Anglais (UK)"),
    ("de", "QWERTZ — Allemand"),
    ("es", "QWERTY — Espagnol"),
    ("it", "QWERTY — Italien"),
    ("br", "QWERTY — Portugais (Brésil)"),
    ("ch", "QWERTZ — Suisse"),
    ("be", "AZERTY — Belge"),
    ("ca", "QWERTY — Canadien multilingue"),
]

PART_COLORS = {
    "ext4": (0.0, 0.47, 0.83, 1.0),
    "ext3": (0.0, 0.47, 0.83, 0.8),
    "ext2": (0.0, 0.47, 0.83, 0.6),
    "btrfs": (0.0, 0.47, 0.83, 0.9),
    "xfs": (0.15, 0.55, 0.85, 1.0),
    "ntfs": (0.9, 0.55, 0.1, 1.0),
    "vfat": (0.3, 0.75, 0.35, 1.0),
    "fat32": (0.3, 0.75, 0.35, 1.0),
    "fat16": (0.3, 0.75, 0.35, 1.0),
    "swap": (0.55, 0.35, 0.75, 1.0),
    "linux-swap": (0.55, 0.35, 0.75, 1.0),
    "crypto_luks": (0.75, 0.2, 0.2, 1.0),
    "free": (0.35, 0.35, 0.45, 1.0),
    "winlux": (0.0, 0.62, 0.95, 1.0),
    "bios": (0.45, 0.45, 0.55, 1.0),
}

TRANSLATIONS = {
    "fr_FR": {
        "steps": ["Langue", "Clavier", "Licence", "Partitionnement", "Chiffrement",
                  "Compte local", "Résumé", "Installation", "Terminé"],
        "choose_language": "Choisissez votre langue",
        "language_subtitle": "Cette langue sera utilisée pendant l'installation et comme langue par défaut du système.",
        "choose_keyboard": "Choisissez la disposition du clavier",
        "keyboard_subtitle": "Sélectionnez le type de clavier que vous utilisez.",
        "keyboard_test": "Testez votre clavier ici :",
        "license_title": "Contrat de licence",
        "license_subtitle": "Veuillez lire et accepter les termes de la licence.",
        "license_accept": "J'accepte les termes de la licence GPL v3",
        "part_title": "Partitionnement",
        "part_subtitle": "Choisissez le disque et le mode de partitionnement.",
        "part_select_disk": "Sélectionnez un disque :",
        "part_type_full": "Utiliser le disque entier (recommandé)",
        "part_type_full_desc": "Efface le disque et installe Winlux comme seul système.\nPartitionnement automatique : BIOS boot + ESP (UEFI) + racine.",
        "part_type_dualboot": "Installation en dual-boot",
        "part_type_dualboot_desc": "Conserver le système existant et installer Winlux à côté.\nRedimensionne la partition existante pour libérer de l'espace.",
        "part_type_manual": "Partitionnement manuel (avancé)",
        "part_type_manual_desc": "Choisissez manuellement les partitions.\n(Pour utilisateurs expérimentés uniquement.)",
        "part_disk_layout": "Disposition actuelle du disque :",
        "part_no_disk": "Aucun disque disponible détecté (taille minimale : 2 Go).",
        "part_free_space": "Espace libre",
        "part_dualboot_slider": "Espace alloué à Winlux :",
        "part_dualboot_no_os": "Aucun système existant détecté sur ce disque.",
        "part_windows_found": "Windows détecté sur {partition}",
        "part_erase_warn": "ATTENTION : le disque sélectionné sera entièrement effacé.",
        "hw_detected": "Matériel détecté",
        "hw_detecting": "Détection en cours...",
        "encrypt_title": "Chiffrement du disque",
        "encrypt_subtitle": "Protégez vos données avec le chiffrement LUKS.",
        "encrypt_enable": "Chiffrer la partition racine avec LUKS",
        "encrypt_info": "Le chiffrement LUKS protège vos données en cas de vol ou de perte de l'ordinateur. "
                        "Vous devrez saisir une phrase de passe à chaque démarrage pour déverrouiller le disque. "
                        "Sans cette phrase de passe, vos données seront inaccessibles.",
        "encrypt_passphrase": "Phrase de passe :",
        "encrypt_confirm": "Confirmer la phrase de passe :",
        "encrypt_mismatch": "Les phrases de passe ne correspondent pas.",
        "encrypt_strength_weak": "Faible — utilisez au moins 12 caractères",
        "encrypt_strength_medium": "Moyen — ajoutez des chiffres ou des symboles",
        "encrypt_strength_strong": "Fort",
        "encrypt_skip_info": "Vous pouvez passer cette étape si vous ne souhaitez pas chiffrer le disque.",
        "account_title": "Compte local",
        "account_subtitle": "Créez un compte local. Winlux n'utilise aucun compte en ligne.",
        "field_hostname": "Nom de l'ordinateur :",
        "field_fullname": "Nom complet :",
        "field_username": "Nom d'utilisateur :",
        "field_password": "Mot de passe :",
        "field_password_confirm": "Confirmer le mot de passe :",
        "autologin": "Connexion automatique (sans mot de passe au démarrage)",
        "password_mismatch": "Les mots de passe ne correspondent pas.",
        "username_invalid": "Nom d'utilisateur : minuscules, chiffres, - ou _ uniquement.",
        "summary_title": "Résumé de l'installation",
        "summary_subtitle": "Vérifiez les paramètres avant de lancer l'installation.",
        "summary_erase_warn": "⚠ TOUTES LES DONNÉES DU DISQUE SÉLECTIONNÉ SERONT DÉFINITIVEMENT EFFACÉES.",
        "summary_dualboot_warn": "⚠ La partition existante sera redimensionnée. Une sauvegarde est recommandée.",
        "installing": "Installation en cours...",
        "preparing": "Préparation...",
        "done_title": "Installation terminée !",
        "done_subtitle": "Winlux a été installé avec succès. Retirez le média d'installation et redémarrez.",
        "btn_next": "Suivant",
        "btn_back": "Retour",
        "btn_install": "Installer",
        "btn_reboot": "Redémarrer maintenant",
        "btn_close": "Fermer l'installateur",
        "error_title": "Erreur d'installation",
        "summary_lang": "Langue",
        "summary_keyboard": "Clavier",
        "summary_type": "Type",
        "summary_type_full": "Disque entier",
        "summary_type_dualboot": "Dual-boot",
        "summary_type_manual": "Manuel",
        "summary_disk": "Disque",
        "summary_hostname": "Nom de l'ordinateur",
        "summary_user": "Utilisateur",
        "summary_autologin": "Connexion automatique",
        "summary_encryption": "Chiffrement LUKS",
        "summary_yes": "Oui",
        "summary_no": "Non",
        "summary_boot": "Démarrage",
        "progress_check_disk": "Vérification du disque {device}...",
        "progress_partitioning": "Partitionnement du disque...",
        "progress_formatting": "Formatage des partitions...",
        "progress_luks_setup": "Configuration du chiffrement LUKS...",
        "progress_extracting": "Extraction de l'ISO...",
        "progress_copying": "Copie du système (peut prendre plusieurs minutes)...",
        "progress_kernel": "Installation du noyau et initramfs...",
        "progress_grub": "Configuration du bootloader GRUB...",
        "progress_account": "Configuration du compte utilisateur...",
        "progress_firstboot": "Configuration du premier démarrage...",
        "progress_sync": "Synchronisation et démontage...",
        "progress_done": "Installation terminée avec succès !",
    },
    "en_US": {
        "steps": ["Language", "Keyboard", "License", "Partitioning", "Encryption",
                  "Local account", "Summary", "Installing", "Done"],
        "choose_language": "Choose your language",
        "language_subtitle": "This language will be used during installation and as the default system language.",
        "choose_keyboard": "Choose your keyboard layout",
        "keyboard_subtitle": "Select the keyboard layout you use.",
        "keyboard_test": "Test your keyboard here:",
        "license_title": "License agreement",
        "license_subtitle": "Please read and accept the license terms.",
        "license_accept": "I accept the terms of the GPL v3 license",
        "part_title": "Partitioning",
        "part_subtitle": "Choose the disk and partitioning mode.",
        "part_select_disk": "Select a disk:",
        "part_type_full": "Use the entire disk (recommended)",
        "part_type_full_desc": "Erases the disk and installs Winlux as the only system.\nAutomatic partitioning: BIOS boot + ESP (UEFI) + root.",
        "part_type_dualboot": "Dual-boot installation",
        "part_type_dualboot_desc": "Keep the existing system and install Winlux alongside.\nResizes the existing partition to free up space.",
        "part_type_manual": "Manual partitioning (advanced)",
        "part_type_manual_desc": "Manually choose partitions.\n(For experienced users only.)",
        "part_disk_layout": "Current disk layout:",
        "part_no_disk": "No available disk detected (minimum size: 2 GB).",
        "part_free_space": "Free space",
        "part_dualboot_slider": "Space allocated to Winlux:",
        "part_dualboot_no_os": "No existing system detected on this disk.",
        "part_windows_found": "Windows detected on {partition}",
        "part_erase_warn": "WARNING: the selected disk will be completely erased.",
        "hw_detected": "Detected hardware",
        "hw_detecting": "Detecting...",
        "encrypt_title": "Disk encryption",
        "encrypt_subtitle": "Protect your data with LUKS encryption.",
        "encrypt_enable": "Encrypt the root partition with LUKS",
        "encrypt_info": "LUKS encryption protects your data in case of theft or loss. "
                        "You will need to enter a passphrase at each boot to unlock the disk. "
                        "Without this passphrase, your data will be inaccessible.",
        "encrypt_passphrase": "Passphrase:",
        "encrypt_confirm": "Confirm passphrase:",
        "encrypt_mismatch": "Passphrases do not match.",
        "encrypt_strength_weak": "Weak — use at least 12 characters",
        "encrypt_strength_medium": "Medium — add digits or symbols",
        "encrypt_strength_strong": "Strong",
        "encrypt_skip_info": "You can skip this step if you do not want to encrypt the disk.",
        "account_title": "Local account",
        "account_subtitle": "Create a local account. Winlux does not use any online account.",
        "field_hostname": "Computer name:",
        "field_fullname": "Full name:",
        "field_username": "Username:",
        "field_password": "Password:",
        "field_password_confirm": "Confirm password:",
        "autologin": "Automatic login (no password at startup)",
        "password_mismatch": "Passwords do not match.",
        "username_invalid": "Username: lowercase letters, digits, - or _ only.",
        "summary_title": "Installation summary",
        "summary_subtitle": "Review settings before starting the installation.",
        "summary_erase_warn": "⚠ ALL DATA ON THE SELECTED DISK WILL BE PERMANENTLY ERASED.",
        "summary_dualboot_warn": "⚠ The existing partition will be resized. A backup is recommended.",
        "installing": "Installing...",
        "preparing": "Preparing...",
        "done_title": "Installation complete!",
        "done_subtitle": "Winlux has been installed successfully. Remove the installation media and reboot.",
        "btn_next": "Next",
        "btn_back": "Back",
        "btn_install": "Install",
        "btn_reboot": "Reboot now",
        "btn_close": "Close installer",
        "error_title": "Installation error",
        "summary_lang": "Language",
        "summary_keyboard": "Keyboard",
        "summary_type": "Type",
        "summary_type_full": "Entire disk",
        "summary_type_dualboot": "Dual-boot",
        "summary_type_manual": "Manual",
        "summary_disk": "Disk",
        "summary_hostname": "Computer name",
        "summary_user": "User",
        "summary_autologin": "Automatic login",
        "summary_encryption": "LUKS encryption",
        "summary_yes": "Yes",
        "summary_no": "No",
        "summary_boot": "Boot mode",
        "progress_check_disk": "Checking disk {device}...",
        "progress_partitioning": "Partitioning disk...",
        "progress_formatting": "Formatting partitions...",
        "progress_luks_setup": "Configuring LUKS encryption...",
        "progress_extracting": "Extracting ISO...",
        "progress_copying": "Copying system (may take several minutes)...",
        "progress_kernel": "Installing kernel and initramfs...",
        "progress_grub": "Configuring GRUB bootloader...",
        "progress_account": "Configuring user account...",
        "progress_firstboot": "Configuring first boot...",
        "progress_sync": "Syncing and unmounting...",
        "progress_done": "Installation completed successfully!",
    },
    "de_DE": {
        "steps": ["Sprache", "Tastatur", "Lizenz", "Partitionierung", "Verschlüsselung",
                  "Lokales Konto", "Zusammenfassung", "Installation", "Fertig"],
        "choose_language": "Wählen Sie Ihre Sprache",
        "language_subtitle": "Diese Sprache wird während der Installation und als Standardsystemsprache verwendet.",
        "choose_keyboard": "Wählen Sie Ihr Tastaturlayout",
        "keyboard_subtitle": "Wählen Sie das Tastaturlayout, das Sie verwenden.",
        "keyboard_test": "Testen Sie Ihre Tastatur hier:",
        "license_title": "Lizenzvereinbarung",
        "license_subtitle": "Bitte lesen und akzeptieren Sie die Lizenzbedingungen.",
        "license_accept": "Ich akzeptiere die Bedingungen der GPL v3-Lizenz",
        "part_title": "Partitionierung",
        "part_subtitle": "Wählen Sie die Festplatte und den Partitionierungsmodus.",
        "part_select_disk": "Wählen Sie eine Festplatte:",
        "part_type_full": "Gesamte Festplatte verwenden (empfohlen)",
        "part_type_full_desc": "Löscht die Festplatte und installiert Winlux als einziges System.\nAutomatische Partitionierung: BIOS-Boot + ESP (UEFI) + Root.",
        "part_type_dualboot": "Dual-Boot-Installation",
        "part_type_dualboot_desc": "Bestehendes System beibehalten und Winlux daneben installieren.\nVerkleinert die bestehende Partition, um Platz zu schaffen.",
        "part_type_manual": "Manuelle Partitionierung (fortgeschritten)",
        "part_type_manual_desc": "Partitionen manuell wählen.\n(Nur für erfahrene Benutzer.)",
        "part_disk_layout": "Aktuelle Festplattenbelegung:",
        "part_no_disk": "Keine verfügbare Festplatte erkannt (Mindestgröße: 2 GB).",
        "part_free_space": "Freier Speicher",
        "part_dualboot_slider": "Für Winlux zugewiesener Speicher:",
        "part_dualboot_no_os": "Kein bestehendes System auf dieser Festplatte erkannt.",
        "part_windows_found": "Windows erkannt auf {partition}",
        "part_erase_warn": "ACHTUNG: Die ausgewählte Festplatte wird vollständig gelöscht.",
        "hw_detected": "Erkannte Hardware",
        "hw_detecting": "Erkennung läuft...",
        "encrypt_title": "Festplattenverschlüsselung",
        "encrypt_subtitle": "Schützen Sie Ihre Daten mit LUKS-Verschlüsselung.",
        "encrypt_enable": "Root-Partition mit LUKS verschlüsseln",
        "encrypt_info": "LUKS-Verschlüsselung schützt Ihre Daten bei Diebstahl oder Verlust. "
                        "Sie müssen bei jedem Start eine Passphrase eingeben, um die Festplatte zu entsperren. "
                        "Ohne diese Passphrase sind Ihre Daten nicht zugänglich.",
        "encrypt_passphrase": "Passphrase:",
        "encrypt_confirm": "Passphrase bestätigen:",
        "encrypt_mismatch": "Passphrasen stimmen nicht überein.",
        "encrypt_strength_weak": "Schwach — verwenden Sie mindestens 12 Zeichen",
        "encrypt_strength_medium": "Mittel — fügen Sie Ziffern oder Symbole hinzu",
        "encrypt_strength_strong": "Stark",
        "encrypt_skip_info": "Sie können diesen Schritt überspringen, wenn Sie die Festplatte nicht verschlüsseln möchten.",
        "account_title": "Lokales Konto",
        "account_subtitle": "Erstellen Sie ein lokales Konto. Winlux verwendet kein Online-Konto.",
        "field_hostname": "Computername:",
        "field_fullname": "Vollständiger Name:",
        "field_username": "Benutzername:",
        "field_password": "Passwort:",
        "field_password_confirm": "Passwort bestätigen:",
        "autologin": "Automatische Anmeldung (kein Passwort beim Start)",
        "password_mismatch": "Passwörter stimmen nicht überein.",
        "username_invalid": "Benutzername: nur Kleinbuchstaben, Ziffern, - oder _.",
        "summary_title": "Installationszusammenfassung",
        "summary_subtitle": "Überprüfen Sie die Einstellungen vor der Installation.",
        "summary_erase_warn": "⚠ ALLE DATEN AUF DER AUSGEWÄHLTEN FESTPLATTE WERDEN UNWIDERRUFLICH GELÖSCHT.",
        "summary_dualboot_warn": "⚠ Die bestehende Partition wird verkleinert. Eine Sicherung wird empfohlen.",
        "installing": "Installation läuft...",
        "preparing": "Vorbereitung...",
        "done_title": "Installation abgeschlossen!",
        "done_subtitle": "Winlux wurde erfolgreich installiert. Entfernen Sie das Installationsmedium und starten Sie neu.",
        "btn_next": "Weiter",
        "btn_back": "Zurück",
        "btn_install": "Installieren",
        "btn_reboot": "Jetzt neu starten",
        "btn_close": "Installer schließen",
        "error_title": "Installationsfehler",
        "summary_lang": "Sprache",
        "summary_keyboard": "Tastatur",
        "summary_type": "Typ",
        "summary_type_full": "Gesamte Festplatte",
        "summary_type_dualboot": "Dual-Boot",
        "summary_type_manual": "Manuell",
        "summary_disk": "Festplatte",
        "summary_hostname": "Computername",
        "summary_user": "Benutzer",
        "summary_autologin": "Automatische Anmeldung",
        "summary_encryption": "LUKS-Verschlüsselung",
        "summary_yes": "Ja",
        "summary_no": "Nein",
        "summary_boot": "Startmodus",
        "progress_check_disk": "Festplatte {device} wird überprüft...",
        "progress_partitioning": "Festplatte wird partitioniert...",
        "progress_formatting": "Partitionen werden formatiert...",
        "progress_luks_setup": "LUKS-Verschlüsselung wird konfiguriert...",
        "progress_extracting": "ISO wird extrahiert...",
        "progress_copying": "System wird kopiert (kann einige Minuten dauern)...",
        "progress_kernel": "Kernel und Initramfs werden installiert...",
        "progress_grub": "GRUB-Bootloader wird konfiguriert...",
        "progress_account": "Benutzerkonto wird konfiguriert...",
        "progress_firstboot": "Erster Start wird konfiguriert...",
        "progress_sync": "Synchronisierung und Aushängen...",
        "progress_done": "Installation erfolgreich abgeschlossen!",
    },
    "es_ES": {
        "steps": ["Idioma", "Teclado", "Licencia", "Particionamiento", "Cifrado",
                  "Cuenta local", "Resumen", "Instalación", "Terminado"],
        "choose_language": "Elija su idioma",
        "language_subtitle": "Este idioma se usará durante la instalación y como idioma predeterminado del sistema.",
        "choose_keyboard": "Elija la disposición del teclado",
        "keyboard_subtitle": "Seleccione el tipo de teclado que usa.",
        "keyboard_test": "Pruebe su teclado aquí:",
        "license_title": "Acuerdo de licencia",
        "license_subtitle": "Lea y acepte los términos de la licencia.",
        "license_accept": "Acepto los términos de la licencia GPL v3",
        "part_title": "Particionamiento",
        "part_subtitle": "Elija el disco y el modo de particionamiento.",
        "part_select_disk": "Seleccione un disco:",
        "part_type_full": "Usar todo el disco (recomendado)",
        "part_type_full_desc": "Borra el disco e instala Winlux como único sistema.\nParticionamiento automático: arranque BIOS + ESP (UEFI) + raíz.",
        "part_type_dualboot": "Instalación en dual-boot",
        "part_type_dualboot_desc": "Conservar el sistema existente e instalar Winlux junto a él.\nRedimensiona la partición existente para liberar espacio.",
        "part_type_manual": "Particionamiento manual (avanzado)",
        "part_type_manual_desc": "Elija particiones manualmente.\n(Solo para usuarios experimentados.)",
        "part_disk_layout": "Disposición actual del disco:",
        "part_no_disk": "No se detectó ningún disco disponible (tamaño mínimo: 2 GB).",
        "part_free_space": "Espacio libre",
        "part_dualboot_slider": "Espacio asignado a Winlux:",
        "part_dualboot_no_os": "No se detectó ningún sistema existente en este disco.",
        "part_windows_found": "Windows detectado en {partition}",
        "part_erase_warn": "ATENCIÓN: el disco seleccionado será borrado por completo.",
        "hw_detected": "Hardware detectado",
        "hw_detecting": "Detectando...",
        "encrypt_title": "Cifrado del disco",
        "encrypt_subtitle": "Proteja sus datos con cifrado LUKS.",
        "encrypt_enable": "Cifrar la partición raíz con LUKS",
        "encrypt_info": "El cifrado LUKS protege sus datos en caso de robo o pérdida. "
                        "Deberá ingresar una frase de contraseña en cada arranque para desbloquear el disco. "
                        "Sin esta frase, sus datos serán inaccesibles.",
        "encrypt_passphrase": "Frase de contraseña:",
        "encrypt_confirm": "Confirmar frase de contraseña:",
        "encrypt_mismatch": "Las frases de contraseña no coinciden.",
        "encrypt_strength_weak": "Débil — use al menos 12 caracteres",
        "encrypt_strength_medium": "Medio — agregue dígitos o símbolos",
        "encrypt_strength_strong": "Fuerte",
        "encrypt_skip_info": "Puede saltar este paso si no desea cifrar el disco.",
        "account_title": "Cuenta local",
        "account_subtitle": "Cree una cuenta local. Winlux no usa ninguna cuenta en línea.",
        "field_hostname": "Nombre del equipo:",
        "field_fullname": "Nombre completo:",
        "field_username": "Nombre de usuario:",
        "field_password": "Contraseña:",
        "field_password_confirm": "Confirmar contraseña:",
        "autologin": "Inicio de sesión automático (sin contraseña al arrancar)",
        "password_mismatch": "Las contraseñas no coinciden.",
        "username_invalid": "Nombre de usuario: solo minúsculas, dígitos, - o _.",
        "summary_title": "Resumen de la instalación",
        "summary_subtitle": "Revise la configuración antes de iniciar la instalación.",
        "summary_erase_warn": "⚠ TODOS LOS DATOS DEL DISCO SELECCIONADO SERÁN BORRADOS PERMANENTEMENTE.",
        "summary_dualboot_warn": "⚠ La partición existente será redimensionada. Se recomienda una copia de seguridad.",
        "installing": "Instalando...",
        "preparing": "Preparando...",
        "done_title": "¡Instalación completada!",
        "done_subtitle": "Winlux se ha instalado correctamente. Retire el medio de instalación y reinicie.",
        "btn_next": "Siguiente",
        "btn_back": "Atrás",
        "btn_install": "Instalar",
        "btn_reboot": "Reiniciar ahora",
        "btn_close": "Cerrar instalador",
        "error_title": "Error de instalación",
        "summary_lang": "Idioma",
        "summary_keyboard": "Teclado",
        "summary_type": "Tipo",
        "summary_type_full": "Disco completo",
        "summary_type_dualboot": "Dual-boot",
        "summary_type_manual": "Manual",
        "summary_disk": "Disco",
        "summary_hostname": "Nombre del equipo",
        "summary_user": "Usuario",
        "summary_autologin": "Inicio automático",
        "summary_encryption": "Cifrado LUKS",
        "summary_yes": "Sí",
        "summary_no": "No",
        "summary_boot": "Modo de arranque",
        "progress_check_disk": "Verificando disco {device}...",
        "progress_partitioning": "Particionando disco...",
        "progress_formatting": "Formateando particiones...",
        "progress_luks_setup": "Configurando cifrado LUKS...",
        "progress_extracting": "Extrayendo ISO...",
        "progress_copying": "Copiando sistema (puede tardar varios minutos)...",
        "progress_kernel": "Instalando kernel e initramfs...",
        "progress_grub": "Configurando cargador de arranque GRUB...",
        "progress_account": "Configurando cuenta de usuario...",
        "progress_firstboot": "Configurando primer arranque...",
        "progress_sync": "Sincronizando y desmontando...",
        "progress_done": "¡Instalación completada con éxito!",
    },
    "it_IT": {
        "steps": ["Lingua", "Tastiera", "Licenza", "Partizionamento", "Crittografia",
                  "Account locale", "Riepilogo", "Installazione", "Completato"],
        "choose_language": "Scegli la tua lingua",
        "language_subtitle": "Questa lingua verrà utilizzata durante l'installazione e come lingua predefinita del sistema.",
        "choose_keyboard": "Scegli il layout della tastiera",
        "keyboard_subtitle": "Seleziona il layout della tastiera che utilizzi.",
        "keyboard_test": "Prova la tua tastiera qui:",
        "license_title": "Contratto di licenza",
        "license_subtitle": "Leggere e accettare i termini della licenza.",
        "license_accept": "Accetto i termini della licenza GPL v3",
        "part_title": "Partizionamento",
        "part_subtitle": "Scegli il disco e la modalità di partizionamento.",
        "part_select_disk": "Seleziona un disco:",
        "part_type_full": "Usa l'intero disco (consigliato)",
        "part_type_full_desc": "Cancella il disco e installa Winlux come unico sistema.\nPartizionamento automatico: BIOS boot + ESP (UEFI) + root.",
        "part_type_dualboot": "Installazione dual-boot",
        "part_type_dualboot_desc": "Mantieni il sistema esistente e installa Winlux accanto ad esso.\nRidimensiona la partizione esistente per liberare spazio.",
        "part_type_manual": "Partizionamento manuale (avanzato)",
        "part_type_manual_desc": "Scegli manualmente le partizioni.\n(Solo per utenti esperti.)",
        "part_disk_layout": "Layout attuale del disco:",
        "part_no_disk": "Nessun disco disponibile rilevato (dimensione minima: 2 GB).",
        "part_free_space": "Spazio libero",
        "part_dualboot_slider": "Spazio assegnato a Winlux:",
        "part_dualboot_no_os": "Nessun sistema esistente rilevato su questo disco.",
        "part_windows_found": "Windows rilevato su {partition}",
        "part_erase_warn": "ATTENZIONE: il disco selezionato verrà completamente cancellato.",
        "hw_detected": "Hardware rilevato",
        "hw_detecting": "Rilevamento in corso...",
        "encrypt_title": "Crittografia del disco",
        "encrypt_subtitle": "Proteggi i tuoi dati con la crittografia LUKS.",
        "encrypt_enable": "Crittografa la partizione root con LUKS",
        "encrypt_info": "La crittografia LUKS protegge i tuoi dati in caso di furto o smarrimento. "
                        "Dovrai inserire una passphrase ad ogni avvio per sbloccare il disco. "
                        "Senza questa passphrase, i tuoi dati saranno inaccessibili.",
        "encrypt_passphrase": "Passphrase:",
        "encrypt_confirm": "Conferma passphrase:",
        "encrypt_mismatch": "Le passphrase non corrispondono.",
        "encrypt_strength_weak": "Debole — usa almeno 12 caratteri",
        "encrypt_strength_medium": "Medio — aggiungi cifre o simboli",
        "encrypt_strength_strong": "Forte",
        "encrypt_skip_info": "Puoi saltare questo passaggio se non vuoi crittografare il disco.",
        "account_title": "Account locale",
        "account_subtitle": "Crea un account locale. Winlux non utilizza alcun account online.",
        "field_hostname": "Nome del computer:",
        "field_fullname": "Nome completo:",
        "field_username": "Nome utente:",
        "field_password": "Password:",
        "field_password_confirm": "Conferma password:",
        "autologin": "Accesso automatico (senza password all'avvio)",
        "password_mismatch": "Le password non corrispondono.",
        "username_invalid": "Nome utente: solo lettere minuscole, cifre, - o _.",
        "summary_title": "Riepilogo dell'installazione",
        "summary_subtitle": "Verifica le impostazioni prima di avviare l'installazione.",
        "summary_erase_warn": "⚠ TUTTI I DATI SUL DISCO SELEZIONATO VERRANNO CANCELLATI PERMANENTEMENTE.",
        "summary_dualboot_warn": "⚠ La partizione esistente verrà ridimensionata. Si consiglia un backup.",
        "installing": "Installazione in corso...",
        "preparing": "Preparazione...",
        "done_title": "Installazione completata!",
        "done_subtitle": "Winlux è stato installato con successo. Rimuovi il supporto di installazione e riavvia.",
        "btn_next": "Avanti",
        "btn_back": "Indietro",
        "btn_install": "Installa",
        "btn_reboot": "Riavvia ora",
        "btn_close": "Chiudi installer",
        "error_title": "Errore di installazione",
        "summary_lang": "Lingua",
        "summary_keyboard": "Tastiera",
        "summary_type": "Tipo",
        "summary_type_full": "Intero disco",
        "summary_type_dualboot": "Dual-boot",
        "summary_type_manual": "Manuale",
        "summary_disk": "Disco",
        "summary_hostname": "Nome del computer",
        "summary_user": "Utente",
        "summary_autologin": "Accesso automatico",
        "summary_encryption": "Crittografia LUKS",
        "summary_yes": "Sì",
        "summary_no": "No",
        "summary_boot": "Modalità di avvio",
        "progress_check_disk": "Verifica del disco {device}...",
        "progress_partitioning": "Partizionamento del disco...",
        "progress_formatting": "Formattazione delle partizioni...",
        "progress_luks_setup": "Configurazione della crittografia LUKS...",
        "progress_extracting": "Estrazione dell'ISO...",
        "progress_copying": "Copia del sistema (potrebbe richiedere diversi minuti)...",
        "progress_kernel": "Installazione del kernel e initramfs...",
        "progress_grub": "Configurazione del bootloader GRUB...",
        "progress_account": "Configurazione dell'account utente...",
        "progress_firstboot": "Configurazione del primo avvio...",
        "progress_sync": "Sincronizzazione e smontaggio...",
        "progress_done": "Installazione completata con successo!",
    },
    "pt_BR": {
        "steps": ["Idioma", "Teclado", "Licença", "Particionamento", "Criptografia",
                  "Conta local", "Resumo", "Instalação", "Concluído"],
        "choose_language": "Escolha seu idioma",
        "language_subtitle": "Este idioma será usado durante a instalação e como idioma padrão do sistema.",
        "choose_keyboard": "Escolha o layout do teclado",
        "keyboard_subtitle": "Selecione o layout de teclado que você usa.",
        "keyboard_test": "Teste seu teclado aqui:",
        "license_title": "Acordo de licença",
        "license_subtitle": "Leia e aceite os termos da licença.",
        "license_accept": "Aceito os termos da licença GPL v3",
        "part_title": "Particionamento",
        "part_subtitle": "Escolha o disco e o modo de particionamento.",
        "part_select_disk": "Selecione um disco:",
        "part_type_full": "Usar o disco inteiro (recomendado)",
        "part_type_full_desc": "Apaga o disco e instala o Winlux como único sistema.\nParticionamento automático: boot BIOS + ESP (UEFI) + raiz.",
        "part_type_dualboot": "Instalação em dual-boot",
        "part_type_dualboot_desc": "Manter o sistema existente e instalar o Winlux ao lado.\nRedimensiona a partição existente para liberar espaço.",
        "part_type_manual": "Particionamento manual (avançado)",
        "part_type_manual_desc": "Escolha partições manualmente.\n(Apenas para usuários experientes.)",
        "part_disk_layout": "Layout atual do disco:",
        "part_no_disk": "Nenhum disco disponível detectado (tamanho mínimo: 2 GB).",
        "part_free_space": "Espaço livre",
        "part_dualboot_slider": "Espaço alocado para o Winlux:",
        "part_dualboot_no_os": "Nenhum sistema existente detectado neste disco.",
        "part_windows_found": "Windows detectado em {partition}",
        "part_erase_warn": "ATENÇÃO: o disco selecionado será completamente apagado.",
        "hw_detected": "Hardware detectado",
        "hw_detecting": "Detectando...",
        "encrypt_title": "Criptografia do disco",
        "encrypt_subtitle": "Proteja seus dados com criptografia LUKS.",
        "encrypt_enable": "Criptografar a partição raiz com LUKS",
        "encrypt_info": "A criptografia LUKS protege seus dados em caso de roubo ou perda. "
                        "Você precisará digitar uma frase secreta a cada inicialização para desbloquear o disco. "
                        "Sem esta frase, seus dados ficarão inacessíveis.",
        "encrypt_passphrase": "Frase secreta:",
        "encrypt_confirm": "Confirmar frase secreta:",
        "encrypt_mismatch": "As frases secretas não coincidem.",
        "encrypt_strength_weak": "Fraca — use pelo menos 12 caracteres",
        "encrypt_strength_medium": "Média — adicione dígitos ou símbolos",
        "encrypt_strength_strong": "Forte",
        "encrypt_skip_info": "Você pode pular esta etapa se não quiser criptografar o disco.",
        "account_title": "Conta local",
        "account_subtitle": "Crie uma conta local. O Winlux não usa nenhuma conta online.",
        "field_hostname": "Nome do computador:",
        "field_fullname": "Nome completo:",
        "field_username": "Nome de usuário:",
        "field_password": "Senha:",
        "field_password_confirm": "Confirmar senha:",
        "autologin": "Login automático (sem senha na inicialização)",
        "password_mismatch": "As senhas não coincidem.",
        "username_invalid": "Nome de usuário: apenas letras minúsculas, dígitos, - ou _.",
        "summary_title": "Resumo da instalação",
        "summary_subtitle": "Verifique as configurações antes de iniciar a instalação.",
        "summary_erase_warn": "⚠ TODOS OS DADOS NO DISCO SELECIONADO SERÃO APAGADOS PERMANENTEMENTE.",
        "summary_dualboot_warn": "⚠ A partição existente será redimensionada. Um backup é recomendado.",
        "installing": "Instalando...",
        "preparing": "Preparando...",
        "done_title": "Instalação concluída!",
        "done_subtitle": "O Winlux foi instalado com sucesso. Remova a mídia de instalação e reinicie.",
        "btn_next": "Próximo",
        "btn_back": "Voltar",
        "btn_install": "Instalar",
        "btn_reboot": "Reiniciar agora",
        "btn_close": "Fechar instalador",
        "error_title": "Erro de instalação",
        "summary_lang": "Idioma",
        "summary_keyboard": "Teclado",
        "summary_type": "Tipo",
        "summary_type_full": "Disco inteiro",
        "summary_type_dualboot": "Dual-boot",
        "summary_type_manual": "Manual",
        "summary_disk": "Disco",
        "summary_hostname": "Nome do computador",
        "summary_user": "Usuário",
        "summary_autologin": "Login automático",
        "summary_encryption": "Criptografia LUKS",
        "summary_yes": "Sim",
        "summary_no": "Não",
        "summary_boot": "Modo de inicialização",
        "progress_check_disk": "Verificando disco {device}...",
        "progress_partitioning": "Particionando disco...",
        "progress_formatting": "Formatando partições...",
        "progress_luks_setup": "Configurando criptografia LUKS...",
        "progress_extracting": "Extraindo ISO...",
        "progress_copying": "Copiando sistema (pode levar vários minutos)...",
        "progress_kernel": "Instalando kernel e initramfs...",
        "progress_grub": "Configurando carregador de inicialização GRUB...",
        "progress_account": "Configurando conta de usuário...",
        "progress_firstboot": "Configurando primeira inicialização...",
        "progress_sync": "Sincronizando e desmontando...",
        "progress_done": "Instalação concluída com sucesso!",
    },
}

EXTRA_TRANSLATIONS = {
    "fr_FR": {
        "encrypt_ascii": "Utilisez uniquement des lettres sans accent, des chiffres et des symboles du clavier : l'écran de déverrouillage au démarrage ne gère pas les accents.",
        "part_type_manual_desc": "Choisissez une partition existante pour Winlux : elle sera formatée, les autres partitions sont conservées.",
        "part_layout_after": "Après l'installation :",
        "part_manual_root": "Partition qui accueillera Winlux :",
        "part_manual_none": "Aucune partition utilisable sur ce disque (8 Go minimum, non montée).",
        "part_manual_need_esp": "Ce disque n'a pas de partition système EFI : le mode manuel en UEFI en exige une.",
        "part_manual_erase": "ATTENTION : {partition} sera formatée et son contenu effacé.",
        "part_dualboot_need_uefi": "Le dual-boot nécessite un démarrage UEFI et un disque GPT avec une partition système EFI.",
        "part_dualboot_bitlocker": "BitLocker est actif sur {partition} : désactivez-le depuis Windows avant d'installer Winlux.",
        "part_dualboot_no_room": "Pas assez d'espace libre dans Windows (20 Go minimum pour Winlux, 10 Go laissés à Windows).",
        "part_dualboot_tools": "ntfsresize est absent de cette image : le dual-boot est indisponible.",
        "part_windows_keep": "Windows conservera {size}.",
        "encrypt_boot_note": "Une petite partition /boot de 1 Go reste non chiffrée : elle contient seulement le noyau et l'écran de déverrouillage.",
        "encrypt_unsupported": "Cette image Winlux ne contient pas cryptsetup : le chiffrement n'est pas disponible.",
        "encrypt_manual": "Le chiffrement est disponible avec « disque entier » ou « dual-boot », pas en partitionnement manuel.",
        "encrypt_keyboard": "Au démarrage, la phrase secrète sera saisie avec le clavier {layout}.",
        "encrypt_keyboard_us": "Attention : au démarrage, la phrase secrète devra être saisie avec un clavier QWERTY (US).",
        "error_luks_initramfs": "L'initramfs de cette image ne sait pas déverrouiller un disque chiffré (cryptsetup absent).",
        "error_ntfs_check": "Windows n'a pas été arrêté complètement (démarrage rapide ou veille prolongée) ou son disque doit être vérifié. Désactivez le démarrage rapide, lancez « chkdsk », arrêtez Windows puis relancez l'installation.",
        "progress_resize": "Redimensionnement de Windows (peut prendre plusieurs minutes)...",
    },
    "en_US": {
        "encrypt_ascii": 'Use only unaccented letters, digits and keyboard symbols: the unlock screen at startup does not support accented characters.',
        "part_type_manual_desc": "Pick an existing partition for Winlux: it will be formatted, other partitions are kept.",
        "part_layout_after": "After installation:",
        "part_manual_root": "Partition that will hold Winlux:",
        "part_manual_none": "No usable partition on this disk (8 GB minimum, not mounted).",
        "part_manual_need_esp": "This disk has no EFI system partition: manual mode on UEFI requires one.",
        "part_manual_erase": "WARNING: {partition} will be formatted and its content erased.",
        "part_dualboot_need_uefi": "Dual-boot requires UEFI boot and a GPT disk with an EFI system partition.",
        "part_dualboot_bitlocker": "BitLocker is active on {partition}: turn it off from Windows before installing Winlux.",
        "part_dualboot_no_room": "Not enough free space in Windows (20 GB minimum for Winlux, 10 GB left to Windows).",
        "part_dualboot_tools": "ntfsresize is missing from this image: dual-boot is unavailable.",
        "part_windows_keep": "Windows will keep {size}.",
        "encrypt_boot_note": "A small 1 GB /boot partition stays unencrypted: it only holds the kernel and the unlock screen.",
        "encrypt_unsupported": "This Winlux image does not include cryptsetup: encryption is not available.",
        "encrypt_manual": "Encryption is available with \"entire disk\" or \"dual-boot\", not with manual partitioning.",
        "encrypt_keyboard": "At startup, the passphrase will be typed with the {layout} keyboard.",
        "encrypt_keyboard_us": "Warning: at startup, the passphrase must be typed on a QWERTY (US) keyboard.",
        "error_luks_initramfs": "This image's initramfs cannot unlock an encrypted disk (cryptsetup missing).",
        "error_ntfs_check": "Windows was not fully shut down (fast startup or hibernation) or its disk needs checking. Turn off fast startup, run \"chkdsk\", shut Windows down and run the installation again.",
        "progress_resize": "Resizing Windows (may take several minutes)...",
    },
    "de_DE": {
        "encrypt_ascii": 'Verwenden Sie nur Buchstaben ohne Akzente/Umlaute, Ziffern und Tastatursymbole: Der Entsperrbildschirm beim Start unterstützt keine Sonderzeichen.',
        "part_type_manual_desc": "Wählen Sie eine vorhandene Partition für Winlux: Sie wird formatiert, andere Partitionen bleiben erhalten.",
        "part_layout_after": "Nach der Installation:",
        "part_manual_root": "Partition für Winlux:",
        "part_manual_none": "Keine nutzbare Partition auf dieser Festplatte (mindestens 8 GB, nicht eingehängt).",
        "part_manual_need_esp": "Diese Festplatte hat keine EFI-Systempartition: Der manuelle Modus unter UEFI benötigt eine.",
        "part_manual_erase": "ACHTUNG: {partition} wird formatiert und ihr Inhalt gelöscht.",
        "part_dualboot_need_uefi": "Dual-Boot erfordert UEFI-Start und eine GPT-Festplatte mit EFI-Systempartition.",
        "part_dualboot_bitlocker": "BitLocker ist auf {partition} aktiv: Deaktivieren Sie es in Windows, bevor Sie Winlux installieren.",
        "part_dualboot_no_room": "Nicht genug freier Speicher in Windows (mindestens 20 GB für Winlux, 10 GB bleiben für Windows).",
        "part_dualboot_tools": "ntfsresize fehlt in diesem Image: Dual-Boot ist nicht verfügbar.",
        "part_windows_keep": "Windows behält {size}.",
        "encrypt_boot_note": "Eine kleine /boot-Partition (1 GB) bleibt unverschlüsselt: Sie enthält nur den Kernel und den Entsperrbildschirm.",
        "encrypt_unsupported": "Dieses Winlux-Image enthält kein cryptsetup: Verschlüsselung ist nicht verfügbar.",
        "encrypt_manual": "Verschlüsselung ist mit „Gesamte Festplatte“ oder „Dual-Boot“ verfügbar, nicht bei manueller Partitionierung.",
        "encrypt_keyboard": "Beim Start wird die Passphrase mit der Tastatur {layout} eingegeben.",
        "encrypt_keyboard_us": "Achtung: Beim Start muss die Passphrase auf einer QWERTY-Tastatur (US) eingegeben werden.",
        "error_luks_initramfs": "Das Initramfs dieses Images kann keine verschlüsselte Festplatte entsperren (cryptsetup fehlt).",
        "error_ntfs_check": "Windows wurde nicht vollständig heruntergefahren (Schnellstart oder Ruhezustand) oder die Festplatte muss geprüft werden. Deaktivieren Sie den Schnellstart, führen Sie „chkdsk“ aus, fahren Sie Windows herunter und starten Sie die Installation erneut.",
        "progress_resize": "Windows wird verkleinert (kann einige Minuten dauern)...",
    },
    "es_ES": {
        "encrypt_ascii": 'Use solo letras sin acentos, dígitos y símbolos del teclado: la pantalla de desbloqueo al arrancar no admite acentos.',
        "part_type_manual_desc": "Elija una partición existente para Winlux: se formateará, las demás particiones se conservan.",
        "part_layout_after": "Después de la instalación:",
        "part_manual_root": "Partición que alojará Winlux:",
        "part_manual_none": "No hay ninguna partición utilizable en este disco (mínimo 8 GB, sin montar).",
        "part_manual_need_esp": "Este disco no tiene partición de sistema EFI: el modo manual en UEFI necesita una.",
        "part_manual_erase": "ATENCIÓN: {partition} se formateará y se borrará su contenido.",
        "part_dualboot_need_uefi": "El arranque dual requiere arranque UEFI y un disco GPT con partición de sistema EFI.",
        "part_dualboot_bitlocker": "BitLocker está activo en {partition}: desactívelo desde Windows antes de instalar Winlux.",
        "part_dualboot_no_room": "No hay suficiente espacio libre en Windows (mínimo 20 GB para Winlux, 10 GB para Windows).",
        "part_dualboot_tools": "ntfsresize no está en esta imagen: el arranque dual no está disponible.",
        "part_windows_keep": "Windows conservará {size}.",
        "encrypt_boot_note": "Una pequeña partición /boot de 1 GB queda sin cifrar: solo contiene el núcleo y la pantalla de desbloqueo.",
        "encrypt_unsupported": "Esta imagen de Winlux no incluye cryptsetup: el cifrado no está disponible.",
        "encrypt_manual": "El cifrado está disponible con «disco completo» o «arranque dual», no con particionado manual.",
        "encrypt_keyboard": "Al arrancar, la frase de contraseña se escribirá con el teclado {layout}.",
        "encrypt_keyboard_us": "Atención: al arrancar, la frase de contraseña deberá escribirse con un teclado QWERTY (EE. UU.).",
        "error_luks_initramfs": "El initramfs de esta imagen no puede desbloquear un disco cifrado (falta cryptsetup).",
        "error_ntfs_check": "Windows no se apagó por completo (inicio rápido o hibernación) o su disco necesita una comprobación. Desactive el inicio rápido, ejecute «chkdsk», apague Windows y vuelva a iniciar la instalación.",
        "progress_resize": "Redimensionando Windows (puede tardar varios minutos)...",
    },
    "it_IT": {
        "encrypt_ascii": "Usa solo lettere non accentate, cifre e simboli della tastiera: la schermata di sblocco all'avvio non supporta gli accenti.",
        "part_type_manual_desc": "Scegli una partizione esistente per Winlux: verrà formattata, le altre partizioni vengono mantenute.",
        "part_layout_after": "Dopo l'installazione:",
        "part_manual_root": "Partizione che ospiterà Winlux:",
        "part_manual_none": "Nessuna partizione utilizzabile su questo disco (minimo 8 GB, non montata).",
        "part_manual_need_esp": "Questo disco non ha una partizione di sistema EFI: la modalità manuale in UEFI ne richiede una.",
        "part_manual_erase": "ATTENZIONE: {partition} verrà formattata e il suo contenuto cancellato.",
        "part_dualboot_need_uefi": "Il dual-boot richiede l'avvio UEFI e un disco GPT con partizione di sistema EFI.",
        "part_dualboot_bitlocker": "BitLocker è attivo su {partition}: disattivalo da Windows prima di installare Winlux.",
        "part_dualboot_no_room": "Spazio libero insufficiente in Windows (minimo 20 GB per Winlux, 10 GB lasciati a Windows).",
        "part_dualboot_tools": "ntfsresize non è presente in questa immagine: il dual-boot non è disponibile.",
        "part_windows_keep": "Windows manterrà {size}.",
        "encrypt_boot_note": "Una piccola partizione /boot da 1 GB resta non cifrata: contiene solo il kernel e la schermata di sblocco.",
        "encrypt_unsupported": "Questa immagine di Winlux non include cryptsetup: la cifratura non è disponibile.",
        "encrypt_manual": "La cifratura è disponibile con «disco intero» o «dual-boot», non con il partizionamento manuale.",
        "encrypt_keyboard": "All'avvio, la passphrase verrà digitata con la tastiera {layout}.",
        "encrypt_keyboard_us": "Attenzione: all'avvio, la passphrase dovrà essere digitata con una tastiera QWERTY (US).",
        "error_luks_initramfs": "L'initramfs di questa immagine non può sbloccare un disco cifrato (cryptsetup assente).",
        "error_ntfs_check": "Windows non è stato arrestato completamente (avvio rapido o ibernazione) oppure il disco deve essere controllato. Disattiva l'avvio rapido, esegui «chkdsk», arresta Windows e riavvia l'installazione.",
        "progress_resize": "Ridimensionamento di Windows (potrebbe richiedere diversi minuti)...",
    },
    "pt_BR": {
        "encrypt_ascii": 'Use apenas letras sem acento, dígitos e símbolos do teclado: a tela de desbloqueio na inicialização não aceita acentos.',
        "part_type_manual_desc": "Escolha uma partição existente para o Winlux: ela será formatada, as outras partições são mantidas.",
        "part_layout_after": "Após a instalação:",
        "part_manual_root": "Partição que receberá o Winlux:",
        "part_manual_none": "Nenhuma partição utilizável neste disco (mínimo de 8 GB, não montada).",
        "part_manual_need_esp": "Este disco não tem partição de sistema EFI: o modo manual em UEFI exige uma.",
        "part_manual_erase": "ATENÇÃO: {partition} será formatada e seu conteúdo apagado.",
        "part_dualboot_need_uefi": "O dual-boot requer inicialização UEFI e um disco GPT com partição de sistema EFI.",
        "part_dualboot_bitlocker": "O BitLocker está ativo em {partition}: desative-o no Windows antes de instalar o Winlux.",
        "part_dualboot_no_room": "Espaço livre insuficiente no Windows (mínimo de 20 GB para o Winlux, 10 GB deixados ao Windows).",
        "part_dualboot_tools": "O ntfsresize não está nesta imagem: o dual-boot não está disponível.",
        "part_windows_keep": "O Windows manterá {size}.",
        "encrypt_boot_note": "Uma pequena partição /boot de 1 GB fica sem criptografia: ela contém apenas o kernel e a tela de desbloqueio.",
        "encrypt_unsupported": "Esta imagem do Winlux não inclui o cryptsetup: a criptografia não está disponível.",
        "encrypt_manual": "A criptografia está disponível com \"disco inteiro\" ou \"dual-boot\", não com particionamento manual.",
        "encrypt_keyboard": "Na inicialização, a frase secreta será digitada com o teclado {layout}.",
        "encrypt_keyboard_us": "Atenção: na inicialização, a frase secreta deverá ser digitada em um teclado QWERTY (EUA).",
        "error_luks_initramfs": "O initramfs desta imagem não consegue desbloquear um disco criptografado (cryptsetup ausente).",
        "error_ntfs_check": "O Windows não foi desligado completamente (inicialização rápida ou hibernação) ou o disco precisa ser verificado. Desative a inicialização rápida, execute \"chkdsk\", desligue o Windows e reinicie a instalação.",
        "progress_resize": "Redimensionando o Windows (pode levar vários minutos)...",
    },
}
for _lang, _extra in EXTRA_TRANSLATIONS.items():
    TRANSLATIONS[_lang].update(_extra)


def tr(key, **kwargs):
    """Translate a key using the current language."""
    lang = getattr(tr, '_lang', 'fr_FR')
    table = TRANSLATIONS.get(lang, TRANSLATIONS["fr_FR"])
    text = table.get(key, TRANSLATIONS["fr_FR"].get(key, key))
    if kwargs:
        text = text.format(**kwargs)
    return text

tr._lang = "fr_FR"

LICENSE_TEXT = """\
LICENCE PUBLIQUE GÉNÉRALE GNU
Version 3, 29 juin 2007

Winlux est un logiciel libre distribué sous licence GPL v3.

Vous êtes libre de :
  • Utiliser ce logiciel pour n'importe quel usage
  • Étudier et modifier le code source
  • Redistribuer des copies
  • Distribuer vos versions modifiées

Conditions :
  • Toute distribution doit inclure le code source ou un moyen d'y accéder
  • Les modifications doivent être sous la même licence GPL v3
  • Les notices de copyright doivent être préservées

Ce logiciel est fourni « en l'état », sans aucune garantie.

Pour le texte complet : https://www.gnu.org/licenses/gpl-3.0.html
"""

STEPS = [
    "Langue",
    "Clavier",
    "Licence",
    "Partitionnement",
    "Chiffrement",
    "Compte local",
    "Résumé",
    "Installation",
    "Terminé",
]


def human_size(size_bytes):
    if size_bytes is None:
        return "?"
    for unit in ("o", "Ko", "Mo", "Go", "To"):
        if abs(size_bytes) < 1024:
            return f"{size_bytes:.1f} {unit}"
        size_bytes /= 1024
    return f"{size_bytes:.1f} Po"


def detect_disks():
    disks = []
    try:
        out = subprocess.check_output(
            ["lsblk", "-bdnJo", "NAME,SIZE,MODEL,TRAN,TYPE,RO,SERIAL,ROTA"],
            text=True, timeout=10
        )
        data = json.loads(out)
        for dev in data.get("blockdevices", []):
            if dev.get("type") not in ("disk", "loop"):
                continue
            if dev.get("ro"):
                continue
            name = dev["name"]
            path = f"/dev/{name}"
            size = dev.get("size", 0) or 0
            if size < 2 * 1024**3:
                continue
            model = (dev.get("model") or "").strip() or "Disque inconnu"
            tran = (dev.get("tran") or "").upper()
            rota = dev.get("rota", True)
            disk_type = "HDD" if rota else "SSD"
            if "NVME" in tran or "nvme" in name:
                disk_type = "NVMe"
            elif "USB" in tran:
                disk_type = "USB"
            disks.append({
                "path": path,
                "name": name,
                "size": size,
                "model": model,
                "transport": tran,
                "type": disk_type,
                "serial": (dev.get("serial") or "").strip(),
            })
    except Exception:
        pass
    return disks


MIB = 1024 * 1024
GIB = 1024 * MIB
ESP_PARTTYPE = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"
BIOS_PARTTYPE = "21686148-6f6e-6974-6576-6e6f74656564"
BOOT_PART_MIB = 1024
MIN_WINLUX_MIB = 20 * 1024
WINDOWS_MARGIN = 10 * GIB
MANUAL_MIN_SIZE = 8 * GIB
LUKS_NAME = "winlux_crypt"


def detect_partitions(device):
    """Partitions of a disk, ordered by position on the disk."""
    parts = []
    try:
        out = subprocess.check_output(
            ["lsblk", "-bnJo", "NAME,SIZE,FSTYPE,LABEL,MOUNTPOINT,TYPE,PARTTYPE,START", device],
            text=True, timeout=10
        )
        data = json.loads(out)
        for bd in data.get("blockdevices", []):
            for child in bd.get("children", []) or []:
                if child.get("type") != "part":
                    continue
                number = 0
                try:
                    with open(f"/sys/class/block/{child['name']}/partition") as f:
                        number = int(f.read())
                except (OSError, ValueError):
                    pass
                part = {
                    "name": child.get("name", ""),
                    "path": f"/dev/{child['name']}",
                    "number": number,
                    "start": int(child.get("start") or 0) * 512,
                    "size": int(child.get("size") or 0),
                    "fstype": (child.get("fstype") or "").strip(),
                    "label": (child.get("label") or "").strip(),
                    "mountpoint": (child.get("mountpoint") or "").strip(),
                    "parttype": (child.get("parttype") or "").strip().lower(),
                }
                if not part["parttype"] or not part["fstype"]:
                    # lsblk takes these from the udev database; probe directly when it is absent.
                    probe = blkid_probe(part["path"])
                    part["fstype"] = part["fstype"] or probe.get("TYPE", "")
                    part["label"] = part["label"] or probe.get("LABEL", "")
                    part["parttype"] = part["parttype"] or probe.get("PART_ENTRY_TYPE", "").lower()
                parts.append(part)
    except Exception:
        pass
    parts.sort(key=lambda p: p["start"])
    return parts


def blkid_probe(path):
    try:
        out = subprocess.run(["blkid", "-p", "-o", "export", path],
                             capture_output=True, text=True, timeout=10).stdout
    except Exception:
        return {}
    return dict(line.split("=", 1) for line in out.splitlines() if "=" in line)


def ntfs_min_size(path):
    """Smallest size ntfsresize accepts for this NTFS volume, or None."""
    try:
        out = subprocess.run(["ntfsresize", "--info", "--force", path],
                             capture_output=True, text=True, timeout=120).stdout
    except Exception:
        return None
    match = re.search(r"resize at (\d+) bytes", out)
    return int(match.group(1)) if match else None


def detect_existing_os(partitions):
    """Windows candidates on a disk: the largest NTFS volume first."""
    found = []
    for p in partitions:
        fstype = p["fstype"].lower()
        if fstype == "bitlocker":
            found.append({"partition": p["path"], "number": p["number"], "os_name": "Windows",
                          "size": p["size"], "label": p["label"], "bitlocker": True,
                          "min_size": None})
        elif fstype == "ntfs" and p["size"] > 30 * GIB:
            found.append({"partition": p["path"], "number": p["number"], "os_name": "Windows",
                          "size": p["size"], "label": p["label"] or "NTFS", "bitlocker": False,
                          "min_size": ntfs_min_size(p["path"]) if shutil.which("ntfsresize") else None})
    found.sort(key=lambda o: o["size"], reverse=True)
    return found


def find_esp(partitions):
    for p in partitions:
        if p["parttype"] == ESP_PARTTYPE:
            return p
    return None


def disk_partitions_sysfs(device):
    """{number: {path, start, size}} as the kernel currently sees the disk."""
    disk = os.path.basename(os.path.realpath(device))
    base = f"/sys/block/{disk}"
    result = {}
    for entry in os.listdir(base):
        pfile = os.path.join(base, entry, "partition")
        if not os.path.exists(pfile):
            continue
        with open(pfile) as f:
            number = int(f.read())
        with open(os.path.join(base, entry, "start")) as f:
            start = int(f.read()) * 512
        with open(os.path.join(base, entry, "size")) as f:
            size = int(f.read()) * 512
        result[number] = {"path": f"/dev/{entry}", "start": start, "size": size}
    return result


def disk_size_bytes(device):
    disk = os.path.basename(os.path.realpath(device))
    with open(f"/sys/block/{disk}/size") as f:
        return int(f.read()) * 512


def partition_path(device, number):
    return f"{device}p{number}" if device[-1].isdigit() else f"{device}{number}"


def wait_for_block(path, timeout=20):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if os.path.exists(path):
            return path
        time.sleep(0.3)
    raise RuntimeError(f"Partition introuvable : {path}")


def reread_partitions(device):
    subprocess.run(["partprobe", device], check=True, timeout=30)
    if shutil.which("udevadm"):
        subprocess.run(["udevadm", "settle"], timeout=30)


def blkid_value(path, tag):
    return subprocess.check_output(["blkid", "-s", tag, "-o", "value", path],
                                   text=True, timeout=10).strip()


def iter_cpio_names(path):
    """Names stored in a (possibly concatenated) gzip newc cpio archive."""
    with gzip.open(path, "rb") as f:
        data = f.read()
    offset = 0
    while offset + 110 <= len(data):
        if data[offset:offset + 6] not in (b"070701", b"070702"):
            # Concatenated archives are padded with zeros between members.
            offset += 4
            continue
        header = data[offset:offset + 110]
        filesize = int(header[54:62], 16)
        namesize = int(header[94:102], 16)
        name = data[offset + 110:offset + 110 + namesize - 1].decode("utf-8", "replace")
        if name == "TRAILER!!!":
            offset += 110 + namesize
            offset = (offset + 3) & ~3
            continue
        yield name
        offset = (offset + 110 + namesize + 3) & ~3
        offset = (offset + filesize + 3) & ~3


def initramfs_supports_luks(path):
    try:
        return any(name.lstrip("./") == "bin/cryptsetup" for name in iter_cpio_names(path))
    except Exception:
        return False


def cpio_newc(entries):
    """Build an uncompressed newc archive from (name, mode, data) entries."""
    out = bytearray()
    for ino, (name, mode, data) in enumerate(entries + [("TRAILER!!!", 0, b"")], start=1):
        encoded = name.encode() + b"\0"
        fields = [ino, mode, 0, 0, 1, 0, len(data), 0, 0, 0, 0, len(encoded), 0]
        out += b"070701" + "".join(f"{v:08x}" for v in fields).encode() + encoded
        out += b"\0" * (-len(out) % 4)
        out += data
        out += b"\0" * (-len(out) % 4)
    return bytes(out)


def keymap_tools_available():
    return bool(shutil.which("ckbcomp") and shutil.which("loadkeys"))


def console_keymap(layout):
    """Binary console keymap (busybox loadkmap format) for an XKB layout, or None."""
    if not keymap_tools_available():
        return None
    try:
        # 8-bit charmap: busybox loadkmap has no Unicode support.
        source = subprocess.run(["ckbcomp", "-charmap", "ISO-8859-15", layout],
                                capture_output=True, timeout=30, check=True).stdout
        out = subprocess.run(["loadkeys", "-b"], input=source, capture_output=True, timeout=30)
    except Exception:
        return None
    return out.stdout if out.returncode == 0 and out.stdout.startswith(b"bkeymap") else None


def passphrase_is_ascii(passphrase):
    return all(" " <= c <= "~" for c in passphrase)


def planned_layout(disk_size, partitions, install_type, luks, dualboot=None, manual_root=None):
    """Segments drawn for the disk after installation."""
    if install_type == "full":
        segments = [
            {"start": MIB, "size": 2 * MIB, "fstype": "bios", "label": "BIOS"},
            {"start": 3 * MIB, "size": 512 * MIB, "fstype": "vfat", "label": "EFI"},
        ]
        start = 515 * MIB
        if luks:
            segments.append({"start": start, "size": BOOT_PART_MIB * MIB, "fstype": "ext4", "label": "/boot"})
            start += BOOT_PART_MIB * MIB
        segments.append({"start": start, "size": max(0, disk_size - start - MIB),
                         "fstype": "crypto_LUKS" if luks else "winlux", "label": "Winlux"})
        return segments
    segments = []
    for p in partitions:
        seg = dict(p)
        if install_type == "dualboot" and dualboot and p["path"] == dualboot["partition"]:
            seg["size"] = dualboot["new_size"]
            seg["label"] = p["label"] or "Windows"
            segments.append(seg)
            start = (p["start"] + dualboot["new_size"] + MIB - 1) // MIB * MIB
            if luks:
                segments.append({"start": start, "size": BOOT_PART_MIB * MIB, "fstype": "ext4", "label": "/boot"})
                start += BOOT_PART_MIB * MIB
            segments.append({"start": start, "size": dualboot["alloc"] - (BOOT_PART_MIB * MIB if luks else 0),
                             "fstype": "crypto_LUKS" if luks else "winlux", "label": "Winlux"})
            continue
        if install_type == "manual" and p["path"] == manual_root:
            seg["fstype"] = "winlux"
            seg["label"] = "Winlux"
        segments.append(seg)
    return segments


def detect_hardware_summary():
    info = {"cpu": "Inconnu", "ram": "Inconnu", "gpu": [], "uefi": False, "secureboot": False}
    try:
        with open("/proc/cpuinfo") as f:
            for line in f:
                if line.startswith("model name"):
                    info["cpu"] = line.split(":", 1)[1].strip()
                    break
    except OSError:
        pass
    try:
        with open("/proc/meminfo") as f:
            for line in f:
                if line.startswith("MemTotal:"):
                    kb = int(line.split()[1])
                    info["ram"] = human_size(kb * 1024)
                    break
    except OSError:
        pass
    try:
        out = subprocess.check_output(["lspci", "-mm"], text=True, timeout=5)
        for line in out.splitlines():
            if "VGA" in line or "3D" in line or "Display" in line:
                parts = line.split('"')
                if len(parts) >= 6:
                    info["gpu"].append(f"{parts[3]} {parts[5]}")
    except Exception:
        pass
    info["uefi"] = os.path.isdir("/sys/firmware/efi")
    try:
        with open("/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c", "rb") as f:
            data = f.read()
            info["secureboot"] = len(data) >= 5 and data[4] == 1
    except OSError:
        pass
    return info


def passphrase_strength(passphrase):
    """Return 0=empty, 1=weak, 2=medium, 3=strong."""
    if not passphrase:
        return 0
    length = len(passphrase)
    has_upper = any(c.isupper() for c in passphrase)
    has_lower = any(c.islower() for c in passphrase)
    has_digit = any(c.isdigit() for c in passphrase)
    has_special = any(not c.isalnum() for c in passphrase)
    variety = sum([has_upper, has_lower, has_digit, has_special])
    if length >= 16 and variety >= 3:
        return 3
    if length >= 12 and variety >= 2:
        return 3
    if length >= 8 and variety >= 2:
        return 2
    if length >= 8:
        return 2
    return 1


class PartitionBar(Gtk.DrawingArea):
    """Disk drawn as a bar: each partition at its real position, gaps as free space."""

    def __init__(self):
        super().__init__()
        self.segments = []
        self.disk_size = 0
        self.set_size_request(-1, 70)
        self.connect("draw", self.on_draw)

    def set_data(self, disk_size, segments):
        self.disk_size = disk_size
        self.segments = sorted(segments, key=lambda s: s.get("start", 0))
        self.queue_draw()

    def _entries(self):
        entries = []
        cursor = 0
        for seg in self.segments:
            start = seg.get("start", cursor)
            if start - cursor > 64 * MIB:
                entries.append({"start": cursor, "size": start - cursor, "fstype": "free",
                                "label": tr("part_free_space")})
            entries.append(seg)
            cursor = max(cursor, start + seg["size"])
        if self.disk_size - cursor > 64 * MIB:
            entries.append({"start": cursor, "size": self.disk_size - cursor, "fstype": "free",
                            "label": tr("part_free_space")})
        return entries

    def on_draw(self, widget, cr):
        w = widget.get_allocated_width()
        bar_y = 4
        bar_h = 36
        label_y = bar_y + bar_h + 14
        cr.select_font_face("sans-serif", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        cr.set_source_rgba(0.18, 0.18, 0.27, 1.0)
        self._rounded_rect(cr, 0, bar_y, w, bar_h, 6)
        cr.fill()
        if self.disk_size <= 0:
            cr.set_source_rgba(0.7, 0.7, 0.8, 0.7)
            cr.set_font_size(11)
            text = tr("part_no_disk")
            ext = cr.text_extents(text)
            cr.move_to((w - ext.width) / 2, bar_y + bar_h / 2 + ext.height / 2)
            cr.show_text(text)
            return
        cr.save()
        self._rounded_rect(cr, 0, bar_y, w, bar_h, 6)
        cr.clip()
        # Tiny partitions (BIOS boot, MSR) still get a visible sliver.
        for part in self._entries():
            x = w * part["start"] / self.disk_size
            pw = max(w * part["size"] / self.disk_size, 3)
            color = PART_COLORS.get(part.get("fstype", "").lower(), (0.35, 0.35, 0.45, 1.0))
            cr.set_source_rgba(*color)
            cr.rectangle(x, bar_y, pw, bar_h)
            cr.fill()
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35)
            cr.set_line_width(1)
            cr.move_to(x + pw, bar_y)
            cr.line_to(x + pw, bar_y + bar_h)
            cr.stroke()
            label = part.get("label") or part.get("name", "")
            size_str = human_size(part["size"])
            cr.set_font_size(10)
            for text in (f"{label} ({size_str})" if label else size_str, size_str):
                ext = cr.text_extents(text)
                if ext.width < pw - 8:
                    cr.set_source_rgba(1, 1, 1, 0.95)
                    cr.move_to(x + (pw - ext.width) / 2, bar_y + bar_h / 2 + ext.height / 2)
                    cr.show_text(text)
                    break
            fs_label = {"winlux": "ext4", "crypto_luks": "LUKS", "free": ""}.get(
                part.get("fstype", "").lower(), part.get("fstype", ""))
            cr.set_font_size(9)
            ext = cr.text_extents(fs_label)
            if fs_label and ext.width < pw - 4:
                cr.restore()
                cr.set_source_rgba(1, 1, 1, 0.7)
                cr.move_to(x + (pw - ext.width) / 2, label_y)
                cr.show_text(fs_label)
                cr.save()
                self._rounded_rect(cr, 0, bar_y, w, bar_h, 6)
                cr.clip()
        cr.restore()
        cr.set_source_rgba(0.47, 0.47, 0.6, 0.6)
        cr.set_line_width(1)
        self._rounded_rect(cr, 0.5, bar_y + 0.5, w - 1, bar_h - 1, 6)
        cr.stroke()

    @staticmethod
    def _rounded_rect(cr, x, y, w, h, r):
        cr.new_sub_path()
        cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
        cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
        cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
        cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
        cr.close_path()


class InstallerApp(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="org.winlux.Installer")
        self.current_step = 0
        self.config = {
            "language": "fr_FR",
            "keyboard": "fr",
            "license_accepted": False,
            "install_type": "full",
            "disk": None,
            "luks_enabled": False,
            "luks_passphrase": "",
            "manual_root": None,
            "hostname": "winlux-desktop",
            "username": "",
            "fullname": "",
            "password": "",
            "autologin": True,
        }
        self.disks = []
        self.hw_info = {}
        self.disk_partitions = {}
        self.detected_os = {}
        self.dualboot_alloc_mb = 0
        self.dualboot_target = None
        self.manual_candidates = []
        self.luks_supported = False
        self.detect_done = False
        self.built_lang = "fr_FR"
        self.install_thread = None
        self.install_cancelled = False

    def do_activate(self):
        provider = Gtk.CssProvider()
        provider.load_from_data(CSS)
        Gtk.StyleContext.add_provider_for_screen(
            Gdk.Screen.get_default(), provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )
        self.win = Gtk.ApplicationWindow(application=self, title="Installer Winlux")
        self.win.get_style_context().add_class("installer-window")
        self.win.set_default_size(960, 640)
        self.win.set_position(Gtk.WindowPosition.CENTER)
        self.win.set_resizable(True)
        self.main_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        self.win.add(self.main_box)
        self.sidebar = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.sidebar.get_style_context().add_class("installer-sidebar")
        self.sidebar.set_size_request(210, -1)
        logo = Gtk.Label(label="Winlux")
        logo.get_style_context().add_class("page-title")
        logo.set_margin_bottom(24)
        self.sidebar.pack_start(logo, False, False, 0)
        self.step_labels = []
        for i, name in enumerate(STEPS):
            lbl = Gtk.Label(label=f"  {i + 1}. {name}", xalign=0)
            lbl.get_style_context().add_class("sidebar-step")
            self.step_labels.append(lbl)
            self.sidebar.pack_start(lbl, False, False, 0)
        self.main_box.pack_start(self.sidebar, False, False, 0)
        self.content_area = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.content_area.get_style_context().add_class("installer-content")
        self.main_box.pack_start(self.content_area, True, True, 0)
        self.pages = Gtk.Stack()
        self.pages.set_transition_type(Gtk.StackTransitionType.SLIDE_LEFT)
        self.content_area.pack_start(self.pages, True, True, 0)
        self.nav_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.nav_box.set_margin_top(16)
        self.btn_back = Gtk.Button(label="Retour")
        self.btn_back.get_style_context().add_class("btn-secondary")
        self.btn_back.connect("clicked", self.on_back)
        self.btn_next = Gtk.Button(label="Suivant")
        self.btn_next.get_style_context().add_class("btn-primary")
        self.btn_next.connect("clicked", self.on_next)
        self.nav_box.pack_end(self.btn_next, False, False, 0)
        self.nav_box.pack_end(self.btn_back, False, False, 0)
        self.content_area.pack_end(self.nav_box, False, False, 0)
        self.build_language_page()
        self.build_keyboard_page()
        self.build_license_page()
        self.build_partition_page()
        self.build_encryption_page()
        self.build_account_page()
        self.build_summary_page()
        self.build_progress_page()
        self.build_done_page()
        self.update_step()
        self.win.show_all()
        threading.Thread(target=self.bg_detect, daemon=True).start()

    def bg_detect(self):
        self.disks = detect_disks()
        self.hw_info = detect_hardware_summary()
        for disk in self.disks:
            parts = detect_partitions(disk["path"])
            self.disk_partitions[disk["path"]] = parts
            self.detected_os[disk["path"]] = detect_existing_os(parts)
        self.luks_supported = self.detect_luks_support()
        self.detect_done = True
        GLib.idle_add(self.refresh_partition_page)

    def detect_luks_support(self):
        """The image must ship cryptsetup here and in the initramfs it installs."""
        if not shutil.which("cryptsetup"):
            return False
        live = "/run/winlux/live/media/boot/winlux-initramfs.cpio.gz"
        if os.path.isfile(live):
            return initramfs_supports_luks(live)
        iso = os.environ.get("WINLUX_ISO") or os.path.join(ROOT_DIR, "build", "winlux-live-test.iso")
        if not os.path.isfile(iso):
            return False
        with tempfile.TemporaryDirectory() as tmp:
            out = f"{tmp}/initramfs.gz"
            result = subprocess.run(["xorriso", "-osirrox", "on", "-indev", iso,
                                     "-extract", "/boot/winlux-initramfs.cpio.gz", out],
                                    capture_output=True, timeout=60)
            return result.returncode == 0 and initramfs_supports_luks(out)

    def update_step(self):
        for i, lbl in enumerate(self.step_labels):
            ctx = lbl.get_style_context()
            ctx.remove_class("active")
            ctx.remove_class("done")
            if i == self.current_step:
                ctx.add_class("active")
            elif i < self.current_step:
                ctx.add_class("done")
        page_name = f"page_{self.current_step}"
        self.pages.set_visible_child_name(page_name)
        self.btn_back.set_visible(self.current_step > 0 and self.current_step < 7)
        if self.current_step == 6:
            self.btn_next.set_label(tr("btn_install"))
            self.btn_next.get_style_context().remove_class("btn-primary")
            self.btn_next.get_style_context().add_class("btn-danger")
            self.btn_next.set_visible(True)
        elif self.current_step >= 7:
            self.btn_next.set_visible(False)
            self.btn_back.set_visible(False)
        else:
            self.btn_next.set_label(tr("btn_next"))
            self.btn_next.get_style_context().remove_class("btn-danger")
            self.btn_next.get_style_context().add_class("btn-primary")
            self.btn_next.set_visible(True)
        self.validate_step()

    def validate_step(self):
        ok = True
        if self.current_step == 2:
            ok = self.config["license_accepted"]
        elif self.current_step == 3:
            itype = self.config["install_type"]
            ok = self.config["disk"] is not None and (
                itype == "full"
                or (itype == "dualboot" and self.dualboot_target is not None)
                or (itype == "manual" and self.manual_ready()))
        elif self.current_step == 4:
            if self.config["luks_enabled"]:
                pp = self.config["luks_passphrase"]
                pp_confirm = self.luks_confirm_entry.get_text() if hasattr(self, 'luks_confirm_entry') else ""
                ok = len(pp) >= 8 and pp == pp_confirm and passphrase_is_ascii(pp)
            else:
                ok = True
        elif self.current_step == 5:
            ok = (len(self.config["username"]) >= 2
                  and len(self.config["password"]) >= 1
                  and re.match(r'^[a-z_][a-z0-9_-]*$', self.config["username"]) is not None)
        self.btn_next.set_sensitive(ok)

    def on_back(self, _btn):
        if self.current_step > 0:
            self.current_step -= 1
            self.update_step()

    def on_next(self, _btn):
        if self.current_step == 6:
            self.start_installation()
        elif self.current_step < len(STEPS) - 1:
            if self.current_step == 0:
                self.update_ui_language()
            if self.current_step == 3:
                self.refresh_encryption_page()
            if self.current_step == 5:
                self.refresh_summary()
            self.current_step += 1
            self.update_step()

    # ── Page 0 : Langue ──────────────────────────────────────────

    def build_language_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Choisissez votre langue", xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label="Cette langue sera utilisée pendant l'installation et comme langue par défaut du système.", xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        sub.set_line_wrap(True)
        box.pack_start(sub, False, False, 0)
        self.lang_group = None
        for locale, label, kb in LANGUAGES:
            row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
            row.get_style_context().add_class("card")
            rb = Gtk.RadioButton.new_with_label_from_widget(self.lang_group, label)
            rb.set_margin_start(8)
            if self.lang_group is None:
                self.lang_group = rb
            rb.connect("toggled", self.on_lang_toggle, locale, kb)
            row.pack_start(rb, True, True, 0)
            kb_label = Gtk.Label(label=f"Clavier : {kb}")
            kb_label.get_style_context().add_class("card-detail")
            row.pack_end(kb_label, False, False, 8)
            box.pack_start(row, False, False, 0)
        self.pages.add_named(box, "page_0")

    def on_lang_toggle(self, btn, locale, kb):
        if btn.get_active():
            self.config["language"] = locale
            self.config["keyboard"] = kb.split("(")[1].rstrip(")")

    def update_ui_language(self):
        tr._lang = self.config["language"]
        steps = tr("steps")
        for i, lbl in enumerate(self.step_labels):
            if i < len(steps):
                lbl.set_text(f"  {i + 1}. {steps[i]}")
        if self.built_lang != self.config["language"]:
            self.rebuild_pages()

    def rebuild_pages(self):
        """Pages read their strings when built, so rebuild them in the chosen language."""
        for i in range(1, 9):
            child = self.pages.get_child_by_name(f"page_{i}")
            if child:
                child.destroy()
        self.config.update({"license_accepted": False, "install_type": "full", "manual_root": None,
                            "luks_enabled": False, "luks_passphrase": "", "hostname": "winlux-desktop",
                            "username": "", "fullname": "", "password": "", "autologin": True})
        self.build_keyboard_page()
        self.build_license_page()
        self.build_partition_page()
        self.build_encryption_page()
        self.build_account_page()
        self.build_summary_page()
        self.build_progress_page()
        self.build_done_page()
        self.pages.show_all()
        self.built_lang = self.config["language"]
        if self.detect_done:
            self.refresh_partition_page()

    # ── Page 1 : Clavier ─────────────────────────────────────────

    def build_keyboard_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("choose_keyboard"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("keyboard_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        sub.set_line_wrap(True)
        box.pack_start(sub, False, False, 0)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        kb_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.kb_group = None
        for layout, label in KEYBOARDS:
            row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
            row.get_style_context().add_class("card")
            rb = Gtk.RadioButton.new_with_label_from_widget(self.kb_group, label)
            rb.set_margin_start(8)
            if self.kb_group is None:
                self.kb_group = rb
            if layout == self.config["keyboard"]:
                rb.set_active(True)
            rb.connect("toggled", self.on_kb_toggle, layout)
            row.pack_start(rb, True, True, 0)
            kb_box.pack_start(row, False, False, 0)
        scroll.add(kb_box)
        box.pack_start(scroll, True, True, 0)
        test_label = Gtk.Label(label=tr("keyboard_test"), xalign=0)
        test_label.get_style_context().add_class("input-label")
        test_label.set_margin_top(12)
        box.pack_start(test_label, False, False, 0)
        test_entry = Gtk.Entry()
        test_entry.get_style_context().add_class("input-field")
        test_entry.set_placeholder_text("Tapez ici pour tester...")
        box.pack_start(test_entry, False, False, 0)
        self.pages.add_named(box, "page_1")

    def on_kb_toggle(self, btn, layout):
        if btn.get_active():
            self.config["keyboard"] = layout

    # ── Page 2 : Licence ─────────────────────────────────────────

    def build_license_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("license_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("license_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        box.pack_start(sub, False, False, 0)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_min_content_height(250)
        text_view = Gtk.TextView()
        text_view.set_editable(False)
        text_view.set_cursor_visible(False)
        text_view.set_wrap_mode(Gtk.WrapMode.WORD)
        text_view.get_buffer().set_text(LICENSE_TEXT)
        text_view.get_style_context().add_class("license-text")
        scroll.add(text_view)
        box.pack_start(scroll, True, True, 0)
        accept = Gtk.CheckButton(label=tr("license_accept"))
        accept.connect("toggled", self.on_license_toggle)
        accept.set_margin_top(12)
        box.pack_start(accept, False, False, 0)
        self.pages.add_named(box, "page_2")

    def on_license_toggle(self, btn):
        self.config["license_accepted"] = btn.get_active()
        self.validate_step()

    # ── Page 3 : Partitionnement ─────────────────────────────────

    def build_partition_page(self):
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)

        title = Gtk.Label(label=tr("part_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("part_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        sub.set_line_wrap(True)
        box.pack_start(sub, False, False, 0)

        disk_label = Gtk.Label(label=tr("part_select_disk"), xalign=0)
        disk_label.get_style_context().add_class("section-label")
        box.pack_start(disk_label, False, False, 0)
        self.part_disk_list = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        box.pack_start(self.part_disk_list, False, False, 0)

        layout_label = Gtk.Label(label=tr("part_disk_layout"), xalign=0)
        layout_label.get_style_context().add_class("section-label")
        layout_label.set_margin_top(8)
        box.pack_start(layout_label, False, False, 0)
        self.partition_bar = PartitionBar()
        box.pack_start(self.partition_bar, False, False, 0)
        after_label = Gtk.Label(label=tr("part_layout_after"), xalign=0)
        after_label.get_style_context().add_class("section-label")
        box.pack_start(after_label, False, False, 0)
        self.partition_bar_after = PartitionBar()
        box.pack_start(self.partition_bar_after, False, False, 0)

        self.part_erase_warn = Gtk.Label(label="", xalign=0)
        self.part_erase_warn.get_style_context().add_class("disk-warn")
        self.part_erase_warn.set_line_wrap(True)
        box.pack_start(self.part_erase_warn, False, False, 0)

        type_label = Gtk.Label(label=tr("summary_type") + " :", xalign=0)
        type_label.get_style_context().add_class("section-label")
        type_label.set_margin_top(8)
        box.pack_start(type_label, False, False, 0)

        def card(radio, desc_key):
            frame = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
            frame.get_style_context().add_class("card")
            frame.pack_start(radio, False, False, 0)
            desc = Gtk.Label(label=tr(desc_key), xalign=0)
            desc.get_style_context().add_class("card-detail")
            desc.set_line_wrap(True)
            desc.set_margin_start(24)
            frame.pack_start(desc, False, False, 0)
            box.pack_start(frame, False, False, 0)
            return frame

        self.type_full_rb = Gtk.RadioButton.new_with_label(None, tr("part_type_full"))
        card(self.type_full_rb, "part_type_full_desc")

        self.type_dualboot_rb = Gtk.RadioButton.new_with_label_from_widget(
            self.type_full_rb, tr("part_type_dualboot"))
        dualboot_card = card(self.type_dualboot_rb, "part_type_dualboot_desc")
        self.dualboot_info = Gtk.Label(label="", xalign=0)
        self.dualboot_info.get_style_context().add_class("info-text")
        self.dualboot_info.set_line_wrap(True)
        self.dualboot_info.set_margin_start(24)
        dualboot_card.pack_start(self.dualboot_info, False, False, 0)
        self.dualboot_slider_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.dualboot_slider_box.set_margin_start(24)
        self.dualboot_slider_box.set_margin_top(8)
        slider_label = Gtk.Label(label=tr("part_dualboot_slider"), xalign=0)
        slider_label.get_style_context().add_class("card-detail")
        self.dualboot_slider_box.pack_start(slider_label, False, False, 0)
        self.dualboot_scale = Gtk.Scale.new_with_range(
            Gtk.Orientation.HORIZONTAL, MIN_WINLUX_MIB, MIN_WINLUX_MIB + 1024, 1024)
        self.dualboot_scale.set_draw_value(True)
        self.dualboot_scale.set_value_pos(Gtk.PositionType.RIGHT)
        self.dualboot_scale.connect("format-value", self._format_scale_mb)
        self.dualboot_scale.connect("value-changed", self._on_dualboot_scale)
        self.dualboot_slider_box.pack_start(self.dualboot_scale, False, False, 0)
        self.dualboot_keep_label = Gtk.Label(label="", xalign=0)
        self.dualboot_keep_label.get_style_context().add_class("card-detail")
        self.dualboot_slider_box.pack_start(self.dualboot_keep_label, False, False, 0)
        self.dualboot_slider_box.set_no_show_all(True)
        dualboot_card.pack_start(self.dualboot_slider_box, False, False, 0)

        self.type_manual_rb = Gtk.RadioButton.new_with_label_from_widget(
            self.type_full_rb, tr("part_type_manual"))
        manual_card = card(self.type_manual_rb, "part_type_manual_desc")
        self.manual_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.manual_box.set_margin_start(24)
        self.manual_box.set_margin_top(8)
        manual_label = Gtk.Label(label=tr("part_manual_root"), xalign=0)
        manual_label.get_style_context().add_class("card-detail")
        self.manual_box.pack_start(manual_label, False, False, 0)
        self.manual_combo = Gtk.ComboBoxText()
        self.manual_combo.connect("changed", self.on_manual_root_changed)
        self.manual_box.pack_start(self.manual_combo, False, False, 0)
        self.manual_info = Gtk.Label(label="", xalign=0)
        self.manual_info.get_style_context().add_class("warning-text")
        self.manual_info.set_line_wrap(True)
        self.manual_box.pack_start(self.manual_info, False, False, 0)
        self.manual_box.set_no_show_all(True)
        manual_card.pack_start(self.manual_box, False, False, 0)

        self.type_full_rb.connect("toggled", self.on_type_toggle, "full")
        self.type_dualboot_rb.connect("toggled", self.on_type_toggle, "dualboot")
        self.type_manual_rb.connect("toggled", self.on_type_toggle, "manual")

        hw_frame = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        hw_frame.set_margin_top(12)
        hw_title = Gtk.Label(label=tr("hw_detected"), xalign=0)
        hw_title.get_style_context().add_class("card-title")
        hw_frame.pack_start(hw_title, False, False, 0)
        self.hw_label = Gtk.Label(label=tr("hw_detecting"), xalign=0)
        self.hw_label.get_style_context().add_class("hw-info")
        self.hw_label.set_line_wrap(True)
        hw_frame.pack_start(self.hw_label, False, False, 0)
        box.pack_start(hw_frame, False, False, 0)

        scroll.add(box)
        self.pages.add_named(scroll, "page_3")

    def _format_scale_mb(self, scale, value):
        return human_size(value * MIB)

    def _on_dualboot_scale(self, scale):
        self.dualboot_alloc_mb = int(scale.get_value())
        self.refresh_layout_preview()

    def refresh_partition_page(self):
        for child in self.part_disk_list.get_children():
            child.destroy()
        if not self.disks:
            lbl = Gtk.Label(label=tr("part_no_disk"))
            lbl.get_style_context().add_class("card-detail")
            self.part_disk_list.pack_start(lbl, False, False, 0)
        group = None
        chosen = (self.config.get("disk") or {}).get("path")
        for disk in self.disks:
            row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
            row.get_style_context().add_class("card")
            rb = Gtk.RadioButton.new_with_label_from_widget(group, f"{disk['path']}  —  {disk['model']}")
            group = group or rb
            row.pack_start(rb, True, True, 0)
            details = Gtk.Label(label=f"{disk['type']}  ·  {human_size(disk['size'])}")
            details.get_style_context().add_class("card-detail")
            row.pack_end(details, False, False, 8)
            self.part_disk_list.pack_start(row, False, False, 0)
            if disk["path"] == chosen:
                rb.set_active(True)
            rb.connect("toggled", self.on_disk_toggle, disk)
        lines = []
        if self.hw_info:
            lines.append(f"CPU : {self.hw_info.get('cpu', '?')}")
            lines.append(f"RAM : {self.hw_info.get('ram', '?')}")
            gpus = self.hw_info.get("gpu", [])
            for i, g in enumerate(gpus):
                lines.append(f"GPU {i}: {g}")
            if not gpus:
                lines.append("GPU : ?")
            mode = "UEFI" if self.hw_info.get("uefi") else "Legacy BIOS"
            sb = " + Secure Boot" if self.hw_info.get("secureboot") else ""
            lines.append(f"{tr('summary_boot')} : {mode}{sb}")
        self.hw_label.set_text("\n".join(lines) if lines else tr("hw_detecting"))
        self.part_disk_list.show_all()
        if self.disks:
            self.select_disk(next((d for d in self.disks if d["path"] == chosen), self.disks[0]))
        self.validate_step()

    def on_disk_toggle(self, btn, disk):
        if btn.get_active():
            self.select_disk(disk)

    def select_disk(self, disk):
        self.config["disk"] = disk
        parts = self.disk_partitions.get(disk["path"], [])
        self.partition_bar.set_data(disk["size"], parts)
        self.dualboot_target = None
        reason = None
        os_list = self.detected_os.get(disk["path"], [])
        win = os_list[0] if os_list else None
        if not win:
            reason = tr("part_dualboot_no_os")
        elif not self.hw_info.get("uefi") or not find_esp(parts):
            reason = tr("part_dualboot_need_uefi")
        elif win["bitlocker"]:
            reason = tr("part_dualboot_bitlocker", partition=win["partition"])
        elif not shutil.which("ntfsresize"):
            reason = tr("part_dualboot_tools")
        else:
            boot_mib = BOOT_PART_MIB if self.config["luks_enabled"] else 0
            free = win["size"] - (win["min_size"] or win["size"]) - WINDOWS_MARGIN
            max_mib = free // MIB
            if win["min_size"] is None or max_mib < MIN_WINLUX_MIB + boot_mib:
                reason = tr("part_dualboot_no_room")
            else:
                self.dualboot_target = win
                low = MIN_WINLUX_MIB + boot_mib
                previous = self.dualboot_alloc_mb
                self.dualboot_scale.set_range(low, max_mib)
                self.dualboot_scale.set_value(previous if low <= previous <= max_mib else max(low, max_mib // 2))
                self.dualboot_info.set_text(tr("part_windows_found", partition=win["partition"]))
        if reason:
            self.dualboot_info.set_text(reason)
        self.type_dualboot_rb.set_sensitive(self.dualboot_target is not None)
        if self.dualboot_target is None and self.config["install_type"] == "dualboot":
            self.type_full_rb.set_active(True)

        self.manual_combo.remove_all()
        self.manual_candidates = [p for p in parts
                                  if p["parttype"] not in (ESP_PARTTYPE, BIOS_PARTTYPE)
                                  and p["size"] >= MANUAL_MIN_SIZE and not p["mountpoint"]]
        for p in self.manual_candidates:
            desc = " · ".join(x for x in (p["fstype"] or "?", p["label"], human_size(p["size"])) if x)
            self.manual_combo.append(p["path"], f"{p['path']}  ({desc})")
        self.config["manual_root"] = None
        if self.manual_candidates:
            self.manual_combo.set_active(0)
        if not self.manual_candidates:
            self.manual_info.set_text(tr("part_manual_none"))
        elif self.hw_info.get("uefi") and not find_esp(parts):
            self.manual_info.set_text(tr("part_manual_need_esp"))
        self.refresh_layout_preview()
        self.validate_step()

    def on_manual_root_changed(self, combo):
        self.config["manual_root"] = combo.get_active_id()
        root = next((p for p in getattr(self, "manual_candidates", [])
                     if p["path"] == self.config["manual_root"]), None)
        if root:
            self.manual_info.set_text(tr("part_manual_erase", partition=root["path"]))
        self.refresh_layout_preview()
        self.validate_step()

    def manual_ready(self):
        disk = self.config.get("disk")
        if not disk or not self.config.get("manual_root"):
            return False
        return not self.hw_info.get("uefi") or find_esp(self.disk_partitions.get(disk["path"], [])) is not None

    def refresh_layout_preview(self):
        disk = self.config.get("disk")
        if not disk:
            return
        itype = self.config["install_type"]
        dualboot = None
        if itype == "dualboot" and self.dualboot_target:
            alloc = self.dualboot_alloc_mb * MIB
            new_size = (self.dualboot_target["size"] - alloc) // MIB * MIB
            dualboot = {"partition": self.dualboot_target["partition"], "new_size": new_size, "alloc": alloc}
            self.dualboot_keep_label.set_text(tr("part_windows_keep", size=human_size(new_size)))
        segments = planned_layout(disk["size"], self.disk_partitions.get(disk["path"], []), itype,
                                  self.config["luks_enabled"], dualboot, self.config.get("manual_root"))
        self.partition_bar_after.set_data(disk["size"], segments)
        if itype == "full":
            self.part_erase_warn.set_text(tr("part_erase_warn"))
        elif itype == "manual" and self.config.get("manual_root"):
            self.part_erase_warn.set_text(tr("part_manual_erase", partition=self.config["manual_root"]))
        else:
            self.part_erase_warn.set_text("")

    @staticmethod
    def _reveal(box, visible):
        # show_all() skips widgets marked no_show_all, which keeps them hidden on page build.
        if visible:
            box.set_no_show_all(False)
            box.show_all()
        else:
            box.hide()
            box.set_no_show_all(True)

    def on_type_toggle(self, btn, install_type):
        if not btn.get_active():
            return
        self.config["install_type"] = install_type
        self._reveal(self.dualboot_slider_box, install_type == "dualboot")
        if install_type == "dualboot":
            self.dualboot_alloc_mb = int(self.dualboot_scale.get_value())
        self._reveal(self.manual_box, install_type == "manual")
        if install_type == "manual" and self.config["luks_enabled"]:
            self.luks_check.set_active(False)
        self.refresh_layout_preview()
        self.validate_step()

    # ── Page 4 : Chiffrement ─────────────────────────────────────

    def build_encryption_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("encrypt_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("encrypt_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        sub.set_line_wrap(True)
        box.pack_start(sub, False, False, 0)

        info_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        info_box.get_style_context().add_class("luks-info-box")
        info_text = Gtk.Label(label=tr("encrypt_info"), xalign=0)
        info_text.get_style_context().add_class("info-text")
        info_text.set_line_wrap(True)
        info_box.pack_start(info_text, False, False, 0)
        box.pack_start(info_box, False, False, 0)

        boot_note = Gtk.Label(label=tr("encrypt_boot_note"), xalign=0)
        boot_note.get_style_context().add_class("info-text")
        boot_note.set_line_wrap(True)
        info_box.pack_start(boot_note, False, False, 0)

        self.luks_check = Gtk.CheckButton(label=tr("encrypt_enable"))
        self.luks_check.set_margin_top(16)
        self.luks_check.connect("toggled", self.on_luks_toggle)
        box.pack_start(self.luks_check, False, False, 0)
        self.luks_unavailable = Gtk.Label(label="", xalign=0)
        self.luks_unavailable.get_style_context().add_class("warning-text")
        self.luks_unavailable.set_line_wrap(True)
        box.pack_start(self.luks_unavailable, False, False, 0)
        self.luks_keyboard_note = Gtk.Label(label="", xalign=0)
        self.luks_keyboard_note.get_style_context().add_class("info-text")
        self.luks_keyboard_note.set_line_wrap(True)

        self.luks_fields_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.luks_fields_box.set_margin_top(12)
        self.luks_fields_box.set_margin_start(24)

        pp_label = Gtk.Label(label=tr("encrypt_passphrase"), xalign=0)
        pp_label.get_style_context().add_class("input-label")
        self.luks_fields_box.pack_start(pp_label, False, False, 0)
        self.luks_pp_entry = Gtk.Entry()
        self.luks_pp_entry.get_style_context().add_class("input-field")
        self.luks_pp_entry.set_visibility(False)
        self.luks_pp_entry.set_input_purpose(Gtk.InputPurpose.PASSWORD)
        self.luks_pp_entry.connect("changed", self.on_luks_pp_changed)
        self.luks_fields_box.pack_start(self.luks_pp_entry, False, False, 0)

        self.luks_strength_label = Gtk.Label(label="", xalign=0)
        self.luks_fields_box.pack_start(self.luks_strength_label, False, False, 0)

        pp_confirm_label = Gtk.Label(label=tr("encrypt_confirm"), xalign=0)
        pp_confirm_label.get_style_context().add_class("input-label")
        pp_confirm_label.set_margin_top(8)
        self.luks_fields_box.pack_start(pp_confirm_label, False, False, 0)
        self.luks_confirm_entry = Gtk.Entry()
        self.luks_confirm_entry.get_style_context().add_class("input-field")
        self.luks_confirm_entry.set_visibility(False)
        self.luks_confirm_entry.set_input_purpose(Gtk.InputPurpose.PASSWORD)
        self.luks_confirm_entry.connect("changed", self.on_luks_pp_changed)
        self.luks_fields_box.pack_start(self.luks_confirm_entry, False, False, 0)

        self.luks_error_label = Gtk.Label(label="", xalign=0)
        self.luks_error_label.get_style_context().add_class("warning-text")
        self.luks_fields_box.pack_start(self.luks_error_label, False, False, 0)
        self.luks_fields_box.pack_start(self.luks_keyboard_note, False, False, 0)

        self.luks_fields_box.set_sensitive(False)
        box.pack_start(self.luks_fields_box, False, False, 0)

        skip_info = Gtk.Label(label=tr("encrypt_skip_info"), xalign=0)
        skip_info.get_style_context().add_class("info-text")
        skip_info.set_margin_top(16)
        skip_info.set_line_wrap(True)
        box.pack_start(skip_info, False, False, 0)

        self.pages.add_named(box, "page_4")

    def refresh_encryption_page(self):
        if self.config["install_type"] == "manual":
            reason = tr("encrypt_manual")
        elif not self.luks_supported:
            reason = tr("encrypt_unsupported")
        else:
            reason = ""
        if reason and self.luks_check.get_active():
            self.luks_check.set_active(False)
        self.luks_check.set_sensitive(not reason)
        self.luks_unavailable.set_text(reason)
        layout = dict(KEYBOARDS).get(self.config["keyboard"], self.config["keyboard"])
        if keymap_tools_available():
            self.luks_keyboard_note.set_text(tr("encrypt_keyboard", layout=layout))
        else:
            self.luks_keyboard_note.set_text(tr("encrypt_keyboard_us"))

    def on_luks_toggle(self, btn):
        enabled = btn.get_active()
        self.config["luks_enabled"] = enabled
        self.luks_fields_box.set_sensitive(enabled)
        if not enabled:
            self.config["luks_passphrase"] = ""
            self.luks_pp_entry.set_text("")
            self.luks_confirm_entry.set_text("")
            self.luks_error_label.set_text("")
            self.luks_strength_label.set_text("")
        disk = self.config.get("disk")
        if disk and self.config["install_type"] == "dualboot":
            # The unencrypted /boot partition comes out of the space given to Winlux.
            self.select_disk(disk)
            self.type_dualboot_rb.set_active(self.dualboot_target is not None)
        self.refresh_layout_preview()
        self.validate_step()

    def on_luks_pp_changed(self, entry):
        pp = self.luks_pp_entry.get_text()
        pp_confirm = self.luks_confirm_entry.get_text()
        self.config["luks_passphrase"] = pp

        strength = passphrase_strength(pp)
        ctx = self.luks_strength_label.get_style_context()
        for cls in ["strength-weak", "strength-medium", "strength-strong"]:
            ctx.remove_class(cls)
        if strength == 1:
            self.luks_strength_label.set_text(tr("encrypt_strength_weak"))
            ctx.add_class("strength-weak")
        elif strength == 2:
            self.luks_strength_label.set_text(tr("encrypt_strength_medium"))
            ctx.add_class("strength-medium")
        elif strength == 3:
            self.luks_strength_label.set_text(tr("encrypt_strength_strong"))
            ctx.add_class("strength-strong")
        else:
            self.luks_strength_label.set_text("")

        if not passphrase_is_ascii(pp):
            self.luks_error_label.set_text(tr("encrypt_ascii"))
        elif pp_confirm and pp != pp_confirm:
            self.luks_error_label.set_text(tr("encrypt_mismatch"))
        else:
            self.luks_error_label.set_text("")
        self.validate_step()

    # ── Page 5 : Compte local ────────────────────────────────────

    def build_account_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("account_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("account_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        sub.set_line_wrap(True)
        box.pack_start(sub, False, False, 0)
        grid = Gtk.Grid(column_spacing=16, row_spacing=12)
        grid.set_margin_top(8)
        fields = [
            (tr("field_hostname"), "hostname", "winlux-desktop", False),
            (tr("field_fullname"), "fullname", "Winlux", False),
            (tr("field_username"), "username", "winlux", False),
            (tr("field_password"), "password", "", True),
            (tr("field_password_confirm"), "password_confirm", "", True),
        ]
        self.account_entries = {}
        for i, (label_text, key, placeholder, is_password) in enumerate(fields):
            lbl = Gtk.Label(label=label_text, xalign=1)
            lbl.get_style_context().add_class("input-label")
            grid.attach(lbl, 0, i, 1, 1)
            entry = Gtk.Entry()
            entry.get_style_context().add_class("input-field")
            entry.set_placeholder_text(placeholder)
            entry.set_hexpand(True)
            if is_password:
                entry.set_visibility(False)
                entry.set_input_purpose(Gtk.InputPurpose.PASSWORD)
            if key == "hostname":
                entry.set_text(self.config["hostname"])
            entry.connect("changed", self.on_account_changed, key)
            grid.attach(entry, 1, i, 1, 1)
            self.account_entries[key] = entry
        box.pack_start(grid, False, False, 0)
        self.account_error = Gtk.Label(label="", xalign=0)
        self.account_error.get_style_context().add_class("warning-text")
        self.account_error.set_margin_top(8)
        box.pack_start(self.account_error, False, False, 0)
        autologin = Gtk.CheckButton(label=tr("autologin"))
        autologin.set_active(True)
        autologin.connect("toggled", lambda b: self.config.update({"autologin": b.get_active()}))
        autologin.set_margin_top(8)
        box.pack_start(autologin, False, False, 0)
        self.pages.add_named(box, "page_5")

    def on_account_changed(self, entry, key):
        if key == "password_confirm":
            pass
        else:
            self.config[key] = entry.get_text()
        pw = self.account_entries.get("password")
        pw_confirm = self.account_entries.get("password_confirm")
        if pw and pw_confirm and pw.get_text() and pw_confirm.get_text():
            if pw.get_text() != pw_confirm.get_text():
                self.account_error.set_text(tr("password_mismatch"))
            else:
                self.account_error.set_text("")
        else:
            self.account_error.set_text("")
        username = self.config.get("username", "")
        if username and not re.match(r'^[a-z_][a-z0-9_-]*$', username):
            self.account_error.set_text(tr("username_invalid"))
        self.validate_step()

    # ── Page 6 : Résumé ──────────────────────────────────────────

    def build_summary_page(self):
        self.summary_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("summary_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        self.summary_box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("summary_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        self.summary_box.pack_start(sub, False, False, 0)
        self.summary_content = Gtk.Label(label="", xalign=0)
        self.summary_content.get_style_context().add_class("card")
        self.summary_content.set_line_wrap(True)
        self.summary_content.set_selectable(True)
        self.summary_box.pack_start(self.summary_content, False, False, 0)
        self.summary_warn = Gtk.Label(label="", xalign=0)
        self.summary_warn.get_style_context().add_class("disk-warn")
        self.summary_warn.set_margin_top(12)
        self.summary_box.pack_start(self.summary_warn, False, False, 0)
        self.pages.add_named(self.summary_box, "page_6")

    def summary_partition_lines(self):
        itype = self.config["install_type"]
        if itype == "dualboot" and self.dualboot_target:
            keep = (self.dualboot_target["size"] - self.dualboot_alloc_mb * MIB) // MIB * MIB
            return [f"  {tr('part_windows_keep', size=human_size(keep))}",
                    f"  Winlux : {human_size(self.dualboot_alloc_mb * MIB)}"]
        if itype == "manual" and self.config.get("manual_root"):
            return [f"  {tr('part_manual_erase', partition=self.config['manual_root'])}"]
        return []

    def refresh_summary(self):
        disk = self.config.get("disk") or {}
        lang = dict((l, n) for l, n, _ in LANGUAGES).get(self.config["language"], self.config["language"])
        itype = self.config["install_type"]
        type_labels = {"full": tr("summary_type_full"),
                       "dualboot": tr("summary_type_dualboot"),
                       "manual": tr("summary_type_manual")}
        lines = [
            f"{tr('summary_lang')} : {lang}",
            f"{tr('summary_keyboard')} : {self.config['keyboard']}",
            f"{tr('summary_type')} : {type_labels.get(itype, itype)}",
            f"{tr('summary_disk')} : {disk.get('path', '?')}  ({disk.get('model', '?')}, {human_size(disk.get('size'))})",
            *self.summary_partition_lines(),
            f"{tr('summary_encryption')} : {tr('summary_yes') if self.config['luks_enabled'] else tr('summary_no')}",
            f"{tr('summary_hostname')} : {self.config['hostname']}",
            f"{tr('summary_user')} : {self.config['fullname']} ({self.config['username']})",
            f"{tr('summary_autologin')} : {tr('summary_yes') if self.config['autologin'] else tr('summary_no')}",
        ]
        if self.hw_info:
            lines.append("")
            lines.append(f"CPU : {self.hw_info.get('cpu', '?')}")
            lines.append(f"RAM : {self.hw_info.get('ram', '?')}")
            gpus = self.hw_info.get("gpu", [])
            for i, g in enumerate(gpus):
                lines.append(f"GPU {i}: {g}")
            mode = "UEFI" if self.hw_info.get("uefi") else "Legacy BIOS"
            lines.append(f"{tr('summary_boot')} : {mode}")
        self.summary_content.set_text("\n".join(lines))
        if itype == "full":
            self.summary_warn.set_text(tr("summary_erase_warn"))
        elif itype == "dualboot":
            self.summary_warn.set_text(tr("summary_dualboot_warn"))
        else:
            self.summary_warn.set_text("")

    # ── Page 7 : Progression ─────────────────────────────────────

    def build_progress_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("installing"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        self.progress_detail = Gtk.Label(label=tr("preparing"), xalign=0)
        self.progress_detail.get_style_context().add_class("progress-text")
        self.progress_detail.set_line_wrap(True)
        box.pack_start(self.progress_detail, False, False, 0)
        self.progress_bar = Gtk.ProgressBar()
        self.progress_bar.get_style_context().add_class("progress-bar")
        self.progress_bar.set_margin_top(16)
        box.pack_start(self.progress_bar, False, False, 0)
        self.progress_percent = Gtk.Label(label="0 %", xalign=0.5)
        self.progress_percent.get_style_context().add_class("progress-text")
        box.pack_start(self.progress_percent, False, False, 0)
        self.progress_log = Gtk.TextView()
        self.progress_log.set_editable(False)
        self.progress_log.set_cursor_visible(False)
        self.progress_log.set_wrap_mode(Gtk.WrapMode.WORD)
        self.progress_log.get_style_context().add_class("hw-info")
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_min_content_height(150)
        scroll.add(self.progress_log)
        scroll.set_margin_top(16)
        box.pack_start(scroll, True, True, 0)
        self.pages.add_named(box, "page_7")

    def set_progress(self, fraction, text):
        def do():
            self.progress_bar.set_fraction(fraction)
            self.progress_percent.set_text(f"{int(fraction * 100)} %")
            self.progress_detail.set_text(text)
            buf = self.progress_log.get_buffer()
            buf.insert(buf.get_end_iter(), text + "\n")
            mark = buf.get_insert()
            self.progress_log.scroll_to_mark(mark, 0, True, 0, 1)
        GLib.idle_add(do)

    # ── Page 8 : Terminé ─────────────────────────────────────────

    def build_done_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        box.set_valign(Gtk.Align.CENTER)
        box.set_halign(Gtk.Align.CENTER)
        icon = Gtk.Label(label="✓")
        icon.get_style_context().add_class("success-icon")
        box.pack_start(icon, False, False, 0)
        self.done_title = Gtk.Label(label=tr("done_title"))
        self.done_title.get_style_context().add_class("page-title")
        box.pack_start(self.done_title, False, False, 0)
        self.done_sub = Gtk.Label(label=tr("done_subtitle"), xalign=0.5)
        self.done_sub.get_style_context().add_class("page-subtitle")
        self.done_sub.set_line_wrap(True)
        self.done_sub.set_justify(Gtk.Justification.CENTER)
        box.pack_start(self.done_sub, False, False, 0)
        btn_box = Gtk.Box(spacing=12)
        btn_box.set_halign(Gtk.Align.CENTER)
        btn_box.set_margin_top(24)
        reboot_btn = Gtk.Button(label=tr("btn_reboot"))
        reboot_btn.get_style_context().add_class("btn-primary")
        reboot_btn.connect("clicked", self.on_reboot)
        btn_box.pack_start(reboot_btn, False, False, 0)
        close_btn = Gtk.Button(label=tr("btn_close"))
        close_btn.get_style_context().add_class("btn-secondary")
        close_btn.connect("clicked", lambda _: self.quit())
        btn_box.pack_start(close_btn, False, False, 0)
        box.pack_start(btn_box, False, False, 0)
        self.pages.add_named(box, "page_8")

    def on_reboot(self, _btn):
        try:
            subprocess.run(["sync"], timeout=10)
            subprocess.run(["reboot"], timeout=10)
        except Exception:
            pass
        self.quit()

    # ── Installation ─────────────────────────────────────────────

    def start_installation(self):
        self.current_step = 7
        self.update_step()
        self.install_cancelled = False
        self.install_thread = threading.Thread(target=self.run_installation, daemon=True)
        self.install_thread.start()

    def run_installation(self):
        disk = self.config["disk"]
        if not disk:
            self.install_error("Aucun disque sélectionné.")
            return
        device = disk["path"]
        iso_path = os.environ.get("WINLUX_ISO", "")
        if not iso_path:
            candidate = os.path.join(ROOT_DIR, "build", "winlux-live-test.iso")
            if os.path.isfile(candidate):
                iso_path = candidate
        live_root = "/run/winlux/live/lower"
        if not (iso_path and os.path.isfile(iso_path)) and not os.path.isdir(live_root):
            self.install_error("ISO source introuvable. Définissez WINLUX_ISO ou démarrez depuis le Live USB.")
            return
        work = f"/tmp/winlux-gui-install.{os.getpid()}"
        target = f"{work}/target"
        os.makedirs(target, exist_ok=True)
        self._mounts = []
        self._luks_open = False
        try:
            if iso_path and os.path.isfile(iso_path):
                self.set_progress(0.02, tr("progress_check_disk", device=device))
                if self.config["install_type"] == "full":
                    result = subprocess.run(
                        ["sh", INSTALL_SH, device, "--iso", iso_path, "--check-only"],
                        capture_output=True, text=True, timeout=600
                    )
                    if result.returncode != 0:
                        raise RuntimeError(f"Vérification échouée :\n{result.stderr.strip()}")
                self.extract_boot_files(iso_path, work)
            else:
                self.copy_live_boot_files(work)
            if self.config["luks_enabled"] and not initramfs_supports_luks(f"{work}/initramfs.gz"):
                raise RuntimeError(tr("error_luks_initramfs"))
            self.set_progress(0.05, tr("progress_partitioning"))
            layout = self.prepare_disk(device)
            self.set_progress(0.12, tr("progress_formatting"))
            self.format_layout(layout)
            self.mount_target(layout, target)
            if iso_path and os.path.isfile(iso_path):
                split_root = self.copy_from_iso(iso_path, work)
            else:
                self.set_progress(0.20, tr("progress_copying"))
                subprocess.run(["cp", "-a", f"{live_root}/.", f"{target}/"], check=True, timeout=1800)
                split_root = True
            self.set_progress(0.70, tr("progress_kernel"))
            os.makedirs(f"{target}/boot", exist_ok=True)
            shutil.copyfile(f"{work}/vmlinuz", f"{target}/boot/vmlinuz")
            shutil.copyfile(f"{work}/initramfs.gz", f"{target}/boot/winlux-initramfs.cpio.gz")
            if self.config["luks_enabled"]:
                self.add_boot_keymap(f"{target}/boot/winlux-initramfs.cpio.gz")
            self.set_progress(0.75, tr("progress_grub"))
            self.install_grub(device, layout, target, split_root)
            self.set_progress(0.85, tr("progress_account"))
            self.configure_user(target)
            self.set_progress(0.90, tr("progress_firstboot"))
            self.configure_first_boot(target)
            self.set_progress(0.95, tr("progress_sync"))
            subprocess.run(["sync"], timeout=60)
        except subprocess.CalledProcessError as e:
            self.release_target()
            self.install_error(f"{tr('error_title')} : {' '.join(map(str, e.cmd[:2]))} ({e.returncode})")
            return
        except Exception as e:
            self.release_target()
            self.install_error(str(e))
            return
        self.release_target()
        self.post_install()

    def extract_boot_files(self, iso_path, work):
        for inside, outside in (("/boot/vmlinuz", "vmlinuz"),
                                ("/boot/winlux-initramfs.cpio.gz", "initramfs.gz")):
            subprocess.run(["xorriso", "-osirrox", "on", "-indev", iso_path,
                            "-extract", inside, f"{work}/{outside}"],
                           check=True, capture_output=True, timeout=60)

    def copy_live_boot_files(self, work):
        media = "/run/winlux/live/media/boot"
        for inside, outside in (("vmlinuz", "vmlinuz"), ("winlux-initramfs.cpio.gz", "initramfs.gz")):
            if not os.path.isfile(f"{media}/{inside}"):
                raise RuntimeError(f"Fichier de démarrage absent du média : {inside}")
            shutil.copyfile(f"{media}/{inside}", f"{work}/{outside}")

    def copy_from_iso(self, iso_path, work):
        """Copy the system root out of the ISO; True when the ISO uses a separate squashfs root."""
        self.set_progress(0.25, tr("progress_extracting"))
        report = subprocess.run(
            ["xorriso", "-indev", iso_path,
             "-find", "/boot/winlux-rootfs.squashfs", "-exec", "report_lba", "--"],
            capture_output=True, text=True, timeout=30
        )
        extent = ""
        for line in report.stdout.splitlines():
            if line.startswith("File data lba:"):
                fields = line.split(",")
                if len(fields) >= 2:
                    extent = fields[1].strip()
                break
        target = f"{work}/target"
        if extent.isdigit():
            # unsquashfs writes straight into the mounted target, no copy in /tmp.
            subprocess.run(["unsquashfs", "-no-progress", "-processors", "2", "-force",
                            "-offset", str(int(extent) * 2048), "-d", target, iso_path],
                           check=True, timeout=1800)
            return True
        subprocess.run(["bsdtar", "-xf", f"{work}/initramfs.gz", "-C", target, "--no-same-owner"],
                       check=True, timeout=600)
        return False

    # ── Partitionnement ─────────────────────────────────────────

    def prepare_disk(self, device):
        """Create or pick the partitions; returns the layout used by later steps."""
        install_type = self.config["install_type"]
        luks = self.config["luks_enabled"]
        if install_type == "full":
            return self.partition_full_disk(device, luks)
        if install_type == "dualboot":
            return self.partition_dualboot(device, luks)
        root = self.config.get("manual_root")
        if not root:
            raise RuntimeError("Aucune partition choisie pour Winlux.")
        esp = find_esp(self.disk_partitions.get(device, []))
        return {"esp": esp["path"] if esp else None, "format_esp": False, "boot": None,
                "root": root, "bios": not self.hw_info.get("uefi"), "own_esp": False}

    def partition_full_disk(self, device, luks):
        subprocess.run(["wipefs", "-a", device], check=True, timeout=60)
        cmds = [
            ["parted", "-s", device, "--", "mklabel", "gpt"],
            ["parted", "-s", device, "--", "mkpart", "BIOS", "1MiB", "3MiB", "set", "1", "bios_grub", "on"],
            ["parted", "-s", device, "--", "mkpart", "ESP", "fat32", "3MiB", "515MiB", "set", "2", "esp", "on"],
        ]
        if luks:
            cmds.append(["parted", "-s", device, "--", "mkpart", "WINLUX_BOOT", "ext4",
                         "515MiB", f"{515 + BOOT_PART_MIB}MiB"])
            cmds.append(["parted", "-s", device, "--", "mkpart", "WINLUX_ROOT",
                         f"{515 + BOOT_PART_MIB}MiB", "100%"])
        else:
            cmds.append(["parted", "-s", device, "--", "mkpart", "WINLUX_ROOT", "ext4", "515MiB", "100%"])
        for cmd in cmds:
            subprocess.run(cmd, check=True, timeout=30)
        reread_partitions(device)
        esp = wait_for_block(partition_path(device, 2))
        if luks:
            boot = wait_for_block(partition_path(device, 3))
            root = wait_for_block(partition_path(device, 4))
        else:
            boot = None
            root = wait_for_block(partition_path(device, 3))
        return {"esp": esp, "format_esp": True, "boot": boot, "root": root,
                "bios": True, "own_esp": True}

    def partition_dualboot(self, device, luks):
        win = self.dualboot_target
        esp = find_esp(self.disk_partitions.get(device, []))
        if not win or not esp:
            raise RuntimeError(tr("part_dualboot_need_uefi"))
        alloc = self.dualboot_alloc_mb * MIB
        parts = disk_partitions_sysfs(device)
        current = parts[win["number"]]
        new_size = (current["size"] - alloc) // MIB * MIB
        if win["min_size"] is None or new_size < win["min_size"] + WINDOWS_MARGIN:
            raise RuntimeError(tr("part_dualboot_no_room"))
        self.set_progress(0.06, tr("progress_resize"))
        check = subprocess.run(["ntfsresize", "--no-action", "--size", str(new_size), win["partition"]],
                               capture_output=True, text=True, timeout=600)
        if check.returncode != 0:
            raise RuntimeError(tr("error_ntfs_check") + "\n\n" + (check.stdout + check.stderr).strip()[-600:])
        # A single --force only skips the confirmation prompt; the dry run above
        # already refused dirty or hibernated volumes.
        subprocess.run(["ntfsresize", "--force", "--size", str(new_size), win["partition"]],
                       check=True, capture_output=True, timeout=7200)
        subprocess.run(["sfdisk", "--no-reread", "--no-tell-kernel", "-N", str(win["number"]), device],
                       input=f",{new_size // 512}\n", text=True, check=True, capture_output=True, timeout=60)
        reread_partitions(device)
        before = set(disk_partitions_sysfs(device))
        start = (current["start"] + new_size + MIB - 1) // MIB * MIB
        following = [p["start"] for p in parts.values() if p["start"] > current["start"]]
        region_end = min(following) if following else disk_size_bytes(device) - MIB
        if region_end - start < MIN_WINLUX_MIB * MIB:
            raise RuntimeError(tr("part_dualboot_no_room"))
        boot = None
        if luks:
            boot_end = start + BOOT_PART_MIB * MIB
            subprocess.run(["parted", "-s", device, "--", "unit", "B", "mkpart", "WINLUX_BOOT", "ext4",
                            f"{start}B", f"{boot_end - 1}B"], check=True, timeout=30)
            start = boot_end
        subprocess.run(["parted", "-s", device, "--", "unit", "B", "mkpart", "WINLUX_ROOT", "ext4",
                        f"{start}B", f"{region_end - 1}B"], check=True, timeout=30)
        reread_partitions(device)
        created = sorted(set(disk_partitions_sysfs(device)) - before,
                         key=lambda n: disk_partitions_sysfs(device)[n]["start"])
        if len(created) != (2 if luks else 1):
            raise RuntimeError("Partitions Winlux introuvables après le redimensionnement.")
        paths = [wait_for_block(partition_path(device, n)) for n in created]
        if luks:
            boot = paths[0]
        return {"esp": esp["path"], "format_esp": False, "boot": boot, "root": paths[-1],
                "bios": False, "own_esp": False}

    def format_layout(self, layout):
        if layout["format_esp"]:
            subprocess.run(["mkfs.fat", "-F32", "-n", "EFI", layout["esp"]], check=True, timeout=60)
        if layout["boot"]:
            subprocess.run(["mkfs.ext4", "-F", "-L", "WINLUX_BOOT", layout["boot"]], check=True, timeout=120)
        layout["root_fs"] = layout["root"]
        if self.config["luks_enabled"]:
            self.set_progress(0.14, tr("progress_luks_setup"))
            fd, keyfile = tempfile.mkstemp(prefix="winlux-luks.", dir="/run" if os.access("/run", os.W_OK) else None)
            try:
                os.write(fd, self.config["luks_passphrase"].encode())
                os.close(fd)
                # Bounded Argon2 memory so the early-boot unlock also works on small-RAM machines.
                subprocess.run(["cryptsetup", "luksFormat", "--batch-mode", "--type", "luks2",
                                "--pbkdf", "argon2id", "--pbkdf-memory", "524288",
                                "--label", "WINLUX_CRYPT", "--key-file", keyfile, layout["root"]],
                               check=True, timeout=300)
                subprocess.run(["cryptsetup", "open", "--key-file", keyfile, layout["root"], LUKS_NAME],
                               check=True, timeout=120)
            finally:
                os.unlink(keyfile)
            self._luks_open = True
            layout["root_fs"] = f"/dev/mapper/{LUKS_NAME}"
        subprocess.run(["mkfs.ext4", "-F", "-L", "WINLUX_ROOT", layout["root_fs"]], check=True, timeout=600)

    def mount_target(self, layout, target):
        subprocess.run(["mount", layout["root_fs"], target], check=True, timeout=30)
        self._mounts.append(target)
        if layout["boot"]:
            os.makedirs(f"{target}/boot", exist_ok=True)
            subprocess.run(["mount", layout["boot"], f"{target}/boot"], check=True, timeout=30)
            self._mounts.append(f"{target}/boot")
            # GRUB and winlux-update both address files as /boot/... on this partition.
            os.symlink(".", f"{target}/boot/boot")

    def release_target(self):
        for mountpoint in reversed(getattr(self, "_mounts", [])):
            subprocess.run(["umount", mountpoint], timeout=60)
        self._mounts = []
        if getattr(self, "_luks_open", False):
            subprocess.run(["cryptsetup", "close", LUKS_NAME], timeout=60)
            self._luks_open = False

    def add_boot_keymap(self, initramfs):
        """Append the chosen keyboard layout so the passphrase prompt matches the keyboard."""
        kmap = console_keymap(self.config["keyboard"])
        if kmap is None:
            self.set_progress(0.72, tr("encrypt_keyboard_us"))
            return
        archive = cpio_newc([("etc", 0o040755, b""), ("etc/boot.kmap", 0o100644, kmap)])
        with open(initramfs, "ab") as f:
            f.write(gzip.compress(archive))

    # ── GRUB ────────────────────────────────────────────────────

    def install_grub(self, device, layout, target, split_root):
        luks = self.config["luks_enabled"]
        root_fs = layout["root_fs"]
        if luks:
            if not split_root:
                raise RuntimeError(tr("error_luks_initramfs"))
            luks_uuid = blkid_value(layout["root"], "UUID")
            boot_uuid = blkid_value(layout["boot"], "UUID")
            search_uuid = boot_uuid
            root_arg = f"/dev/mapper/{LUKS_NAME}"
            extra = f" winlux.luks=UUID={luks_uuid} winlux.boot=UUID={boot_uuid}"
        else:
            search_uuid = blkid_value(root_fs, "UUID")
            root_arg = f"UUID={search_uuid}" if split_root else f"PARTUUID={blkid_value(root_fs, 'PARTUUID')}"
            extra = ""
        initrd = "initrd /boot/winlux-initramfs.cpio.gz" if split_root else ""
        efi_dir = f"{target}/boot/efi"
        windows_entry = ""
        if layout["esp"]:
            os.makedirs(efi_dir, exist_ok=True)
            subprocess.run(["mount", "-t", "vfat", "-o", "iocharset=ascii", layout["esp"], efi_dir],
                           check=True, timeout=30)
            self._mounts.append(efi_dir)
            if os.path.isfile(f"{efi_dir}/EFI/Microsoft/Boot/bootmgfw.efi"):
                esp_uuid = blkid_value(layout["esp"], "UUID")
                windows_entry = f"""menuentry "Windows" --hotkey=w {{
    insmod part_gpt
    insmod fat
    insmod chain
    search --no-floppy --fs-uuid {esp_uuid} --set=root
    chainloader /EFI/Microsoft/Boot/bootmgfw.efi
}}
"""
        grub_cfg = f"""set default=0
set timeout=8
serial --unit=0 --speed=115200
terminal_input console serial
terminal_output console serial
menuentry "Winlux Desktop" {{
    search --no-floppy --fs-uuid {search_uuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 winlux.interactive=1{extra}
    {initrd}
}}
{windows_entry}menuentry "Winlux diagnostic" --hotkey=d {{
    search --no-floppy --fs-uuid {search_uuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 init=/init console=ttyS0,115200n8{extra}
    {initrd}
}}
menuentry "Winlux secours (restauration ou console)" --hotkey=s {{
    search --no-floppy --fs-uuid {search_uuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 console=tty1 console=ttyS0,115200n8 winlux.recovery=1{extra}
    initrd /boot/winlux-initramfs.cpio.gz
}}
"""
        grub_path = f"{target}/boot/grub/grub.cfg"
        os.makedirs(os.path.dirname(grub_path), exist_ok=True)
        with open(grub_path, "w") as f:
            f.write(grub_cfg)
        subprocess.run(["grub-script-check", grub_path], check=True, timeout=10)
        if layout["bios"]:
            subprocess.run(["grub-install", "--target=i386-pc",
                            f"--boot-directory={target}/boot", "--recheck", device],
                           check=True, timeout=120)
        if layout["esp"]:
            if layout["own_esp"]:
                efi_args = ["--removable", "--no-nvram"]
            else:
                # Shared with Windows: own directory and firmware entry, never EFI/BOOT.
                efi_args = ["--bootloader-id=winlux"]
            subprocess.run(["grub-install", "--target=x86_64-efi", f"--efi-directory={efi_dir}",
                            f"--boot-directory={target}/boot", "--recheck", *efi_args],
                           check=True, timeout=120)
        if split_root:
            with open(f"{target}/boot/winlux-split-root", "w") as f:
                f.write("1\n")

    def configure_user(self, root_path):
        username = self.config["username"]
        fullname = self.config["fullname"]
        password = self.config["password"]
        hostname = self.config["hostname"]
        if not username:
            return
        etc = f"{root_path}/etc"
        os.makedirs(etc, exist_ok=True)
        with open(f"{etc}/hostname", "w") as f:
            f.write(f"{hostname}\n")
        hosts_path = f"{etc}/hosts"
        with open(hosts_path, "w") as f:
            f.write(f"127.0.0.1\tlocalhost\n127.0.1.1\t{hostname}\n")
            f.write("::1\t\tlocalhost ip6-localhost ip6-loopback\n")
        home = f"{root_path}/home/{username}"
        os.makedirs(home, exist_ok=True)
        passwd_path = f"{etc}/passwd"
        shadow_path = f"{etc}/shadow"
        group_path = f"{etc}/group"
        uid = 1000
        gid = 1000
        passwd_line = f"{username}:x:{uid}:{gid}:{fullname}:/home/{username}:/bin/bash\n"
        groups_to_join = ["sudo", "audio", "video", "plugdev", "netdev", "bluetooth",
                          "input", "dialout", "cdrom", "floppy"]
        if os.path.exists(passwd_path):
            with open(passwd_path, "a") as f:
                f.write(passwd_line)
        else:
            with open(passwd_path, "w") as f:
                f.write("root:x:0:0:root:/root:/bin/bash\n")
                f.write(passwd_line)
        if not os.path.exists(group_path):
            with open(group_path, "w") as f:
                f.write(f"root:x:0:\n{username}:x:{gid}:\n")
                for grp in groups_to_join:
                    f.write(f"{grp}:x::{username}\n")
        else:
            with open(group_path, "r") as f:
                lines = f.readlines()
            with open(group_path, "w") as f:
                user_group_found = False
                for line in lines:
                    parts = line.strip().split(":")
                    if len(parts) >= 4 and parts[0] in groups_to_join:
                        members = parts[3].split(",") if parts[3] else []
                        if username not in members:
                            members.append(username)
                        parts[3] = ",".join(members)
                        f.write(":".join(parts) + "\n")
                    else:
                        f.write(line)
                        if len(parts) >= 1 and parts[0] == username:
                            user_group_found = True
                if not user_group_found:
                    f.write(f"{username}:x:{gid}:\n")
        if password:
            try:
                hashed = subprocess.check_output(
                    ["openssl", "passwd", "-6", password],
                    text=True, timeout=10
                ).strip()
            except Exception:
                hashed = "!"
            if os.path.exists(shadow_path):
                with open(shadow_path, "a") as f:
                    f.write(f"{username}:{hashed}:19000:0:99999:7:::\n")
            else:
                with open(shadow_path, "w") as f:
                    f.write(f"root:!:19000:0:99999:7:::\n")
                    f.write(f"{username}:{hashed}:19000:0:99999:7:::\n")
                os.chmod(shadow_path, 0o640)
        for d in [".config", ".local/share", "Bureau", "Documents",
                  "Téléchargements", "Images", "Musique", "Vidéos"]:
            os.makedirs(f"{home}/{d}", exist_ok=True)
        subprocess.run(["chown", "-R", f"{uid}:{gid}", home], timeout=30)
        if os.path.exists(f"{root_path}/etc/sudoers.d"):
            sudoers_file = f"{root_path}/etc/sudoers.d/{username}"
            with open(sudoers_file, "w") as f:
                f.write(f"{username} ALL=(ALL:ALL) ALL\n")
            os.chmod(sudoers_file, 0o440)
        else:
            os.makedirs(f"{root_path}/etc/sudoers.d", exist_ok=True)
            sudoers_file = f"{root_path}/etc/sudoers.d/{username}"
            with open(sudoers_file, "w") as f:
                f.write(f"{username} ALL=(ALL:ALL) ALL\n")
            os.chmod(sudoers_file, 0o440)

    def configure_first_boot(self, root_path):
        etc = f"{root_path}/etc/winlux"
        os.makedirs(etc, exist_ok=True)
        locale = self.config["language"] + ".UTF-8"
        keyboard = self.config["keyboard"]
        locale_conf = f"{root_path}/etc/default/locale"
        os.makedirs(os.path.dirname(locale_conf), exist_ok=True)
        with open(locale_conf, "w") as f:
            f.write(f'LANG={locale}\nLANGUAGE={self.config["language"]}:en\nLC_ALL={locale}\n')
        kb_conf = f"{root_path}/etc/default/keyboard"
        with open(kb_conf, "w") as f:
            f.write(f'XKBMODEL="pc105"\nXKBLAYOUT="{keyboard}"\nXKBVARIANT=""\nXKBOPTIONS=""\n')
        locale_gen = f"{root_path}/etc/locale.gen"
        if os.path.exists(locale_gen):
            with open(locale_gen, "a") as f:
                f.write(f"\n{locale} UTF-8\n")
        else:
            with open(locale_gen, "w") as f:
                f.write(f"{locale} UTF-8\nen_US.UTF-8 UTF-8\n")
        config = {
            "language": self.config["language"],
            "keyboard": self.config["keyboard"],
            "hostname": self.config["hostname"],
            "username": self.config["username"],
            "fullname": self.config["fullname"],
            "autologin": self.config["autologin"],
            "luks_enabled": self.config["luks_enabled"],
            "install_type": self.config["install_type"],
            "install_date": time.strftime("%Y-%m-%d %H:%M:%S"),
        }
        with open(f"{etc}/install-config.json", "w") as f:
            json.dump(config, f, ensure_ascii=False, indent=2)
        fb_script = os.path.join(SCRIPT_DIR, "winlux-first-boot.sh")
        if os.path.exists(fb_script):
            dest = f"{root_path}/usr/bin/winlux-first-boot"
            subprocess.run(["cp", fb_script, dest], timeout=10)
            os.chmod(dest, 0o755)
        hw_script = os.path.join(SCRIPT_DIR, "winlux-hw-detect.sh")
        if os.path.exists(hw_script):
            dest = f"{root_path}/usr/bin/winlux-hw-detect"
            subprocess.run(["cp", hw_script, dest], timeout=10)
            os.chmod(dest, 0o755)

    def post_install(self):
        self.set_progress(1.0, tr("progress_done"))
        GLib.idle_add(self.show_done)

    def show_done(self):
        self.current_step = 8
        self.update_step()

    def install_error(self, message):
        def do():
            self.done_title.set_text(tr("error_title"))
            self.done_sub.set_text(message)
            self.current_step = 8
            self.update_step()
        GLib.idle_add(do)


def main():
    if os.geteuid() != 0:
        print("ERREUR : l'installateur doit être lancé en tant que root (sudo).", file=sys.stderr)
        sys.exit(1)
    signal.signal(signal.SIGINT, signal.SIG_DFL)
    app = InstallerApp()
    app.run(sys.argv)


if __name__ == "__main__":
    main()
