# Boutiques de logiciels

Le **Store Winlux** (`docs/STORE.md`) est l'unique application de boutique :
il réunit les logiciels de plusieurs catalogues dans une seule bibliothèque.
Les dépôts Ubuntu du système (`apt`) y figurent toujours, car c'est la source
du système lui-même. Les catalogues **tiers** sont des « boutiques » que
l'utilisateur choisit, dans le même esprit que les navigateurs
(`docs/BROWSERS.md`) :

- aucune n'est fournie dans l'image, aucune n'est cochée d'office, la liste est
  en ordre alphabétique ;
- le choix se fait dans l'installateur (page Profil), au premier démarrage
  (OOBE), dans **Paramètres > Applications > Boutiques de logiciels**, et
  Bienvenue y mène (« Choisir des boutiques de logiciels ») ;
- ce qu'il faut est téléchargé chez l'éditeur puis vérifié ; sans réseau, la
  boutique reste « en attente » et l'init la reprend au démarrage suivant ;
- une boutique ajoutée a son propre lanceur dans **Toutes les applications**,
  épinglé une fois au menu Démarrer de chaque compte (le Store s'ouvre alors
  sur ses seuls logiciels). Détachée par l'utilisateur, elle le reste.

## Critères

Catalogue **contrôlé** (relecture ou examen avant publication), risque de
logiciel malveillant **très faible**, **libre** de préférence, et réellement
installable par Winlux sans code ni service de Microsoft.

## Boutiques retenues

| Boutique | Pour | Pourquoi elle est sûre | Ce que Winlux télécharge et vérifie |
|---|---|---|---|
| **Flathub** | logiciels Linux | chaque application est examinée à son entrée par l'équipe de Flathub, ses recettes de construction sont publiques, les éditeurs officiels sont marqués « vérifiés », les applications tournent en bac à sable, tout est signé | Flatpak depuis les dépôts Ubuntu, puis `flathub.flatpakrepo` sur `dl.flathub.org` ; la clé de signature qu'il contient doit avoir l'empreinte OpenPGP épinglée dans `integration/store-catalogs.json`, sinon la boutique est refusée. Flatpak vérifie ensuite chaque téléchargement avec cette clé. |
| **Scoop Extras** | logiciels Windows | dépôt communautaire public (domaine public), chaque manifeste est relu avant fusion et pointe vers le site de l'éditeur avec son empreinte | l'archive des manifestes sur GitHub, puis chaque installateur chez son éditeur : refusé si son SHA-256 (ou SHA-512) diffère. Le Store ne garde que le 64 bits, sans script PowerShell. |
| **Scoop Games** | jeux Windows | même organisation et mêmes règles que Scoop Extras, réservé aux jeux libres ou gratuits et aux outils de jeu | idem ; tout est rangé dans la catégorie Jeux du Store |

Sur une installation existante, le catalogue Scoop Extras que l'ancienne image
plaçait dans `/etc/winlux/store-sources.d/` reste actif : rien ne disparaît
avec la mise à jour.

## Boutiques écartées

| Boutique | Raison |
|---|---|
| **Snap Store** | des applications malveillantes y ont été publiées (mineurs de cryptomonnaie en 2018, faux portefeuilles de cryptomonnaie en 2023-2024, qui ont conduit Canonical à examiner à la main les nouvelles inscriptions) ; le serveur n'est pas libre ; `snapd` exige systemd, absent de Winlux. Un snapd installé à la main reste lu par le Store. |
| **AppImageHub** | aucun examen ni signature : chacun y référence son AppImage, et le fichier vient d'une version GitHub sans empreinte publiée. Son catalogue n'est plus proposé ; les AppImage déjà installées restent gérées par le Store (et un compte qui en a installé garde le catalogue). |
| **Chocolatey** (communautaire) | paquets modérés, mais chacun s'installe par un script PowerShell que Winlux n'exécute pas, et beaucoup sont en 32 bits. |
| **winget**, **Microsoft Store** | services de Microsoft : contraires à la règle « aucune association avec Microsoft au-delà de la compatibilité .exe ». Le Microsoft Store demande en plus un compte Microsoft et des paquets MSIX liés à Windows. |
| **Ninite** | service fermé sans catalogue public ; son installateur est un programme 32 bits qui télécharge lui-même ses logiciels. |
| **PortableApps.com** | catalogue sérieux et libre, mais ses paquets `.paf.exe` et sa plateforme sont des programmes 32 bits, que le runtime ne lance pas encore (#232). À réexaminer ensuite. |
| **Scoop Main**, **Scoop Nonportable** | Main ne contient presque que des outils en ligne de commande ; Nonportable des installateurs qui exigent des droits administrateur et PowerShell. |
| **F-Droid** | catalogue exemplaire, mais d'applications Android : Winlux ne les exécute pas (#297). |
| **Steam**, **GOG Galaxy**, **Epic Games Store**, **itch.io** | boutiques de jeux commerciales à compte et client propriétaires, hors critères ; itch.io publie sans examen. Le client Steam reste installable depuis Flathub par qui l'a activé. |
| **GNOME Logiciels**, **KDE Discover**, **Bazaar** | ce ne sont pas des catalogues mais des vitrines, en double du Store Winlux qui affiche déjà Flathub ; GNOME Logiciels et Discover reposent en plus sur PackageKit et tirent de nombreuses dépendances. |

## Fonctionnement

- Liste des boutiques : `integration/store-catalogs.json`, copiée dans
  `/usr/share/winlux/store-catalogs.json` (seulement leur description).
- Outil : `winlux-store-catalogs` (`scripts/winlux-store-catalogs.py`)
  - `--list` : `id|état|plateforme|nom|résumé|site`, état `on`, `pending` ou `off` ;
  - `--set [id…]`, `--add id`, `--remove id` (root, via `winlux-control
    store-catalog-add-<id>` et le Contrôle de compte d'utilisateur) ;
  - `--resume` : lancé par l'init tant que `/etc/winlux/store-catalogs-pending`
    n'est pas vide, une fois le réseau présent ;
  - `--check-key flathub` : télécharge la clé de Flathub et affiche son empreinte
    (la CI la compare à celle épinglée).
  - Codes : 0 fait, 75 reporté faute de réseau, 1 refusé (clé différente,
    Flatpak impossible à installer), 2 usage.
- Fichiers : `/etc/winlux/store-catalogs` (choix), `/etc/winlux/store-catalogs-pending`,
  `/etc/winlux/store-repos/<id>.flatpakrepo` (dépôt vérifié),
  `/usr/local/share/applications/winlux-store-<id>.desktop` (lanceur),
  journal `/var/log/winlux-store-catalogs.log`.
- Flathub est déclaré pour chaque compte (`flatpak --user`) à l'ouverture du
  Store, à partir du dépôt vérifié et sans réseau : les logiciels s'installent
  sans mot de passe et apparaissent dans Toutes les applications. Retirer la
  boutique la masque du Store sans désinstaller ce qui a été installé.
- Fichier de réponse de l'installation automatique : `"store_catalogs": ["flathub"]`.
- Test : `scripts/test-store-catalogs.py` (dans `scripts/test-all.sh` et la CI).
