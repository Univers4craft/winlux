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
| `windows` | catalogues interchangeables, Scoop Extras par défaut (voir plus bas) | runtime Winlux (`winlux setup.exe`, msiexec intégré, applications portables) |

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

### Logiciels courants (appli Bienvenue)

L'étape « Vos premiers logiciels » de Bienvenue propose dix logiciels courants
(Firefox, LibreOffice, VLC, Thunderbird, GIMP, 7-Zip, KeePassXC, Geany, OBS
Studio, Notepad++), sans aucun produit Microsoft. La table `STARTER` de
`winlux-store` donne pour chacun ses clés candidates par ordre de préférence
(apt, puis Flathub, puis catalogue Windows) ; un logiciel sans clé utilisable
sur la machine n'est pas affiché.

- `starter --list` écrit `starter.txt` : `busy=0|1` puis
  `app=id|état|pourcentage|icône|source` (état `i` installé, `a` disponible,
  `w` en attente, `r` en cours, `f` échec) ;
- `starter <id>...` ajoute les logiciels à la file
  `~/.local/share/winlux/starter-queue.txt` et les installe un par un (en
  attendant la fin d'une installation lancée depuis le Store) ;
- `starter --resume` reprend la file au login suivant si la session a été
  fermée pendant l'installation (lancé par le bureau et la session native).

### Catalogues Windows interchangeables

Aucune source Windows n'est codée dans le Store. Chaque fichier JSON de
`/etc/winlux/store-sources.d/` ou `~/.config/winlux/store-sources.d/` déclare
un catalogue :

```json
{"id": "winlux", "name": "Catalogue Winlux", "format": "winlux",
 "url": "https://exemple.org/winlux-windows.json"}
```

(`"file": "/chemin/catalog.json"` pour un fichier local, `"enabled": false`
pour le désactiver.)

**Catalogue par défaut : Scoop Extras.** L'ISO livre
`/etc/winlux/store-sources.d/scoop-extras.json` (format `scoop`), qui lit
l'archive du dépôt communautaire [ScoopInstaller/Extras](https://github.com/ScoopInstaller/Extras) :
manifestes libres (domaine public), sans compte ni clé, mis à jour en continu,
indépendants de Microsoft. Le Store n'en garde que ce que Winlux sait installer
seul, soit environ 630 applications graphiques :

- architecture 64 bits déclarée (le runtime n'exécute pas encore le 32 bits) ;
- empreinte SHA-256 ou SHA-512 ;
- pas de script PowerShell (`installer`, `pre_install`, `uninstaller`) ni de dépendance ;
- un fichier `.zip` (décompressé dans `C:\Program Files\<nom>`), un `.exe`
  portable ou un `.msi` (msiexec intégré). Les raccourcis du manifeste vont dans
  le menu Démarrer ; un programme qui s'avère 32 bits est refusé proprement.

Garde-fous : l'archive est mise en cache (rafraîchie toutes les 6 h) et le
Store reprend la dernière copie si le dépôt est injoignable ; sans aucune copie
(premier démarrage hors ligne), la source Windows est simplement masquée. Les
fiches converties sont mémorisées tant que l'archive ne change pas.

Le format `winlux` :

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

`type` vaut `exe` (installateur lancé par le runtime), `msi`, `zip` ou
`portable` ; pour `zip`/`portable`, `extract_dir` et `shortcuts`
(`[["app.exe", "Nom"]]`) décrivent l'application. Une fiche sans empreinte
SHA-256 (ou `sha512`) est ignorée, et un installateur dont l'empreinte diffère
est refusé. Un autre format se branche en ajoutant un adaptateur à
`WindowsCatalog.FORMATS` (fonction : octets du catalogue → fiches au format
ci-dessus), sans modifier le reste du Store.

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
