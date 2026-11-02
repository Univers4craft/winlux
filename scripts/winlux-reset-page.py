#!/usr/bin/python3
"""Paramètres > Système > Récupération (#283), façon Windows 11.

Réinitialiser ce PC (conserver mes fichiers / tout supprimer), revenir à la
version précédente, démarrage avancé (menu de secours), réparer le démarrage
et créer une clé de secours. Les opérations passent par winlux-reset, en root
via le relais winlux-root (Contrôle de compte d'utilisateur). Une option que
la machine ne permet pas (Live, outils absents) n'apparaît pas.

Chargé par winlux-settings-pages (page 30). Lancé seul, ouvre la page dans sa
propre fenêtre (captures, tests) : winlux-reset-page [--wizard reset|key].
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk, Pango


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
_TR = {
    "Récupération": ("Recovery", "Recuperación", "Wiederherstellung"),
    "Options de récupération": ("Recovery options", "Opciones de recuperación", "Wiederherstellungsoptionen"),
    "Réinitialiser ce PC": ("Reset this PC", "Restablecer este PC", "Diesen PC zurücksetzen"),
    "Choisissez de conserver ou de supprimer vos fichiers personnels, puis réinstallez Winlux.": ("Choose to keep or remove your personal files, then reinstall Winlux.", "Elija conservar o quitar sus archivos personales y vuelva a instalar Winlux.", "Wählen Sie, ob Ihre persönlichen Dateien behalten oder entfernt werden, und installieren Sie Winlux neu."),
    "Réinitialiser le PC": ("Reset PC", "Restablecer PC", "PC zurücksetzen"),
    "Revenir en arrière": ("Go back", "Volver", "Zurück"),
    "Revient à la version de Winlux d’avant la dernière mise à jour.": ("Returns to the Winlux version from before the last update.", "Vuelve a la versión de Winlux anterior a la última actualización.", "Kehrt zur Winlux-Version vor dem letzten Update zurück."),
    "Démarrage avancé": ("Advanced startup", "Inicio avanzado", "Erweiterter Start"),
    "Redémarre sur le menu de secours : réparer le démarrage, restaurer ou réinitialiser, même si Winlux ne démarre plus.": ("Restarts into the recovery menu: repair startup, restore or reset, even if Winlux no longer starts.", "Reinicia en el menú de recuperación: reparar el inicio, restaurar o restablecer, aunque Winlux ya no arranque.", "Startet in das Wiederherstellungsmenü: Start reparieren, wiederherstellen oder zurücksetzen, auch wenn Winlux nicht mehr startet."),
    "Redémarrer maintenant": ("Restart now", "Reiniciar ahora", "Jetzt neu starten"),
    "Réparer le démarrage": ("Repair startup", "Reparar el inicio", "Start reparieren"),
    "Réinstalle le chargeur de démarrage (GRUB) en BIOS et en UEFI, et réécrit son menu s’il est abîmé.": ("Reinstalls the boot loader (GRUB) for BIOS and UEFI, and rewrites its menu if damaged.", "Reinstala el cargador de arranque (GRUB) para BIOS y UEFI y reescribe su menú si está dañado.", "Installiert den Bootloader (GRUB) für BIOS und UEFI neu und schreibt sein Menü neu, falls beschädigt."),
    "Réparer": ("Repair", "Reparar", "Reparieren"),
    "Clé de secours": ("Recovery drive", "Unidad de recuperación", "Wiederherstellungslaufwerk"),
    "Une clé USB qui démarre directement sur le menu de secours de ce PC, même s’il ne démarre plus, et qui peut réinstaller Winlux. 4 Go au moins ; tout son contenu sera effacé.": ("A USB drive that starts straight into this PC’s recovery menu, even if it no longer starts, and can reinstall Winlux. At least 4 GB; everything on it will be erased.", "Una memoria USB que arranca directamente en el menú de recuperación de este PC, aunque ya no arranque, y que puede reinstalar Winlux. Al menos 4 GB; se borrará todo su contenido.", "Ein USB-Stick, der direkt im Wiederherstellungsmenü dieses PCs startet, auch wenn er nicht mehr startet, und Winlux neu installieren kann. Mindestens 4 GB; alles darauf wird gelöscht."),
    "Clé USB": ("USB drive", "Unidad USB", "USB-Laufwerk"),
    "Aucune clé USB branchée.": ("No USB drive plugged in.", "No hay ninguna unidad USB conectada.", "Kein USB-Laufwerk angeschlossen."),
    "Actualiser": ("Refresh", "Actualizar", "Aktualisieren"),
    "Créer une clé de secours": ("Create a recovery drive", "Crear una unidad de recuperación", "Wiederherstellungslaufwerk erstellen"),
    "Créer la clé": ("Create drive", "Crear unidad", "Laufwerk erstellen"),
    "Une réinitialisation est prête : redémarrez pour la terminer.": ("A reset is ready: restart to finish it.", "Hay un restablecimiento listo: reinicie para terminarlo.", "Eine Zurücksetzung ist bereit: Starten Sie neu, um sie abzuschließen."),
    "Annuler la réinitialisation": ("Cancel the reset", "Cancelar el restablecimiento", "Zurücksetzen abbrechen"),
    "Conserver mes fichiers": ("Keep my files", "Conservar mis archivos", "Eigene Dateien behalten"),
    "Supprime les applications et les réglages, mais conserve vos comptes et vos fichiers personnels.": ("Removes apps and settings, but keeps your accounts and personal files.", "Quita aplicaciones y configuración, pero conserva sus cuentas y archivos personales.", "Entfernt Apps und Einstellungen, behält aber Ihre Konten und persönlichen Dateien."),
    "Tout supprimer": ("Remove everything", "Quitar todo", "Alles entfernen"),
    "Supprime tous vos fichiers personnels, comptes, applications et réglages.": ("Removes all your personal files, accounts, apps and settings.", "Quita todos sus archivos personales, cuentas, aplicaciones y configuración.", "Entfernt alle persönlichen Dateien, Konten, Apps und Einstellungen."),
    "Choisir une option": ("Choose an option", "Elegir una opción", "Option auswählen"),
    "Nettoyer aussi le lecteur": ("Also clean the drive", "Limpiar también la unidad", "Laufwerk auch bereinigen"),
    "Remplit l’espace libre de zéros : vos fichiers ne pourront plus être récupérés. Beaucoup plus long (parfois des heures) ; utile avant de donner ou vendre le PC.": ("Fills free space with zeros so your files cannot be recovered. Much longer (sometimes hours); useful before giving away or selling the PC.", "Rellena el espacio libre con ceros para que sus archivos no puedan recuperarse. Mucho más lento (a veces horas); útil antes de regalar o vender el PC.", "Füllt den freien Speicher mit Nullen, damit Ihre Dateien nicht wiederhergestellt werden können. Viel länger (teils Stunden); sinnvoll vor Weitergabe oder Verkauf."),
    "Nouveau compte": ("New account", "Nueva cuenta", "Neues Konto"),
    "Nom complet": ("Full name", "Nombre completo", "Vollständiger Name"),
    "Nom du compte": ("Account name", "Nombre de la cuenta", "Kontoname"),
    "Mot de passe": ("Password", "Contraseña", "Kennwort"),
    "Confirmer le mot de passe": ("Confirm password", "Confirmar contraseña", "Kennwort bestätigen"),
    "Le nom du compte : lettres minuscules, chiffres, - et _.": ("Account name: lowercase letters, digits, - and _.", "Nombre de la cuenta: minúsculas, cifras, - y _.", "Kontoname: Kleinbuchstaben, Ziffern, - und _."),
    "Les mots de passe sont vides ou différents.": ("The passwords are empty or different.", "Las contraseñas están vacías o no coinciden.", "Die Kennwörter sind leer oder verschieden."),
    "Comment réinstaller Winlux ?": ("How do you want to reinstall Winlux?", "¿Cómo desea reinstalar Winlux?", "Wie soll Winlux neu installiert werden?"),
    "Image ISO Winlux…": ("Winlux ISO image…", "Imagen ISO de Winlux…", "Winlux-ISO-Abbild…"),
    "Image ISO Winlux": ("Winlux ISO image", "Imagen ISO de Winlux", "Winlux-ISO-Abbild"),
    "Prêt à réinitialiser ce PC": ("Ready to reset this PC", "Listo para restablecer este PC", "Bereit, diesen PC zurückzusetzen"),
    "Ce qui va se passer :": ("This will:", "Esto hará lo siguiente:", "Dabei wird:"),
    "Vos comptes, leurs mots de passe et vos fichiers personnels (dossier /home) sont conservés.": ("Keep your accounts, their passwords and your personal files (/home folder).", "Conservar sus cuentas, sus contraseñas y sus archivos personales (carpeta /home).", "Ihre Konten, deren Kennwörter und Ihre persönlichen Dateien (Ordner /home) behalten."),
    "Les programmes Windows installés dans votre dossier personnel restent.": ("Keep the Windows programs installed in your personal folder.", "Mantener los programas de Windows instalados en su carpeta personal.", "Die in Ihrem persönlichen Ordner installierten Windows-Programme behalten."),
    "Les applications installées depuis la Boutique et tous les réglages reviennent à l’état d’origine ; leur liste est déposée sur votre bureau.": ("Remove apps installed from the Store and restore every setting; their list is left on your desktop.", "Quitar las aplicaciones instaladas desde la Tienda y restaurar toda la configuración; su lista queda en el escritorio.", "Aus dem Store installierte Apps entfernen und alle Einstellungen zurücksetzen; ihre Liste liegt auf Ihrem Desktop."),
    "Le réseau Wi-Fi, la langue, le clavier et le nom du PC sont gardés.": ("Keep the Wi-Fi network, language, keyboard and PC name.", "Conservar la red Wi-Fi, el idioma, el teclado y el nombre del PC.", "WLAN, Sprache, Tastatur und PC-Namen behalten."),
    "Tous les comptes et tous les fichiers personnels sont supprimés, y compris une partition /home séparée.": ("Remove all accounts and all personal files, including a separate /home partition.", "Quitar todas las cuentas y todos los archivos personales, incluida una partición /home separada.", "Alle Konten und alle persönlichen Dateien entfernen, auch eine eigene /home-Partition."),
    "Toutes les applications et tous les réglages sont supprimés.": ("Remove all apps and settings.", "Quitar todas las aplicaciones y la configuración.", "Alle Apps und Einstellungen entfernen."),
    "Winlux est réinstallé avec le compte « {user} ». La langue, le clavier et le nom du PC sont gardés.": ("Reinstall Winlux with the “{user}” account. Language, keyboard and PC name are kept.", "Reinstalar Winlux con la cuenta «{user}». Se conservan el idioma, el teclado y el nombre del PC.", "Winlux mit dem Konto „{user}“ neu installieren. Sprache, Tastatur und PC-Name bleiben."),
    "Le lecteur est ensuite nettoyé (effacement sûr).": ("Then clean the drive (secure erase).", "Después limpiar la unidad (borrado seguro).", "Danach das Laufwerk bereinigen (sicheres Löschen)."),
    "Source : {source}": ("Source: {source}", "Origen: {source}", "Quelle: {source}"),
    "Le PC redémarrera pour terminer ; ne l’éteignez pas pendant l’opération.": ("The PC will restart to finish; do not turn it off during the operation.", "El PC se reiniciará para terminar; no lo apague durante la operación.", "Der PC startet zum Abschließen neu; schalten Sie ihn währenddessen nicht aus."),
    "Je comprends que tous mes fichiers seront supprimés définitivement.": ("I understand that all my files will be permanently deleted.", "Entiendo que todos mis archivos se eliminarán definitivamente.", "Ich verstehe, dass alle meine Dateien endgültig gelöscht werden."),
    "Réinitialiser": ("Reset", "Restablecer", "Zurücksetzen"),
    "Suivant": ("Next", "Siguiente", "Weiter"),
    "Précédent": ("Back", "Atrás", "Zurück"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Fermer": ("Close", "Cerrar", "Schließen"),
    "Plus tard": ("Later", "Más tarde", "Später"),
    "Préparation…": ("Preparing…", "Preparando…", "Wird vorbereitet…"),
    "Préparation de la réinitialisation": ("Preparing the reset", "Preparando el restablecimiento", "Zurücksetzen wird vorbereitet"),
    "Vous pouvez continuer à utiliser le PC pendant la préparation. Rien n’est effacé avant le redémarrage.": ("You can keep using the PC while it prepares. Nothing is erased before the restart.", "Puede seguir usando el PC durante la preparación. No se borra nada antes del reinicio.", "Sie können den PC während der Vorbereitung weiter nutzen. Vor dem Neustart wird nichts gelöscht."),
    "Tout le contenu de « {key} » sera effacé.": ("Everything on “{key}” will be erased.", "Se borrará todo el contenido de «{key}».", "Alles auf „{key}“ wird gelöscht."),
    "Choisissez une clé USB.": ("Choose a USB drive.", "Elija una unidad USB.", "Wählen Sie ein USB-Laufwerk."),
    "L’opération a échoué": ("The operation failed", "La operación ha fallado", "Der Vorgang ist fehlgeschlagen"),
    "Confirmer cette opération ?": ("Confirm this operation?", "¿Confirmar esta operación?", "Diesen Vorgang bestätigen?"),
    "Enregistrez vos documents avant de continuer.": ("Save your documents before continuing.", "Guarde sus documentos antes de continuar.", "Speichern Sie Ihre Dokumente, bevor Sie fortfahren."),
    "Opération terminée.": ("Operation complete.", "Operación terminada.", "Vorgang abgeschlossen."),
    "Si Winlux ne démarre plus, choisissez « Winlux secours » dans le menu de démarrage (touche Échap au démarrage) ou démarrez sur la clé de secours.": ("If Winlux no longer starts, choose “Winlux secours” in the boot menu (Esc key at startup) or start from the recovery drive.", "Si Winlux ya no arranca, elija «Winlux secours» en el menú de arranque (tecla Esc al iniciar) o arranque desde la unidad de recuperación.", "Wenn Winlux nicht mehr startet, wählen Sie „Winlux secours“ im Startmenü (Esc beim Start) oder starten Sie vom Wiederherstellungslaufwerk."),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


TOOL = os.environ.get("WINLUX_RESET_TOOL", "winlux-reset")
PREVIOUS_PACKAGE = "/var/cache/winlux/previous.deb"


def reset_command(args, admin=True):
    """winlux-reset en root : directement en root, sinon par le relais (UAC)."""
    if not admin or os.geteuid() == 0 or not shutil.which("winlux-root") or os.environ.get("WINLUX_RESET_TOOL"):
        return [TOOL, *args]
    return ["winlux-root", "exec", TOOL, *args]


def tool_lines(args, timeout=20):
    try:
        result = subprocess.run(reset_command(args, admin=False), capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError):
        return []
    return [line for line in result.stdout.splitlines() if line.strip()]


def reset_status():
    return dict(line.split("=", 1) for line in tool_lines(["status"]) if "=" in line)


def available():
    """La page existe si winlux-reset est là (sinon elle est masquée)."""
    return shutil.which(TOOL) is not None or Path(TOOL).is_file()


def size_text(size):
    return f"{size / 1000 ** 3:.1f} Go".replace(".", ",") if _LANG == "fr" else f"{size / 1000 ** 3:.1f} GB"


class Runner:
    """Lance winlux-reset et suit ses lignes PROGRESS / DONE / ERROR."""

    def __init__(self, args, bar, label, finished, stdin_text=None):
        self.bar, self.label, self.finished = bar, label, finished
        self.result, self.message = None, ""
        self.process = subprocess.Popen(reset_command(args), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True, bufsize=1)
        if stdin_text is not None:
            self.process.stdin.write(stdin_text + "\n")
        self.process.stdin.close()
        GLib.io_add_watch(self.process.stdout, GLib.PRIORITY_DEFAULT, GLib.IO_IN | GLib.IO_HUP, self.read)

    def read(self, stream, condition):
        line = stream.readline()
        if line:
            kind, _sep, text = line.rstrip("\n").partition(" ")
            if kind == "PROGRESS":
                percent, _sep, text = text.partition(" ")
                try:
                    self.bar.set_fraction(max(0, min(100, int(percent))) / 100)
                except ValueError:
                    self.bar.pulse()
                self.label.set_text(text)
            elif kind in ("DONE", "ERROR"):
                self.result, self.message = kind, text
            return True
        code = self.process.wait()
        if self.result is None:
            self.result = "DONE" if code == 0 else "ERROR"
            self.message = self.message or (self.process.stderr.read() or "").strip()[-1500:]
        self.finished(self.result == "DONE" and code == 0, self.message)
        return False


def wrap_label(text, style=None, width=None):
    label = Gtk.Label(label=text, xalign=0, wrap=True)
    label.set_line_wrap_mode(Pango.WrapMode.WORD_CHAR)
    label.set_max_width_chars(70)
    if width:
        # Sans largeur fixe, GTK 3 calcule la hauteur d'un texte replié pour
        # une colonne d'un mot et la fenêtre de l'assistant s'étire.
        label.set_size_request(width, -1)
    if style:
        label.get_style_context().add_class(style)
    return label


def choice_button(title, text, callback):
    button = Gtk.Button()
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4, margin=6)
    head = wrap_label(title, width=WIDE)
    head.get_style_context().add_class("section")
    box.pack_start(head, False, False, 0)
    box.pack_start(wrap_label(text, "subtitle", WIDE), False, False, 0)
    button.add(box)
    button.connect("clicked", lambda *_: callback())
    return button


WIDE = 560


def WRAP(text, style=None):
    """Texte replié des assistants, à largeur fixe."""
    return wrap_label(text, style, WIDE)


class Wizard(Gtk.Dialog):
    """Assistant en plusieurs étapes, comme « Réinitialiser ce PC » de Windows 11."""

    def __init__(self, parent, title):
        super().__init__(title=title, transient_for=parent, modal=True)
        self.set_default_size(620, 460)
        self.get_style_context().add_class("winlux-reset")
        area = self.get_content_area()
        area.set_spacing(12)
        area.set_border_width(18)
        self.heading = WRAP(title)
        self.heading.get_style_context().add_class("heading")
        area.pack_start(self.heading, False, False, 0)
        self.stack = Gtk.Stack(transition_type=Gtk.StackTransitionType.SLIDE_LEFT)
        area.pack_start(self.stack, True, True, 0)
        self.footer = Gtk.Box(spacing=8, halign=Gtk.Align.END)
        area.pack_end(self.footer, False, False, 0)
        self.running = False
        self.connect("delete-event", lambda *_: self.running)

    def buttons(self, *specs):
        for child in self.footer.get_children():
            child.destroy()
        for title, callback, style in specs:
            button = Gtk.Button(label=title)
            if style:
                button.get_style_context().add_class(style)
            button.connect("clicked", lambda *_, cb=callback: cb())
            self.footer.pack_start(button, False, False, 0)
        self.footer.show_all()

    def page(self, name, title, *widgets):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        if title:
            head = WRAP(title)
            head.get_style_context().add_class("section")
            box.pack_start(head, False, False, 0)
        for widget in widgets:
            box.pack_start(widget, False, False, 0)
        old = self.stack.get_child_by_name(name)
        if old is not None:
            old.destroy()
        self.stack.add_named(box, name)
        box.show_all()
        self.stack.set_visible_child(box)
        return box

    def progress_page(self, title, hint):
        self.bar = Gtk.ProgressBar(show_text=False)
        self.step = WRAP(_t("Préparation…"), "subtitle")
        self.page("progress", title, WRAP(hint, "subtitle"), self.bar, self.step)
        self.buttons()
        self.running = True

    def result_page(self, ok, message, buttons):
        self.running = False
        label = WRAP(message or (_t("Opération terminée.") if ok else _t("L’opération a échoué")))
        label.set_selectable(True)
        self.page("result", _t("Opération terminée.") if ok else _t("L’opération a échoué"), label)
        self.buttons(*buttons)


class ResetWizard(Wizard):
    def __init__(self, parent, done):
        super().__init__(parent, _t("Réinitialiser ce PC"))
        self.done = done
        self.mode, self.source, self.source_label = None, None, ""
        self.choose()

    def choose(self):
        self.page("choose", _t("Choisir une option"),
                  choice_button(_t("Conserver mes fichiers"),
                                _t("Supprime les applications et les réglages, mais conserve vos comptes et vos fichiers personnels."),
                                lambda: self.picked("keep")),
                  choice_button(_t("Tout supprimer"),
                                _t("Supprime tous vos fichiers personnels, comptes, applications et réglages."),
                                lambda: self.picked("wipe")))
        self.buttons((_t("Annuler"), self.destroy, None))

    def picked(self, mode):
        self.mode = mode
        if mode == "wipe":
            self.account()
        else:
            self.sources()

    def account(self):
        grid = Gtk.Grid(row_spacing=8, column_spacing=12)
        self.fullname, self.user = Gtk.Entry(), Gtk.Entry()
        self.password = Gtk.Entry(visibility=False, input_purpose=Gtk.InputPurpose.PASSWORD)
        self.again = Gtk.Entry(visibility=False, input_purpose=Gtk.InputPurpose.PASSWORD)
        for row, (title, entry) in enumerate(((_t("Nom complet"), self.fullname), (_t("Nom du compte"), self.user),
                                              (_t("Mot de passe"), self.password),
                                              (_t("Confirmer le mot de passe"), self.again))):
            grid.attach(Gtk.Label(label=title, xalign=0), 0, row, 1, 1)
            entry.set_hexpand(True)
            grid.attach(entry, 1, row, 1, 1)
        self.fullname.connect("changed", self.suggest_user)
        self.clean = Gtk.Switch(halign=Gtk.Align.START)
        clean_row = Gtk.Box(spacing=12)
        clean_row.pack_start(wrap_label(_t("Nettoyer aussi le lecteur"), width=460), True, True, 0)
        clean_row.pack_start(self.clean, False, False, 0)
        self.account_error = WRAP("", "subtitle")
        self.page("account", _t("Nouveau compte"), grid, self.account_error, clean_row,
                  WRAP(_t("Remplit l’espace libre de zéros : vos fichiers ne pourront plus être récupérés. Beaucoup plus long (parfois des heures) ; utile avant de donner ou vendre le PC."), "subtitle"))
        self.buttons((_t("Précédent"), self.choose, None), (_t("Suivant"), self.account_next, "suggested-action"))

    def suggest_user(self, entry):
        import unicodedata
        text = unicodedata.normalize("NFKD", entry.get_text()).encode("ascii", "ignore").decode().lower()
        name = "".join(c for c in text.split(" ")[0] if c.isalnum() or c in "-_")[:32]
        if self.user.get_text() in ("", getattr(self, "_suggested", "")):
            self.user.set_text(name)
        self._suggested = name

    def account_next(self):
        import re
        user = self.user.get_text().strip()
        if not re.fullmatch(r"[a-z_][a-z0-9_-]{0,31}", user) or user in ("root", "winlux-web", "nobody"):
            self.account_error.set_text(_t("Le nom du compte : lettres minuscules, chiffres, - et _."))
            return
        if not self.password.get_text() or self.password.get_text() != self.again.get_text():
            self.account_error.set_text(_t("Les mots de passe sont vides ou différents."))
            return
        self.sources()

    def sources(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        group = None
        self.choices = []
        for line in tool_lines(["sources"]):
            spec, _sep, label = line.partition("|")
            radio = Gtk.RadioButton.new_with_label_from_widget(group, label)
            group = group or radio
            self.choices.append((radio, spec, label))
            box.pack_start(radio, False, False, 0)
        iso = Gtk.RadioButton.new_with_label_from_widget(group, _t("Image ISO Winlux…"))
        self.choices.append((iso, None, _t("Image ISO Winlux")))
        box.pack_start(iso, False, False, 0)
        self.page("source", _t("Comment réinstaller Winlux ?"), box)
        self.buttons((_t("Précédent"), self.account if self.mode == "wipe" else self.choose, None),
                     (_t("Suivant"), self.source_next, "suggested-action"))

    def source_next(self):
        for radio, spec, label in self.choices:
            if radio.get_active():
                if spec is None:
                    dialog = Gtk.FileChooserNative.new(_t("Image ISO Winlux"), self, Gtk.FileChooserAction.OPEN, None, None)
                    flt = Gtk.FileFilter()
                    flt.add_pattern("*.iso")
                    flt.add_pattern("*.ISO")
                    dialog.add_filter(flt)
                    if dialog.run() != Gtk.ResponseType.ACCEPT or not dialog.get_filename():
                        dialog.destroy()
                        return
                    spec, label = "iso:" + dialog.get_filename(), dialog.get_filename()
                    dialog.destroy()
                self.source, self.source_label = spec, label
                self.summary()
                return

    def summary(self):
        lines = [_t("Ce qui va se passer :")]
        if self.mode == "keep":
            lines += [_t("Vos comptes, leurs mots de passe et vos fichiers personnels (dossier /home) sont conservés."),
                      _t("Les programmes Windows installés dans votre dossier personnel restent."),
                      _t("Les applications installées depuis la Boutique et tous les réglages reviennent à l’état d’origine ; leur liste est déposée sur votre bureau."),
                      _t("Le réseau Wi-Fi, la langue, le clavier et le nom du PC sont gardés.")]
        else:
            lines += [_t("Tous les comptes et tous les fichiers personnels sont supprimés, y compris une partition /home séparée."),
                      _t("Toutes les applications et tous les réglages sont supprimés."),
                      _t("Winlux est réinstallé avec le compte « {user} ». La langue, le clavier et le nom du PC sont gardés.").format(user=self.user.get_text().strip())]
            if self.clean.get_active():
                lines.append(_t("Le lecteur est ensuite nettoyé (effacement sûr)."))
        text = lines[0] + "\n" + "\n".join("  •  " + line for line in lines[1:])
        widgets = [WRAP(text), WRAP(_t("Source : {source}").format(source=self.source_label), "subtitle"),
                   WRAP(_t("Le PC redémarrera pour terminer ; ne l’éteignez pas pendant l’opération."), "subtitle")]
        self.go = None
        if self.mode == "wipe":
            self.understood = Gtk.CheckButton(label=_t("Je comprends que tous mes fichiers seront supprimés définitivement."))
            self.understood.connect("toggled", lambda button: self.go and self.go.set_sensitive(button.get_active()))
            widgets.append(self.understood)
        self.page("summary", _t("Prêt à réinitialiser ce PC"), *widgets)
        self.buttons((_t("Précédent"), self.sources, None), (_t("Annuler"), self.destroy, None),
                     (_t("Réinitialiser"), self.start, "destructive-action"))
        self.go = self.footer.get_children()[-1]
        self.go.set_sensitive(self.mode == "keep")

    def start(self):
        args = ["prepare", "--mode", self.mode, "--source", self.source]
        password = None
        if self.mode == "wipe":
            args += ["--user", self.user.get_text().strip(),
                     "--fullname", self.fullname.get_text().strip().replace(":", " ").replace(",", " ") or self.user.get_text().strip()]
            if self.clean.get_active():
                args.append("--clean")
            password = self.password.get_text()
        self.progress_page(_t("Préparation de la réinitialisation"),
                           _t("Vous pouvez continuer à utiliser le PC pendant la préparation. Rien n’est effacé avant le redémarrage."))
        self.runner = Runner(args, self.bar, self.step, self.finished, password)

    def finished(self, ok, message):
        if ok:
            self.result_page(True, message, [(_t("Plus tard"), self.close_done, None),
                                             (_t("Redémarrer maintenant"), self.reboot, "suggested-action")])
        else:
            self.result_page(False, message, [(_t("Fermer"), self.close_done, None)])

    def close_done(self):
        self.destroy()
        self.done()

    def reboot(self):
        subprocess.Popen(["winlux-control", "reboot"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.close_done()


class KeyWizard(Wizard):
    def __init__(self, parent, device, label, done):
        super().__init__(parent, _t("Créer une clé de secours"))
        self.device, self.label, self.done = device, label, done
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        group = None
        self.choices = []
        for line in tool_lines(["sources"]):
            spec, _sep, text = line.partition("|")
            radio = Gtk.RadioButton.new_with_label_from_widget(group, text)
            group = group or radio
            self.choices.append((radio, spec))
            box.pack_start(radio, False, False, 0)
        warning = WRAP(_t("Tout le contenu de « {key} » sera effacé.").format(key=label))
        warning.get_style_context().add_class("warning")
        self.page("source", _t("Comment réinstaller Winlux ?"), box, warning)
        self.buttons((_t("Annuler"), self.destroy, None), (_t("Créer la clé"), self.start, "destructive-action"))

    def start(self):
        spec = next((spec for radio, spec in self.choices if radio.get_active()), "cloud")
        self.progress_page(_t("Créer une clé de secours"), _t("Tout le contenu de « {key} » sera effacé.").format(key=self.label))
        self.runner = Runner(["make-key", "--device", self.device, "--source", spec], self.bar, self.step, self.finished)

    def finished(self, ok, message):
        self.result_page(ok, message, [(_t("Fermer"), lambda: (self.destroy(), self.done()), None)])


class RecoveryPanel:
    """Construit la page dans `host` (fenêtre Paramètres : section, card, row, hint…)."""

    def __init__(self, host):
        self.host = host
        self.status = reset_status()
        installed = self.status.get("installed") == "1" and self.status.get("live") != "1"
        host.section(_t("Options de récupération"))
        box = host.card()
        if installed and self.status.get("pending"):
            line = Gtk.Box(spacing=8)
            for title, callback in ((_t("Redémarrer maintenant"), self.reboot),
                                    (_t("Annuler la réinitialisation"), self.cancel)):
                button = Gtk.Button(label=title)
                button.connect("clicked", lambda *_, cb=callback: cb())
                line.pack_start(button, False, False, 0)
            host.row(box, _t("Réinitialiser ce PC"), line, _t("Une réinitialisation est prête : redémarrez pour la terminer."))
        elif installed:
            host.row(box, _t("Réinitialiser ce PC"), self.button(_t("Réinitialiser le PC"), self.reset),
                     _t("Choisissez de conserver ou de supprimer vos fichiers personnels, puis réinstallez Winlux."))
        if installed and Path(PREVIOUS_PACKAGE).is_file() and hasattr(host, "execute"):
            host.row(box, _t("Revenir en arrière"), self.button(_t("Revenir en arrière"), lambda: host.execute("update-rollback")),
                     _t("Revient à la version de Winlux d’avant la dernière mise à jour."))
        if installed:
            host.row(box, _t("Démarrage avancé"), self.button(_t("Redémarrer maintenant"), self.advanced),
                     _t("Redémarre sur le menu de secours : réparer le démarrage, restaurer ou réinitialiser, même si Winlux ne démarre plus."))
        if installed and self.status.get("grub_install") == "1":
            host.row(box, _t("Réparer le démarrage"), self.button(_t("Réparer"), self.repair),
                     _t("Réinstalle le chargeur de démarrage (GRUB) en BIOS et en UEFI, et réécrit son menu s’il est abîmé."))
        if not box.get_children():
            box.destroy()
        if self.status.get("key_tools") == "1":
            host.section(_t("Clé de secours"),
                         _t("Une clé USB qui démarre directement sur le menu de secours de ce PC, même s’il ne démarre plus, et qui peut réinstaller Winlux. 4 Go au moins ; tout son contenu sera effacé."))
            box = host.card()
            self.keys = Gtk.ComboBoxText()
            line = Gtk.Box(spacing=8)
            line.pack_start(self.keys, False, False, 0)
            line.pack_start(self.button(_t("Actualiser"), self.fill_keys), False, False, 0)
            host.row(box, _t("Clé USB"), line)
            self.key_hint = host.hint("", box)
            self.create = self.button(_t("Créer une clé de secours"), self.make_key)
            host.row(box, _t("Créer une clé de secours"), self.create)
            self.fill_keys()
        host.hint(_t("Si Winlux ne démarre plus, choisissez « Winlux secours » dans le menu de démarrage (touche Échap au démarrage) ou démarrez sur la clé de secours."))

    @staticmethod
    def button(title, callback):
        button = Gtk.Button(label=title)
        button.connect("clicked", lambda *_: callback())
        return button

    def window(self):
        return self.host if isinstance(self.host, Gtk.Window) else None

    def refresh(self):
        if hasattr(self.host, "extra_refresh"):
            self.host.extra_refresh()

    def fill_keys(self):
        self.keys.remove_all()
        for line in tool_lines(["list-keys"]):
            path, size, model = (line.split("|") + ["", ""])[:3]
            if size.isdigit():
                self.keys.append(path, f"{model} ({size_text(int(size))}) — {path}")
        has = bool(self.keys.get_model().iter_n_children(None))
        if has:
            self.keys.set_active(0)
        self.keys.set_visible(has)
        self.create.set_sensitive(has)
        self.key_hint.set_text("" if has else _t("Aucune clé USB branchée."))

    def confirm(self):
        dialog = Gtk.MessageDialog(transient_for=self.window(), modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                   text=_t("Confirmer cette opération ?"),
                                   secondary_text=_t("Enregistrez vos documents avant de continuer."))
        answer = dialog.run()
        dialog.destroy()
        return answer == Gtk.ResponseType.OK

    def reset(self):
        ResetWizard(self.window(), self.refresh).show_all()

    def make_key(self):
        device = self.keys.get_active_id()
        if not device:
            self.key_hint.set_text(_t("Choisissez une clé USB."))
            return
        KeyWizard(self.window(), device, self.keys.get_active_text(), self.fill_keys).show_all()

    def simple(self, title, args, after=None):
        dialog = Wizard(self.window(), title)
        dialog.progress_page(title, "")

        def finished(ok, message):
            if ok and after:
                dialog.destroy()
                after()
                return
            dialog.result_page(ok, message, [(_t("Fermer"), lambda: (dialog.destroy(), self.refresh()), None)])
        dialog.runner = Runner(args, dialog.bar, dialog.step, finished)
        dialog.show_all()

    def repair(self):
        self.simple(_t("Réparer le démarrage"), ["repair-boot"])

    def advanced(self):
        if self.confirm():
            self.simple(_t("Démarrage avancé"), ["reboot-recovery"], self.reboot_now)

    def cancel(self):
        self.simple(_t("Annuler la réinitialisation"), ["cancel"])

    def reboot(self):
        if self.confirm():
            self.reboot_now()

    @staticmethod
    def reboot_now():
        subprocess.Popen(["winlux-control", "reboot"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


CSS = b"""
  window, dialog { background: #181b24; color: #ecedf5; }
  .heading { font-size: 24px; font-weight: bold; }
  .subtitle { color: #adb6c9; }
  .warning { color: #ffb4a8; font-weight: bold; }
  .card { background: #242a38; border-radius: 8px; padding: 16px; }
  .section { font-size: 16px; font-weight: bold; margin-top: 6px; }
  entry { background: #171b25; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 8px; }
  button { background: #30384b; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 10px 14px; box-shadow: none; }
  button:hover { background: #414b64; }
  button.suggested-action { background: #4f6bed; border-color: #4f6bed; }
  button.destructive-action { background: #c42b1c; border-color: #c42b1c; }
  button:disabled { color: #858da0; background: #252a36; }
"""


class StandaloneWindow(Gtk.Window):
    """Page seule (captures, tests) avec les briques des Paramètres."""

    def __init__(self):
        super().__init__(title=_t("Récupération"))
        self.set_default_size(980, 720)
        css = Gtk.CssProvider()
        css.load_from_data(CSS)
        Gtk.StyleContext.add_provider_for_screen(self.get_screen(), css, 600)
        scroll = Gtk.ScrolledWindow()
        self.add(scroll)
        self.body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=18, margin=28)
        scroll.add(self.body)
        title = Gtk.Label(label=_t("Récupération"), xalign=0)
        title.get_style_context().add_class("heading")
        self.body.pack_start(title, False, False, 0)
        self.panel = RecoveryPanel(self)
        self.connect("destroy", lambda *_: Gtk.main_level() and Gtk.main_quit())

    def section(self, title, text=None):
        label = Gtk.Label(label=title, xalign=0)
        label.get_style_context().add_class("section")
        self.body.pack_start(label, False, False, 0)
        if text:
            self.hint(text)

    def hint(self, text, parent=None):
        label = wrap_label(text, "subtitle")
        (parent or self.body).pack_start(label, False, False, 0)
        return label

    def card(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        box.get_style_context().add_class("card")
        self.body.pack_start(box, False, False, 0)
        return box

    def row(self, parent, title, widget, subtitle=None):
        line = Gtk.Box(spacing=12)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, valign=Gtk.Align.CENTER)
        texts.pack_start(wrap_label(title), False, False, 0)
        if subtitle:
            self.hint(subtitle, texts)
        line.pack_start(texts, True, True, 0)
        if widget is not None:
            widget.set_valign(Gtk.Align.CENTER)
            line.pack_start(widget, False, False, 0)
        parent.pack_start(line, False, False, 0)
        return line


def main(argv):
    window = StandaloneWindow()
    window.show_all()
    if "--wizard" in argv:
        kind = argv[argv.index("--wizard") + 1] if len(argv) > argv.index("--wizard") + 1 else "reset"
        if kind == "key":
            KeyWizard(window, "/dev/sdb", "Clé USB (16,0 Go) — /dev/sdb", lambda: None).show_all()
        else:
            ResetWizard(window, lambda: None).show_all()
    Gtk.main()
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
