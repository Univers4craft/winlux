# Boutique : relecture des 157 entrées qui perdent une recommandation (#2075) ;
# chaque logiciel cassé sans elle a sa ligne compagnons= (#2081 à #2103).
_c = (ROOT / "integration/store-catalog.txt").read_text(encoding="utf-8")
_paquets = {l.split("|")[3] for l in _c.splitlines() if l and not l.startswith("#") and l.count("|") == 8}
_comp = {l[11:].partition(":")[0]: l.partition(":")[2].split(",") for l in _c.splitlines() if l.startswith("compagnons=")}
_attendus = {"remmina": "remmina-plugin-rdp", "krdc": "freerdp2-x11", "knights": "stockfish", "kigo": "gnugo",
             "codeblocks": "g++", "kdevelop": "g++", "cmake": "make", "gnome-builder": "build-essential",
             "xarchiver": "unzip", "ark": "p7zip-full", "k3b": "growisofs", "freeciv-client-gtk3": "freeciv-server",
             "openttd": "openttd-opengfx", "kicad": "kicad-symbols", "syncthing-gtk": "syncthing",
             "gnome-calendar": "evolution-data-server", "libreoffice-base": "libreoffice-sdbc-hsqldb",
             "asunder": "lame", "inkscape": "python3-lxml", "vlc": "vlc-l10n", "smplayer": "smplayer-l10n",
             "nomacs": "nomacs-l10n", "claws-mail": "claws-mail-i18n"}
ok(all(c in _comp.get(p, []) for p, c in _attendus.items()),
   "Boutique : les 23 logiciels cassés sans leurs recommandations les reçoivent (#2081 à #2103)")
ok(set(_comp) <= _paquets, "Boutique : chaque ligne compagnons= vise un paquet du catalogue (#2081)")
ok(not any(c in ("remmina-plugin-secret", "kicad-packages3d", "kicad-libraries") for v in _comp.values() for c in v),
   "Boutique : ni trousseau (#1488) ni modèles 3D de plusieurs gigaoctets parmi les compagnons (#2081)")
