# #2589 : le dialogue « Affichage web » refuse le téléchargement par défaut.
w = (SCRIPTS / "winlux-webview.py").read_text(encoding="utf-8")
ok("dialog.set_default_response(Gtk.ResponseType.CANCEL)" in w
   and "set_default_response(Gtk.ResponseType.OK)" not in w,
   "Affichage web : « Plus tard » est la réponse par défaut (#2589)")
