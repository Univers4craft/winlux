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
import json
import math
import os
import re
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
    "crypto_LUKS": (0.75, 0.2, 0.2, 1.0),
    "free": (0.35, 0.35, 0.45, 1.0),
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


def detect_partitions(device):
    """Detect existing partitions on a device using lsblk."""
    parts = []
    try:
        out = subprocess.check_output(
            ["lsblk", "-bnJo", "NAME,SIZE,FSTYPE,LABEL,MOUNTPOINT,TYPE,PARTUUID",
             device],
            text=True, timeout=10
        )
        data = json.loads(out)
        for bd in data.get("blockdevices", []):
            children = bd.get("children", [])
            if not children:
                continue
            for child in children:
                if child.get("type") not in ("part", "partition"):
                    continue
                parts.append({
                    "name": child.get("name", ""),
                    "path": f"/dev/{child['name']}",
                    "size": child.get("size", 0) or 0,
                    "fstype": (child.get("fstype") or "").strip(),
                    "label": (child.get("label") or "").strip(),
                    "mountpoint": (child.get("mountpoint") or "").strip(),
                })
    except Exception:
        pass
    return parts


def detect_existing_os(partitions):
    """Check if Windows is installed on any NTFS partition.
    Returns a list of dicts: {partition, os_name, size}"""
    found = []
    for p in partitions:
        if p["fstype"].lower() == "ntfs":
            label = p["label"].lower()
            if "windows" in label or "system" in label or "os" in label:
                found.append({
                    "partition": p["path"],
                    "os_name": "Windows",
                    "size": p["size"],
                    "label": p["label"],
                })
            elif p["size"] > 30 * 1024**3:
                found.append({
                    "partition": p["path"],
                    "os_name": "Windows (probable)",
                    "size": p["size"],
                    "label": p["label"] or "NTFS",
                })
    return found


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
    """Custom widget drawing a graphical partition bar."""

    def __init__(self):
        super().__init__()
        self.partitions = []
        self.disk_size = 0
        self.set_size_request(-1, 80)
        self.connect("draw", self.on_draw)

    def set_data(self, disk_size, partitions):
        self.disk_size = disk_size
        self.partitions = partitions
        self.queue_draw()

    def on_draw(self, widget, cr):
        alloc = widget.get_allocation()
        w = alloc.width
        h = alloc.height
        bar_y = 8
        bar_h = 36
        label_y = bar_y + bar_h + 4

        cr.set_source_rgba(0.18, 0.18, 0.27, 1.0)
        self._rounded_rect(cr, 0, bar_y, w, bar_h, 6)
        cr.fill()

        if self.disk_size <= 0:
            cr.set_source_rgba(0.7, 0.7, 0.8, 0.7)
            cr.select_font_face("sans-serif", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
            cr.set_font_size(11)
            ext = cr.text_extents(tr("part_no_disk"))
            cr.move_to((w - ext.width) / 2, bar_y + bar_h / 2 + ext.height / 2)
            cr.show_text(tr("part_no_disk"))
            return

        used = sum(p.get("size", 0) for p in self.partitions)
        free = max(0, self.disk_size - used)
        entries = list(self.partitions)
        if free > 0:
            entries.append({"name": "free", "size": free, "fstype": "free",
                            "label": tr("part_free_space")})

        x = 1.0
        usable_w = w - 2.0
        cr.save()
        self._rounded_rect(cr, 0, bar_y, w, bar_h, 6)
        cr.clip()

        cr.select_font_face("sans-serif", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        cr.set_font_size(10)

        for i, part in enumerate(entries):
            frac = part["size"] / self.disk_size if self.disk_size else 0
            pw = max(usable_w * frac, 2)
            if i == len(entries) - 1:
                pw = w - 1.0 - x

            fstype = part.get("fstype", "").lower()
            color = PART_COLORS.get(fstype, (0.35, 0.35, 0.45, 1.0))
            cr.set_source_rgba(*color)
            cr.rectangle(x, bar_y, pw, bar_h)
            cr.fill()

            if pw > 3:
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.25)
                cr.move_to(x + pw, bar_y)
                cr.line_to(x + pw, bar_y + bar_h)
                cr.set_line_width(1)
                cr.stroke()

            label = part.get("label") or part.get("name", "")
            size_str = human_size(part["size"])
            display = f"{label} ({size_str})" if label else size_str
            if pw > 60:
                cr.set_source_rgba(1, 1, 1, 0.95)
                ext = cr.text_extents(display)
                tx = x + (pw - ext.width) / 2
                ty = bar_y + bar_h / 2 + ext.height / 2
                if ext.width < pw - 8:
                    cr.move_to(tx, ty)
                    cr.show_text(display)
                else:
                    short = size_str
                    ext2 = cr.text_extents(short)
                    if ext2.width < pw - 8:
                        cr.move_to(x + (pw - ext2.width) / 2, ty)
                        cr.show_text(short)

            cr.set_source_rgba(1, 1, 1, 0.7)
            cr.set_font_size(9)
            if pw > 40:
                fs_label = part.get("fstype", "") or ""
                ext3 = cr.text_extents(fs_label)
                if ext3.width < pw - 4:
                    cr.move_to(x + (pw - ext3.width) / 2, label_y + 12)
                    cr.show_text(fs_label)
            cr.set_font_size(10)

            x += pw

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
        GLib.idle_add(self.refresh_partition_page)

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
            ok = self.config["disk"] is not None
        elif self.current_step == 4:
            if self.config["luks_enabled"]:
                pp = self.config["luks_passphrase"]
                pp_confirm = self.luks_confirm_entry.get_text() if hasattr(self, 'luks_confirm_entry') else ""
                ok = len(pp) >= 8 and pp == pp_confirm
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

    # ── Page 1 : Clavier ─────────────────────────────────────────

    def build_keyboard_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Choisissez la disposition du clavier", xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label="Sélectionnez le type de clavier que vous utilisez.", xalign=0)
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
            if layout == "fr":
                rb.set_active(True)
            rb.connect("toggled", self.on_kb_toggle, layout)
            row.pack_start(rb, True, True, 0)
            kb_box.pack_start(row, False, False, 0)
        scroll.add(kb_box)
        box.pack_start(scroll, True, True, 0)
        test_label = Gtk.Label(label="Testez votre clavier ici :", xalign=0)
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
        title = Gtk.Label(label="Contrat de licence", xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label="Veuillez lire et accepter les termes de la licence.", xalign=0)
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
        accept = Gtk.CheckButton(label="J'accepte les termes de la licence GPL v3")
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

        self.part_erase_warn = Gtk.Label(label="", xalign=0)
        self.part_erase_warn.get_style_context().add_class("disk-warn")
        self.part_erase_warn.set_margin_top(4)
        box.pack_start(self.part_erase_warn, False, False, 0)

        type_label = Gtk.Label(label=tr("summary_type") + " :", xalign=0)
        type_label.get_style_context().add_class("section-label")
        type_label.set_margin_top(8)
        box.pack_start(type_label, False, False, 0)

        full_card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        full_card.get_style_context().add_class("card")
        self.type_full_rb = Gtk.RadioButton.new_with_label(None, tr("part_type_full"))
        self.type_full_rb.connect("toggled", self.on_type_toggle, "full")
        full_card.pack_start(self.type_full_rb, False, False, 0)
        full_desc = Gtk.Label(label=tr("part_type_full_desc"), xalign=0)
        full_desc.get_style_context().add_class("card-detail")
        full_desc.set_line_wrap(True)
        full_desc.set_margin_start(24)
        full_card.pack_start(full_desc, False, False, 0)
        box.pack_start(full_card, False, False, 0)

        dualboot_card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        dualboot_card.get_style_context().add_class("card")
        self.type_dualboot_rb = Gtk.RadioButton.new_with_label_from_widget(
            self.type_full_rb, tr("part_type_dualboot"))
        self.type_dualboot_rb.connect("toggled", self.on_type_toggle, "dualboot")
        dualboot_card.pack_start(self.type_dualboot_rb, False, False, 0)
        dualboot_desc = Gtk.Label(label=tr("part_type_dualboot_desc"), xalign=0)
        dualboot_desc.get_style_context().add_class("card-detail")
        dualboot_desc.set_line_wrap(True)
        dualboot_desc.set_margin_start(24)
        dualboot_card.pack_start(dualboot_desc, False, False, 0)
        self.dualboot_info = Gtk.Label(label="", xalign=0)
        self.dualboot_info.get_style_context().add_class("info-text")
        self.dualboot_info.set_margin_start(24)
        dualboot_card.pack_start(self.dualboot_info, False, False, 0)
        self.dualboot_slider_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.dualboot_slider_box.set_margin_start(24)
        self.dualboot_slider_box.set_margin_top(8)
        slider_label = Gtk.Label(label=tr("part_dualboot_slider"), xalign=0)
        slider_label.get_style_context().add_class("card-detail")
        self.dualboot_slider_box.pack_start(slider_label, False, False, 0)
        self.dualboot_scale = Gtk.Scale.new_with_range(
            Gtk.Orientation.HORIZONTAL, 20000, 100000, 1000)
        self.dualboot_scale.set_draw_value(True)
        self.dualboot_scale.set_value_pos(Gtk.PositionType.RIGHT)
        self.dualboot_scale.connect("format-value", self._format_scale_mb)
        self.dualboot_scale.connect("value-changed", self._on_dualboot_scale)
        self.dualboot_slider_box.pack_start(self.dualboot_scale, False, False, 0)
        self.dualboot_slider_box.set_no_show_all(True)
        dualboot_card.pack_start(self.dualboot_slider_box, False, False, 0)
        box.pack_start(dualboot_card, False, False, 0)

        manual_card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        manual_card.get_style_context().add_class("card")
        self.type_manual_rb = Gtk.RadioButton.new_with_label_from_widget(
            self.type_full_rb, tr("part_type_manual"))
        self.type_manual_rb.connect("toggled", self.on_type_toggle, "manual")
        manual_card.pack_start(self.type_manual_rb, False, False, 0)
        manual_desc = Gtk.Label(label=tr("part_type_manual_desc"), xalign=0)
        manual_desc.get_style_context().add_class("card-detail")
        manual_desc.set_line_wrap(True)
        manual_desc.set_margin_start(24)
        manual_card.pack_start(manual_desc, False, False, 0)
        self.type_manual_rb.set_sensitive(False)
        box.pack_start(manual_card, False, False, 0)

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
        return human_size(value * 1024 * 1024)

    def _on_dualboot_scale(self, scale):
        self.dualboot_alloc_mb = int(scale.get_value())

    def refresh_partition_page(self):
        for child in self.part_disk_list.get_children():
            child.destroy()
        if not self.disks:
            lbl = Gtk.Label(label=tr("part_no_disk"))
            lbl.get_style_context().add_class("card-detail")
            self.part_disk_list.pack_start(lbl, False, False, 0)

        self.disk_group = None
        for disk in self.disks:
            row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
            row.get_style_context().add_class("card")
            rb = Gtk.RadioButton.new_with_label_from_widget(
                self.disk_group,
                f"{disk['path']}  —  {disk['model']}"
            )
            if self.disk_group is None:
                self.disk_group = rb
            rb.connect("toggled", self.on_disk_toggle, disk)
            row.pack_start(rb, True, True, 0)
            details = Gtk.Label(label=f"{disk['type']}  ·  {human_size(disk['size'])}")
            details.get_style_context().add_class("card-detail")
            row.pack_end(details, False, False, 8)
            self.part_disk_list.pack_start(row, False, False, 0)

        lines = []
        if self.hw_info:
            lines.append(f"CPU : {self.hw_info.get('cpu', '?')}")
            lines.append(f"RAM : {self.hw_info.get('ram', '?')}")
            gpus = self.hw_info.get("gpu", [])
            if gpus:
                for i, g in enumerate(gpus):
                    lines.append(f"GPU {i}: {g}")
            else:
                lines.append("GPU : non détecté")
            mode = "UEFI" if self.hw_info.get("uefi") else "Legacy BIOS"
            sb = " + Secure Boot" if self.hw_info.get("secureboot") else ""
            lines.append(f"Démarrage : {mode}{sb}")
        self.hw_label.set_text("\n".join(lines) if lines else tr("hw_detecting"))
        self.part_disk_list.show_all()
        self.validate_step()

    def on_disk_toggle(self, btn, disk):
        if btn.get_active():
            self.config["disk"] = disk
            parts = self.disk_partitions.get(disk["path"], [])
            self.partition_bar.set_data(disk["size"], parts)
            os_list = self.detected_os.get(disk["path"], [])
            if os_list:
                names = ", ".join(o["os_name"] for o in os_list)
                self.dualboot_info.set_text(
                    tr("part_windows_found", partition=os_list[0]["partition"]))
                self.type_dualboot_rb.set_sensitive(True)
                max_mb = int(disk["size"] / (1024 * 1024)) - 2048
                min_mb = 20000
                if max_mb > min_mb:
                    self.dualboot_scale.set_range(min_mb, max_mb)
                    self.dualboot_scale.set_value(max_mb // 2)
            else:
                self.dualboot_info.set_text(tr("part_dualboot_no_os"))
                self.type_dualboot_rb.set_sensitive(False)
                if self.config["install_type"] == "dualboot":
                    self.type_full_rb.set_active(True)
                    self.config["install_type"] = "full"
            if self.config["install_type"] == "full":
                self.part_erase_warn.set_text(tr("part_erase_warn"))
            self.validate_step()

    def on_type_toggle(self, btn, install_type):
        if btn.get_active():
            self.config["install_type"] = install_type
            if install_type == "full":
                self.part_erase_warn.set_text(tr("part_erase_warn"))
                self.dualboot_slider_box.set_visible(False)
            elif install_type == "dualboot":
                self.part_erase_warn.set_text("")
                self.dualboot_slider_box.set_visible(True)
                self.dualboot_slider_box.show_all()
            elif install_type == "manual":
                self.part_erase_warn.set_text("")
                self.dualboot_slider_box.set_visible(False)

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

        self.luks_check = Gtk.CheckButton(label=tr("encrypt_enable"))
        self.luks_check.set_margin_top(16)
        self.luks_check.connect("toggled", self.on_luks_toggle)
        box.pack_start(self.luks_check, False, False, 0)

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

        self.luks_fields_box.set_sensitive(False)
        box.pack_start(self.luks_fields_box, False, False, 0)

        skip_info = Gtk.Label(label=tr("encrypt_skip_info"), xalign=0)
        skip_info.get_style_context().add_class("info-text")
        skip_info.set_margin_top(16)
        skip_info.set_line_wrap(True)
        box.pack_start(skip_info, False, False, 0)

        self.pages.add_named(box, "page_4")

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

        if pp_confirm and pp != pp_confirm:
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
            ("Nom de l'ordinateur :", "hostname", "winlux-desktop", False),
            ("Nom complet :", "fullname", "Utilisateur Winlux", False),
            ("Nom d'utilisateur :", "username", "winlux", False),
            ("Mot de passe :", "password", "", True),
            ("Confirmer le mot de passe :", "password_confirm", "", True),
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
            entry.connect("changed", self.on_account_changed, key)
            if key == "hostname":
                entry.set_text("winlux-desktop")
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
            candidates = [
                os.path.join(ROOT_DIR, "build", "winlux-live-test.iso"),
                "/run/winlux/live/media",
            ]
            for c in candidates:
                if os.path.exists(c):
                    iso_path = c
                    break
        if not iso_path or not os.path.exists(iso_path):
            live_squashfs = "/run/winlux/live/lower"
            if os.path.isdir(live_squashfs):
                self.install_from_live(device, live_squashfs)
                return
            self.install_error("ISO source introuvable. Définissez WINLUX_ISO ou démarrez depuis le Live USB.")
            return
        self.install_from_iso(device, iso_path)

    def install_from_iso(self, device, iso_path):
        self.set_progress(0.02, tr("progress_check_disk", device=device))
        try:
            result = subprocess.run(
                ["sh", INSTALL_SH, device, "--iso", iso_path, "--check-only"],
                capture_output=True, text=True, timeout=30
            )
            if result.returncode != 0:
                self.install_error(f"Vérification échouée :\n{result.stderr.strip()}")
                return
        except Exception as e:
            self.install_error(f"Erreur de vérification : {e}")
            return
        self.set_progress(0.05, tr("progress_partitioning"))
        try:
            self.run_partitioning(device)
        except Exception as e:
            self.install_error(f"Erreur de partitionnement : {e}")
            return
        self.set_progress(0.10, tr("progress_formatting"))
        try:
            esp, root_part = self.format_partitions(device)
        except Exception as e:
            self.install_error(f"Erreur de formatage : {e}")
            return
        self.set_progress(0.15, tr("progress_extracting"))
        work = f"/tmp/winlux-gui-install.{os.getpid()}"
        os.makedirs(f"{work}/target", exist_ok=True)
        os.makedirs(f"{work}/source", exist_ok=True)
        try:
            subprocess.run(["mount", root_part, f"{work}/target"], check=True, timeout=30)
        except Exception as e:
            self.install_error(f"Montage échoué : {e}")
            return
        try:
            self.extract_and_copy(iso_path, work, device, esp, root_part)
        except Exception as e:
            self.install_error(f"Erreur d'installation : {e}")
            subprocess.run(["umount", f"{work}/target"], timeout=10)
            return
        subprocess.run(["umount", f"{work}/target"], timeout=10)
        self.cleanup_luks()
        self.post_install()

    def install_from_live(self, device, live_root):
        self.set_progress(0.05, tr("progress_partitioning"))
        try:
            self.run_partitioning(device)
        except Exception as e:
            self.install_error(f"Erreur de partitionnement : {e}")
            return
        self.set_progress(0.10, tr("progress_formatting"))
        try:
            esp, root_part = self.format_partitions(device)
        except Exception as e:
            self.install_error(f"Erreur de formatage : {e}")
            return
        self.set_progress(0.15, "Montage de la partition cible...")
        work = f"/tmp/winlux-gui-install.{os.getpid()}"
        os.makedirs(f"{work}/target", exist_ok=True)
        try:
            subprocess.run(["mount", root_part, f"{work}/target"], check=True, timeout=30)
        except Exception as e:
            self.install_error(f"Montage échoué : {e}")
            return
        self.set_progress(0.20, tr("progress_copying"))
        try:
            subprocess.run(
                ["cp", "-a", f"{live_root}/.", f"{work}/target/"],
                check=True, timeout=1800
            )
        except Exception as e:
            self.install_error(f"Copie échouée : {e}")
            subprocess.run(["umount", f"{work}/target"], timeout=10)
            return
        self.set_progress(0.70, tr("progress_kernel"))
        boot_dir = f"{work}/target/boot"
        os.makedirs(boot_dir, exist_ok=True)
        for src in ["/run/winlux/live/media/boot/vmlinuz"]:
            if os.path.exists(src):
                subprocess.run(["cp", src, f"{boot_dir}/vmlinuz"], timeout=30)
                break
        for src in ["/run/winlux/live/media/boot/winlux-initramfs.cpio.gz"]:
            if os.path.exists(src):
                subprocess.run(["cp", src, f"{boot_dir}/winlux-initramfs.cpio.gz"], timeout=30)
                break
        self.set_progress(0.75, tr("progress_grub"))
        try:
            self.install_grub(device, esp, root_part, work)
        except Exception as e:
            self.install_error(f"Erreur GRUB : {e}")
            subprocess.run(["umount", f"{work}/target"], timeout=10)
            return
        self.set_progress(0.85, tr("progress_account"))
        try:
            self.configure_user(f"{work}/target")
        except Exception as e:
            self.install_error(f"Erreur configuration utilisateur : {e}")
        self.set_progress(0.90, tr("progress_firstboot"))
        try:
            self.configure_first_boot(f"{work}/target")
        except Exception as e:
            self.set_progress(0.90, f"Avertissement premier démarrage : {e}")
        self.set_progress(0.95, tr("progress_sync"))
        subprocess.run(["sync"], timeout=30)
        subprocess.run(["umount", f"{work}/target"], timeout=30)
        self.cleanup_luks()
        self.post_install()

    def run_partitioning(self, device):
        if self.config["install_type"] == "dualboot":
            self.run_dualboot_partitioning(device)
            return
        cmds = [
            ["parted", "-s", device, "--", "mklabel", "gpt"],
            ["parted", "-s", device, "--", "mkpart", "BIOS", "1MiB", "3MiB", "set", "1", "bios_grub", "on"],
            ["parted", "-s", device, "--", "mkpart", "ESP", "fat32", "3MiB", "515MiB", "set", "2", "esp", "on"],
            ["parted", "-s", device, "--", "mkpart", "WINLUX_ROOT", "ext4", "515MiB", "100%"],
            ["partprobe", device],
        ]
        for cmd in cmds:
            subprocess.run(cmd, check=True, timeout=30)
        time.sleep(1)

    def run_dualboot_partitioning(self, device):
        alloc_mb = self.dualboot_alloc_mb
        if alloc_mb < 20000:
            alloc_mb = 20000
        os_list = self.detected_os.get(device, [])
        if not os_list:
            raise RuntimeError("Pas de système existant trouvé pour dual-boot")
        target_part = os_list[0]["partition"]
        try:
            subprocess.run(
                ["ntfsresize", "--force", "--no-action",
                 "--size", f"{int((os_list[0]['size'] / (1024*1024)) - alloc_mb)}M",
                 target_part],
                check=True, capture_output=True, timeout=60
            )
        except Exception:
            pass
        new_size_mb = int(os_list[0]["size"] / (1024 * 1024)) - alloc_mb
        if new_size_mb < 10000:
            raise RuntimeError("Pas assez d'espace pour redimensionner la partition existante")
        subprocess.run(
            ["ntfsresize", "--force",
             "--size", f"{new_size_mb}M", target_part],
            check=True, timeout=600
        )
        part_num = re.search(r'(\d+)$', target_part)
        if not part_num:
            raise RuntimeError("Impossible de déterminer le numéro de partition")
        pnum = part_num.group(1)
        subprocess.run(
            ["parted", "-s", device, "--", "resizepart", pnum, f"{new_size_mb}MiB"],
            check=True, timeout=30
        )
        end_pos = f"{new_size_mb}MiB"
        esp_end = f"{new_size_mb + 512}MiB"
        cmds = [
            ["parted", "-s", device, "--", "mkpart", "ESP", "fat32",
             end_pos, esp_end, "set", str(int(pnum)+1), "esp", "on"],
            ["parted", "-s", device, "--", "mkpart", "WINLUX_ROOT", "ext4",
             esp_end, "100%"],
            ["partprobe", device],
        ]
        for cmd in cmds:
            subprocess.run(cmd, check=True, timeout=30)
        time.sleep(1)

    def format_partitions(self, device):
        esp = None
        root_part = None
        if self.config["install_type"] == "dualboot":
            os_list = self.detected_os.get(device, [])
            if os_list:
                part_num = re.search(r'(\d+)$', os_list[0]["partition"])
                if part_num:
                    pnum = int(part_num.group(1))
                    esp_num = pnum + 1
                    root_num = pnum + 2
                    for suffix in [str(esp_num), f"p{esp_num}"]:
                        candidate = f"{device}{suffix}"
                        if os.path.exists(candidate):
                            esp = candidate
                            break
                    for suffix in [str(root_num), f"p{root_num}"]:
                        candidate = f"{device}{suffix}"
                        if os.path.exists(candidate):
                            root_part = candidate
                            break
        else:
            for attempt in range(20):
                for suffix in ["2", "p2"]:
                    candidate = f"{device}{suffix}"
                    if os.path.exists(candidate):
                        esp = candidate
                        break
                for suffix in ["3", "p3"]:
                    candidate = f"{device}{suffix}"
                    if os.path.exists(candidate):
                        root_part = candidate
                        break
                if esp and root_part:
                    break
                time.sleep(0.5)

        if not esp or not root_part:
            raise RuntimeError("Partitions introuvables après partitionnement")

        subprocess.run(["mkfs.fat", "-F32", esp], check=True, timeout=30)

        if self.config["luks_enabled"]:
            self.set_progress(0.12, tr("progress_luks_setup"))
            raw_part = root_part
            passphrase = self.config["luks_passphrase"]
            with tempfile.NamedTemporaryFile(mode='w', suffix='.key', delete=False) as kf:
                kf.write(passphrase)
                keyfile = kf.name
            try:
                subprocess.run(
                    ["cryptsetup", "luksFormat", "--batch-mode",
                     "--type", "luks2",
                     "--key-file", keyfile,
                     raw_part],
                    check=True, timeout=120
                )
                subprocess.run(
                    ["cryptsetup", "luksOpen",
                     "--key-file", keyfile,
                     raw_part, "winlux_crypt"],
                    check=True, timeout=30
                )
            finally:
                os.unlink(keyfile)
            root_part = "/dev/mapper/winlux_crypt"
            self._luks_raw_part = raw_part

        subprocess.run(["mkfs.ext4", "-F", "-L", "WINLUX_ROOT", root_part],
                       check=True, timeout=120)
        return esp, root_part

    def cleanup_luks(self):
        if self.config["luks_enabled"]:
            try:
                subprocess.run(["cryptsetup", "luksClose", "winlux_crypt"],
                               timeout=30)
            except Exception:
                pass

    def extract_and_copy(self, iso_path, work, device, esp, root_part):
        self.set_progress(0.20, "Extraction du noyau depuis l'ISO...")
        subprocess.run(
            ["xorriso", "-osirrox", "on", "-indev", iso_path,
             "-extract", "/boot/vmlinuz", f"{work}/vmlinuz"],
            check=True, capture_output=True, timeout=60
        )
        subprocess.run(
            ["xorriso", "-osirrox", "on", "-indev", iso_path,
             "-extract", "/boot/winlux-initramfs.cpio.gz", f"{work}/initramfs.gz"],
            check=True, capture_output=True, timeout=60
        )
        self.set_progress(0.25, "Extraction du système de fichiers...")
        root_extent_out = subprocess.run(
            ["xorriso", "-indev", iso_path,
             "-find", "/boot/winlux-rootfs.squashfs", "-exec", "report_lba", "--"],
            capture_output=True, text=True, timeout=30
        )
        split_root = False
        root_extent = ""
        for line in root_extent_out.stdout.splitlines():
            if line.startswith("File data lba:"):
                parts = line.split(",")
                if len(parts) >= 2:
                    root_extent = parts[1].strip()
                    break
        if root_extent and root_extent.isdigit():
            self.set_progress(0.30, "Décompression du squashfs...")
            subprocess.run(
                ["unsquashfs", "-no-progress", "-processors", "2",
                 "-offset", str(int(root_extent) * 2048),
                 "-d", f"{work}/source", iso_path],
                check=True, timeout=600
            )
            split_root = True
        else:
            self.set_progress(0.30, "Extraction du cpio...")
            subprocess.run(
                ["bsdtar", "-xf", f"{work}/initramfs.gz", "-C", f"{work}/source", "--no-same-owner"],
                check=True, timeout=300
            )
        self.set_progress(0.50, tr("progress_copying"))
        subprocess.run(
            ["cp", "-a", f"{work}/source/.", f"{work}/target/"],
            check=True, timeout=1800
        )
        os.makedirs(f"{work}/target/boot/grub", exist_ok=True)
        os.makedirs(f"{work}/target/boot/efi", exist_ok=True)
        subprocess.run(
            ["cp", f"{work}/vmlinuz", f"{work}/target/boot/vmlinuz"],
            check=True, timeout=30
        )
        subprocess.run(
            ["cp", f"{work}/initramfs.gz", f"{work}/target/boot/winlux-initramfs.cpio.gz"],
            check=True, timeout=30
        )
        self.set_progress(0.70, tr("progress_grub"))
        self.install_grub(device, esp, root_part, work, split_root=split_root)
        self.set_progress(0.85, tr("progress_account"))
        self.configure_user(f"{work}/target")
        self.set_progress(0.90, tr("progress_firstboot"))
        self.configure_first_boot(f"{work}/target")
        self.set_progress(0.95, tr("progress_sync"))
        subprocess.run(["sync"], timeout=30)
        subprocess.run(["umount", f"{work}/target/boot/efi"], timeout=10)
        subprocess.run(["umount", f"{work}/target"], timeout=10)

    def install_grub(self, device, esp, root_part, work, split_root=True):
        actual_root = root_part
        if self.config["luks_enabled"] and hasattr(self, '_luks_raw_part'):
            actual_root = self._luks_raw_part

        partuuid = subprocess.check_output(
            ["blkid", "-s", "PARTUUID", "-o", "value", actual_root],
            text=True, timeout=10
        ).strip()
        fsuuid = subprocess.check_output(
            ["blkid", "-s", "UUID", "-o", "value", actual_root],
            text=True, timeout=10
        ).strip()

        if self.config["luks_enabled"]:
            luks_uuid = fsuuid
            root_fs_uuid = subprocess.check_output(
                ["blkid", "-s", "UUID", "-o", "value", root_part],
                text=True, timeout=10
            ).strip()
            root_arg = f"UUID={root_fs_uuid}"
            crypto_arg = f"cryptdevice=UUID={luks_uuid}:winlux_crypt"
            kernel_extra = f" {crypto_arg}"
        else:
            root_arg = f"UUID={fsuuid}" if split_root else f"PARTUUID={partuuid}"
            kernel_extra = ""
            root_fs_uuid = fsuuid

        initrd = "initrd /boot/winlux-initramfs.cpio.gz" if split_root else ""
        search_uuid = root_fs_uuid if not self.config["luks_enabled"] else root_fs_uuid

        grub_cfg = f"""set default=0
set timeout=8
serial --unit=0 --speed=115200
terminal_input console serial
terminal_output console serial
"""
        if self.config["luks_enabled"]:
            grub_cfg += """insmod cryptodisk
insmod luks2
insmod gcry_rijndael
insmod gcry_sha256
"""

        grub_cfg += f"""menuentry "Winlux Desktop" {{
    search --no-floppy --fs-uuid {search_uuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 winlux.interactive=1{kernel_extra}
    {initrd}
}}
menuentry "Winlux diagnostic" --hotkey=d {{
    search --no-floppy --fs-uuid {search_uuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 init=/init console=ttyS0,115200n8{kernel_extra}
    {initrd}
}}
menuentry "Winlux secours (restauration ou console)" --hotkey=s {{
    search --no-floppy --fs-uuid {search_uuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 console=tty1 console=ttyS0,115200n8 winlux.recovery=1{kernel_extra}
    initrd /boot/winlux-initramfs.cpio.gz
}}
"""
        grub_path = f"{work}/target/boot/grub/grub.cfg"
        os.makedirs(os.path.dirname(grub_path), exist_ok=True)
        with open(grub_path, "w") as f:
            f.write(grub_cfg)
        subprocess.run(["grub-script-check", grub_path], check=True, timeout=10)
        subprocess.run(
            ["grub-install", "--target=i386-pc",
             f"--boot-directory={work}/target/boot", "--recheck", device],
            check=True, timeout=60
        )
        efi_dir = f"{work}/target/boot/efi"
        os.makedirs(efi_dir, exist_ok=True)
        subprocess.run(["mount", "-t", "vfat", "-o", "iocharset=ascii", esp, efi_dir],
                       check=True, timeout=10)
        subprocess.run(
            ["grub-install", "--target=x86_64-efi",
             f"--efi-directory={efi_dir}",
             f"--boot-directory={work}/target/boot",
             "--removable", "--no-nvram", "--recheck"],
            check=True, timeout=60
        )
        subprocess.run(["umount", efi_dir], timeout=10)

        if self.config["luks_enabled"]:
            grub_default = f"{work}/target/etc/default/grub"
            os.makedirs(os.path.dirname(grub_default), exist_ok=True)
            with open(grub_default, "w") as f:
                f.write('GRUB_ENABLE_CRYPTODISK=y\n')
                f.write(f'GRUB_CMDLINE_LINUX="cryptdevice=UUID={luks_uuid}:winlux_crypt"\n')
            crypttab = f"{work}/target/etc/crypttab"
            os.makedirs(os.path.dirname(crypttab), exist_ok=True)
            with open(crypttab, "w") as f:
                f.write(f"winlux_crypt UUID={luks_uuid} none luks\n")
            hook_dir = f"{work}/target/etc/initramfs-tools/hooks"
            os.makedirs(hook_dir, exist_ok=True)
            hook_script = f"{hook_dir}/winlux-cryptsetup"
            with open(hook_script, "w") as f:
                f.write("#!/bin/sh\n")
                f.write("set -e\n")
                f.write("PREREQ=\"\"\n")
                f.write("prereqs() { echo \"$PREREQ\"; }\n")
                f.write("case \"$1\" in prereqs) prereqs; exit 0;; esac\n")
                f.write(". /usr/share/initramfs-tools/hook-functions\n")
                f.write("copy_exec /sbin/cryptsetup /sbin\n")
                f.write("copy_exec /sbin/dmsetup /sbin\n")
                f.write("manual_add_modules dm_crypt aes sha256\n")
            os.chmod(hook_script, 0o755)

        if split_root:
            with open(f"{work}/target/boot/winlux-split-root", "w") as f:
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
