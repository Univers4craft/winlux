#!/usr/bin/python3
"""Installateur graphique Winlux — style Windows 11.

Lance l'installation de Winlux sur un disque physique depuis le Live USB/DVD.
Interface GTK3 avec étapes : langue, licence, réseau, partitionnement guidé,
import Windows, chiffrement LUKS, performances, profil d'utilisation, compte local,
migration depuis un autre PC, progression.
"""
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk, GLib, Pango
import gzip
import html
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
SECUREBOOT_SH = next((p for p in (os.path.join(SCRIPT_DIR, "winlux-secureboot.sh"), "/usr/bin/winlux-secureboot") if os.path.exists(p)), None)
sys.path[:0] = [SCRIPT_DIR, "/usr/share/winlux"]
import winlux_autoinstall as autoinstall  # noqa: E402
# Modules partagés : à côté du script dans le dépôt, /usr/lib/winlux/python une fois installé.
for _path in (SCRIPT_DIR, "/usr/lib/winlux/python"):
    if os.path.isfile(os.path.join(_path, "winlux_benchmark.py")):
        sys.path.insert(0, _path)
        break
import winlux_benchmark  # noqa: E402
import winlux_migrate  # noqa: E402
import winlux_profiles  # noqa: E402
import winlux_partitioner  # noqa: E402
import winlux_partition_ui  # noqa: E402
import winlux_screen  # noqa: E402

UI_SCALE = 1.0

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
    background-image: none;
    text-shadow: none;
    box-shadow: none;
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
    color: alpha(@fg_primary, 0.6);
}
.btn-secondary:hover {
    background-color: shade(@bg_surface, 1.2);
}
.btn-secondary {
    background-image: none;
    text-shadow: none;
    box-shadow: none;
    background-color: @bg_surface;
    color: @fg_primary;
    border: 1px solid @fg_secondary;
    border-radius: 6px;
    padding: 10px 24px;
    font-size: 14px;
    min-height: 36px;
}
.btn-danger {
    background-image: none;
    text-shadow: none;
    box-shadow: none;
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
.installer-content checkbutton label,
.installer-content radiobutton label {
    color: @fg_primary;
}
.status-ok {
    color: #2ecc71;
}
list.wifi-list {
    background-color: transparent;
}
list.wifi-list row {
    background-color: transparent;
    padding: 0;
    margin-bottom: 6px;
}
list.wifi-list row:selected .card {
    border: 2px solid @accent;
    background-color: shade(@bg_card, 1.15);
}
.score-big {
    color: @fg_primary;
    font-size: 44px;
    font-weight: bold;
}
.score-value {
    color: @fg_primary;
    font-size: 18px;
    font-weight: bold;
}
levelbar.score-bar trough {
    background-color: @bg_surface;
    border-radius: 3px;
    min-height: 6px;
}
levelbar.score-bar block.filled {
    background-color: @accent;
    border-radius: 3px;
}
levelbar.score-bar block.empty {
    background-color: transparent;
}
.badge {
    background-color: alpha(@accent, 0.25);
    color: @accent_hover;
    border-radius: 10px;
    padding: 2px 10px;
    font-size: 11px;
    font-weight: bold;
}
.tip {
    color: @fg_secondary;
    font-size: 12px;
}
/* Independent of the system GTK theme (light Adwaita in the live session). */
window.installer-window radiobutton label,
window.installer-window checkbutton label,
label.card {
    color: @fg_primary;
}
.btn-primary, .btn-secondary, .btn-danger {
    background-image: none;
    box-shadow: none;
    text-shadow: none;
    -gtk-icon-shadow: none;
}
.btn-secondary:hover {
    background-color: shade(@bg_surface, 1.2);
}
window.installer-window list,
window.installer-window list row {
    background-color: transparent;
    color: @fg_primary;
}
window.installer-window list row:selected {
    background-color: alpha(@accent, 0.35);
}
/* Keep the dark look whatever GTK theme the live session ships. */
.installer-content label, .installer-content radiobutton label, .installer-content checkbutton label {
    color: @fg_primary;
}
.installer-content .card-detail, .installer-content .page-subtitle, .installer-content .progress-text,
.installer-content .hw-info, .installer-content .tip {
    color: @fg_secondary;
}
.installer-content .warning-text, .installer-content .disk-warn {
    color: @danger;
}
.installer-content .badge {
    color: @accent_hover;
}
radiobutton:disabled label, checkbutton:disabled label {
    color: alpha(@fg_secondary, 0.5);
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
        "steps": ["Langue", "Clavier", "Licence", "Réseau", "Partitionnement",
                  "Import Windows", "Chiffrement", "Performances", "Profil d'utilisation", "Compte local", "Migration", "Résumé", "Installation", "Terminé"],
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
        "summary_local_account": "Compte : local (aucun compte en ligne)",
        "summary_profile": "Profil d'utilisation",
        "summary_profile_apps": "+ applications recommandées",
        "summary_benchmark": "Note de performance",
        "summary_benchmark_none": "non mesurée",
        "summary_migration": "Transfert de PC",
        "summary_timezone": "Fuseau horaire",
        "summary_packages": "Paquets supplémentaires",
        "summary_answer_file": "Fichier de réponse",
        "answer_file_ignored": "Fichier de réponse ignoré : {error}",
        "answer_disk_missing": "Disque « {disk} » du fichier de réponse introuvable ou ambigu : choisissez-le.",
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
        "network_title": "Connexion réseau",
        "network_subtitle": "Connectez-vous à Internet pour télécharger les mises à jour pendant l'installation.",
        "network_scan": "Rechercher les réseaux",
        "network_scanning": "Recherche en cours...",
        "network_none": "Aucun réseau Wi-Fi détecté.",
        "network_connected": "Connecté",
        "network_connect": "Se connecter",
        "network_password": "Mot de passe Wi-Fi :",
        "network_ethernet": "Connexion filaire détectée",
        "network_proxy_title": "Proxy entreprise (optionnel)",
        "network_proxy_host": "Serveur proxy :",
        "network_proxy_port": "Port :",
        "network_proxy_enable": "Utiliser un proxy",
        "network_status_connected": "Connecté à {ssid}",
        "network_status_none": "Non connecté",
        "network_no_nm": "NetworkManager n'est pas disponible : la connexion Wi-Fi est impossible depuis l'installateur.",
        "network_connecting": "Connexion à {ssid}...",
        "network_failed": "Échec de la connexion : {error}",
        "network_proxy_invalid": "Proxy invalide : indiquez un nom d'hôte ou une adresse IP et un port (1-65535).",
        "network_open": "Ouvert",
        "import_title": "Importer vos données Windows",
        "import_subtitle": "Nous avons détecté une installation Windows. Vous pouvez importer certaines données.",
        "import_none": "Aucune installation Windows détectée sur ce système.",
        "import_scanning": "Recherche des installations Windows...",
        "import_wallpaper": "Fond d'écran",
        "import_bookmarks": "Favoris navigateur ({browser})",
        "import_documents": "Documents ({count} fichiers, {size})",
        "import_pictures": "Images ({count} fichiers, {size})",
        "import_music": "Musique ({count} fichiers, {size})",
        "import_videos": "Vidéos ({count} fichiers, {size})",
        "import_desktop": "Fichiers du Bureau ({count} fichiers, {size})",
        "import_select_all": "Tout sélectionner",
        "import_select_none": "Tout désélectionner",
        "import_partition": "Partition Windows : {partition} ({size})",
        "import_user": "Profil utilisateur : {user}",
        "import_on_target": "Cette partition Windows se trouve sur le disque qui va être effacé. Seuls le fond d'écran et les favoris peuvent être conservés.",
        "import_dualboot_keep": "Dual-boot : Windows est conservé, ses dossiers seront copiés après le partitionnement.",
        "import_total": "Total à importer : {size}",
        "import_too_big": "Pas assez de place sur le disque cible pour ces données.",
        "import_note": "Lecture seule : rien n'est modifié sur la partition Windows. Aucun compte Microsoft n'est utilisé.",
        "import_remount_failed": "Impossible de remonter la partition Windows : dossiers non importés.",
        "progress_import": "Importation des données Windows...",
        "summary_network": "Réseau",
        "summary_network_wifi": "Wi-Fi « {ssid} »",
        "summary_network_wired": "Filaire",
        "summary_network_none": "Aucun",
        "summary_proxy": "proxy {proxy}",
        "summary_import": "Import Windows",
        "summary_import_none": "aucun",
        "summary_import_bookmarks": "favoris {browser}",
        "summary_item_wallpaper": "fond d'écran",
        "summary_item_desktop": "Bureau",
        "summary_item_documents": "Documents",
        "summary_item_pictures": "Images",
        "summary_item_music": "Musique",
        "summary_item_videos": "Vidéos",
    },
    "en_US": {
        "steps": ["Language", "Keyboard", "License", "Network", "Partitioning",
                  "Windows Import", "Encryption", "Performance", "Usage profile", "Local account", "Migration", "Summary", "Installing", "Done"],
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
        "summary_local_account": "Account: local (no online account)",
        "summary_profile": "Usage profile",
        "summary_profile_apps": "+ recommended apps",
        "summary_benchmark": "Performance score",
        "summary_benchmark_none": "not measured",
        "summary_migration": "PC transfer",
        "summary_timezone": "Time zone",
        "summary_packages": "Additional packages",
        "summary_answer_file": "Answer file",
        "answer_file_ignored": "Answer file ignored: {error}",
        "answer_disk_missing": "Disk '{disk}' from the answer file was not found or is ambiguous: please choose it.",
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
        "network_title": "Network connection",
        "network_subtitle": "Connect to the Internet to download updates during installation.",
        "network_scan": "Scan networks",
        "network_scanning": "Scanning...",
        "network_none": "No Wi-Fi network detected.",
        "network_connected": "Connected",
        "network_connect": "Connect",
        "network_password": "Wi-Fi password:",
        "network_ethernet": "Wired connection detected",
        "network_proxy_title": "Enterprise proxy (optional)",
        "network_proxy_host": "Proxy server:",
        "network_proxy_port": "Port:",
        "network_proxy_enable": "Use a proxy",
        "network_status_connected": "Connected to {ssid}",
        "network_status_none": "Not connected",
        "network_no_nm": "NetworkManager is not available: Wi-Fi cannot be configured from the installer.",
        "network_connecting": "Connecting to {ssid}...",
        "network_failed": "Connection failed: {error}",
        "network_proxy_invalid": "Invalid proxy: enter a host name or IP address and a port (1-65535).",
        "network_open": "Open",
        "import_title": "Import your Windows data",
        "import_subtitle": "A Windows installation was detected. You can import some of your data.",
        "import_none": "No Windows installation detected on this system.",
        "import_scanning": "Scanning for Windows installations...",
        "import_wallpaper": "Wallpaper",
        "import_bookmarks": "Browser bookmarks ({browser})",
        "import_documents": "Documents ({count} files, {size})",
        "import_pictures": "Pictures ({count} files, {size})",
        "import_music": "Music ({count} files, {size})",
        "import_videos": "Videos ({count} files, {size})",
        "import_desktop": "Desktop files ({count} files, {size})",
        "import_select_all": "Select all",
        "import_select_none": "Deselect all",
        "import_partition": "Windows partition: {partition} ({size})",
        "import_user": "User profile: {user}",
        "import_on_target": "This Windows partition is on the disk that will be erased. Only the wallpaper and bookmarks can be kept.",
        "import_dualboot_keep": "Dual-boot: Windows is kept, its folders will be copied after partitioning.",
        "import_total": "Total to import: {size}",
        "import_too_big": "Not enough space on the target disk for this data.",
        "import_note": "Read-only: nothing is changed on the Windows partition. No Microsoft account is used.",
        "import_remount_failed": "Could not remount the Windows partition: folders not imported.",
        "progress_import": "Importing Windows data...",
        "summary_network": "Network",
        "summary_network_wifi": "Wi-Fi “{ssid}”",
        "summary_network_wired": "Wired",
        "summary_network_none": "None",
        "summary_proxy": "proxy {proxy}",
        "summary_import": "Windows import",
        "summary_import_none": "none",
        "summary_import_bookmarks": "{browser} bookmarks",
        "summary_item_wallpaper": "wallpaper",
        "summary_item_desktop": "Desktop",
        "summary_item_documents": "Documents",
        "summary_item_pictures": "Pictures",
        "summary_item_music": "Music",
        "summary_item_videos": "Videos",
    },
    "de_DE": {
        "steps": ["Sprache", "Tastatur", "Lizenz", "Netzwerk", "Partitionierung",
                  "Windows-Import", "Verschlüsselung", "Leistung", "Nutzungsprofil", "Lokales Konto", "Migration", "Zusammenfassung", "Installation", "Fertig"],
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
        "summary_local_account": "Konto: lokal (kein Online-Konto)",
        "summary_profile": "Nutzungsprofil",
        "summary_profile_apps": "+ empfohlene Anwendungen",
        "summary_benchmark": "Leistungsbewertung",
        "summary_benchmark_none": "nicht gemessen",
        "summary_migration": "PC-Übertragung",
        "summary_timezone": "Zeitzone",
        "summary_packages": "Zusätzliche Pakete",
        "summary_answer_file": "Antwortdatei",
        "answer_file_ignored": "Antwortdatei ignoriert: {error}",
        "answer_disk_missing": "Datenträger „{disk}“ aus der Antwortdatei nicht gefunden oder mehrdeutig: bitte auswählen.",
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
        "network_title": "Netzwerkverbindung",
        "network_subtitle": "Verbinden Sie sich mit dem Internet, um während der Installation Updates herunterzuladen.",
        "network_scan": "Netzwerke suchen",
        "network_scanning": "Suche läuft...",
        "network_none": "Kein WLAN-Netzwerk gefunden.",
        "network_connected": "Verbunden",
        "network_connect": "Verbinden",
        "network_password": "WLAN-Passwort:",
        "network_ethernet": "Kabelverbindung erkannt",
        "network_proxy_title": "Unternehmens-Proxy (optional)",
        "network_proxy_host": "Proxy-Server:",
        "network_proxy_port": "Port:",
        "network_proxy_enable": "Proxy verwenden",
        "network_status_connected": "Verbunden mit {ssid}",
        "network_status_none": "Nicht verbunden",
        "network_no_nm": "NetworkManager ist nicht verfügbar: WLAN kann im Installationsprogramm nicht eingerichtet werden.",
        "network_connecting": "Verbindung mit {ssid}...",
        "network_failed": "Verbindung fehlgeschlagen: {error}",
        "network_proxy_invalid": "Ungültiger Proxy: Geben Sie einen Hostnamen oder eine IP-Adresse und einen Port (1-65535) an.",
        "network_open": "Offen",
        "import_title": "Ihre Windows-Daten importieren",
        "import_subtitle": "Eine Windows-Installation wurde erkannt. Sie können einige Ihrer Daten importieren.",
        "import_none": "Keine Windows-Installation auf diesem System gefunden.",
        "import_scanning": "Suche nach Windows-Installationen...",
        "import_wallpaper": "Hintergrundbild",
        "import_bookmarks": "Browser-Lesezeichen ({browser})",
        "import_documents": "Dokumente ({count} Dateien, {size})",
        "import_pictures": "Bilder ({count} Dateien, {size})",
        "import_music": "Musik ({count} Dateien, {size})",
        "import_videos": "Videos ({count} Dateien, {size})",
        "import_desktop": "Desktop-Dateien ({count} Dateien, {size})",
        "import_select_all": "Alle auswählen",
        "import_select_none": "Auswahl aufheben",
        "import_partition": "Windows-Partition: {partition} ({size})",
        "import_user": "Benutzerprofil: {user}",
        "import_on_target": "Diese Windows-Partition liegt auf der Festplatte, die gelöscht wird. Nur Hintergrundbild und Lesezeichen können übernommen werden.",
        "import_dualboot_keep": "Dual-Boot: Windows bleibt erhalten, seine Ordner werden nach der Partitionierung kopiert.",
        "import_total": "Zu importieren: {size}",
        "import_too_big": "Nicht genug Platz auf der Zielfestplatte für diese Daten.",
        "import_note": "Nur lesend: Auf der Windows-Partition wird nichts verändert. Es wird kein Microsoft-Konto verwendet.",
        "import_remount_failed": "Windows-Partition konnte nicht erneut eingebunden werden: Ordner nicht importiert.",
        "progress_import": "Windows-Daten werden importiert...",
        "summary_network": "Netzwerk",
        "summary_network_wifi": "WLAN „{ssid}“",
        "summary_network_wired": "Kabel",
        "summary_network_none": "Keine",
        "summary_proxy": "Proxy {proxy}",
        "summary_import": "Windows-Import",
        "summary_import_none": "keiner",
        "summary_import_bookmarks": "{browser}-Lesezeichen",
        "summary_item_wallpaper": "Hintergrundbild",
        "summary_item_desktop": "Desktop",
        "summary_item_documents": "Dokumente",
        "summary_item_pictures": "Bilder",
        "summary_item_music": "Musik",
        "summary_item_videos": "Videos",
    },
    "es_ES": {
        "steps": ["Idioma", "Teclado", "Licencia", "Red", "Particionamiento",
                  "Importar Windows", "Cifrado", "Rendimiento", "Perfil de uso", "Cuenta local", "Migración", "Resumen", "Instalación", "Terminado"],
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
        "summary_local_account": "Cuenta: local (sin cuenta en línea)",
        "summary_profile": "Perfil de uso",
        "summary_profile_apps": "+ aplicaciones recomendadas",
        "summary_benchmark": "Puntuación de rendimiento",
        "summary_benchmark_none": "no medida",
        "summary_migration": "Transferencia de PC",
        "summary_timezone": "Zona horaria",
        "summary_packages": "Paquetes adicionales",
        "summary_answer_file": "Archivo de respuestas",
        "answer_file_ignored": "Archivo de respuestas ignorado: {error}",
        "answer_disk_missing": "Disco «{disk}» del archivo de respuestas no encontrado o ambiguo: elíjalo.",
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
        "network_title": "Conexión de red",
        "network_subtitle": "Conéctese a Internet para descargar actualizaciones durante la instalación.",
        "network_scan": "Buscar redes",
        "network_scanning": "Buscando...",
        "network_none": "No se detectó ninguna red Wi-Fi.",
        "network_connected": "Conectado",
        "network_connect": "Conectar",
        "network_password": "Contraseña Wi-Fi:",
        "network_ethernet": "Conexión por cable detectada",
        "network_proxy_title": "Proxy de empresa (opcional)",
        "network_proxy_host": "Servidor proxy:",
        "network_proxy_port": "Puerto:",
        "network_proxy_enable": "Usar un proxy",
        "network_status_connected": "Conectado a {ssid}",
        "network_status_none": "Sin conexión",
        "network_no_nm": "NetworkManager no está disponible: no se puede configurar el Wi-Fi desde el instalador.",
        "network_connecting": "Conectando a {ssid}...",
        "network_failed": "Error de conexión: {error}",
        "network_proxy_invalid": "Proxy no válido: indique un nombre de host o una dirección IP y un puerto (1-65535).",
        "network_open": "Abierta",
        "import_title": "Importar sus datos de Windows",
        "import_subtitle": "Se detectó una instalación de Windows. Puede importar algunos de sus datos.",
        "import_none": "No se detectó ninguna instalación de Windows en este sistema.",
        "import_scanning": "Buscando instalaciones de Windows...",
        "import_wallpaper": "Fondo de pantalla",
        "import_bookmarks": "Marcadores del navegador ({browser})",
        "import_documents": "Documentos ({count} archivos, {size})",
        "import_pictures": "Imágenes ({count} archivos, {size})",
        "import_music": "Música ({count} archivos, {size})",
        "import_videos": "Vídeos ({count} archivos, {size})",
        "import_desktop": "Archivos del escritorio ({count} archivos, {size})",
        "import_select_all": "Seleccionar todo",
        "import_select_none": "Deseleccionar todo",
        "import_partition": "Partición de Windows: {partition} ({size})",
        "import_user": "Perfil de usuario: {user}",
        "import_on_target": "Esta partición de Windows está en el disco que se va a borrar. Solo se pueden conservar el fondo de pantalla y los marcadores.",
        "import_dualboot_keep": "Arranque dual: Windows se conserva, sus carpetas se copiarán después del particionado.",
        "import_total": "Total a importar: {size}",
        "import_too_big": "No hay espacio suficiente en el disco de destino para estos datos.",
        "import_note": "Solo lectura: no se modifica nada en la partición de Windows. No se usa ninguna cuenta de Microsoft.",
        "import_remount_failed": "No se pudo volver a montar la partición de Windows: carpetas no importadas.",
        "progress_import": "Importando datos de Windows...",
        "summary_network": "Red",
        "summary_network_wifi": "Wi-Fi «{ssid}»",
        "summary_network_wired": "Cable",
        "summary_network_none": "Ninguna",
        "summary_proxy": "proxy {proxy}",
        "summary_import": "Importación de Windows",
        "summary_import_none": "ninguna",
        "summary_import_bookmarks": "marcadores de {browser}",
        "summary_item_wallpaper": "fondo de pantalla",
        "summary_item_desktop": "Escritorio",
        "summary_item_documents": "Documentos",
        "summary_item_pictures": "Imágenes",
        "summary_item_music": "Música",
        "summary_item_videos": "Vídeos",
    },
    "it_IT": {
        "steps": ["Lingua", "Tastiera", "Licenza", "Rete", "Partizionamento",
                  "Importa Windows", "Crittografia", "Prestazioni", "Profilo d'uso", "Account locale", "Migrazione", "Riepilogo", "Installazione", "Completato"],
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
        "summary_local_account": "Account: locale (nessun account online)",
        "summary_profile": "Profilo d'uso",
        "summary_profile_apps": "+ applicazioni consigliate",
        "summary_benchmark": "Punteggio prestazioni",
        "summary_benchmark_none": "non misurato",
        "summary_migration": "Trasferimento PC",
        "summary_timezone": "Fuso orario",
        "summary_packages": "Pacchetti aggiuntivi",
        "summary_answer_file": "File di risposta",
        "answer_file_ignored": "File di risposta ignorato: {error}",
        "answer_disk_missing": "Disco «{disk}» del file di risposta non trovato o ambiguo: sceglilo.",
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
        "network_title": "Connessione di rete",
        "network_subtitle": "Connettiti a Internet per scaricare gli aggiornamenti durante l'installazione.",
        "network_scan": "Cerca reti",
        "network_scanning": "Ricerca in corso...",
        "network_none": "Nessuna rete Wi-Fi rilevata.",
        "network_connected": "Connesso",
        "network_connect": "Connetti",
        "network_password": "Password Wi-Fi:",
        "network_ethernet": "Connessione via cavo rilevata",
        "network_proxy_title": "Proxy aziendale (facoltativo)",
        "network_proxy_host": "Server proxy:",
        "network_proxy_port": "Porta:",
        "network_proxy_enable": "Usa un proxy",
        "network_status_connected": "Connesso a {ssid}",
        "network_status_none": "Non connesso",
        "network_no_nm": "NetworkManager non è disponibile: impossibile configurare il Wi-Fi dal programma di installazione.",
        "network_connecting": "Connessione a {ssid}...",
        "network_failed": "Connessione non riuscita: {error}",
        "network_proxy_invalid": "Proxy non valido: indica un nome host o un indirizzo IP e una porta (1-65535).",
        "network_open": "Aperta",
        "import_title": "Importa i tuoi dati di Windows",
        "import_subtitle": "È stata rilevata un'installazione di Windows. Puoi importare alcuni dei tuoi dati.",
        "import_none": "Nessuna installazione di Windows rilevata su questo sistema.",
        "import_scanning": "Ricerca delle installazioni di Windows...",
        "import_wallpaper": "Sfondo",
        "import_bookmarks": "Segnalibri del browser ({browser})",
        "import_documents": "Documenti ({count} file, {size})",
        "import_pictures": "Immagini ({count} file, {size})",
        "import_music": "Musica ({count} file, {size})",
        "import_videos": "Video ({count} file, {size})",
        "import_desktop": "File del desktop ({count} file, {size})",
        "import_select_all": "Seleziona tutto",
        "import_select_none": "Deseleziona tutto",
        "import_partition": "Partizione Windows: {partition} ({size})",
        "import_user": "Profilo utente: {user}",
        "import_on_target": "Questa partizione Windows si trova sul disco che verrà cancellato. Si possono conservare solo lo sfondo e i segnalibri.",
        "import_dualboot_keep": "Dual-boot: Windows viene mantenuto, le sue cartelle saranno copiate dopo il partizionamento.",
        "import_total": "Totale da importare: {size}",
        "import_too_big": "Spazio insufficiente sul disco di destinazione per questi dati.",
        "import_note": "Sola lettura: nulla viene modificato sulla partizione Windows. Non viene usato alcun account Microsoft.",
        "import_remount_failed": "Impossibile rimontare la partizione Windows: cartelle non importate.",
        "progress_import": "Importazione dei dati di Windows...",
        "summary_network": "Rete",
        "summary_network_wifi": "Wi-Fi «{ssid}»",
        "summary_network_wired": "Cavo",
        "summary_network_none": "Nessuna",
        "summary_proxy": "proxy {proxy}",
        "summary_import": "Importazione Windows",
        "summary_import_none": "nessuna",
        "summary_import_bookmarks": "segnalibri {browser}",
        "summary_item_wallpaper": "sfondo",
        "summary_item_desktop": "Desktop",
        "summary_item_documents": "Documenti",
        "summary_item_pictures": "Immagini",
        "summary_item_music": "Musica",
        "summary_item_videos": "Video",
    },
    "pt_BR": {
        "steps": ["Idioma", "Teclado", "Licença", "Rede", "Particionamento",
                  "Importar Windows", "Criptografia", "Desempenho", "Perfil de uso", "Conta local", "Migração", "Resumo", "Instalação", "Concluído"],
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
        "summary_local_account": "Conta: local (sem conta online)",
        "summary_profile": "Perfil de uso",
        "summary_profile_apps": "+ aplicativos recomendados",
        "summary_benchmark": "Nota de desempenho",
        "summary_benchmark_none": "não medida",
        "summary_migration": "Transferência de PC",
        "summary_timezone": "Fuso horário",
        "summary_packages": "Pacotes adicionais",
        "summary_answer_file": "Arquivo de resposta",
        "answer_file_ignored": "Arquivo de resposta ignorado: {error}",
        "answer_disk_missing": "Disco «{disk}» do arquivo de resposta não encontrado ou ambíguo: escolha-o.",
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
        "network_title": "Conexão de rede",
        "network_subtitle": "Conecte-se à Internet para baixar atualizações durante a instalação.",
        "network_scan": "Procurar redes",
        "network_scanning": "Procurando...",
        "network_none": "Nenhuma rede Wi-Fi detectada.",
        "network_connected": "Conectado",
        "network_connect": "Conectar",
        "network_password": "Senha do Wi-Fi:",
        "network_ethernet": "Conexão cabeada detectada",
        "network_proxy_title": "Proxy corporativo (opcional)",
        "network_proxy_host": "Servidor proxy:",
        "network_proxy_port": "Porta:",
        "network_proxy_enable": "Usar um proxy",
        "network_status_connected": "Conectado a {ssid}",
        "network_status_none": "Não conectado",
        "network_no_nm": "O NetworkManager não está disponível: não é possível configurar o Wi-Fi pelo instalador.",
        "network_connecting": "Conectando a {ssid}...",
        "network_failed": "Falha na conexão: {error}",
        "network_proxy_invalid": "Proxy inválido: informe um nome de host ou endereço IP e uma porta (1-65535).",
        "network_open": "Aberta",
        "import_title": "Importar seus dados do Windows",
        "import_subtitle": "Uma instalação do Windows foi detectada. Você pode importar alguns dos seus dados.",
        "import_none": "Nenhuma instalação do Windows detectada neste sistema.",
        "import_scanning": "Procurando instalações do Windows...",
        "import_wallpaper": "Papel de parede",
        "import_bookmarks": "Favoritos do navegador ({browser})",
        "import_documents": "Documentos ({count} arquivos, {size})",
        "import_pictures": "Imagens ({count} arquivos, {size})",
        "import_music": "Músicas ({count} arquivos, {size})",
        "import_videos": "Vídeos ({count} arquivos, {size})",
        "import_desktop": "Arquivos da área de trabalho ({count} arquivos, {size})",
        "import_select_all": "Selecionar tudo",
        "import_select_none": "Desmarcar tudo",
        "import_partition": "Partição do Windows: {partition} ({size})",
        "import_user": "Perfil de usuário: {user}",
        "import_on_target": "Esta partição do Windows está no disco que será apagado. Apenas o papel de parede e os favoritos podem ser mantidos.",
        "import_dualboot_keep": "Dual-boot: o Windows é mantido, suas pastas serão copiadas após o particionamento.",
        "import_total": "Total a importar: {size}",
        "import_too_big": "Espaço insuficiente no disco de destino para estes dados.",
        "import_note": "Somente leitura: nada é alterado na partição do Windows. Nenhuma conta Microsoft é usada.",
        "import_remount_failed": "Não foi possível remontar a partição do Windows: pastas não importadas.",
        "progress_import": "Importando dados do Windows...",
        "summary_network": "Rede",
        "summary_network_wifi": "Wi-Fi “{ssid}”",
        "summary_network_wired": "Cabeada",
        "summary_network_none": "Nenhuma",
        "summary_proxy": "proxy {proxy}",
        "summary_import": "Importação do Windows",
        "summary_import_none": "nenhuma",
        "summary_import_bookmarks": "favoritos do {browser}",
        "summary_item_wallpaper": "papel de parede",
        "summary_item_desktop": "Área de trabalho",
        "summary_item_documents": "Documentos",
        "summary_item_pictures": "Imagens",
        "summary_item_music": "Músicas",
        "summary_item_videos": "Vídeos",
    },
}

EXTRA_TRANSLATIONS = {
    "fr_FR": {
        "keyboard_name_fr": "AZERTY — Français",
        "keyboard_name_us": "QWERTY — Anglais (US)",
        "keyboard_name_gb": "QWERTY — Anglais (UK)",
        "keyboard_name_de": "QWERTZ — Allemand",
        "keyboard_name_es": "QWERTY — Espagnol",
        "keyboard_name_it": "QWERTY — Italien",
        "keyboard_name_br": "QWERTY — Portugais (Brésil)",
        "keyboard_name_ch": "QWERTZ — Suisse",
        "keyboard_name_be": "AZERTY — Belge",
        "keyboard_name_ca": "QWERTY — Canadien multilingue",
        "keyboard_test_placeholder": "Tapez ici pour tester...",
        "language_keyboard": "Clavier : {kb}",
        "disk_unknown": "Disque inconnu",
        "network_proxy_placeholder": "proxy.entreprise.local",
        "bench_title": "Évaluation des performances",
        "bench_subtitle": "Test rapide de votre matériel (environ 10 secondes) pour régler Winlux au mieux. Le disque est seulement lu : rien n'est écrit avant le lancement de l'installation.",
        "bench_overall": "Note globale",
        "bench_starting": "Le test démarre…",
        "bench_retry": "Relancer",
        "bench_cpu": "Processeur",
        "bench_ram": "Mémoire",
        "bench_disk": "Disque",
        "bench_gpu": "Graphique",
        "bench_waiting": "en attente",
        "bench_tips": "Recommandations",
        "bench_running": "Test en cours… vous pouvez continuer, il se termine en arrière-plan.",
        "bench_failed": "Test impossible : {error}",
        "bench_measuring": "mesure en cours…",
        "bench_done": "Déterminée par le composant le plus lent, sur une échelle de 1,0 à 9,9. L'écriture disque sera mesurée pendant l'installation.",
        "bench_no_tip": "Aucun réglage particulier nécessaire.",
        "bench_stopped": "Le test s'est arrêté avant la fin. Vous pouvez le relancer ou continuer.",
        "profile_title": "Comment allez-vous utiliser ce PC ?",
        "profile_subtitle": "Winlux règle le processeur, la mémoire, le disque et la carte graphique selon votre usage. Vous pourrez en changer plus tard (winlux-profile --set).",
        "profile_recommended": "Recommandé pour ce PC",
        "profile_apps": "Apps : {apps}",
        "profile_install_apps": "Installer les applications recommandées au premier démarrage (connexion Internet requise)",
        "fullname_placeholder": "Utilisateur Winlux",
        "migration_title": "Transférer depuis un autre PC",
        "migration_subtitle": "Récupérez vos documents, images et paramètres depuis un PC du même réseau. Le transfert a lieu à la fin de l'installation, directement dans votre nouveau compte.",
        "err_no_disk": "Aucun disque sélectionné.",
        "err_iso_missing": "ISO source introuvable. Définissez WINLUX_ISO ou démarrez depuis le Live USB.",
        "err_verify": "Vérification échouée :\n{error}",
        "err_boot_file": "Fichier de démarrage absent du média : {path}",
        "err_manual_plan": "Plan de partitionnement manuel invalide.",
        "err_partitions_missing": "Partitions introuvables après le partitionnement.",
        "err_resize_missing": "Partitions Winlux introuvables après le redimensionnement.",
        "err_partition_not_found": "Partition introuvable : {path}",
        "warn_import_skipped": "Import Windows ignoré : {error}",
        "warn_network": "Avertissement réseau : {error}",
        "warn_bookmarks": "Favoris {browser} ignorés : {error}",
        "progress_import_folder": "Import Windows : {folder}...",
        "warn_copy_partial": "Certains fichiers de {folder} n'ont pas pu être copiés.",
        "warn_import": "Avertissement import Windows : {error}",
        "warn_write_bench": "Mesure d'écriture disque impossible : {error}",
        "warn_transfer": "Avertissement transfert : {error}",
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
        "keyboard_name_fr": "AZERTY — French",
        "keyboard_name_us": "QWERTY — English (US)",
        "keyboard_name_gb": "QWERTY — English (UK)",
        "keyboard_name_de": "QWERTZ — German",
        "keyboard_name_es": "QWERTY — Spanish",
        "keyboard_name_it": "QWERTY — Italian",
        "keyboard_name_br": "QWERTY — Portuguese (Brazil)",
        "keyboard_name_ch": "QWERTZ — Swiss",
        "keyboard_name_be": "AZERTY — Belgian",
        "keyboard_name_ca": "QWERTY — Canadian Multilingual",
        "keyboard_test_placeholder": "Type here to test...",
        "language_keyboard": "Keyboard: {kb}",
        "disk_unknown": "Unknown disk",
        "network_proxy_placeholder": "proxy.company.local",
        "bench_title": "Performance assessment",
        "bench_subtitle": "Quick test of your hardware (about 10 seconds) to tune Winlux as well as possible. The disk is only read: nothing is written before the installation starts.",
        "bench_overall": "Overall score",
        "bench_starting": "The test is starting…",
        "bench_retry": "Run again",
        "bench_cpu": "Processor",
        "bench_ram": "Memory",
        "bench_disk": "Disk",
        "bench_gpu": "Graphics",
        "bench_waiting": "waiting",
        "bench_tips": "Recommendations",
        "bench_running": "Test in progress… you can continue, it finishes in the background.",
        "bench_failed": "Test failed: {error}",
        "bench_measuring": "measuring…",
        "bench_done": "Set by the slowest component, on a scale from 1.0 to 9.9. Disk writing will be measured during installation.",
        "bench_no_tip": "No special tuning needed.",
        "bench_stopped": "The test stopped before the end. You can run it again or continue.",
        "profile_title": "How will you use this PC?",
        "profile_subtitle": "Winlux tunes the processor, memory, disk and graphics card for your use. You can change it later (winlux-profile --set).",
        "profile_recommended": "Recommended for this PC",
        "profile_apps": "Apps: {apps}",
        "profile_install_apps": "Install the recommended apps at first startup (Internet connection required)",
        "fullname_placeholder": "Winlux user",
        "migration_title": "Transfer from another PC",
        "migration_subtitle": "Bring over your documents, pictures and settings from a PC on the same network. The transfer happens at the end of installation, straight into your new account.",
        "err_no_disk": "No disk selected.",
        "err_iso_missing": "Source ISO not found. Set WINLUX_ISO or start from the Live USB.",
        "err_verify": "Verification failed:\n{error}",
        "err_boot_file": "Boot file missing from the media: {path}",
        "err_manual_plan": "Invalid manual partitioning plan.",
        "err_partitions_missing": "Partitions not found after partitioning.",
        "err_resize_missing": "Winlux partitions not found after resizing.",
        "err_partition_not_found": "Partition not found: {path}",
        "warn_import_skipped": "Windows import skipped: {error}",
        "warn_network": "Network warning: {error}",
        "warn_bookmarks": "{browser} bookmarks skipped: {error}",
        "progress_import_folder": "Windows import: {folder}...",
        "warn_copy_partial": "Some files in {folder} could not be copied.",
        "warn_import": "Windows import warning: {error}",
        "warn_write_bench": "Disk write measurement failed: {error}",
        "warn_transfer": "Transfer warning: {error}",
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
        "keyboard_name_fr": "AZERTY — Französisch",
        "keyboard_name_us": "QWERTY — Englisch (USA)",
        "keyboard_name_gb": "QWERTY — Englisch (GB)",
        "keyboard_name_de": "QWERTZ — Deutsch",
        "keyboard_name_es": "QWERTY — Spanisch",
        "keyboard_name_it": "QWERTY — Italienisch",
        "keyboard_name_br": "QWERTY — Portugiesisch (Brasilien)",
        "keyboard_name_ch": "QWERTZ — Schweiz",
        "keyboard_name_be": "AZERTY — Belgisch",
        "keyboard_name_ca": "QWERTY — Kanadisch (mehrsprachig)",
        "keyboard_test_placeholder": "Hier tippen zum Testen...",
        "language_keyboard": "Tastatur: {kb}",
        "disk_unknown": "Unbekannter Datenträger",
        "network_proxy_placeholder": "proxy.firma.local",
        "bench_title": "Leistungstest",
        "bench_subtitle": "Schneller Test Ihrer Hardware (etwa 10 Sekunden), um Winlux optimal einzustellen. Der Datenträger wird nur gelesen: Vor dem Start der Installation wird nichts geschrieben.",
        "bench_overall": "Gesamtbewertung",
        "bench_starting": "Der Test startet…",
        "bench_retry": "Erneut ausführen",
        "bench_cpu": "Prozessor",
        "bench_ram": "Arbeitsspeicher",
        "bench_disk": "Datenträger",
        "bench_gpu": "Grafik",
        "bench_waiting": "wartet",
        "bench_tips": "Empfehlungen",
        "bench_running": "Test läuft… Sie können fortfahren, er wird im Hintergrund beendet.",
        "bench_failed": "Test nicht möglich: {error}",
        "bench_measuring": "wird gemessen…",
        "bench_done": "Bestimmt durch die langsamste Komponente, auf einer Skala von 1,0 bis 9,9. Die Schreibleistung wird während der Installation gemessen.",
        "bench_no_tip": "Keine besonderen Einstellungen nötig.",
        "bench_stopped": "Der Test wurde vorzeitig beendet. Sie können ihn erneut ausführen oder fortfahren.",
        "profile_title": "Wie werden Sie diesen PC verwenden?",
        "profile_subtitle": "Winlux stellt Prozessor, Arbeitsspeicher, Datenträger und Grafikkarte auf Ihre Nutzung ein. Sie können das später ändern (winlux-profile --set).",
        "profile_recommended": "Für diesen PC empfohlen",
        "profile_apps": "Apps: {apps}",
        "profile_install_apps": "Empfohlene Apps beim ersten Start installieren (Internetverbindung erforderlich)",
        "fullname_placeholder": "Winlux-Benutzer",
        "migration_title": "Von einem anderen PC übertragen",
        "migration_subtitle": "Übernehmen Sie Dokumente, Bilder und Einstellungen von einem PC im selben Netzwerk. Die Übertragung erfolgt am Ende der Installation direkt in Ihr neues Konto.",
        "err_no_disk": "Kein Datenträger ausgewählt.",
        "err_iso_missing": "Quell-ISO nicht gefunden. Setzen Sie WINLUX_ISO oder starten Sie vom Live-USB.",
        "err_verify": "Überprüfung fehlgeschlagen:\n{error}",
        "err_boot_file": "Startdatei fehlt auf dem Medium: {path}",
        "err_manual_plan": "Ungültiger manueller Partitionierungsplan.",
        "err_partitions_missing": "Partitionen nach der Partitionierung nicht gefunden.",
        "err_resize_missing": "Winlux-Partitionen nach der Größenänderung nicht gefunden.",
        "err_partition_not_found": "Partition nicht gefunden: {path}",
        "warn_import_skipped": "Windows-Import übersprungen: {error}",
        "warn_network": "Netzwerkwarnung: {error}",
        "warn_bookmarks": "{browser}-Favoriten übersprungen: {error}",
        "progress_import_folder": "Windows-Import: {folder}...",
        "warn_copy_partial": "Einige Dateien aus {folder} konnten nicht kopiert werden.",
        "warn_import": "Warnung beim Windows-Import: {error}",
        "warn_write_bench": "Messung der Schreibleistung nicht möglich: {error}",
        "warn_transfer": "Übertragungswarnung: {error}",
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
        "keyboard_name_fr": "AZERTY — Francés",
        "keyboard_name_us": "QWERTY — Inglés (EE. UU.)",
        "keyboard_name_gb": "QWERTY — Inglés (Reino Unido)",
        "keyboard_name_de": "QWERTZ — Alemán",
        "keyboard_name_es": "QWERTY — Español",
        "keyboard_name_it": "QWERTY — Italiano",
        "keyboard_name_br": "QWERTY — Portugués (Brasil)",
        "keyboard_name_ch": "QWERTZ — Suizo",
        "keyboard_name_be": "AZERTY — Belga",
        "keyboard_name_ca": "QWERTY — Canadiense multilingüe",
        "keyboard_test_placeholder": "Escriba aquí para probar...",
        "language_keyboard": "Teclado: {kb}",
        "disk_unknown": "Disco desconocido",
        "network_proxy_placeholder": "proxy.empresa.local",
        "bench_title": "Evaluación del rendimiento",
        "bench_subtitle": "Prueba rápida de su hardware (unos 10 segundos) para ajustar Winlux lo mejor posible. El disco solo se lee: no se escribe nada antes de iniciar la instalación.",
        "bench_overall": "Puntuación global",
        "bench_starting": "La prueba se está iniciando…",
        "bench_retry": "Volver a ejecutar",
        "bench_cpu": "Procesador",
        "bench_ram": "Memoria",
        "bench_disk": "Disco",
        "bench_gpu": "Gráficos",
        "bench_waiting": "en espera",
        "bench_tips": "Recomendaciones",
        "bench_running": "Prueba en curso… puede continuar, termina en segundo plano.",
        "bench_failed": "Prueba imposible: {error}",
        "bench_measuring": "midiendo…",
        "bench_done": "Determinada por el componente más lento, en una escala de 1,0 a 9,9. La escritura en disco se medirá durante la instalación.",
        "bench_no_tip": "No se necesita ningún ajuste especial.",
        "bench_stopped": "La prueba se detuvo antes de terminar. Puede volver a ejecutarla o continuar.",
        "profile_title": "¿Cómo va a usar este PC?",
        "profile_subtitle": "Winlux ajusta el procesador, la memoria, el disco y la tarjeta gráfica según su uso. Podrá cambiarlo más tarde (winlux-profile --set).",
        "profile_recommended": "Recomendado para este PC",
        "profile_apps": "Aplicaciones: {apps}",
        "profile_install_apps": "Instalar las aplicaciones recomendadas en el primer arranque (se requiere conexión a Internet)",
        "fullname_placeholder": "Usuario de Winlux",
        "migration_title": "Transferir desde otro PC",
        "migration_subtitle": "Recupere sus documentos, imágenes y configuración desde un PC de la misma red. La transferencia se realiza al final de la instalación, directamente en su nueva cuenta.",
        "err_no_disk": "No se ha seleccionado ningún disco.",
        "err_iso_missing": "No se encuentra la ISO de origen. Defina WINLUX_ISO o inicie desde el Live USB.",
        "err_verify": "Verificación fallida:\n{error}",
        "err_boot_file": "Falta el archivo de arranque en el medio: {path}",
        "err_manual_plan": "Plan de particionado manual no válido.",
        "err_partitions_missing": "No se encuentran las particiones tras el particionado.",
        "err_resize_missing": "No se encuentran las particiones de Winlux tras el redimensionado.",
        "err_partition_not_found": "Partición no encontrada: {path}",
        "warn_import_skipped": "Importación de Windows omitida: {error}",
        "warn_network": "Advertencia de red: {error}",
        "warn_bookmarks": "Favoritos de {browser} omitidos: {error}",
        "progress_import_folder": "Importación de Windows: {folder}...",
        "warn_copy_partial": "No se pudieron copiar algunos archivos de {folder}.",
        "warn_import": "Advertencia de importación de Windows: {error}",
        "warn_write_bench": "No se pudo medir la escritura en disco: {error}",
        "warn_transfer": "Advertencia de transferencia: {error}",
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
        "keyboard_name_fr": "AZERTY — Francese",
        "keyboard_name_us": "QWERTY — Inglese (USA)",
        "keyboard_name_gb": "QWERTY — Inglese (Regno Unito)",
        "keyboard_name_de": "QWERTZ — Tedesco",
        "keyboard_name_es": "QWERTY — Spagnolo",
        "keyboard_name_it": "QWERTY — Italiano",
        "keyboard_name_br": "QWERTY — Portoghese (Brasile)",
        "keyboard_name_ch": "QWERTZ — Svizzero",
        "keyboard_name_be": "AZERTY — Belga",
        "keyboard_name_ca": "QWERTY — Canadese multilingue",
        "keyboard_test_placeholder": "Digita qui per provare...",
        "language_keyboard": "Tastiera: {kb}",
        "disk_unknown": "Disco sconosciuto",
        "network_proxy_placeholder": "proxy.azienda.local",
        "bench_title": "Valutazione delle prestazioni",
        "bench_subtitle": "Test rapido del tuo hardware (circa 10 secondi) per regolare al meglio Winlux. Il disco viene solo letto: nulla viene scritto prima dell'avvio dell'installazione.",
        "bench_overall": "Punteggio globale",
        "bench_starting": "Il test sta per iniziare…",
        "bench_retry": "Ripeti",
        "bench_cpu": "Processore",
        "bench_ram": "Memoria",
        "bench_disk": "Disco",
        "bench_gpu": "Grafica",
        "bench_waiting": "in attesa",
        "bench_tips": "Raccomandazioni",
        "bench_running": "Test in corso… puoi continuare, termina in background.",
        "bench_failed": "Test non riuscito: {error}",
        "bench_measuring": "misurazione…",
        "bench_done": "Determinato dal componente più lento, su una scala da 1,0 a 9,9. La scrittura su disco sarà misurata durante l'installazione.",
        "bench_no_tip": "Nessuna regolazione particolare necessaria.",
        "bench_stopped": "Il test si è interrotto prima della fine. Puoi ripeterlo o continuare.",
        "profile_title": "Come userai questo PC?",
        "profile_subtitle": "Winlux regola processore, memoria, disco e scheda grafica in base al tuo utilizzo. Potrai cambiarlo in seguito (winlux-profile --set).",
        "profile_recommended": "Consigliato per questo PC",
        "profile_apps": "App: {apps}",
        "profile_install_apps": "Installa le app consigliate al primo avvio (connessione Internet richiesta)",
        "fullname_placeholder": "Utente Winlux",
        "migration_title": "Trasferisci da un altro PC",
        "migration_subtitle": "Recupera documenti, immagini e impostazioni da un PC della stessa rete. Il trasferimento avviene alla fine dell'installazione, direttamente nel tuo nuovo account.",
        "err_no_disk": "Nessun disco selezionato.",
        "err_iso_missing": "ISO di origine non trovata. Imposta WINLUX_ISO o avvia dalla Live USB.",
        "err_verify": "Verifica non riuscita:\n{error}",
        "err_boot_file": "File di avvio assente dal supporto: {path}",
        "err_manual_plan": "Piano di partizionamento manuale non valido.",
        "err_partitions_missing": "Partizioni non trovate dopo il partizionamento.",
        "err_resize_missing": "Partizioni Winlux non trovate dopo il ridimensionamento.",
        "err_partition_not_found": "Partizione non trovata: {path}",
        "warn_import_skipped": "Importazione Windows ignorata: {error}",
        "warn_network": "Avviso di rete: {error}",
        "warn_bookmarks": "Preferiti di {browser} ignorati: {error}",
        "progress_import_folder": "Importazione Windows: {folder}...",
        "warn_copy_partial": "Alcuni file di {folder} non sono stati copiati.",
        "warn_import": "Avviso importazione Windows: {error}",
        "warn_write_bench": "Misurazione della scrittura su disco non riuscita: {error}",
        "warn_transfer": "Avviso di trasferimento: {error}",
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
        "keyboard_name_fr": "AZERTY — Francês",
        "keyboard_name_us": "QWERTY — Inglês (EUA)",
        "keyboard_name_gb": "QWERTY — Inglês (Reino Unido)",
        "keyboard_name_de": "QWERTZ — Alemão",
        "keyboard_name_es": "QWERTY — Espanhol",
        "keyboard_name_it": "QWERTY — Italiano",
        "keyboard_name_br": "QWERTY — Português (Brasil)",
        "keyboard_name_ch": "QWERTZ — Suíço",
        "keyboard_name_be": "AZERTY — Belga",
        "keyboard_name_ca": "QWERTY — Canadense multilíngue",
        "keyboard_test_placeholder": "Digite aqui para testar...",
        "language_keyboard": "Teclado: {kb}",
        "disk_unknown": "Disco desconhecido",
        "network_proxy_placeholder": "proxy.empresa.local",
        "bench_title": "Avaliação de desempenho",
        "bench_subtitle": "Teste rápido do seu hardware (cerca de 10 segundos) para ajustar o Winlux da melhor forma. O disco é apenas lido: nada é gravado antes do início da instalação.",
        "bench_overall": "Nota geral",
        "bench_starting": "O teste está começando…",
        "bench_retry": "Executar novamente",
        "bench_cpu": "Processador",
        "bench_ram": "Memória",
        "bench_disk": "Disco",
        "bench_gpu": "Gráficos",
        "bench_waiting": "aguardando",
        "bench_tips": "Recomendações",
        "bench_running": "Teste em andamento… você pode continuar, ele termina em segundo plano.",
        "bench_failed": "Teste impossível: {error}",
        "bench_measuring": "medindo…",
        "bench_done": "Determinada pelo componente mais lento, numa escala de 1,0 a 9,9. A gravação em disco será medida durante a instalação.",
        "bench_no_tip": "Nenhum ajuste especial necessário.",
        "bench_stopped": "O teste parou antes do fim. Você pode executá-lo novamente ou continuar.",
        "profile_title": "Como você vai usar este PC?",
        "profile_subtitle": "O Winlux ajusta o processador, a memória, o disco e a placa de vídeo conforme o seu uso. Você pode mudar isso depois (winlux-profile --set).",
        "profile_recommended": "Recomendado para este PC",
        "profile_apps": "Aplicativos: {apps}",
        "profile_install_apps": "Instalar os aplicativos recomendados na primeira inicialização (conexão com a Internet necessária)",
        "fullname_placeholder": "Usuário do Winlux",
        "migration_title": "Transferir de outro PC",
        "migration_subtitle": "Recupere seus documentos, imagens e configurações de um PC da mesma rede. A transferência ocorre no fim da instalação, diretamente na sua nova conta.",
        "err_no_disk": "Nenhum disco selecionado.",
        "err_iso_missing": "ISO de origem não encontrada. Defina WINLUX_ISO ou inicie pelo Live USB.",
        "err_verify": "Falha na verificação:\n{error}",
        "err_boot_file": "Arquivo de inicialização ausente na mídia: {path}",
        "err_manual_plan": "Plano de particionamento manual inválido.",
        "err_partitions_missing": "Partições não encontradas após o particionamento.",
        "err_resize_missing": "Partições do Winlux não encontradas após o redimensionamento.",
        "err_partition_not_found": "Partição não encontrada: {path}",
        "warn_import_skipped": "Importação do Windows ignorada: {error}",
        "warn_network": "Aviso de rede: {error}",
        "warn_bookmarks": "Favoritos do {browser} ignorados: {error}",
        "progress_import_folder": "Importação do Windows: {folder}...",
        "warn_copy_partial": "Alguns arquivos de {folder} não puderam ser copiados.",
        "warn_import": "Aviso de importação do Windows: {error}",
        "warn_write_bench": "Não foi possível medir a gravação em disco: {error}",
        "warn_transfer": "Aviso de transferência: {error}",
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



def keyboard_name(layout):
    """Nom affiché d'une disposition de clavier, dans la langue de l'installateur."""
    key = "keyboard_name_" + layout
    name = tr(key)
    return dict(KEYBOARDS).get(layout, layout) if name == key else name

# Colonnes des tables _TR des modules (le français sert de clé).
MODULE_LANG_COLUMNS = {"en": 0, "de": 1, "es": 2, "it": 3, "pt": 4}


def relabel_map(old, new):
    """Texte affiché dans la langue `old` -> même texte dans la langue `new`."""
    mapping = {}
    french = TRANSLATIONS["fr_FR"]
    old_table, new_table = TRANSLATIONS.get(old, french), TRANSLATIONS.get(new, french)
    for key, value in french.items():
        if key.startswith("summary_") and key not in ("summary_title", "summary_subtitle"):
            continue  # lignes du résumé : recalculées à chaque affichage de la page
        before, after = old_table.get(key, value), new_table.get(key, value)
        if isinstance(before, str) and isinstance(after, str) and "{" not in before:
            mapping.setdefault(before, after)
    old_col, new_col = MODULE_LANG_COLUMNS.get(old[:2]), MODULE_LANG_COLUMNS.get(new[:2])
    for table in (winlux_benchmark._TR, winlux_profiles._TR, winlux_migrate._TR):
        for text, row in table.items():
            if "{" not in text:
                mapping.setdefault(text if old_col is None else row[old_col],
                                   text if new_col is None else row[new_col])
    return mapping


def relabel_widgets(widget, mapping):
    """Retraduit sur place les libellés et textes d'aide d'une arborescence GTK."""
    if isinstance(widget, Gtk.Label):
        text = widget.get_label()
        if text in mapping:
            widget.set_label(mapping[text])
    elif isinstance(widget, Gtk.Entry):
        hint = widget.get_placeholder_text()
        if hint in mapping:
            widget.set_placeholder_text(mapping[hint])
    if isinstance(widget, Gtk.Container):
        widget.forall(relabel_widgets, mapping)

for _lang, _table in winlux_partitioner.TRANSLATIONS.items():
    TRANSLATIONS.setdefault(_lang, {}).update(_table)

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
    "Réseau",
    "Partitionnement",
    "Import Windows",
    "Chiffrement",
    "Performances",
    "Profil d'utilisation",
    "Compte local",
    "Migration",
    "Résumé",
    "Installation",
    "Terminé",
]
# Clés des pages, dans le même ordre que STEPS et que winlux_autoinstall.STEP_KEYS.
STEP_KEYS = ["language", "keyboard", "license", "network", "disk", "import", "encryption",
             "benchmark", "profile", "account", "migration", "summary", "progress", "done"]


def step_index(key):
    return STEP_KEYS.index(key)


def page_name(key):
    return f"page_{key}"


def human_size(size_bytes):
    if size_bytes is None:
        return "?"
    for unit in ("o", "Ko", "Mo", "Go", "To"):
        if abs(size_bytes) < 1024:
            return f"{size_bytes:.1f} {unit}"
        size_bytes /= 1024
    return f"{size_bytes:.1f} Po"


def live_medium_disk(mounts="/proc/self/mounts", sysfs="/sys"):
    """Disque (« sdb ») du média Live dont Winlux a démarré : clé USB ou DVD.

    L'installateur ne doit jamais le proposer comme destination : effacer la
    clé d'installation en cours de route rendrait le PC inutilisable.
    """
    try:
        with open(mounts, encoding="utf-8") as f:
            lines = f.read().splitlines()
    except OSError:
        return None
    for line in lines:
        parts = line.split()
        if len(parts) >= 2 and parts[1] == "/run/winlux/live/media" and parts[0].startswith("/dev/"):
            name = os.path.basename(os.path.realpath(parts[0]))
            # Une partition (sdb1) désigne le disque qui la porte (sdb).
            if os.path.exists(os.path.join(sysfs, "class/block", name, "partition")):
                parent = os.path.realpath(os.path.join(sysfs, "class/block", name, ".."))
                return os.path.basename(parent)
            return name
    return None


def detect_disks():
    disks = []
    live_disk = live_medium_disk()
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
            if name == live_disk:
                continue  # la clé USB d'installation elle-même
            path = f"/dev/{name}"
            size = dev.get("size", 0) or 0
            if size < 2 * 1024**3:
                continue
            model = (dev.get("model") or "").strip() or tr("disk_unknown")
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
    raise RuntimeError(tr("err_partition_not_found", path=path))


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


def planned_layout(disk_size, partitions, install_type, luks, dualboot=None):
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
        self.set_size_request(-1, int(70 * UI_SCALE))
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
        cr.scale(UI_SCALE, UI_SCALE)
        w = widget.get_allocated_width() / UI_SCALE
        bar_y = 4
        bar_h = 36
        label_y = bar_y + bar_h + 14
        cr.select_font_face("sans-serif", 0, 0)  # slant/weight NORMAL, sans importer cairo
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


def nmcli_split(line):
    fields, cur, esc = [], [], False
    for ch in line:
        if esc:
            cur.append(ch)
            esc = False
        elif ch == "\\":
            esc = True
        elif ch == ":":
            fields.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    fields.append("".join(cur))
    return fields


def nm_available():
    if not shutil.which("nmcli"):
        return False
    try:
        out = subprocess.run(["nmcli", "-t", "-f", "RUNNING", "general"],
                             capture_output=True, text=True, timeout=5).stdout
        return out.strip() == "running"
    except Exception:
        return False


def network_status():
    ethernet, ssid = False, ""
    try:
        out = subprocess.run(["nmcli", "-t", "-f", "TYPE,STATE,CONNECTION", "device"],
                             capture_output=True, text=True, timeout=5).stdout
        for line in out.splitlines():
            f = nmcli_split(line)
            if len(f) < 3 or not f[1].startswith("connected"):
                continue
            if f[0] == "ethernet":
                ethernet = True
            elif f[0] == "wifi":
                ssid = f[2]
    except Exception:
        pass
    return ethernet, ssid


def scan_wifi():
    networks = {}
    try:
        out = subprocess.run(
            ["nmcli", "-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "yes"],
            capture_output=True, text=True, timeout=30).stdout
    except Exception:
        return []
    for line in out.splitlines():
        f = nmcli_split(line)
        if len(f) < 4 or not f[1]:
            continue
        signal_pct = int(f[2]) if f[2].isdigit() else 0
        security = f[3].strip() if f[3].strip() not in ("", "--") else ""
        net = {"ssid": f[1], "signal": signal_pct, "security": security, "in_use": f[0].strip() == "*"}
        old = networks.get(net["ssid"])
        if old is None or net["signal"] > old["signal"] or net["in_use"]:
            net["in_use"] = net["in_use"] or (old or {}).get("in_use", False)
            networks[net["ssid"]] = net
    return sorted(networks.values(), key=lambda n: (not n["in_use"], -n["signal"]))


def connect_wifi(ssid, password):
    cmd = ["nmcli", "device", "wifi", "connect", ssid]
    if password:
        cmd += ["password", password]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
        return r.returncode == 0, (r.stderr or r.stdout).strip()
    except Exception as e:
        return False, str(e)


def normalize_proxy(host, port):
    host = re.sub(r"^[a-zA-Z]+://", "", host.strip()).rstrip("/")
    if not re.fullmatch(r"[A-Za-z0-9](?:[A-Za-z0-9.-]{0,252})", host or ""):
        return None
    if not port.strip().isdigit() or not 1 <= int(port) <= 65535:
        return None
    return host, int(port)


# ── Import depuis une partition Windows ──────────────────────────

WIN_IMPORT_MOUNT = "/run/winlux/win-import"
WIN_IMPORT_STAGE = "/run/winlux/win-import-stage"
WIN_SKIP_USERS = {"public", "default", "default user", "all users", "defaultapppool",
                  "wdagutilityaccount", "desktop.ini"}
WIN_FOLDERS = [
    ("desktop", "Desktop", "Bureau"),
    ("documents", "Documents", "Documents"),
    ("pictures", "Pictures", "Images"),
    ("music", "Music", "Musique"),
    ("videos", "Videos", "Vidéos"),
]
WIN_JUNK = {"desktop.ini", "thumbs.db"}


def find_ci(parent, *names):
    """Résout un chemin Windows sans tenir compte de la casse."""
    path = parent
    for name in names:
        try:
            entries = os.listdir(path)
        except OSError:
            return None
        match = next((e for e in entries if e.lower() == name.lower()), None)
        if match is None:
            return None
        path = os.path.join(path, match)
    return path


def mount_ntfs_readonly(device, target):
    os.makedirs(target, exist_ok=True)
    for cmd in (["mount", "-t", "ntfs3", "-o", "ro", device, target],
                ["mount", "-t", "ntfs-3g", "-o", "ro", device, target],
                ["mount", "-o", "ro", device, target]):
        try:
            if subprocess.run(cmd, capture_output=True, timeout=30).returncode == 0:
                return True
        except Exception:
            continue
    return False


def detect_windows_installations():
    found = []
    try:
        out = subprocess.check_output(
            ["lsblk", "-lbJ", "-o", "NAME,PATH,FSTYPE,SIZE,PKNAME,MOUNTPOINT"], text=True, timeout=10)
        devices = json.loads(out).get("blockdevices", [])
    except Exception:
        return found
    for dev in devices:
        if (dev.get("fstype") or "").lower() != "ntfs":
            continue
        path = dev.get("path") or f"/dev/{dev['name']}"
        mount = dev.get("mountpoint")
        ours = False
        if not mount:
            mount = os.path.join(WIN_IMPORT_MOUNT, dev["name"])
            if not mount_ntfs_readonly(path, mount):
                continue
            ours = True
        users_dir = find_ci(mount, "Users")
        if not find_ci(mount, "Windows", "System32") or not users_dir:
            if ours:
                subprocess.run(["umount", mount], timeout=30)
            continue
        users = sorted(u for u in os.listdir(users_dir)
                       if u.lower() not in WIN_SKIP_USERS and os.path.isdir(os.path.join(users_dir, u)))
        found.append({
            "partition": path,
            "disk": dev.get("pkname") or "",
            "size": dev.get("size") or 0,
            "mount": mount,
            "ours": ours,
            "users_dir": users_dir,
            "users": users,
        })
    return found


def measure_tree(path, max_files=300000, max_seconds=20):
    count, size, start = 0, 0, time.monotonic()
    for root, _dirs, files in os.walk(path, followlinks=False):
        for name in files:
            if name.lower() in WIN_JUNK:
                continue
            try:
                st = os.lstat(os.path.join(root, name))
            except OSError:
                continue
            count += 1
            size += st.st_size
        if count >= max_files or time.monotonic() - start > max_seconds:
            return count, size, True
    return count, size, False


def scan_windows_profile(users_dir, user):
    home = os.path.join(users_dir, user)
    data = {"home": home, "wallpaper": None, "bookmarks": [], "folders": {}}
    wp = find_ci(home, "AppData", "Roaming", "Microsoft", "Windows", "Themes", "TranscodedWallpaper")
    if wp and os.path.isfile(wp) and os.path.getsize(wp) > 0:
        data["wallpaper"] = wp
    for browser, parts in (("Edge", ("AppData", "Local", "Microsoft", "Edge", "User Data", "Default", "Bookmarks")),
                           ("Chrome", ("AppData", "Local", "Google", "Chrome", "User Data", "Default", "Bookmarks"))):
        p = find_ci(home, *parts)
        if p and os.path.isfile(p):
            data["bookmarks"].append({"browser": browser, "kind": "chromium", "path": p})
    profiles = find_ci(home, "AppData", "Roaming", "Mozilla", "Firefox", "Profiles")
    if profiles:
        places = [os.path.join(profiles, d, "places.sqlite") for d in os.listdir(profiles)]
        places = [p for p in places if os.path.isfile(p)]
        if places:
            best = max(places, key=os.path.getmtime)
            data["bookmarks"].append({"browser": "Firefox", "kind": "firefox", "path": best})
    for key, win_name, _dest in WIN_FOLDERS:
        p = find_ci(home, win_name)
        if p and os.path.isdir(p):
            count, size, partial = measure_tree(p)
            if count:
                data["folders"][key] = {"path": p, "count": count, "size": size, "partial": partial}
    return data


def _bookmark_html(tree):
    lines = ["<!DOCTYPE NETSCAPE-Bookmark-file-1>",
             '<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">',
             "<TITLE>Bookmarks</TITLE>", "<H1>Bookmarks</H1>", "<DL><p>"]

    def walk(node, depth):
        pad = "    " * depth
        if node["url"] is not None:
            if re.match(r"^(https?|ftp)://", node["url"], re.I):
                lines.append(f'{pad}<DT><A HREF="{html.escape(node["url"])}">{html.escape(node["title"] or node["url"])}</A>')
            return
        lines.append(f"{pad}<DT><H3>{html.escape(node['title'] or 'Dossier')}</H3>")
        lines.append(f"{pad}<DL><p>")
        for child in node["children"]:
            walk(child, depth + 1)
        lines.append(f"{pad}</DL><p>")

    for node in tree:
        walk(node, 1)
    lines.append("</DL><p>")
    return "\n".join(lines) + "\n"


def chromium_bookmarks_tree(path):
    with open(path, encoding="utf-8") as f:
        data = json.load(f)

    def conv(n):
        if n.get("type") == "url":
            return {"title": n.get("name", ""), "url": n.get("url", ""), "children": []}
        return {"title": n.get("name", ""), "url": None,
                "children": [conv(c) for c in n.get("children", [])]}

    roots = data.get("roots", {})
    return [conv(roots[k]) for k in ("bookmark_bar", "other", "synced") if k in roots and roots[k].get("children")]


def firefox_bookmarks_tree(path, workdir):
    import sqlite3
    copy = os.path.join(workdir, "places.sqlite")
    shutil.copyfile(path, copy)
    if os.path.exists(path + "-wal"):
        shutil.copyfile(path + "-wal", copy + "-wal")
    con = sqlite3.connect(copy)
    try:
        rows = con.execute(
            "SELECT b.id, b.parent, b.type, b.title, p.url FROM moz_bookmarks b "
            "LEFT JOIN moz_places p ON b.fk = p.id ORDER BY b.parent, b.position").fetchall()
    finally:
        con.close()
    nodes = {}
    for bid, parent, typ, title, url in rows:
        if typ not in (1, 2):
            continue
        nodes[bid] = {"title": title or "", "url": (url or "") if typ == 1 else None,
                      "children": [], "parent": parent}
    for bid, node in nodes.items():
        if node["parent"] in nodes and bid != node["parent"]:
            nodes[node["parent"]]["children"].append(node)
    # 1 = racine, 4 = étiquettes (ignorées)
    return [n for bid, n in nodes.items() if n["parent"] == 1 and bid != 4 and n["children"]]


def detect_image_ext(path):
    with open(path, "rb") as f:
        head = f.read(8)
    if head.startswith(b"\x89PNG"):
        return ".png"
    if head.startswith(b"BM"):
        return ".bmp"
    return ".jpg"


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
            "password_hash": "",
            "timezone": None,
            "packages": [],
            "proxy_enabled": False,
            "proxy_host": "",
            "proxy_port": "",
            "network_ethernet": False,
            "import_items": [],
        }
        self.answer_path = None
        self.answer_settings = {}
        self.skip_steps = set()
        self.disks = []
        self.wifi_networks = []
        self.wifi_connected_ssid = ""
        self.win_partitions = []
        self.hw_info = {}
        self.disk_partitions = {}
        self.detected_os = {}
        self.dualboot_alloc_mb = 0
        self.dualboot_target = None
        self.luks_supported = False
        self.detect_done = False
        self.install_thread = None
        self.install_cancelled = False
        self.bench_state = "idle"
        self.bench_device = None
        self.bench_proc = None
        self.benchmark = None
        self.profile_chosen_by_user = False
        self.config["profile"] = "office"
        self.config["install_apps"] = True

    def apply_display_scale(self):
        """Échelle de l'interface selon la résolution et la densité réelles de l'écran."""
        global UI_SCALE
        geom, gdk_factor, width_mm, height_mm = winlux_screen.monitor_info(
            Gdk, getattr(self, "win", None))
        scale = winlux_screen.compute_scale(
            geom.width * gdk_factor, geom.height * gdk_factor, width_mm, height_mm,
            override=winlux_screen.scale_override())
        # GDK applique déjà son facteur entier (GDK_SCALE) : on ne fait que le complément.
        UI_SCALE = max(0.5, scale / gdk_factor)
        css = winlux_screen.scale_css(CSS + winlux_partition_ui.CSS, UI_SCALE)
        if not hasattr(self, "css_provider"):
            self.css_provider = Gtk.CssProvider()
            Gtk.StyleContext.add_provider_for_screen(
                Gdk.Screen.get_default(), self.css_provider,
                Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        self.css_provider.load_from_data(css)
        return geom

    def fit_window_to_screen(self, *_args):
        old_scale = UI_SCALE
        geom = self.apply_display_scale()
        if os.environ.get("WINLUX_INSTALLER_WINDOWED") == "1":
            self.win.resize(min(geom.width, int(1100 * UI_SCALE)),
                            min(geom.height, int(720 * UI_SCALE)))
            return
        # Sans gestionnaire de fenêtres (Xorg nu du live), fullscreen() est ignoré :
        # on positionne aussi la fenêtre explicitement sur tout le moniteur.
        self.win.move(geom.x, geom.y)
        self.win.resize(geom.width, geom.height)
        self.win.fullscreen()
        if abs(old_scale - UI_SCALE) > 0.01 and hasattr(self, "partition_bar"):
            for bar in (self.partition_bar, self.partition_bar_after, self.manual_editor.bar):
                bar.set_size_request(-1, int(70 * UI_SCALE))

    def on_key_press(self, _win, event):
        if event.keyval == Gdk.KEY_F11:
            state = self.win.get_window().get_state() if self.win.get_window() else 0
            if state & Gdk.WindowState.FULLSCREEN:
                self.win.unfullscreen()
            else:
                self.win.fullscreen()
            return True
        return False

    def make_pages_scrollable(self):
        """Toutes les pages défilent : Suivant/Retour restent visibles même sur un petit écran."""
        for child in list(self.pages.get_children()):
            if isinstance(child, Gtk.ScrolledWindow):
                child.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
                continue
            name = self.pages.child_get_property(child, "name")
            self.pages.remove(child)
            scroll = Gtk.ScrolledWindow()
            scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
            scroll.add(child)
            self.pages.add_named(scroll, name)

    def do_activate(self):
        self.apply_display_scale()
        self.win = Gtk.ApplicationWindow(application=self, title="Installer Winlux")
        self.win.get_style_context().add_class("installer-window")
        self.win.set_position(Gtk.WindowPosition.CENTER)
        self.win.set_resizable(True)
        self.win.connect("key-press-event", self.on_key_press)
        self.main_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        self.win.add(self.main_box)
        self.sidebar = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.sidebar.get_style_context().add_class("installer-sidebar")
        self.sidebar.set_size_request(int(210 * UI_SCALE), -1)
        brand = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=int(10 * UI_SCALE))
        brand.set_margin_bottom(24)
        # Logo Winlux « Prisme » (assets/brand), à la taille de l'écran.
        logo_size = 32 if UI_SCALE < 1.5 else 64
        logo_path = f"/usr/share/winlux/brand/winlux-logo-{logo_size}.png"
        if os.path.exists(logo_path):
            brand.pack_start(Gtk.Image.new_from_file(logo_path), False, False, 0)
        logo = Gtk.Label(label="Winlux")
        logo.get_style_context().add_class("page-title")
        brand.pack_start(logo, False, False, 0)
        self.sidebar.pack_start(brand, False, False, 0)
        self.step_labels = []
        for i, name in enumerate(STEPS):
            lbl = Gtk.Label(label=f"  {i + 1}. {name}", xalign=0)
            lbl.get_style_context().add_class("sidebar-step")
            self.step_labels.append(lbl)
            self.sidebar.pack_start(lbl, False, False, 0)
        sidebar_scroll = Gtk.ScrolledWindow()
        sidebar_scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        sidebar_scroll.add(self.sidebar)
        self.main_box.pack_start(sidebar_scroll, False, False, 0)
        self.content_area = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.content_area.get_style_context().add_class("installer-content")
        self.main_box.pack_start(self.content_area, True, True, 0)
        self.pages = Gtk.Stack()
        self.pages.set_transition_type(Gtk.StackTransitionType.SLIDE_LEFT)
        self.content_area.pack_start(self.pages, True, True, 0)
        self.nav_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.nav_box.set_margin_top(16)
        self.btn_back = Gtk.Button(label=tr("btn_back"))
        self.btn_back.get_style_context().add_class("btn-secondary")
        self.btn_back.connect("clicked", self.on_back)
        self.btn_next = Gtk.Button(label=tr("btn_next"))
        self.btn_next.get_style_context().add_class("btn-primary")
        self.btn_next.connect("clicked", self.on_next)
        self.nav_box.pack_end(self.btn_next, False, False, 0)
        self.nav_box.pack_end(self.btn_back, False, False, 0)
        self.content_area.pack_end(self.nav_box, False, False, 0)
        self.ui_lang = tr._lang
        self.lang_keyboard_labels = []
        self.profile_tune_labels = {}
        self.profile_apps_labels = {}
        self.build_language_page()
        self.build_keyboard_page()
        self.build_license_page()
        self.build_network_page()
        self.build_partition_page()
        self.build_import_page()
        self.build_encryption_page()
        self.build_benchmark_page()
        self.build_profile_page()
        self.build_account_page()
        self.build_migration_page()
        self.build_summary_page()
        self.build_progress_page()
        self.build_done_page()
        self.make_pages_scrollable()
        self.load_answer_file()
        self.current_step = self.next_step(-1, 1)
        self.update_step()
        self.fit_window_to_screen()
        screen = Gdk.Screen.get_default()
        screen.connect("size-changed", self.fit_window_to_screen)
        screen.connect("monitors-changed", self.fit_window_to_screen)
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
        GLib.idle_add(self.apply_answer_disk)

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

    # ── Fichier de réponse (installation silencieuse) ────────────

    def load_answer_file(self):
        try:
            self.answer_path = autoinstall.find_answer_file()
            if not self.answer_path:
                return
            settings, steps = autoinstall.load(self.answer_path)
        except (autoinstall.AnswerFileError, OSError) as error:
            self.answer_path = None
            self.show_message(tr("answer_file_ignored", error=error))
            return
        self.answer_settings = settings
        for key in ("language", "keyboard", "license_accepted", "install_type", "hostname",
                    "username", "fullname", "password", "password_hash", "autologin", "timezone", "packages"):
            if key in settings:
                self.config[key] = settings[key]
        self.skip_steps = set(steps)
        self.skip_steps.discard(autoinstall.STEP_DISK)
        self.skip_steps.discard(autoinstall.STEP_SUMMARY)
        if autoinstall.STEP_LANGUAGE in steps:
            self.update_ui_language()

    def apply_answer_disk(self):
        request = self.answer_settings.get("disk_request")
        if not request:
            return
        disk = autoinstall.resolve_disk(request, self.disks)
        if disk is None:
            self.answer_settings["unattended"] = False
            self.show_message(tr("answer_disk_missing", disk=request))
            return
        self.config["disk"] = disk
        self.skip_steps.add(autoinstall.STEP_DISK)
        if self.answer_settings.get("unattended"):
            self.skip_steps.add(autoinstall.STEP_SUMMARY)
            self.refresh_summary()
            self.start_installation()
        elif STEP_KEYS[self.current_step] == "disk":
            self.current_step = self.next_step(self.current_step, 1)
            self.update_step()

    def next_step(self, step, direction):
        step += direction
        while 0 <= step < autoinstall.STEP_SUMMARY + 1 and step in self.skip_steps:
            step += direction
        return max(step, 0)

    def show_message(self, text):
        dialog = Gtk.MessageDialog(transient_for=getattr(self, "win", None), modal=True,
                                   message_type=Gtk.MessageType.WARNING,
                                   buttons=Gtk.ButtonsType.OK, text=text)
        dialog.run()
        dialog.destroy()

    def update_step(self):
        for i, lbl in enumerate(self.step_labels):
            ctx = lbl.get_style_context()
            ctx.remove_class("active")
            ctx.remove_class("done")
            if i == self.current_step:
                ctx.add_class("active")
            elif i < self.current_step:
                ctx.add_class("done")
        key = STEP_KEYS[self.current_step]
        if key == "summary":
            self.refresh_summary()
        self.pages.set_visible_child_name(page_name(key))
        if key == "network" and not self.wifi_scanned:
            self.on_wifi_scan(None)
        if key == "benchmark" and self.bench_state == "idle":
            self.start_benchmark()
        previous = self.next_step(self.current_step, -1)
        self.btn_back.set_visible(0 < self.current_step < step_index("progress") and previous < self.current_step
                                  and previous not in self.skip_steps)
        if key == "summary":
            self.btn_next.set_label(tr("btn_install"))
            self.btn_next.get_style_context().remove_class("btn-primary")
            self.btn_next.get_style_context().add_class("btn-danger")
            self.btn_next.set_visible(True)
        elif self.current_step >= step_index("progress"):
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
        key = STEP_KEYS[self.current_step]
        if key == "license":
            ok = self.config["license_accepted"]
        elif key == "network":
            ok = not self.config["proxy_enabled"] or self.proxy_settings() is not None
            self.proxy_error.set_text("" if ok else tr("network_proxy_invalid"))
        elif key == "disk":
            itype = self.config["install_type"]
            ok = self.config["disk"] is not None and (
                itype == "full"
                or (itype == "dualboot" and self.dualboot_target is not None)
                or (itype == "manual" and self.manual_ready()))
        elif key == "import":
            fits = self.import_fits()
            ok = self.win_detect_state == "done" and fits
            self.import_space.set_text("" if fits else tr("import_too_big"))
        elif key == "encryption":
            if self.config["luks_enabled"]:
                pp = self.config["luks_passphrase"]
                pp_confirm = self.luks_confirm_entry.get_text() if hasattr(self, 'luks_confirm_entry') else ""
                ok = len(pp) >= 8 and pp == pp_confirm and passphrase_is_ascii(pp)
            else:
                ok = True
        elif key == "account":
            ok = (len(self.config["username"]) >= 2
                  and len(self.config["password"]) >= 1
                  and re.match(r'^[a-z_][a-z0-9_-]*$', self.config["username"]) is not None)
        elif key == "migration":
            ok = self.migration.ready()
        self.btn_next.set_sensitive(ok)

    def on_back(self, _btn):
        previous = self.next_step(self.current_step, -1)
        if previous < self.current_step and previous not in self.skip_steps:
            self.current_step = previous
            self.update_step()

    def on_next(self, _btn):
        key = STEP_KEYS[self.current_step]
        if key == "summary":
            if self.config["install_type"] == "manual" and not winlux_partition_ui.confirm_apply(
                    self.win, self.manual_editor.plan, tr, UI_SCALE):
                return
            self.start_installation()
        elif self.current_step < len(STEPS) - 1:
            if key == "language":
                self.update_ui_language()
            target = self.next_step(self.current_step, 1)
            if key == "network":
                self.apply_live_proxy()
            if target > step_index("disk") and self.bench_device != (self.config["disk"] or {}).get("path"):
                # Le benchmark lit le disque choisi : on le relance si le disque a changé.
                self.bench_state = "idle"
            if target == step_index("import"):
                # Aussi quand le fichier de réponse a fait sauter la page des disques.
                self.start_windows_detection()
            if target == step_index("encryption"):
                self.refresh_encryption_page()
            if target == step_index("summary"):
                self.refresh_summary()
            self.current_step = target
            self.update_step()

    # ── Page : Langue ──────────────────────────────────────────

    def build_language_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("choose_language"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("language_subtitle"), xalign=0)
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
            kb_label = Gtk.Label(label=tr("language_keyboard", kb=kb))
            self.lang_keyboard_labels.append((kb_label, kb))
            kb_label.get_style_context().add_class("card-detail")
            row.pack_end(kb_label, False, False, 8)
            box.pack_start(row, False, False, 0)
        self.pages.add_named(box, page_name("language"))

    def on_lang_toggle(self, btn, locale, kb):
        if btn.get_active():
            self.config["language"] = locale
            self.config["keyboard"] = kb.split("(")[1].rstrip(")")

    def update_ui_language(self):
        previous = self.ui_lang
        tr._lang = self.config["language"]
        # Modules du benchmark, des profils et du transfert : même langue que l'installateur.
        os.environ["WINLUX_LANG"] = tr._lang[:2]
        steps = tr("steps")
        for i, lbl in enumerate(self.step_labels):
            if i < len(steps):
                lbl.set_text(f"  {i + 1}. {steps[i]}")
        if previous == tr._lang:
            return
        # Les pages sont construites une seule fois : on retraduit leurs textes sur
        # place, sans rien reconstruire, pour garder la saisie et les choix en cours.
        self.ui_lang = tr._lang
        relabel_widgets(self.win, relabel_map(previous, tr._lang))
        for label, kb in self.lang_keyboard_labels:
            label.set_text(tr("language_keyboard", kb=kb))
        for pid, label in self.profile_tune_labels.items():
            label.set_text(winlux_profiles.summary_line(pid))
        for pid, label in self.profile_apps_labels.items():
            label.set_text(tr("profile_apps", apps=", ".join(winlux_profiles.PROFILES[pid]["apps"])))

    # ── Page : Clavier ─────────────────────────────────────────

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
            rb = Gtk.RadioButton.new_with_label_from_widget(self.kb_group, keyboard_name(layout))
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
        test_entry.set_placeholder_text(tr("keyboard_test_placeholder"))
        box.pack_start(test_entry, False, False, 0)
        self.pages.add_named(box, page_name("keyboard"))

    def on_kb_toggle(self, btn, layout):
        if btn.get_active():
            self.config["keyboard"] = layout

    # ── Page : Licence ─────────────────────────────────────────

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
        self.pages.add_named(box, page_name("license"))

    def on_license_toggle(self, btn):
        self.config["license_accepted"] = btn.get_active()
        self.validate_step()

    # ── Page : Réseau ──────────────────────────────────────────

    def build_network_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("network_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("network_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        sub.set_line_wrap(True)
        box.pack_start(sub, False, False, 0)
        status_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        status_row.get_style_context().add_class("card")
        self.net_status = Gtk.Label(label=tr("network_status_none"), xalign=0)
        self.net_status.get_style_context().add_class("card-title")
        self.net_status.set_ellipsize(Pango.EllipsizeMode.END)
        status_row.pack_start(self.net_status, True, True, 0)
        self.btn_scan = Gtk.Button(label=tr("network_scan"))
        self.btn_scan.get_style_context().add_class("btn-secondary")
        self.btn_scan.connect("clicked", self.on_wifi_scan)
        status_row.pack_end(self.btn_scan, False, False, 0)
        box.pack_start(status_row, False, False, 0)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_min_content_height(150)
        self.wifi_list = Gtk.ListBox()
        self.wifi_list.get_style_context().add_class("wifi-list")
        self.wifi_list.set_selection_mode(Gtk.SelectionMode.SINGLE)
        self.wifi_list.connect("row-selected", self.on_wifi_selected)
        scroll.add(self.wifi_list)
        box.pack_start(scroll, True, True, 0)
        connect_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.wifi_password = Gtk.Entry()
        self.wifi_password.get_style_context().add_class("input-field")
        self.wifi_password.set_visibility(False)
        self.wifi_password.set_input_purpose(Gtk.InputPurpose.PASSWORD)
        self.wifi_password.set_placeholder_text(tr("network_password"))
        self.wifi_password.set_hexpand(True)
        self.wifi_password.set_sensitive(False)
        self.wifi_password.connect("activate", self.on_wifi_connect)
        connect_row.pack_start(self.wifi_password, True, True, 0)
        self.btn_wifi_connect = Gtk.Button(label=tr("network_connect"))
        self.btn_wifi_connect.get_style_context().add_class("btn-primary")
        self.btn_wifi_connect.set_sensitive(False)
        self.btn_wifi_connect.connect("clicked", self.on_wifi_connect)
        connect_row.pack_end(self.btn_wifi_connect, False, False, 0)
        box.pack_start(connect_row, False, False, 0)
        self.net_message = Gtk.Label(label="", xalign=0)
        self.net_message.get_style_context().add_class("progress-text")
        self.net_message.set_line_wrap(True)
        box.pack_start(self.net_message, False, False, 0)
        proxy_title = Gtk.Label(label=tr("network_proxy_title"), xalign=0)
        proxy_title.get_style_context().add_class("card-title")
        proxy_title.set_margin_top(12)
        box.pack_start(proxy_title, False, False, 0)
        self.proxy_check = Gtk.CheckButton(label=tr("network_proxy_enable"))
        self.proxy_check.connect("toggled", self.on_proxy_changed)
        box.pack_start(self.proxy_check, False, False, 0)
        proxy_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        host_lbl = Gtk.Label(label=tr("network_proxy_host"))
        host_lbl.get_style_context().add_class("input-label")
        proxy_row.pack_start(host_lbl, False, False, 0)
        self.proxy_host = Gtk.Entry()
        self.proxy_host.get_style_context().add_class("input-field")
        self.proxy_host.set_placeholder_text(tr("network_proxy_placeholder"))
        self.proxy_host.set_hexpand(True)
        self.proxy_host.connect("changed", self.on_proxy_changed)
        proxy_row.pack_start(self.proxy_host, True, True, 0)
        port_lbl = Gtk.Label(label=tr("network_proxy_port"))
        port_lbl.get_style_context().add_class("input-label")
        proxy_row.pack_start(port_lbl, False, False, 0)
        self.proxy_port = Gtk.Entry()
        self.proxy_port.get_style_context().add_class("input-field")
        self.proxy_port.set_placeholder_text("3128")
        self.proxy_port.set_width_chars(6)
        self.proxy_port.set_max_length(5)
        self.proxy_port.connect("changed", self.on_proxy_changed)
        proxy_row.pack_start(self.proxy_port, False, False, 0)
        box.pack_start(proxy_row, False, False, 0)
        self.proxy_error = Gtk.Label(label="", xalign=0)
        self.proxy_error.get_style_context().add_class("warning-text")
        box.pack_start(self.proxy_error, False, False, 0)
        self.proxy_host.set_sensitive(False)
        self.proxy_port.set_sensitive(False)
        self.wifi_scanned = False
        self.wifi_selected = None
        self.pages.add_named(box, page_name("network"))

    def on_wifi_scan(self, _btn):
        self.wifi_scanned = True
        self.btn_scan.set_sensitive(False)
        self.btn_scan.set_label(tr("network_scanning"))

        def work():
            if not nm_available():
                GLib.idle_add(self.on_wifi_unavailable)
                return
            nets = scan_wifi()
            status = network_status()
            GLib.idle_add(self.refresh_wifi_list, nets, status)
        threading.Thread(target=work, daemon=True).start()

    def on_wifi_unavailable(self):
        self.btn_scan.set_label(tr("network_scan"))
        self.btn_scan.set_sensitive(True)
        self.net_message.set_text(tr("network_no_nm"))
        return False

    def refresh_wifi_list(self, nets, status):
        self.btn_scan.set_label(tr("network_scan"))
        self.btn_scan.set_sensitive(True)
        ethernet, ssid = status
        self.wifi_connected_ssid = ssid
        self.config["network_ethernet"] = ethernet
        parts = []
        if ssid:
            parts.append(tr("network_status_connected", ssid=ssid))
        if ethernet:
            parts.append(tr("network_ethernet"))
        self.net_status.set_text("  ·  ".join(parts) or tr("network_status_none"))
        ctx = self.net_status.get_style_context()
        (ctx.add_class if parts else ctx.remove_class)("status-ok")
        for child in self.wifi_list.get_children():
            child.destroy()
        self.wifi_selected = None
        self.wifi_password.set_sensitive(False)
        self.btn_wifi_connect.set_sensitive(False)
        if not nets:
            row = Gtk.ListBoxRow()
            row.set_selectable(False)
            lbl = Gtk.Label(label=tr("network_none"), xalign=0)
            lbl.get_style_context().add_class("card-detail")
            row.add(lbl)
            self.wifi_list.add(row)
        for net in nets:
            row = Gtk.ListBoxRow()
            row.net = net
            line = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
            line.get_style_context().add_class("card")
            name = Gtk.Label(label=net["ssid"], xalign=0)
            name.get_style_context().add_class("card-title")
            name.set_ellipsize(Pango.EllipsizeMode.END)
            line.pack_start(name, True, True, 0)
            bars = "▂▄▆█"[:min(4, 1 + net["signal"] // 26)]
            detail = f"{net['security'] or tr('network_open')}   {bars} {net['signal']} %"
            if net["in_use"] or net["ssid"] == ssid:
                detail = f"{tr('network_connected')}  ·  {detail}"
            d = Gtk.Label(label=detail)
            d.get_style_context().add_class("card-detail")
            line.pack_end(d, False, False, 0)
            row.add(line)
            self.wifi_list.add(row)
        self.wifi_list.show_all()
        return False

    def on_wifi_selected(self, _listbox, row):
        self.wifi_selected = getattr(row, "net", None) if row else None
        secured = bool(self.wifi_selected and self.wifi_selected["security"])
        self.wifi_password.set_sensitive(secured)
        if not secured:
            self.wifi_password.set_text("")
        self.btn_wifi_connect.set_sensitive(self.wifi_selected is not None)
        if secured:
            self.wifi_password.grab_focus()

    def on_wifi_connect(self, _widget):
        net = self.wifi_selected
        if not net:
            return
        password = self.wifi_password.get_text()
        self.btn_wifi_connect.set_sensitive(False)
        self.net_message.get_style_context().remove_class("warning-text")
        self.net_message.set_text(tr("network_connecting", ssid=net["ssid"]))

        def work():
            ok, err = connect_wifi(net["ssid"], password)
            GLib.idle_add(self.on_wifi_result, ok, err)
        threading.Thread(target=work, daemon=True).start()

    def on_wifi_result(self, ok, err):
        self.btn_wifi_connect.set_sensitive(True)
        if ok:
            self.net_message.set_text("")
            self.wifi_password.set_text("")
            self.on_wifi_scan(None)
        else:
            self.net_message.get_style_context().add_class("warning-text")
            self.net_message.set_text(tr("network_failed", error=err or "?"))
        return False

    def on_proxy_changed(self, *_args):
        enabled = self.proxy_check.get_active()
        self.proxy_host.set_sensitive(enabled)
        self.proxy_port.set_sensitive(enabled)
        self.config["proxy_enabled"] = enabled
        self.config["proxy_host"] = self.proxy_host.get_text()
        self.config["proxy_port"] = self.proxy_port.get_text()
        self.validate_step()

    def proxy_settings(self):
        if not self.config["proxy_enabled"]:
            return None
        return normalize_proxy(self.config["proxy_host"], self.config["proxy_port"])

    def apply_live_proxy(self):
        proxy = self.proxy_settings()
        for var in ("http_proxy", "https_proxy", "ftp_proxy"):
            for name in (var, var.upper()):
                if proxy:
                    os.environ[name] = f"http://{proxy[0]}:{proxy[1]}/"
                else:
                    os.environ.pop(name, None)
        if proxy:
            os.environ["no_proxy"] = os.environ["NO_PROXY"] = "localhost,127.0.0.1,::1"

    # ── Page : Partitionnement ─────────────────────────────────

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
        self.partition_bar_after_label = after_label
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
        # Éditeur complet : choisir une partition existante pour /, ou créer, supprimer,
        # redimensionner et formater. Rien n'est écrit avant la confirmation finale.
        self.manual_editor = winlux_partition_ui.ManualPartitionEditor(
            tr, PartitionBar(), self.validate_step, scale=UI_SCALE)
        self.manual_box.pack_start(self.manual_editor, True, True, 0)
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
        self.pages.add_named(scroll, page_name("disk"))

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

        if self.config["install_type"] == "manual":
            self.manual_editor.set_disk(disk)
        self.refresh_layout_preview()
        self.validate_step()

    def manual_ready(self):
        return self.config.get("disk") is not None and self.manual_editor.is_valid()

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
        # En mode manuel, l'éditeur dessine lui-même le disque prévu.
        self.partition_bar_after_label.set_visible(itype != "manual")
        self.partition_bar_after.set_visible(itype != "manual")
        segments = planned_layout(disk["size"], self.disk_partitions.get(disk["path"], []), itype,
                                  self.config["luks_enabled"], dualboot)
        self.partition_bar_after.set_data(disk["size"], segments)
        if itype == "full":
            self.part_erase_warn.set_text(tr("part_erase_warn"))
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
        if install_type == "manual":
            self.manual_editor.set_disk(self.config["disk"])
        if install_type == "manual" and self.config["luks_enabled"]:
            self.luks_check.set_active(False)
        self.refresh_layout_preview()
        self.validate_step()

    # ── Page : Import Windows ──────────────────────────────────

    def build_import_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("import_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        self.import_sub = Gtk.Label(label=tr("import_scanning"), xalign=0)
        self.import_sub.get_style_context().add_class("page-subtitle")
        self.import_sub.set_line_wrap(True)
        box.pack_start(self.import_sub, False, False, 0)
        self.import_source = Gtk.ComboBoxText()
        self.import_source.connect("changed", lambda _c: self.refresh_import_page())
        self.import_source.set_no_show_all(True)
        box.pack_start(self.import_source, False, False, 0)
        self.import_warn = Gtk.Label(label="", xalign=0)
        self.import_warn.get_style_context().add_class("disk-warn")
        self.import_warn.set_line_wrap(True)
        box.pack_start(self.import_warn, False, False, 0)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        self.import_items_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        scroll.add(self.import_items_box)
        box.pack_start(scroll, True, True, 0)
        sel_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        for key, value in (("import_select_all", True), ("import_select_none", False)):
            b = Gtk.Button(label=tr(key))
            b.get_style_context().add_class("btn-secondary")
            b.connect("clicked", self.on_import_select, value)
            sel_row.pack_start(b, False, False, 0)
        self.import_total = Gtk.Label(label="", xalign=1)
        self.import_total.get_style_context().add_class("progress-text")
        sel_row.pack_end(self.import_total, False, False, 0)
        box.pack_start(sel_row, False, False, 0)
        self.import_space = Gtk.Label(label="", xalign=0)
        self.import_space.get_style_context().add_class("warning-text")
        box.pack_start(self.import_space, False, False, 0)
        note = Gtk.Label(label=tr("import_note"), xalign=0)
        note.get_style_context().add_class("card-detail")
        note.set_line_wrap(True)
        box.pack_start(note, False, False, 0)
        self.win_detect_state = "idle"
        self.win_sources = []
        self.import_checks = {}
        self.import_src_current = None
        self.import_on_target = False
        self.import_total_bytes = 0
        self.import_plan = None
        self.pages.add_named(box, page_name("import"))

    def start_windows_detection(self):
        if self.win_detect_state == "idle":
            self.win_detect_state = "running"
            self.import_sub.set_text(tr("import_scanning"))
            threading.Thread(target=self.bg_detect_windows, daemon=True).start()
        else:
            self.refresh_import_page()

    def bg_detect_windows(self):
        parts = detect_windows_installations()
        sources = []
        for part in parts:
            for user in part["users"]:
                sources.append({"part": part, "user": user,
                                "data": scan_windows_profile(part["users_dir"], user)})
        GLib.idle_add(self.on_windows_detected, parts, sources)

    def on_windows_detected(self, parts, sources):
        self.win_partitions = parts
        self.win_sources = sources
        self.win_detect_state = "done"
        self.import_source.remove_all()
        for s in sources:
            p = s["part"]
            self.import_source.append_text(
                f"{tr('import_user', user=s['user'])}  —  "
                f"{tr('import_partition', partition=p['partition'], size=human_size(p['size']))}")
        if sources:
            self.import_source.set_active(0)
        self.refresh_import_page()
        return False

    def refresh_import_page(self):
        if self.win_detect_state != "done":
            return
        for child in self.import_items_box.get_children():
            child.destroy()
        self.import_checks = {}
        if not self.win_sources:
            self.import_sub.set_text(tr("import_none"))
            self.import_source.hide()
            self.import_warn.set_text("")
            self.import_src_current = None
            self.on_import_toggled()
            return
        self.import_sub.set_text(tr("import_subtitle"))
        self.import_source.show()
        src = self.win_sources[max(self.import_source.get_active(), 0)]
        disk = self.config.get("disk") or {}
        same_disk = bool(disk) and src["part"]["disk"] == disk.get("name")
        # Disque entier : la partition Windows est effacée. Dual-boot : elle est conservée.
        on_target = same_disk and self.config["install_type"] != "dualboot"
        self.import_src_current = src
        self.import_on_target = on_target
        if on_target:
            self.import_warn.set_text(tr("import_on_target"))
        elif same_disk:
            self.import_warn.set_text(tr("import_dualboot_keep"))
        else:
            self.import_warn.set_text("")
        data = src["data"]
        entries = []
        if data["wallpaper"]:
            entries.append(("wallpaper", tr("import_wallpaper"), True))
        for b in data["bookmarks"]:
            entries.append((f"bookmarks:{b['browser']}", tr("import_bookmarks", browser=b["browser"]), True))
        for key, _win, _dest in WIN_FOLDERS:
            f = data["folders"].get(key)
            if f:
                count = f"{f['count']}{'+' if f['partial'] else ''}"
                entries.append((key, tr(f"import_{key}", count=count, size=human_size(f["size"])), not on_target))
        if not entries:
            lbl = Gtk.Label(label=tr("import_none"), xalign=0)
            lbl.get_style_context().add_class("card-detail")
            self.import_items_box.pack_start(lbl, False, False, 0)
        for key, label, allowed in entries:
            row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
            row.get_style_context().add_class("card")
            cb = Gtk.CheckButton(label=label)
            cb.set_active(allowed)
            cb.set_sensitive(allowed)
            cb.connect("toggled", lambda _b: self.on_import_toggled())
            row.pack_start(cb, True, True, 0)
            self.import_items_box.pack_start(row, False, False, 0)
            self.import_checks[key] = cb
        self.import_items_box.show_all()
        self.on_import_toggled()

    def on_import_select(self, _btn, value):
        for cb in self.import_checks.values():
            if cb.get_sensitive():
                cb.set_active(value)

    def on_import_toggled(self):
        selected = [k for k, cb in self.import_checks.items() if cb.get_active() and cb.get_sensitive()]
        self.config["import_items"] = selected
        folders = self.import_src_current["data"]["folders"] if self.import_src_current else {}
        self.import_total_bytes = sum(folders[k]["size"] for k in selected if k in folders)
        self.import_total.set_text(tr("import_total", size=human_size(self.import_total_bytes)) if selected else "")
        self.validate_step()

    def import_fits(self):
        disk = self.config.get("disk") or {}
        if not disk or not self.import_total_bytes:
            return True
        if self.config["install_type"] == "dualboot":
            room = self.dualboot_alloc_mb * 1024**2
        else:
            room = disk.get("size", 0)
        # Garde ~12 Go pour le système et les mises à jour.
        return self.import_total_bytes <= room - 12 * 1024**3

    # ── Page : Chiffrement ─────────────────────────────────────

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

        self.pages.add_named(box, page_name("encryption"))

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
        layout = keyboard_name(self.config["keyboard"])
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

    # ── Page : Performances (benchmark) ──────────────────────────

    BENCH_ROWS = [("cpu", "bench_cpu"), ("ram", "bench_ram"), ("disk", "bench_disk"), ("gpu", "bench_gpu")]

    def build_benchmark_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("bench_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("bench_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        sub.set_line_wrap(True)
        box.pack_start(sub, False, False, 0)
        head = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=20)
        head.get_style_context().add_class("card")
        self.bench_base = Gtk.Label(label="–")
        self.bench_base.get_style_context().add_class("score-big")
        head.pack_start(self.bench_base, False, False, 8)
        head_text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        head_text.set_valign(Gtk.Align.CENTER)
        head_title = Gtk.Label(label=tr("bench_overall"), xalign=0)
        head_title.get_style_context().add_class("card-title")
        head_text.pack_start(head_title, False, False, 0)
        self.bench_status = Gtk.Label(label=tr("bench_starting"), xalign=0)
        self.bench_status.get_style_context().add_class("card-detail")
        self.bench_status.set_line_wrap(True)
        head_text.pack_start(self.bench_status, False, False, 0)
        head.pack_start(head_text, True, True, 0)
        self.bench_retry = Gtk.Button(label=tr("bench_retry"))
        self.bench_retry.get_style_context().add_class("btn-secondary")
        self.bench_retry.set_valign(Gtk.Align.CENTER)
        self.bench_retry.connect("clicked", lambda _b: self.start_benchmark())
        self.bench_retry.set_sensitive(False)
        head.pack_end(self.bench_retry, False, False, 0)
        box.pack_start(head, False, False, 0)
        grid = Gtk.Grid(column_spacing=16, row_spacing=10)
        grid.get_style_context().add_class("card")
        self.bench_widgets = {}
        for row, (key, label) in enumerate(self.BENCH_ROWS):
            name = Gtk.Label(label=tr(label), xalign=0)
            name.get_style_context().add_class("card-title")
            name.set_size_request(110, -1)
            grid.attach(name, 0, row, 1, 1)
            bar = Gtk.LevelBar(min_value=0, max_value=9.9)
            bar.get_style_context().add_class("score-bar")
            bar.set_hexpand(True)
            bar.set_valign(Gtk.Align.CENTER)
            bar.set_size_request(160, -1)
            grid.attach(bar, 1, row, 1, 1)
            value = Gtk.Label(label="…")
            value.get_style_context().add_class("score-value")
            value.set_size_request(48, -1)
            grid.attach(value, 2, row, 1, 1)
            detail = Gtk.Label(label=tr("bench_waiting"), xalign=0)
            detail.get_style_context().add_class("card-detail")
            detail.set_ellipsize(Pango.EllipsizeMode.END)
            detail.set_hexpand(True)
            grid.attach(detail, 3, row, 1, 1)
            self.bench_widgets[key] = (bar, value, detail)
        box.pack_start(grid, False, False, 0)
        tips_title = Gtk.Label(label=tr("bench_tips"), xalign=0)
        tips_title.get_style_context().add_class("card-title")
        tips_title.set_margin_top(8)
        box.pack_start(tips_title, False, False, 0)
        self.bench_tips = Gtk.Label(label="", xalign=0)
        self.bench_tips.get_style_context().add_class("tip")
        self.bench_tips.set_line_wrap(True)
        box.pack_start(self.bench_tips, False, False, 0)
        self.pages.add_named(box, page_name("benchmark"))

    def start_benchmark(self):
        if self.bench_proc and self.bench_proc.poll() is None:
            self.bench_proc.kill()
        device = (self.config.get("disk") or {}).get("path")
        self.bench_device = device
        self.bench_state = "running"
        self.benchmark = None
        self.bench_retry.set_sensitive(False)
        self.bench_base.set_text("–")
        self.bench_status.set_text(tr("bench_running"))
        self.bench_tips.set_text("")
        for bar, value, detail in self.bench_widgets.values():
            bar.set_value(0)
            value.set_text("…")
            detail.set_text(tr("bench_waiting"))
        argv = [sys.executable, winlux_benchmark.__file__, "--json-lines"]
        if device:
            argv += ["--device", device]
        try:
            proc = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        except OSError as e:
            self.bench_status.set_text(tr("bench_failed", error=e))
            self.bench_state = "done"
            self.bench_retry.set_sensitive(True)
            return
        self.bench_proc = proc
        threading.Thread(target=self.read_benchmark, args=(proc,), daemon=True).start()

    def read_benchmark(self, proc):
        for line in proc.stdout:
            try:
                event = json.loads(line)
            except ValueError:
                continue
            GLib.idle_add(self.on_benchmark_event, proc, event)
        proc.wait()
        GLib.idle_add(self.on_benchmark_exit, proc)

    def on_benchmark_event(self, proc, event):
        if proc is not self.bench_proc:
            return False
        kind = event.get("event")
        if kind == "start" and event["test"] in self.bench_widgets:
            self.bench_widgets[event["test"]][2].set_text(tr("bench_measuring"))
        elif kind == "result" and event["test"] in self.bench_widgets:
            bar, value, detail = self.bench_widgets[event["test"]]
            result = event["result"]
            bar.set_value(result["score"])
            value.set_text(winlux_benchmark.fr_number(result["score"], 1))
            detail.set_text(result["detail"])
            detail.set_tooltip_text(result["detail"])
        elif kind == "done":
            self.benchmark = event["summary"]
            self.bench_state = "done"
            self.bench_base.set_text(winlux_benchmark.fr_number(self.benchmark["base_score"], 1))
            self.bench_status.set_text(tr("bench_done"))
            tips = self.benchmark["recommendations"] or [tr("bench_no_tip")]
            self.bench_tips.set_text("\n".join(f"•  {t}" for t in tips))
            self.bench_retry.set_sensitive(True)
            if not self.profile_chosen_by_user:
                self.select_profile(self.benchmark["recommended_profile"])
            self.refresh_profile_badges()
        return False

    def on_benchmark_exit(self, proc):
        if proc is self.bench_proc and self.bench_state == "running":
            self.bench_state = "done"
            self.bench_status.set_text(tr("bench_stopped"))
            self.bench_retry.set_sensitive(True)
        return False

    # ── Page : Profil d'utilisation ──────────────────────────────

    def build_profile_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("profile_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("profile_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        sub.set_line_wrap(True)
        box.pack_start(sub, False, False, 0)
        grid = Gtk.Grid(column_spacing=12, row_spacing=12, column_homogeneous=True)
        self.profile_buttons = {}
        self.profile_cards = {}
        self.profile_badges = {}
        group = None
        for i, pid in enumerate(winlux_profiles.ORDER):
            profile = winlux_profiles.PROFILES[pid]
            card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
            card.get_style_context().add_class("card")
            top = Gtk.Box(spacing=8)
            rb = Gtk.RadioButton.new_with_label_from_widget(group, winlux_profiles.display_name(pid))
            rb.get_style_context().add_class("card-title")
            group = group or rb
            rb.connect("toggled", self.on_profile_toggle, pid)
            top.pack_start(rb, False, False, 0)
            badge = Gtk.Label(label=tr("profile_recommended"))
            badge.get_style_context().add_class("badge")
            badge.set_no_show_all(True)
            top.pack_end(badge, False, False, 0)
            card.pack_start(top, False, False, 0)
            desc = Gtk.Label(label=winlux_profiles.display_description(pid), xalign=0)
            desc.get_style_context().add_class("card-detail")
            desc.set_line_wrap(True)
            desc.set_max_width_chars(40)
            card.pack_start(desc, False, False, 0)
            tune = Gtk.Label(label=winlux_profiles.summary_line(pid), xalign=0)
            self.profile_tune_labels[pid] = tune
            tune.get_style_context().add_class("hw-info")
            tune.set_line_wrap(True)
            card.pack_start(tune, False, False, 0)
            apps = Gtk.Label(label=tr("profile_apps", apps=", ".join(profile["apps"])), xalign=0)
            self.profile_apps_labels[pid] = apps
            apps.get_style_context().add_class("tip")
            apps.set_line_wrap(True)
            apps.set_max_width_chars(40)
            card.pack_start(apps, False, False, 0)
            grid.attach(card, i % 2, i // 2, 1, 1)
            self.profile_buttons[pid] = rb
            self.profile_cards[pid] = card
            self.profile_badges[pid] = badge
        box.pack_start(grid, False, False, 0)
        apps_check = Gtk.CheckButton(label=tr("profile_install_apps"))
        apps_check.set_active(True)
        apps_check.set_margin_top(8)
        apps_check.connect("toggled", lambda b: self.config.update({"install_apps": b.get_active()}))
        box.pack_start(apps_check, False, False, 0)
        self.selecting_profile = False
        self.select_profile("office")
        self.pages.add_named(box, page_name("profile"))

    def select_profile(self, pid):
        self.selecting_profile = True
        self.profile_buttons[pid].set_active(True)
        self.selecting_profile = False
        self.config["profile"] = pid
        for other, card in self.profile_cards.items():
            ctx = card.get_style_context()
            if other == pid:
                ctx.add_class("selected")
            else:
                ctx.remove_class("selected")

    def on_profile_toggle(self, btn, pid):
        if not btn.get_active():
            return
        if not self.selecting_profile:
            self.profile_chosen_by_user = True
            self.select_profile(pid)

    def refresh_profile_badges(self):
        recommended = (self.benchmark or {}).get("recommended_profile")
        for pid, badge in self.profile_badges.items():
            badge.set_visible(pid == recommended)

    # ── Page : Compte local ────────────────────────────────────

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
            (tr("field_fullname"), "fullname", tr("fullname_placeholder"), False),
            (tr("field_username"), "username", "winlux", False),
            (tr("field_password"), "password", "", True),
            (tr("field_password_confirm"), "password_confirm", "", True),
        ]
        self.account_entries = {}
        self.account_error = Gtk.Label(label="", xalign=0)
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
                entry.set_text("winlux-desktop")
            entry.connect("changed", self.on_account_changed, key)
            grid.attach(entry, 1, i, 1, 1)
            self.account_entries[key] = entry
        box.pack_start(grid, False, False, 0)
        self.account_error.get_style_context().add_class("warning-text")
        self.account_error.set_margin_top(8)
        box.pack_start(self.account_error, False, False, 0)
        autologin = Gtk.CheckButton(label=tr("autologin"))
        autologin.set_active(True)
        autologin.connect("toggled", lambda b: self.config.update({"autologin": b.get_active()}))
        autologin.set_margin_top(8)
        box.pack_start(autologin, False, False, 0)
        self.pages.add_named(box, page_name("account"))

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

    # ── Page : Migration depuis un autre PC ──────────────────────

    def build_migration_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=tr("migration_title"), xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label=tr("migration_subtitle"), xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        sub.set_line_wrap(True)
        box.pack_start(sub, False, False, 0)
        self.migration = winlux_migrate.MigrationPanel(Gtk, GLib, self.validate_step)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.add(self.migration.widget)
        box.pack_start(scroll, True, True, 0)
        self.pages.add_named(box, page_name("migration"))

    # ── Page : Résumé ──────────────────────────────────────────

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
        self.pages.add_named(self.summary_box, page_name("summary"))

    def summary_partition_lines(self):
        itype = self.config["install_type"]
        if itype == "dualboot" and self.dualboot_target:
            keep = (self.dualboot_target["size"] - self.dualboot_alloc_mb * MIB) // MIB * MIB
            return [f"  {tr('part_windows_keep', size=human_size(keep))}",
                    f"  Winlux : {human_size(self.dualboot_alloc_mb * MIB)}"]
        if itype == "manual":
            return ["  " + tr("pm_ops_title")] + ["    " + op for op in self.manual_editor.operations_text()]
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
        if self.wifi_connected_ssid:
            net = tr("summary_network_wifi", ssid=self.wifi_connected_ssid)
        elif self.config.get("network_ethernet"):
            net = tr("summary_network_wired")
        else:
            net = tr("summary_network_none")
        proxy = self.proxy_settings()
        if proxy:
            net += "  (" + tr("summary_proxy", proxy=f"{proxy[0]}:{proxy[1]}") + ")"
        lines.append(f"{tr('summary_network')} : {net}")
        items = [tr(f"summary_item_{k}") if not k.startswith("bookmarks:")
                 else tr("summary_import_bookmarks", browser=k.split(":", 1)[1])
                 for k in self.config["import_items"]]
        if items and self.import_src_current:
            lines.append(f"{tr('summary_import')} ({self.import_src_current['user']}) : {', '.join(items)}")
        else:
            lines.append(f"{tr('summary_import')} : {tr('summary_import_none')}")
        lines.append(tr("summary_local_account"))
        profile_line = f"{tr('summary_profile')} : {winlux_profiles.display_name(self.config['profile'])}"
        if self.config["install_apps"]:
            profile_line += " " + tr("summary_profile_apps")
        lines.append(profile_line)
        score = (winlux_benchmark.fr_number(self.benchmark["base_score"], 1) if self.benchmark
                 else tr("summary_benchmark_none"))
        lines.append(f"{tr('summary_benchmark')} : {score}")
        lines.append(f"{tr('summary_migration')} : {self.migration.describe()}")
        if self.config.get("timezone"):
            lines.append(f"{tr('summary_timezone')} : {self.config['timezone']}")
        if self.config.get("packages"):
            lines.append(f"{tr('summary_packages')} : {', '.join(self.config['packages'])}")
        if self.answer_path:
            lines.append(f"{tr('summary_answer_file')} : {self.answer_path}")
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
        self.summary_content.select_region(0, 0)
        if itype == "full":
            self.summary_warn.set_text(tr("summary_erase_warn"))
        elif itype == "dualboot":
            self.summary_warn.set_text(tr("summary_dualboot_warn"))
        else:
            self.summary_warn.set_text("")

    # ── Page : Progression ─────────────────────────────────────

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
        self.pages.add_named(box, page_name("progress"))

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

    # ── Page : Terminé ─────────────────────────────────────────

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
        self.pages.add_named(box, page_name("done"))

    def on_reboot(self, _btn):
        try:
            subprocess.run(["sync"], timeout=10)
            subprocess.run(["reboot"], timeout=10)
        except Exception:
            pass
        self.quit()

    # ── Installation ─────────────────────────────────────────────

    def start_installation(self):
        self.current_step = step_index("progress")
        self.update_step()
        self.install_cancelled = False
        self.install_thread = threading.Thread(target=self.run_installation, daemon=True)
        self.install_thread.start()

    def run_installation(self):
        disk = self.config["disk"]
        if not disk:
            self.install_error(tr("err_no_disk"))
            return
        device = disk["path"]
        # Fond d'écran et favoris sont copiés en mémoire avant que le disque soit modifié,
        # puis les partitions Windows de ce disque sont démontées (ntfsresize en dual-boot).
        try:
            self.prepare_windows_import(disk)
        except Exception as e:
            self.import_plan = None
            self.set_progress(0.01, tr("warn_import_skipped", error=e))
        finally:
            self.unmount_windows(disk["name"])
        iso_path = os.environ.get("WINLUX_ISO", "")
        if not iso_path:
            candidate = os.path.join(ROOT_DIR, "build", "winlux-live-test.iso")
            if os.path.isfile(candidate):
                iso_path = candidate
        live_root = "/run/winlux/live/lower"
        if not (iso_path and os.path.isfile(iso_path)) and not os.path.isdir(live_root):
            self.install_error(tr("err_iso_missing"))
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
                        raise RuntimeError(tr("err_verify", error=result.stderr.strip()))
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
            self.measure_disk_write(target)
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
            self.import_windows_data(target)
            self.set_progress(0.90, tr("progress_firstboot"))
            self.configure_first_boot(target)
            self.write_manual_fstab(layout, target)
            self.run_migration(target)
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
                raise RuntimeError(tr("err_boot_file", path=inside))
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
        return self.partition_manual(device)

    def partition_manual(self, device):
        """Apply the manual editor's plan; the root is formatted later by format_layout."""
        plan = self.manual_editor.plan
        if plan is None or plan.device != device or plan.validate():
            raise RuntimeError(tr("err_manual_plan"))
        self.set_progress(0.06, tr("progress_manual_apply"))
        winlux_partitioner.run_steps(plan.build_steps(), log=lambda cmd: self.set_progress(0.07, cmd))
        if not winlux_partitioner.wait_for_partitions(plan):
            raise RuntimeError(tr("err_partitions_missing"))
        root = plan.root()
        for argv, stdin in plan.format_steps(skip=(root,)):
            self.set_progress(0.10, " ".join(argv))
            subprocess.run(argv, input=stdin, text=True, check=True, timeout=600)
        esp = plan.esp()
        esp_path = plan.path_of(esp) if esp else None
        # Une ESP déjà présente (souvent celle de Windows) n'est jamais traitée comme la nôtre.
        return {"esp": esp_path, "format_esp": False, "boot": None, "root": plan.path_of(root),
                "bios": not self.hw_info.get("uefi") or esp_path is None, "own_esp": False,
                "plan": plan}

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
            raise RuntimeError(tr("err_resize_missing"))
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
        plan = layout.get("plan")
        if plan is not None:
            # /home, /var… du plan manuel, montés avant la copie du système.
            for part in plan.mounts():
                if part.fstype == "swap":
                    continue
                path = plan.path_of(part)
                dest = target + part.mountpoint
                self.set_progress(0.16, tr("progress_manual_mount", path=path, mount=part.mountpoint))
                os.makedirs(dest, exist_ok=True)
                subprocess.run(["mount", path, dest], check=True, timeout=30)
                self._mounts.append(dest)

    def write_manual_fstab(self, layout, target):
        plan = layout.get("plan")
        if plan is None:
            return
        os.makedirs(f"{target}/etc", exist_ok=True)
        with open(f"{target}/etc/fstab", "w") as f:
            f.write(plan.fstab(winlux_partitioner.blkid_uuid, root_luks=self.config["luks_enabled"]))

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
            # The Secure Boot chain goes to EFI/BOOT, which belongs to Windows on a shared ESP.
            if layout["own_esp"] and SECUREBOOT_SH and os.path.exists("/usr/lib/shim/shimx64.efi.signed"):
                subprocess.run(["sh", SECUREBOOT_SH, "install-esp", efi_dir, search_uuid], check=True, timeout=60)
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
        hashed = self.config.get("password_hash") or ""
        if password and not hashed:
            try:
                hashed = subprocess.run(
                    ["openssl", "passwd", "-6", "-stdin"], input=password + "\n",
                    capture_output=True, text=True, timeout=10, check=True
                ).stdout.strip()
            except Exception:
                hashed = "!"
        if hashed:
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
            "timezone": self.config.get("timezone"),
            "packages": self.config.get("packages", []),
            "answer_file": self.answer_path,
            "luks_enabled": self.config["luks_enabled"],
            "install_type": self.config["install_type"],
            "profile": self.config["profile"],
            "install_date": time.strftime("%Y-%m-%d %H:%M:%S"),
        }
        profile = winlux_profiles.build_config(self.config["profile"], self.benchmark, self.config["install_apps"])
        winlux_profiles.write_config(root_path, profile, self.benchmark)
        with open(f"{etc}/install-config.json", "w") as f:
            json.dump(config, f, ensure_ascii=False, indent=2)
        timezone = self.config.get("timezone")
        if timezone:
            with open(f"{root_path}/etc/timezone", "w") as f:
                f.write(timezone + "\n")
            if os.path.exists(f"{root_path}/usr/share/zoneinfo/{timezone}"):
                localtime = f"{root_path}/etc/localtime"
                if os.path.lexists(localtime):
                    os.remove(localtime)
                os.symlink(f"/usr/share/zoneinfo/{timezone}", localtime)
        if self.config.get("packages"):
            # Installés par l'init du système au premier démarrage avec réseau.
            with open(f"{etc}/first-boot-packages", "w") as f:
                f.write("\n".join(self.config["packages"]) + "\n")
        try:
            self.configure_network_target(root_path)
        except Exception as e:
            self.set_progress(0.91, tr("warn_network", error=e))
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

    def configure_network_target(self, root_path):
        src_dir = "/etc/NetworkManager/system-connections"
        if os.path.isdir(src_dir):
            dst_dir = f"{root_path}{src_dir}"
            os.makedirs(dst_dir, exist_ok=True)
            os.chmod(dst_dir, 0o700)
            for name in os.listdir(src_dir):
                if name.endswith(".nmconnection"):
                    dst = os.path.join(dst_dir, name)
                    shutil.copyfile(os.path.join(src_dir, name), dst)
                    os.chmod(dst, 0o600)
        proxy = self.proxy_settings()
        if not proxy:
            return
        url = f"http://{proxy[0]}:{proxy[1]}/"
        env_path = f"{root_path}/etc/environment"
        keep = []
        if os.path.exists(env_path):
            with open(env_path) as f:
                keep = [l for l in f if not re.match(r"^\s*(https?|ftp|no)_proxy=", l, re.I)]
        keep += [f'{v}="{url}"\n' for v in ("http_proxy", "https_proxy", "ftp_proxy",
                                              "HTTP_PROXY", "HTTPS_PROXY", "FTP_PROXY")]
        keep += ['no_proxy="localhost,127.0.0.1,::1"\n', 'NO_PROXY="localhost,127.0.0.1,::1"\n']
        os.makedirs(os.path.dirname(env_path), exist_ok=True)
        with open(env_path, "w") as f:
            f.writelines(keep)
        apt_dir = f"{root_path}/etc/apt/apt.conf.d"
        os.makedirs(apt_dir, exist_ok=True)
        with open(f"{apt_dir}/95winlux-proxy", "w") as f:
            f.write(f'Acquire::http::Proxy "{url}";\nAcquire::https::Proxy "{url}";\n')

    # ── Import Windows ───────────────────────────────────────────

    def unmount_windows(self, disk_name=None):
        for part in self.win_partitions:
            if part["ours"] and (disk_name is None or part["disk"] == disk_name) \
                    and os.path.ismount(part["mount"]):
                subprocess.run(["umount", part["mount"]], timeout=30)

    def prepare_windows_import(self, disk):
        """Copie fond d'écran et favoris en mémoire avant que le disque cible soit effacé."""
        self.import_plan = None
        src = self.import_src_current
        items = list(self.config["import_items"])
        if not src or not items:
            return
        self.set_progress(0.01, tr("progress_import"))
        on_target = (src["part"]["disk"] == disk["name"]
                     and self.config["install_type"] != "dualboot")
        data = src["data"]
        stage = WIN_IMPORT_STAGE
        shutil.rmtree(stage, ignore_errors=True)
        os.makedirs(stage, mode=0o700)
        plan = {"wallpaper": None, "bookmarks": [], "folders": []}
        if "wallpaper" in items and data["wallpaper"]:
            dst = os.path.join(stage, "wallpaper" + detect_image_ext(data["wallpaper"]))
            shutil.copyfile(data["wallpaper"], dst)
            plan["wallpaper"] = dst
        for b in data["bookmarks"]:
            if f"bookmarks:{b['browser']}" not in items:
                continue
            try:
                if b["kind"] == "chromium":
                    tree = chromium_bookmarks_tree(b["path"])
                else:
                    tree = firefox_bookmarks_tree(b["path"], stage)
                html_path = os.path.join(stage, f"Favoris {b['browser']}.html")
                with open(html_path, "w", encoding="utf-8") as f:
                    f.write(_bookmark_html(tree))
                entry = {"browser": b["browser"], "html": html_path, "json": None}
                if b["kind"] == "chromium":
                    entry["json"] = os.path.join(stage, f"{b['browser']}.Bookmarks")
                    shutil.copyfile(b["path"], entry["json"])
                plan["bookmarks"].append(entry)
            except Exception as e:
                self.set_progress(0.01, tr("warn_bookmarks", browser=b["browser"], error=e))
        for name in ("places.sqlite", "places.sqlite-wal"):
            try:
                os.remove(os.path.join(stage, name))
            except OSError:
                pass
        if not on_target:
            for key, _win, dest in WIN_FOLDERS:
                if key in items and key in data["folders"]:
                    plan["folders"].append((data["folders"][key]["path"], dest))
            # La partition peut être démontée entre-temps (dual-boot) : on la remontera.
            plan["source"] = src["part"]
        self.import_plan = plan

    def import_windows_data(self, root_path):
        plan = self.import_plan
        username = self.config["username"]
        if not plan or not username:
            return
        self.set_progress(0.86, tr("progress_import"))
        home = f"{root_path}/home/{username}"
        try:
            if plan["wallpaper"]:
                pictures = f"{home}/Images"
                os.makedirs(pictures, exist_ok=True)
                name = "Fond d'écran Windows" + os.path.splitext(plan["wallpaper"])[1]
                shutil.copyfile(plan["wallpaper"], os.path.join(pictures, name))
                # Le bureau XFCE n'existe qu'une fois la session ouverte : on applique au premier login.
                script_dir = f"{home}/.local/share/winlux"
                os.makedirs(script_dir, exist_ok=True)
                with open(f"{script_dir}/apply-imported-wallpaper.sh", "w") as f:
                    f.write("#!/bin/sh\nsleep 5\n"
                            f'winlux-personalize wallpaper "$HOME/Images/{name}" && '
                            'rm -f "$HOME/.config/autostart/winlux-imported-wallpaper.desktop" "$0"\n')
                os.makedirs(f"{home}/.config/autostart", exist_ok=True)
                with open(f"{home}/.config/autostart/winlux-imported-wallpaper.desktop", "w") as f:
                    f.write("[Desktop Entry]\nType=Application\nName=Fond d'écran importé\n"
                            f"Exec=/bin/sh /home/{username}/.local/share/winlux/apply-imported-wallpaper.sh\n"
                            "NoDisplay=true\n")
            profiles = {"Edge": ".config/microsoft-edge/Default", "Chrome": ".config/google-chrome/Default"}
            for b in plan["bookmarks"]:
                fav_dir = f"{home}/Documents/Favoris Windows"
                os.makedirs(fav_dir, exist_ok=True)
                shutil.copyfile(b["html"], os.path.join(fav_dir, os.path.basename(b["html"])))
                if b["json"] and b["browser"] in profiles:
                    target = f"{home}/{profiles[b['browser']]}/Bookmarks"
                    if not os.path.exists(target):
                        os.makedirs(os.path.dirname(target), exist_ok=True)
                        shutil.copyfile(b["json"], target)
            part = plan.get("source")
            if plan["folders"] and part and not os.path.ismount(part["mount"]):
                if mount_ntfs_readonly(part["partition"], part["mount"]):
                    part["ours"] = True
                else:
                    self.set_progress(0.87, tr("import_remount_failed"))
                    plan["folders"] = []
            for src, dest in plan["folders"]:
                self.set_progress(0.87, tr("progress_import_folder", folder=dest))
                dst = f"{home}/{dest}"
                os.makedirs(dst, exist_ok=True)
                r = subprocess.run(["cp", "-r", "--preserve=timestamps", f"{src}/.", dst],
                                   capture_output=True, text=True, timeout=7200)
                if r.returncode != 0:
                    self.set_progress(0.87, tr("warn_copy_partial", folder=dest))
                for root, _dirs, files in os.walk(dst):
                    for name in files:
                        if name.lower() in WIN_JUNK:
                            try:
                                os.remove(os.path.join(root, name))
                            except OSError:
                                pass
                subprocess.run(["chmod", "-R", "u+rwX,go-w", dst], timeout=600)
        except Exception as e:
            self.set_progress(0.88, tr("warn_import", error=e))
        subprocess.run(["chown", "-R", "1000:1000", home], timeout=600)

    def cleanup_windows_import(self):
        shutil.rmtree(WIN_IMPORT_STAGE, ignore_errors=True)
        self.unmount_windows()

    def measure_disk_write(self, target):
        """Complète le benchmark : écriture sur la partition fraîchement formatée."""
        if not self.benchmark or "disk" not in self.benchmark["results"]:
            return
        try:
            result = winlux_benchmark.bench_disk_write(target)
        except OSError as e:
            self.set_progress(0.16, tr("warn_write_bench", error=e))
            return
        self.benchmark["results"]["disk"].update(result)
        self.set_progress(0.16, f"Disque : {result['detail']}")

    def run_migration(self, root_path):
        panel = self.migration
        if not panel.enabled or not panel.session:
            return
        folders, settings = panel.selection()
        home = f"{root_path}/home/{self.config['username']}"

        def report(fraction, text):
            self.set_progress(0.91 + 0.04 * fraction, text)
        try:
            errors = panel.session.pull(home, folders, settings, report, uid=1000, gid=1000)
        except Exception as e:
            errors = [str(e)]
        finally:
            panel.session.close()
        for err in errors:
            self.set_progress(0.95, tr("warn_transfer", error=err))

    def post_install(self):
        self.cleanup_windows_import()
        self.set_progress(1.0, tr("progress_done"))
        GLib.idle_add(self.show_done)
        if self.answer_settings.get("unattended") and self.answer_settings.get("reboot"):
            GLib.timeout_add_seconds(10, lambda: self.on_reboot(None))

    def show_done(self):
        self.current_step = step_index("done")
        self.update_step()

    def install_error(self, message):
        self.cleanup_windows_import()

        def do():
            self.done_title.set_text(tr("error_title"))
            self.done_sub.set_text(message)
            self.current_step = step_index("done")
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
