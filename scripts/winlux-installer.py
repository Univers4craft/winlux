#!/usr/bin/python3
"""Installateur graphique Winlux — style Windows 11.

Lance l'installation de Winlux sur un disque physique depuis le Live USB/DVD.
Interface GTK3 avec étapes : langue, licence, type, disque, compte, progression.
"""
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk, GLib, Pango
import json
import os
import re
import signal
import subprocess
import sys
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

TRANSLATIONS = {
    "fr_FR": {
        "steps": ["Langue", "Clavier", "Licence", "Type d'installation", "Disque cible",
                  "Compte utilisateur", "Résumé", "Installation", "Terminé"],
        "choose_language": "Choisissez votre langue",
        "language_subtitle": "Cette langue sera utilisée pendant l'installation et comme langue par défaut du système.",
        "choose_keyboard": "Choisissez la disposition du clavier",
        "keyboard_subtitle": "Sélectionnez le type de clavier que vous utilisez.",
        "keyboard_test": "Testez votre clavier ici :",
        "license_title": "Contrat de licence",
        "license_subtitle": "Veuillez lire et accepter les termes de la licence.",
        "license_accept": "J'accepte les termes de la licence GPL v3",
        "type_title": "Type d'installation",
        "type_subtitle": "Choisissez comment installer Winlux.",
        "type_full": "Installation complète (recommandé)",
        "type_full_desc": "Efface le disque sélectionné et installe Winlux comme seul système.\nPartitionnement automatique : BIOS boot + ESP (UEFI) + racine ext4.",
        "type_custom": "Installation personnalisée",
        "type_custom_desc": "Choisissez manuellement les partitions. Pour utilisateurs avancés.\n(Non disponible dans cette version — à venir.)",
        "disk_title": "Sélectionnez le disque cible",
        "disk_warning": "ATTENTION : le disque sélectionné sera entièrement effacé. Sauvegardez vos données.",
        "disk_none": "Aucun disque disponible détecté (taille minimale : 2 Go).",
        "hw_detected": "Matériel détecté",
        "hw_detecting": "Détection en cours...",
        "account_title": "Créez votre compte",
        "account_subtitle": "Ce compte sera l'administrateur de votre système Winlux.",
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
        "summary_type_full": "Installation complète",
        "summary_disk": "Disque",
        "summary_hostname": "Nom de l'ordinateur",
        "summary_user": "Utilisateur",
        "summary_autologin": "Connexion automatique",
        "summary_yes": "Oui",
        "summary_no": "Non",
        "summary_boot": "Démarrage",
        "progress_check_disk": "Vérification du disque {device}...",
        "progress_partitioning": "Partitionnement du disque...",
        "progress_formatting": "Formatage des partitions...",
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
        "steps": ["Language", "Keyboard", "License", "Installation type", "Target disk",
                  "User account", "Summary", "Installing", "Done"],
        "choose_language": "Choose your language",
        "language_subtitle": "This language will be used during installation and as the default system language.",
        "choose_keyboard": "Choose your keyboard layout",
        "keyboard_subtitle": "Select the keyboard layout you use.",
        "keyboard_test": "Test your keyboard here:",
        "license_title": "License agreement",
        "license_subtitle": "Please read and accept the license terms.",
        "license_accept": "I accept the terms of the GPL v3 license",
        "type_title": "Installation type",
        "type_subtitle": "Choose how to install Winlux.",
        "type_full": "Full installation (recommended)",
        "type_full_desc": "Erases the selected disk and installs Winlux as the only system.\nAutomatic partitioning: BIOS boot + ESP (UEFI) + ext4 root.",
        "type_custom": "Custom installation",
        "type_custom_desc": "Manually choose partitions. For advanced users.\n(Not available in this version — coming soon.)",
        "disk_title": "Select the target disk",
        "disk_warning": "WARNING: the selected disk will be completely erased. Back up your data.",
        "disk_none": "No available disk detected (minimum size: 2 GB).",
        "hw_detected": "Detected hardware",
        "hw_detecting": "Detecting...",
        "account_title": "Create your account",
        "account_subtitle": "This account will be the administrator of your Winlux system.",
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
        "summary_type_full": "Full installation",
        "summary_disk": "Disk",
        "summary_hostname": "Computer name",
        "summary_user": "User",
        "summary_autologin": "Automatic login",
        "summary_yes": "Yes",
        "summary_no": "No",
        "summary_boot": "Boot mode",
        "progress_check_disk": "Checking disk {device}...",
        "progress_partitioning": "Partitioning disk...",
        "progress_formatting": "Formatting partitions...",
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
        "steps": ["Sprache", "Tastatur", "Lizenz", "Installationstyp", "Zielfestplatte",
                  "Benutzerkonto", "Zusammenfassung", "Installation", "Fertig"],
        "choose_language": "Wählen Sie Ihre Sprache",
        "language_subtitle": "Diese Sprache wird während der Installation und als Standardsystemsprache verwendet.",
        "choose_keyboard": "Wählen Sie Ihr Tastaturlayout",
        "keyboard_subtitle": "Wählen Sie das Tastaturlayout, das Sie verwenden.",
        "keyboard_test": "Testen Sie Ihre Tastatur hier:",
        "license_title": "Lizenzvereinbarung",
        "license_subtitle": "Bitte lesen und akzeptieren Sie die Lizenzbedingungen.",
        "license_accept": "Ich akzeptiere die Bedingungen der GPL v3-Lizenz",
        "type_title": "Installationstyp",
        "type_subtitle": "Wählen Sie, wie Winlux installiert werden soll.",
        "type_full": "Vollständige Installation (empfohlen)",
        "type_full_desc": "Löscht die ausgewählte Festplatte und installiert Winlux als einziges System.\nAutomatische Partitionierung: BIOS-Boot + ESP (UEFI) + ext4-Root.",
        "type_custom": "Benutzerdefinierte Installation",
        "type_custom_desc": "Partitionen manuell wählen. Für fortgeschrittene Benutzer.\n(In dieser Version nicht verfügbar — kommt bald.)",
        "disk_title": "Zielfestplatte auswählen",
        "disk_warning": "ACHTUNG: Die ausgewählte Festplatte wird vollständig gelöscht. Sichern Sie Ihre Daten.",
        "disk_none": "Keine verfügbare Festplatte erkannt (Mindestgröße: 2 GB).",
        "hw_detected": "Erkannte Hardware",
        "hw_detecting": "Erkennung läuft...",
        "account_title": "Erstellen Sie Ihr Konto",
        "account_subtitle": "Dieses Konto wird der Administrator Ihres Winlux-Systems.",
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
        "summary_type_full": "Vollständige Installation",
        "summary_disk": "Festplatte",
        "summary_hostname": "Computername",
        "summary_user": "Benutzer",
        "summary_autologin": "Automatische Anmeldung",
        "summary_yes": "Ja",
        "summary_no": "Nein",
        "summary_boot": "Startmodus",
        "progress_check_disk": "Festplatte {device} wird überprüft...",
        "progress_partitioning": "Festplatte wird partitioniert...",
        "progress_formatting": "Partitionen werden formatiert...",
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
        "steps": ["Idioma", "Teclado", "Licencia", "Tipo de instalación", "Disco destino",
                  "Cuenta de usuario", "Resumen", "Instalación", "Terminado"],
        "choose_language": "Elija su idioma",
        "language_subtitle": "Este idioma se usará durante la instalación y como idioma predeterminado del sistema.",
        "choose_keyboard": "Elija la disposición del teclado",
        "keyboard_subtitle": "Seleccione el tipo de teclado que usa.",
        "keyboard_test": "Pruebe su teclado aquí:",
        "license_title": "Acuerdo de licencia",
        "license_subtitle": "Lea y acepte los términos de la licencia.",
        "license_accept": "Acepto los términos de la licencia GPL v3",
        "type_title": "Tipo de instalación",
        "type_subtitle": "Elija cómo instalar Winlux.",
        "type_full": "Instalación completa (recomendado)",
        "type_full_desc": "Borra el disco seleccionado e instala Winlux como único sistema.\nParticionamiento automático: arranque BIOS + ESP (UEFI) + raíz ext4.",
        "type_custom": "Instalación personalizada",
        "type_custom_desc": "Elija particiones manualmente. Para usuarios avanzados.\n(No disponible en esta versión — próximamente.)",
        "disk_title": "Seleccione el disco destino",
        "disk_warning": "ATENCIÓN: el disco seleccionado será borrado por completo. Haga una copia de seguridad.",
        "disk_none": "No se detectó ningún disco disponible (tamaño mínimo: 2 GB).",
        "hw_detected": "Hardware detectado",
        "hw_detecting": "Detectando...",
        "account_title": "Cree su cuenta",
        "account_subtitle": "Esta cuenta será el administrador de su sistema Winlux.",
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
        "summary_type_full": "Instalación completa",
        "summary_disk": "Disco",
        "summary_hostname": "Nombre del equipo",
        "summary_user": "Usuario",
        "summary_autologin": "Inicio automático",
        "summary_yes": "Sí",
        "summary_no": "No",
        "summary_boot": "Modo de arranque",
        "progress_check_disk": "Verificando disco {device}...",
        "progress_partitioning": "Particionando disco...",
        "progress_formatting": "Formateando particiones...",
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
        "steps": ["Lingua", "Tastiera", "Licenza", "Tipo di installazione", "Disco di destinazione",
                  "Account utente", "Riepilogo", "Installazione", "Completato"],
        "choose_language": "Scegli la tua lingua",
        "language_subtitle": "Questa lingua verrà utilizzata durante l'installazione e come lingua predefinita del sistema.",
        "choose_keyboard": "Scegli il layout della tastiera",
        "keyboard_subtitle": "Seleziona il layout della tastiera che utilizzi.",
        "keyboard_test": "Prova la tua tastiera qui:",
        "license_title": "Contratto di licenza",
        "license_subtitle": "Leggere e accettare i termini della licenza.",
        "license_accept": "Accetto i termini della licenza GPL v3",
        "type_title": "Tipo di installazione",
        "type_subtitle": "Scegli come installare Winlux.",
        "type_full": "Installazione completa (consigliata)",
        "type_full_desc": "Cancella il disco selezionato e installa Winlux come unico sistema.\nPartizionamento automatico: BIOS boot + ESP (UEFI) + root ext4.",
        "type_custom": "Installazione personalizzata",
        "type_custom_desc": "Scegli manualmente le partizioni. Per utenti avanzati.\n(Non disponibile in questa versione — in arrivo.)",
        "disk_title": "Seleziona il disco di destinazione",
        "disk_warning": "ATTENZIONE: il disco selezionato verrà completamente cancellato. Esegui un backup dei tuoi dati.",
        "disk_none": "Nessun disco disponibile rilevato (dimensione minima: 2 GB).",
        "hw_detected": "Hardware rilevato",
        "hw_detecting": "Rilevamento in corso...",
        "account_title": "Crea il tuo account",
        "account_subtitle": "Questo account sarà l'amministratore del tuo sistema Winlux.",
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
        "summary_type_full": "Installazione completa",
        "summary_disk": "Disco",
        "summary_hostname": "Nome del computer",
        "summary_user": "Utente",
        "summary_autologin": "Accesso automatico",
        "summary_yes": "Sì",
        "summary_no": "No",
        "summary_boot": "Modalità di avvio",
        "progress_check_disk": "Verifica del disco {device}...",
        "progress_partitioning": "Partizionamento del disco...",
        "progress_formatting": "Formattazione delle partizioni...",
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
        "steps": ["Idioma", "Teclado", "Licença", "Tipo de instalação", "Disco de destino",
                  "Conta de usuário", "Resumo", "Instalação", "Concluído"],
        "choose_language": "Escolha seu idioma",
        "language_subtitle": "Este idioma será usado durante a instalação e como idioma padrão do sistema.",
        "choose_keyboard": "Escolha o layout do teclado",
        "keyboard_subtitle": "Selecione o layout de teclado que você usa.",
        "keyboard_test": "Teste seu teclado aqui:",
        "license_title": "Acordo de licença",
        "license_subtitle": "Leia e aceite os termos da licença.",
        "license_accept": "Aceito os termos da licença GPL v3",
        "type_title": "Tipo de instalação",
        "type_subtitle": "Escolha como instalar o Winlux.",
        "type_full": "Instalação completa (recomendado)",
        "type_full_desc": "Apaga o disco selecionado e instala o Winlux como único sistema.\nParticionamento automático: boot BIOS + ESP (UEFI) + raiz ext4.",
        "type_custom": "Instalação personalizada",
        "type_custom_desc": "Escolha partições manualmente. Para usuários avançados.\n(Não disponível nesta versão — em breve.)",
        "disk_title": "Selecione o disco de destino",
        "disk_warning": "ATENÇÃO: o disco selecionado será completamente apagado. Faça backup dos seus dados.",
        "disk_none": "Nenhum disco disponível detectado (tamanho mínimo: 2 GB).",
        "hw_detected": "Hardware detectado",
        "hw_detecting": "Detectando...",
        "account_title": "Crie sua conta",
        "account_subtitle": "Esta conta será o administrador do seu sistema Winlux.",
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
        "summary_type_full": "Instalação completa",
        "summary_disk": "Disco",
        "summary_hostname": "Nome do computador",
        "summary_user": "Usuário",
        "summary_autologin": "Login automático",
        "summary_yes": "Sim",
        "summary_no": "Não",
        "summary_boot": "Modo de inicialização",
        "progress_check_disk": "Verificando disco {device}...",
        "progress_partitioning": "Particionando disco...",
        "progress_formatting": "Formatando partições...",
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
    "Type d'installation",
    "Disque cible",
    "Compte utilisateur",
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
            "hostname": "winlux-desktop",
            "username": "",
            "fullname": "",
            "password": "",
            "autologin": True,
        }
        self.disks = []
        self.hw_info = {}
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
        self.win.set_default_size(900, 600)
        self.win.set_position(Gtk.WindowPosition.CENTER)
        self.win.set_resizable(True)
        self.main_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        self.win.add(self.main_box)
        self.sidebar = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.sidebar.get_style_context().add_class("installer-sidebar")
        self.sidebar.set_size_request(200, -1)
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
        self.build_type_page()
        self.build_disk_page()
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
        GLib.idle_add(self.refresh_disk_page)

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
        elif self.current_step == 4:
            ok = self.config["disk"] is not None
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

    # ── Page 2 : Type d'installation ─────────────────────────────

    def build_type_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Type d'installation", xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label="Choisissez comment installer Winlux.", xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        box.pack_start(sub, False, False, 0)
        full_card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        full_card.get_style_context().add_class("card")
        full_rb = Gtk.RadioButton.new_with_label(None, "Installation complète (recommandé)")
        full_rb.connect("toggled", self.on_type_toggle, "full")
        full_card.pack_start(full_rb, False, False, 0)
        full_desc = Gtk.Label(
            label="Efface le disque sélectionné et installe Winlux comme seul système.\n"
                  "Partitionnement automatique : BIOS boot + ESP (UEFI) + racine ext4.",
            xalign=0
        )
        full_desc.get_style_context().add_class("card-detail")
        full_desc.set_line_wrap(True)
        full_desc.set_margin_start(24)
        full_card.pack_start(full_desc, False, False, 0)
        box.pack_start(full_card, False, False, 0)
        custom_card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        custom_card.get_style_context().add_class("card")
        custom_rb = Gtk.RadioButton.new_with_label_from_widget(full_rb, "Installation personnalisée")
        custom_rb.connect("toggled", self.on_type_toggle, "custom")
        custom_card.pack_start(custom_rb, False, False, 0)
        custom_desc = Gtk.Label(
            label="Choisissez manuellement les partitions. Pour utilisateurs avancés.\n"
                  "(Non disponible dans cette version — à venir.)",
            xalign=0
        )
        custom_desc.get_style_context().add_class("card-detail")
        custom_desc.set_line_wrap(True)
        custom_desc.set_margin_start(24)
        custom_card.pack_start(custom_desc, False, False, 0)
        custom_rb.set_sensitive(False)
        box.pack_start(custom_card, False, False, 0)
        self.pages.add_named(box, "page_3")

    def on_type_toggle(self, btn, install_type):
        if btn.get_active():
            self.config["install_type"] = install_type

    # ── Page 3 : Disque cible ────────────────────────────────────

    def build_disk_page(self):
        self.disk_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Sélectionnez le disque cible", xalign=0)
        title.get_style_context().add_class("page-title")
        self.disk_box.pack_start(title, False, False, 0)
        sub = Gtk.Label(
            label="ATTENTION : le disque sélectionné sera entièrement effacé. Sauvegardez vos données.",
            xalign=0
        )
        sub.get_style_context().add_class("disk-warn")
        sub.set_line_wrap(True)
        self.disk_box.pack_start(sub, False, False, 0)
        self.disk_list_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.disk_list_box.set_margin_top(12)
        self.disk_box.pack_start(self.disk_list_box, True, True, 0)
        hw_frame = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        hw_frame.set_margin_top(16)
        hw_title = Gtk.Label(label="Matériel détecté", xalign=0)
        hw_title.get_style_context().add_class("card-title")
        hw_frame.pack_start(hw_title, False, False, 0)
        self.hw_label = Gtk.Label(label="Détection en cours...", xalign=0)
        self.hw_label.get_style_context().add_class("hw-info")
        self.hw_label.set_line_wrap(True)
        hw_frame.pack_start(self.hw_label, False, False, 0)
        self.disk_box.pack_end(hw_frame, False, False, 0)
        self.pages.add_named(self.disk_box, "page_4")

    def refresh_disk_page(self):
        for child in self.disk_list_box.get_children():
            child.destroy()
        if not self.disks:
            lbl = Gtk.Label(label="Aucun disque disponible détecté (taille minimale : 2 Go).")
            lbl.get_style_context().add_class("card-detail")
            self.disk_list_box.pack_start(lbl, False, False, 0)
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
            self.disk_list_box.pack_start(row, False, False, 0)
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
        self.hw_label.set_text("\n".join(lines) if lines else "Détection en cours...")
        self.disk_list_box.show_all()
        self.validate_step()

    def on_disk_toggle(self, btn, disk):
        if btn.get_active():
            self.config["disk"] = disk
            self.validate_step()

    # ── Page 4 : Compte utilisateur ──────────────────────────────

    def build_account_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Créez votre compte", xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label="Ce compte sera l'administrateur de votre système Winlux.", xalign=0)
        sub.get_style_context().add_class("page-subtitle")
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
        autologin = Gtk.CheckButton(label="Connexion automatique (sans mot de passe au démarrage)")
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
                self.account_error.set_text("Les mots de passe ne correspondent pas.")
            else:
                self.account_error.set_text("")
        else:
            self.account_error.set_text("")
        username = self.config.get("username", "")
        if username and not re.match(r'^[a-z_][a-z0-9_-]*$', username):
            self.account_error.set_text("Nom d'utilisateur : minuscules, chiffres, - ou _ uniquement.")
        self.validate_step()

    # ── Page 5 : Résumé ──────────────────────────────────────────

    def build_summary_page(self):
        self.summary_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Résumé de l'installation", xalign=0)
        title.get_style_context().add_class("page-title")
        self.summary_box.pack_start(title, False, False, 0)
        sub = Gtk.Label(label="Vérifiez les paramètres avant de lancer l'installation.", xalign=0)
        sub.get_style_context().add_class("page-subtitle")
        self.summary_box.pack_start(sub, False, False, 0)
        self.summary_content = Gtk.Label(label="", xalign=0)
        self.summary_content.get_style_context().add_class("card")
        self.summary_content.set_line_wrap(True)
        self.summary_content.set_selectable(True)
        self.summary_box.pack_start(self.summary_content, False, False, 0)
        warn = Gtk.Label(
            label="⚠ TOUTES LES DONNÉES DU DISQUE SÉLECTIONNÉ SERONT DÉFINITIVEMENT EFFACÉES.",
            xalign=0
        )
        warn.get_style_context().add_class("disk-warn")
        warn.set_margin_top(12)
        self.summary_box.pack_start(warn, False, False, 0)
        self.pages.add_named(self.summary_box, "page_6")

    def refresh_summary(self):
        disk = self.config.get("disk") or {}
        lang = dict((l, n) for l, n, _ in LANGUAGES).get(self.config["language"], self.config["language"])
        lines = [
            f"Langue : {lang}",
            f"Clavier : {self.config['keyboard']}",
            f"Type : Installation complète",
            f"Disque : {disk.get('path', '?')}  ({disk.get('model', '?')}, {human_size(disk.get('size'))})",
            f"Nom de l'ordinateur : {self.config['hostname']}",
            f"Utilisateur : {self.config['fullname']} ({self.config['username']})",
            f"Connexion automatique : {'Oui' if self.config['autologin'] else 'Non'}",
        ]
        if self.hw_info:
            lines.append("")
            lines.append(f"CPU : {self.hw_info.get('cpu', '?')}")
            lines.append(f"RAM : {self.hw_info.get('ram', '?')}")
            gpus = self.hw_info.get("gpu", [])
            for i, g in enumerate(gpus):
                lines.append(f"GPU {i}: {g}")
            mode = "UEFI" if self.hw_info.get("uefi") else "Legacy BIOS"
            lines.append(f"Démarrage : {mode}")
        self.summary_content.set_text("\n".join(lines))

    # ── Page 6 : Progression ─────────────────────────────────────

    def build_progress_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Installation en cours...", xalign=0)
        title.get_style_context().add_class("page-title")
        box.pack_start(title, False, False, 0)
        self.progress_detail = Gtk.Label(label="Préparation...", xalign=0)
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

    # ── Page 7 : Terminé ─────────────────────────────────────────

    def build_done_page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        box.set_valign(Gtk.Align.CENTER)
        box.set_halign(Gtk.Align.CENTER)
        icon = Gtk.Label(label="✓")
        icon.get_style_context().add_class("success-icon")
        box.pack_start(icon, False, False, 0)
        self.done_title = Gtk.Label(label="Installation terminée !")
        self.done_title.get_style_context().add_class("page-title")
        box.pack_start(self.done_title, False, False, 0)
        self.done_sub = Gtk.Label(
            label="Winlux a été installé avec succès. Retirez le média d'installation et redémarrez.",
            xalign=0.5
        )
        self.done_sub.get_style_context().add_class("page-subtitle")
        self.done_sub.set_line_wrap(True)
        self.done_sub.set_justify(Gtk.Justification.CENTER)
        box.pack_start(self.done_sub, False, False, 0)
        btn_box = Gtk.Box(spacing=12)
        btn_box.set_halign(Gtk.Align.CENTER)
        btn_box.set_margin_top(24)
        reboot_btn = Gtk.Button(label="Redémarrer maintenant")
        reboot_btn.get_style_context().add_class("btn-primary")
        reboot_btn.connect("clicked", self.on_reboot)
        btn_box.pack_start(reboot_btn, False, False, 0)
        close_btn = Gtk.Button(label="Fermer l'installateur")
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
        self.set_progress(0.02, f"Vérification du disque {device}...")
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
        self.set_progress(0.05, "Partitionnement du disque...")
        try:
            self.run_partitioning(device)
        except Exception as e:
            self.install_error(f"Erreur de partitionnement : {e}")
            return
        self.set_progress(0.10, "Formatage des partitions...")
        try:
            esp, root_part = self.format_partitions(device)
        except Exception as e:
            self.install_error(f"Erreur de formatage : {e}")
            return
        self.set_progress(0.15, "Extraction de l'ISO...")
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
        self.post_install()

    def install_from_live(self, device, live_root):
        self.set_progress(0.05, "Partitionnement du disque...")
        try:
            self.run_partitioning(device)
        except Exception as e:
            self.install_error(f"Erreur de partitionnement : {e}")
            return
        self.set_progress(0.10, "Formatage des partitions...")
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
        self.set_progress(0.20, "Copie du système (peut prendre plusieurs minutes)...")
        try:
            subprocess.run(
                ["cp", "-a", f"{live_root}/.", f"{work}/target/"],
                check=True, timeout=1800
            )
        except Exception as e:
            self.install_error(f"Copie échouée : {e}")
            subprocess.run(["umount", f"{work}/target"], timeout=10)
            return
        self.set_progress(0.70, "Installation du noyau et initramfs...")
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
        self.set_progress(0.75, "Configuration du bootloader GRUB...")
        try:
            self.install_grub(device, esp, root_part, work)
        except Exception as e:
            self.install_error(f"Erreur GRUB : {e}")
            subprocess.run(["umount", f"{work}/target"], timeout=10)
            return
        self.set_progress(0.85, "Configuration du compte utilisateur...")
        try:
            self.configure_user(f"{work}/target")
        except Exception as e:
            self.install_error(f"Erreur configuration utilisateur : {e}")
        self.set_progress(0.90, "Configuration du premier démarrage...")
        try:
            self.configure_first_boot(f"{work}/target")
        except Exception as e:
            self.set_progress(0.90, f"Avertissement premier démarrage : {e}")
        self.set_progress(0.95, "Synchronisation et démontage...")
        subprocess.run(["sync"], timeout=30)
        subprocess.run(["umount", f"{work}/target"], timeout=30)
        self.post_install()

    def run_partitioning(self, device):
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

    def format_partitions(self, device):
        esp = None
        root_part = None
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
        subprocess.run(["mkfs.ext4", "-F", "-L", "WINLUX_ROOT", root_part], check=True, timeout=120)
        return esp, root_part

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
        self.set_progress(0.50, "Copie sur le disque cible (peut prendre plusieurs minutes)...")
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
        self.set_progress(0.70, "Installation du bootloader GRUB...")
        self.install_grub(device, esp, root_part, work, split_root=split_root)
        self.set_progress(0.85, "Configuration du compte utilisateur...")
        self.configure_user(f"{work}/target")
        self.set_progress(0.90, "Configuration du premier démarrage...")
        self.configure_first_boot(f"{work}/target")
        self.set_progress(0.95, "Synchronisation...")
        subprocess.run(["sync"], timeout=30)
        subprocess.run(["umount", f"{work}/target/boot/efi"], timeout=10)
        subprocess.run(["umount", f"{work}/target"], timeout=10)

    def install_grub(self, device, esp, root_part, work, split_root=True):
        partuuid = subprocess.check_output(
            ["blkid", "-s", "PARTUUID", "-o", "value", root_part],
            text=True, timeout=10
        ).strip()
        fsuuid = subprocess.check_output(
            ["blkid", "-s", "UUID", "-o", "value", root_part],
            text=True, timeout=10
        ).strip()
        root_arg = f"UUID={fsuuid}" if split_root else f"PARTUUID={partuuid}"
        initrd = "initrd /boot/winlux-initramfs.cpio.gz" if split_root else ""
        grub_cfg = f"""set default=0
set timeout=8
serial --unit=0 --speed=115200
terminal_input console serial
terminal_output console serial
menuentry "Winlux Desktop" {{
    search --no-floppy --fs-uuid {fsuuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 winlux.interactive=1
    {initrd}
}}
menuentry "Winlux diagnostic" --hotkey=d {{
    search --no-floppy --fs-uuid {fsuuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 init=/init console=ttyS0,115200n8
    {initrd}
}}
menuentry "Winlux secours (restauration ou console)" --hotkey=s {{
    search --no-floppy --fs-uuid {fsuuid} --set=root
    linux /boot/vmlinuz root={root_arg} rw rootwait rootfstype=ext4 console=tty1 console=ttyS0,115200n8 winlux.recovery=1
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
        for d in [".config", ".local/share", "Bureau", "Documents", "Téléchargements", "Images", "Musique", "Vidéos"]:
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
        self.set_progress(1.0, "Installation terminée avec succès !")
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
