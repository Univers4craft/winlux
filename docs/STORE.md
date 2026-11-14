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
| `flatpak` | AppStream XML des remotes configurées (Flathub quand l'utilisateur l'a choisi) | `flatpak install --user`, `update`, `uninstall` |
| `snap` | API locale de snapd (`/run/snapd.socket`) | `snap install/refresh/remove` |
| `appimage` | flux AppImageHub, dernière version sur GitHub ; lu seulement pour un compte qui a déjà installé une AppImage (catalogue ni examiné ni signé, `docs/BOUTIQUES.md`) | `~/Applications` + lanceur `.desktop` |
| `editeur` | `integration/store-editeurs.json` : index du dépôt de l'éditeur, en HTTPS (Claude, Spotify, Signal…) | confirmation, autorisation avant le téléchargement, SHA-256 vérifié deux fois, `apt-get install -- <copie>` ; purge + dépôt retiré |
| `windows` | catalogues interchangeables, Scoop Extras par défaut (voir plus bas) | runtime Winlux (`winlux setup.exe`, msiexec intégré, applications portables) |

Un même logiciel présent dans plusieurs sources n'apparaît qu'une fois (ID
AppStream ou nom) : la source installée, sinon apt, sinon Flatpak, Snap,
AppImage, Windows. Les autres sources restent choisissables dans la fiche.

**Paquets compagnons (#2075).** apt installe sans les paquets recommandés.
Quand une application ne marche pas sans l'un d'eux, une ligne
`compagnons=paquet:a,b` du catalogue les installe avec elle, marqués
automatiques (`--mark-auto`) : `purge --autoremove` les retire avec
l'application si plus rien ne les utilise, et un paquet déjà installé à la
main le reste. virt-manager et GNOME Boxes reçoivent ainsi QEMU, le pilote
libvirt, `passt`, OVMF et swtpm. Jamais `libvirt-daemon-system`, qui tire la
variante systemd : libvirt tourne en mode compte (`qemu:///session`), sans
root, sans polkit et sans systemd.

**Présentation (#716).** Une barre d'icônes à gauche (Accueil, Applis,
Jeux, Windows ; Bibliothèque et Actualiser en bas), la recherche en haut. L'accueil
montre un grand bandeau tournant (sélection Winlux, vignettes des autres
sélections à droite), puis des rangées de cartes : applications essentielles,
jeux, programmes Windows, et une rangée par catégorie qui a des logiciels.
« Applis » affiche toutes les applications avec des puces de catégorie et
leur nombre ; « Windows » montre les programmes des catalogues Windows
ajoutés, ou explique comment en ajouter un. Le bouton d'une carte installe
ou met à jour directement ; un clic sur la carte ouvre la fiche.

**Catégories.** Onze clés, communes à `winlux-store` (`CATEGORY_RULES`) et à
`store.rs` (`CATS`) : `bureautique`, `internet`, `multimedia` (audio et vidéo),
`graphisme` (photo et graphisme), `developpement`, `jeux`, `education`,
`sciences`, `utilitaires`, `systeme`, `accessibilite`. Les catégories
AppStream/Flathub sont rangées par la première règle qui correspond (un jeu
éducatif reste un jeu) ; l'ancienne clé `navigateurs` vaut `internet`.

**Sélection Winlux.** `integration/store-catalog.txt` (environ 300 logiciels) :
`id|catégorie|nom|paquet apt|éditeur|fr|en|es|de`, une description par
langue de l'interface (le français à défaut) ; `featured=` liste les
vedettes du bandeau, jamais un navigateur (`docs/BROWSERS.md`). Un paquet
absent des dépôts configurés est masqué. L'index peut compter jusqu'à
6 000 logiciels (`MAX_INDEX`, `MAX_APPS` dans `store.rs`) : le catalogue
Ubuntu seul en donne déjà près de 2 000.

**Toujours à jour.** `winlux-store refresh` est lancé à l'ouverture du Store :
il écrit d'abord l'index des sources locales, puis rafraîchit en arrière-plan
(`apt-get update` si les listes ont plus de 6 h et que le service est root,
`flatpak update --appstream`, Snap Store, AppImageHub, catalogues Windows),
télécharge les vignettes, et réécrit l'index. Les outils standards et leurs
verrous sont utilisés tels quels : `apt update` / `apt upgrade` en terminal
continuent de fonctionner, et ce que le Store installe se met à jour aussi
par `apt upgrade`, `flatpak update` ou `snap refresh`.

Pendant les déclencheurs de dpkg (shared-mime-info…), le bandeau dit « Mise à
jour du système après l'installation » sans pourcentage ; un clic sur un autre
« Installer » y affiche « Une installation est déjà en cours : attendez qu'elle
se termine » (#2186).

Sur un système neuf, l'image n'a pas de listes apt : la Boutique lance
`apt-get update` dès l'ouverture, sans autorisation (le relais root l'admet au
niveau « user »), jamais hors ligne (#2184).

**Lanceurs.** Le Store est dans Toutes les applications et épinglé au menu
Démarrer (« Store Winlux », trouvé aussi en tapant « boutique »). Chaque
boutique choisie a en plus son lanceur, qui ouvre le Store sur ses seuls
logiciels (`winlux-shell-app store flatpak: Flathub` → `WINLUX_STORE_SOURCE`).

Fichiers échangés avec le shell (UTF-8), dans le dossier propre au compte
`WINLUX_STORE_RUN` (`/run/winlux/store-<uid>`, posé par `winlux-shell-app`),
sinon `/run/winlux` ; partagé, l'index d'un premier compte bloquait celui d'un second :

- `store.txt` : `app=clé|catégorie|nom|résumé|éditeur|état|version|disponible|taille_ko|icône|badge|sélection|autres_clés`
  (état `a` disponible, `i` installé, `u` mise à jour), `updates=`, `system_updates=`, `sources=`, `refreshing=`, `win=nom|exe` ;
- `store-detail.txt` : fiche (`desc=`, `shot=` captures PNG en cache, `release=version|date|notes`, `alt=clé|source`, licence, taille, site) ;
- `store-progress.txt` : `op`, `target`, `state=running|done|error`, `percent`, `message`.

Commandes : `refresh [--force]`, `details <clé>`, `install|remove|update <clé>`,
`update-all`, `install-file <fichier>` (.exe, .msi, .deb, .AppImage, .flatpakref).

### Logiciels publiés pour Linux par leur éditeur (#1056)

Source `editeur` : les logiciels courants dont l'éditeur publie lui-même un
paquet .deb pour Linux, listés dans `integration/store-editeurs.json`
(`/usr/share/winlux/store-editeurs.json`), par ordre alphabétique : 1Password,
AnyDesk, ChatGPT, Claude, Discord, Element, ONLYOFFICE, Signal, Spotify,
Sublime Text, VSCodium. Rien n'est fourni dans l'ISO.

- `kind: index` : le Store lit l'index `Packages` du dépôt de l'éditeur
  (`base` + `index`) et prend la version la plus récente du paquet, avec son
  empreinte SHA-256. L'adresse du fichier doit rester sur un des `hosts` de
  l'éditeur. Pour un dépôt « plat » (Sublime Text), l'adresse reste relative
  à la racine `base`.
- `kind: deb` : fichier unique épinglé dans le catalogue (`url`, `version`,
  `sha256`). Sans version ni empreinte, l'entrée reste masquée.

**Installer.** Une fenêtre (`/usr/lib/winlux/winlux-store-confirm`) montre le
produit, la version, l'éditeur, l'adresse exacte, la taille, la licence et
l'empreinte, dans la langue de l'interface, avec **Annuler** par défaut ;
Annuler, Échap, fermer la fenêtre ou l'absence d'affichage valent refus, et
rien n'est téléchargé. Après « Installer », le Contrôle de compte
d'utilisateur vient tout de suite, avant le téléchargement
(`winlux-store-repos grant <id> <sha256>`, #1451) : le relais root en garde un
accord à usage unique, en mémoire seulement, lié au compte, à ce logiciel et à
cette empreinte, valable une heure (#1453). Puis téléchargement dans un
dossier neuf et vide du cache (avancement réel en Mo), empreinte comparée à
celle montrée, nom et version du paquet contrôlés (`dpkg-deb -f`), et
`winlux-store-repos install <id> <sha256> <fichier>` consomme l'accord : il
recopie le paquet dans `/var/cache/winlux-store`, revérifie empreinte et nom,
et lance `apt-get install -- <copie>` seulement. Un téléchargement annulé ou
en échec retire l'accord (`revoke`). Les dossiers sont effacés ensuite.
Une mise à jour (`u`) repasse par la même confirmation.

**Désinstaller.** `apt-get purge --autoremove`, puis `winlux-store-repos clean
<id>` (administrateur, relais root) : le paquet d'un éditeur peut déclarer son
dépôt pour les mises à jour ; tout fichier de `/etc/apt/sources.list.d` dont
toutes les sources pointent vers ses `hosts` est retiré, avec sa clé
`signed-by` quand plus aucune source ne s'en sert. Refus tant que le paquet
est installé. Les documents et réglages de l'utilisateur restent dans son
dossier personnel.

**Sonde.** `scripts/check-store-editeurs.py` (étape non bloquante de la CI)
donne pour chaque éditeur la version et l'empreinte du jour, et pour une
entrée `deb` l'adresse finale, l'empreinte à épingler et les lignes de dépôt
déclarées par les scripts du paquet (lues comme du texte).

Écartés : WinRAR (sous Linux, seulement RAR en ligne de commande, payant ;
7-Zip, Gestionnaire d'archives et Ark sont dans la sélection), Steam (le
lanceur de Valve exige l'architecture i386 en plus), et les logiciels sans
dépôt ni empreinte publiés (Zoom, Slack, Telegram, Obsidian, Bitwarden),
proposés par Flathub quand on l'a choisi.

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

**Testés sur Winlux (#1515).** Seul catalogue Windows livré :
`integration/store-windows-valides.json` (format `winlux`, installé dans
`/usr/share/winlux/store-windows-valides.json`), les programmes .exe
installés et utilisés sur le banc (liste de `site/content/fr/software.md`),
chacun avec adresse officielle, version et SHA-256 relevés par la sonde de la
CI (`scripts/check-store-editeurs.py`). Section dédiée en tête de la page
Windows de la boutique et sur l'accueil. Comme pour tout programme .exe, une
fenêtre de confirmation (produit, version, éditeur, adresse, empreinte,
Annuler par défaut) précède le téléchargement (#1517) ; la désinstallation
garde les documents.

**Boutiques au choix : Scoop Extras et Scoop Games.** Aucun autre catalogue Windows
n'est actif d'office : l'utilisateur choisit ses boutiques (installateur,
premier démarrage, Paramètres > Applications ; voir `docs/BOUTIQUES.md`, qui
liste aussi les boutiques écartées et pourquoi). Une installation antérieure
garde son `/etc/winlux/store-sources.d/scoop-extras.json`. Scoop Extras (format
`scoop`) lit l'archive du dépôt communautaire [ScoopInstaller/Extras](https://github.com/ScoopInstaller/Extras) :
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

**Contrat de licence (#2129).** Une installation sans interface (`args` avec
`/S`, `/SILENT`, `/VERYSILENT`, `/quiet`, `/passive` ou `/q…`, et tout `.msi`,
installé par `msiexec /i … /qn`) ne montre pas le contrat de l'installateur ;
comme sous Windows, l'option silencieuse vaudrait acceptation. Le Store ouvre
donc d'abord la page « Contrat de licence » de l'assistant
(`winlux-setup-wizard FICHIER --contrat-seulement`, contrat lu par
`winlux --setup --license`, « Je n'accepte pas » coché au départ). Paquet sans
contrat ou contrat accepté : l'installation suit. Refus, page fermée ou
impossible à ouvrir : rien n'est lancé, et le Store affiche « Contrat de
licence non accepté : rien n'a été installé. ». Un `.exe` ouvert par
« Installer un fichier » garde son interface : le runtime présente lui-même le
contrat, et un refus (1602, vu 66 sur 8 bits) est dit de la même façon.

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
