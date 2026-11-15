# #2591 : le dialogue « Affichage web » nomme paquet, version, éditeur et source avant de télécharger.
w = (SCRIPTS / "winlux-webview.py").read_text(encoding="utf-8")
ok("def package_details():" in w and "version, publisher, source = package_details()" in w
   and "Herausgeber: {publisher}" in w and "Editor: {publisher}" in w and "Publisher: {publisher}" in w,
   "Affichage web : paquet, version, éditeur et source nommés dans les 4 langues (#2591)")
