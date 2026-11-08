#!/usr/bin/python3 -I
"""Confirmation avant de télécharger un logiciel d'éditeur (Store, #1056).

    winlux-store-confirm --title T --text X --ok Installer --cancel Annuler

Le texte (produit, version, éditeur, source, empreinte SHA-256) est écrit par
winlux-store dans la langue de l'interface. Annuler est le bouton par défaut :
Entrée, Échap ou fermer la fenêtre refusent. Code 0 seulement pour « OK » ;
1 pour un refus, 2 si aucune fenêtre ne peut s'ouvrir (refus aussi)."""
import argparse
import sys


def main():
    parser = argparse.ArgumentParser()
    for name in ("--title", "--text", "--ok", "--cancel"):
        parser.add_argument(name, required=True)
    args = parser.parse_args()
    try:
        import gi
        gi.require_version("Gtk", "3.0")
        from gi.repository import Gtk
        if not Gtk.init_check(sys.argv)[0]:
            return 2
    except (ImportError, ValueError):
        return 2
    dialog = Gtk.MessageDialog(modal=True, message_type=Gtk.MessageType.QUESTION, text=args.title)
    dialog.set_title(args.title)
    dialog.format_secondary_text(args.text)
    for child in dialog.get_message_area().get_children():
        if isinstance(child, Gtk.Label):
            child.set_selectable(True)  # adresse et empreinte copiables
    cancel = dialog.add_button(args.cancel, Gtk.ResponseType.CANCEL)
    dialog.add_button(args.ok, Gtk.ResponseType.OK)
    dialog.set_default_response(Gtk.ResponseType.CANCEL)
    dialog.set_keep_above(True)
    dialog.set_position(Gtk.WindowPosition.CENTER)
    cancel.grab_focus()
    answer = dialog.run()
    dialog.destroy()
    return 0 if answer == Gtk.ResponseType.OK else 1


if __name__ == "__main__":
    sys.exit(main())
