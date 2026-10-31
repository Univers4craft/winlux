# Store Winlux et gestionnaire de polices

## Store : une seule bibliothèque Linux + Windows

L'application **Store** du shell affiche une bibliothèque unique. Le service
`winlux-store` (`scripts/winlux-store.py`, Python standard sans dépendance)
réunit plusieurs sources ; une source n'apparaît que si elle est utilisable
sur la machine (outil installé, dépôt configuré, catalogue en cache ou
réseau disponible).

| Source | Catalogue et métadonnées | Installation |
|---|---|---|
| `apt` | AppStream DEP-11 des dépôts (`/var/lib/swcatalog`, `/var/lib/app-info`) + sélection Winlux `integration/store-catalog.txt` ; captures de screenshots.debian.net à défaut | `apt-get` avec `APT::Status-Fd` |
| `flatpak` | AppStream XML des remotes configurées (Flathub…) | `flatpak install/update/uninstall` |
| `snap` | API locale de snapd (`/run/snapd.socket`) | `snap install/refresh/remove` |
| `appimage` | flux AppImageHub, dernière version sur GitHub | `~/Applications` + lanceur `.desktop` |
| `windows` | catalogues interchangeables (voir plus bas) | runtime Winlux (`winlux setup.exe`) |

Un même logiciel présent dans plusieurs sources n'apparaît qu'une fois (ID
AppStream ou nom) : la source installée, sinon apt, sinon Flatpak, Snap,
AppImage, Windows. Les autres sources restent choisissables dans la fiche.

**Toujours à jour.** `winlux-store refresh` est lancé à l'ouverture du Store :
il écrit d'abord l'index des sources locales, puis rafraîchit en arrière-plan
(`apt-get update` si les listes ont plus de 6 h et que le service est root,
`flatpak update --appstream`, Snap Store, AppImageHub, catalogues Windows),
télécharge les vignettes, et réécrit l'index. Les outils standards et leurs
verrous sont utilisés tels quels : `apt update` / `apt upgrade` en terminal
continuent de fonctionner, et ce que le Store installe se met à jour aussi
par `apt upgrade`, `flatpak update` ou `snap refresh`.

Fichiers échangés avec le shell (`/run/winlux`, UTF-8) :

- `store.txt` : `app=clé|catégorie|nom|résumé|éditeur|état|version|disponible|taille_ko|icône|badge|sélection|autres_clés`
  (état `a` disponible, `i` installé, `u` mise à jour), `updates=`, `system_updates=`, `sources=`, `refreshing=`, `win=nom|exe` ;
- `store-detail.txt` : fiche (`desc=`, `shot=` captures PNG en cache, `release=version|date|notes`, `alt=clé|source`, licence, taille, site) ;
- `store-progress.txt` : `op`, `target`, `state=running|done|error`, `percent`, `message`.

Commandes : `refresh [--force]`, `details <clé>`, `install|remove|update <clé>`,
`update-all`, `install-file <fichier>` (.exe, .msi, .deb, .AppImage, .flatpakref).

### Catalogues Windows interchangeables

Aucune source Windows n'est codée en dur. Chaque fichier JSON de
`/etc/winlux/store-sources.d/` ou `~/.config/winlux/store-sources.d/` déclare
un catalogue :

```json
{"id": "winlux", "name": "Catalogue Winlux", "format": "winlux",
 "url": "https://exemple.org/winlux-windows.json"}
```

(`"file": "/chemin/catalog.json"` pour un fichier local, `"enabled": false`
pour le désactiver.) Le format `winlux` :

```json
{"apps": [{
  "id": "notepadpp", "name": "Notepad++", "version": "8.6",
  "category": "developpement", "publisher": "Don Ho", "license": "GPL-3.0",
  "summary": "Éditeur de code", "description": ["Paragraphe 1", "Paragraphe 2"],
  "icon": "https://…/icon.png", "screenshots": ["https://…/1.png"],
  "homepage": "https://…", "featured": true, "size_kb": 4500,
  "releases": [{"version": "8.6", "date": "2024-01-01", "notes": "…"}],
  "installer": {"url": "https://…/setup.exe", "sha256": "<64 hex>", "type": "exe", "args": ["/S"]}
}]}
```

Une fiche sans empreinte SHA-256 est ignorée, et un installateur dont
l'empreinte diffère est refusé. Un autre format de catalogue se branche en
ajoutant un adaptateur à `WindowsCatalog.FORMATS` (fonction qui renvoie des
fiches au format ci-dessus), sans modifier le reste du Store. Les `.msi`
demandent `C:\Windows\System32\msiexec.exe`, pas encore fourni par le runtime.

## Gestionnaire de polices

L'application **Polices** liste les polices fontconfig (système et
utilisateur), les prévisualise à plusieurs tailles, installe un `.ttf/.otf/.ttc/.otc`
(bouton ou glisser-déposer depuis l'Explorateur) dans `~/.local/share/fonts`
et désinstalle les polices de l'utilisateur (les polices système sont
protégées). Service : `winlux-fonts list|install|remove` (`scripts/winlux-fonts.sh`).

Les `.exe` voient les mêmes polices : `EnumFontFamilies(Ex)A/W` et
`EnumFontsA/W` lisent directement les tables `name`/`OS/2`/`head`/`hhea`/`post`
des fichiers des dossiers fontconfig et de `C:\Windows\Fonts`, et
`AddFontResource(Ex)` / `RemoveFontResource(Ex)` ajoutent des polices privées
au processus (signalées à fontconfig pour que Xft les dessine).
