# Navigateurs dans Winlux : aucun n'est fourni, l'utilisateur choisit

**Décision du 2 novembre 2026 (damien).** L'image de Winlux ne contient plus
aucun navigateur, ni Microsoft Edge, ni Google Chrome, ni Mozilla Firefox.
L'utilisateur choisit le sien dans **Paramètres > Navigateurs**. Winlux le
télécharge sur le site officiel de son éditeur, vérifie le fichier et
l'installe pour lui, sans qu'il ait rien à faire à la main.

## Navigateurs proposés (#1716)

Le 11 novembre 2026, damien a demandé d'ajouter les navigateurs de sa liste
qui ont une version Linux officielle, et d'essayer en `.exe`, sans jamais le
proposer par défaut, un navigateur qui n'en aurait pas. Relevé fait ce
jour-là : **les cinq ont une version Linux officielle**, aucun ne passe donc
par le `.exe`.

| Navigateur | Éditeur | Source officielle | Format |
|---|---|---|---|
| Brave | Brave Software, Inc. | brave-browser-apt-release.s3.brave.com (`stable main`, paquet `brave-browser`) | .deb, dépôt APT |
| Google Chrome | Google LLC | dl.google.com/linux/chrome/deb | .deb, dépôt APT |
| Microsoft Edge | Microsoft Corporation | packages.microsoft.com/repos/edge | .deb, dépôt APT |
| Mozilla Firefox | Mozilla Corporation | packages.mozilla.org/apt | .deb, dépôt APT |
| Opera | Opera Norway AS | deb.opera.com/opera-stable (`stable non-free`, paquet `opera-stable`) | .deb, dépôt APT |
| Opera GX | Opera Norway AS | deb.opera.com/opera-gx-stable (`stable non-free`) ; version Linux sortie le 19 mars 2026 | .deb, dépôt APT |
| Tor Browser | The Tor Project, Inc. | dist.torproject.org (`tor-browser-linux-x86_64-<version>.tar.xz`) | archive signée |
| Vivaldi | Vivaldi Technologies AS | repo.vivaldi.com/archive/deb (`stable main`, paquet `vivaldi-stable`) | .deb, dépôt APT |

Le conteneur de développement n'atteint pas ces sites : l'étape de CI
« Navigateurs proposés présents chez leurs éditeurs »
(`scripts/check-navigateurs-editeurs.sh`) lit les vrais index et affiche, pour
chacun, la version, l'adresse et le SHA-256 du paquet retenu. Si un éditeur
renomme son paquet ou son programme, `winlux-browsers` cherche le programme
par le lien que le paquet pose dans `/usr/bin` et note le chemin trouvé dans
`/opt/winlux/browsers/<id>/program`.

**Tor Browser** n'a ni `.deb` ni dépôt APT. Son archive n'est acceptée que si :
1. la liste `sha256sums-signed-build.txt` de la version est signée par la clé
   des développeurs de Tor Browser, d'empreinte
   `EF6E 286D DA85 EA2A 4BA7 DE68 4E2C 6E87 9329 8290` (épinglée dans
   `prepare-browser-packages.py` et dans `winlux-browsers.sh`) ;
2. son SHA-256 est celui de cette liste ;
3. sa propre signature `.asc` est valide avec la même clé, vérifiée au
   téléchargement puis à nouveau juste avant l'extraction ;
4. elle ne contient que le dossier `tor-browser` (aucun chemin absolu ni `..`).

La clé est lue chez le projet Tor (WKD de torproject.org, puis
keys.openpgp.org, le serveur que cite sa documentation) et refusée si son
empreinte principale n'est pas celle épinglée. La vérification utilise
`gpgv`, copié dans l'image. Tor Browser écrit son profil dans son propre
dossier : il tourne depuis une copie personnelle
(`~/.local/share/winlux/tor-browser`, compte `winlux-web`), refaite à chaque
nouvelle version en gardant le profil.

Hors de cette demande : Internet Download Manager, JDownloader 2 et Free
Download Manager (gestionnaires de téléchargement), FileZilla et WinSCP
(transfert de fichiers) ne sont pas des navigateurs.

## Pourquoi ce choix

1. **La licence d'Edge interdit de le redistribuer.** Le texte « MICROSOFT
   SOFTWARE LICENSE TERMS – MICROSOFT EDGE », section 2.8 (plateformes autres
   que Windows), dit : « you will not (and have no right to): … (f) share,
   publish, distribute, or lend the Software, provide the Software as a
   stand-alone hosted solution for others to use, or transfer the Software or
   this agreement to any third party. » Les conditions de Google (« You may not
   copy, modify, distribute, sell, or lease any part of our services or
   software ») posent la même limite pour Chrome.
2. **Aucun lien avec Microsoft au-delà de la compatibilité des `.exe`.** Une ISO
   qui contient un exécutable et des logos Microsoft contredit la position
   publique du projet (docs/DEFENSE-DU-NOM.md, docs/RISQUES-JURIDIQUES.md).
3. **Le libre choix du navigateur.** Dans l'esprit des règles européennes sur
   le choix du navigateur (règlement sur les marchés numériques, qui l'impose
   aux grands systèmes d'exploitation), Winlux n'en fournit ni n'en favorise
   aucun. Firefox, pourtant redistribuable (binaire officiel, licence MPL), est
   donc retiré lui aussi, pour que les trois soient traités de la même façon.

## Ce qui a été retiré

Depuis #155, `release.yml` préparait les `.deb` officiels d'Edge, de Chrome et
de Firefox (`scripts/prepare-browser-packages.py`). Ils étaient copiés dans
`/usr/share/winlux/browsers` de l'ISO, et l'init installait Edge au premier
démarrage comme navigateur par défaut. Les fichiers ISO des versions publiées
qui les contiennent (v0.2.14 et antérieures) sont retirés des versions GitHub.
Les paquets de mise à jour `.deb` de ces versions restent : ils ne contiennent
aucun navigateur. Le détail (versions, date) est noté dans les notes de
v0.2.15. Un PC déjà installé garde le navigateur que son utilisateur avait :
c'est sa copie, faite sur sa machine.

## Comment ça marche maintenant

- **Écran de choix** : Paramètres > Navigateurs liste Brave, Google Chrome,
  Microsoft Edge, Mozilla Firefox, Opera, Opera GX, Tor Browser et Vivaldi
  dans l'ordre alphabétique, sans mise en avant, avec
  pour chacun « Installer », « Par défaut » (qui l'installe au besoin) et
  « Ouvrir ». Aucun navigateur par défaut tant que l'utilisateur n'en a pas
  choisi un.
- **Liens web** : xdg-open (GIO et exo-open d'XFCE), la variable `BROWSER` et
  ShellExecute des `.exe` aboutissent tous à `winlux-browser`. Il ouvre le lien
  dans le navigateur par défaut s'il est installé, sinon dans un navigateur
  installé (catalogue ou système). S'il y en a plusieurs et qu'aucun n'est
  par défaut, une fenêtre (`winlux-browser-choice`) les propose dans l'ordre
  alphabétique, sans présélection, rappelle qu'un navigateur par défaut évite
  de choisir à chaque fois, et « Toujours utiliser ce navigateur » en fait le
  navigateur par défaut. L'écran de choix ne s'ouvre que s'il n'y a aucun navigateur : le
  lien est alors gardé dans `/run/winlux/browser-pending-<uid>` et la session
  l'ouvre dès qu'un navigateur est installé (pendant une heure au plus) :
  Paramètres l'ouvre à la fin de l'installation, et la session le reprend
  sinon. L'en-tête « Navigateur par défaut » indique le navigateur qui ouvre
  vraiment les liens (le seul installé, ou « demandé à chaque lien »).
- **Langue** : Firefox est installé avec les paquets de langue officiels de
  Mozilla (`firefox-l10n-fr`, `-de`, `-es`, même dépôt, SHA-256 de l'index)
  et suit la langue choisie dans Paramètres > Langue : `winlux-browsers`
  donne `LANG`/`LANGUAGE` d'après `/etc/winlux/locale.conf` à tout
  navigateur lancé (Chrome et Edge contiennent déjà leurs langues). Les
  paquets sont posés dans `distribution/extensions` (profil neuf) et
  `browser/extensions` (relu à chaque démarrage, pour un profil déjà créé).
  `sh scripts/check-firefox-langue.sh`, sur le Winlux installé, vérifie la
  langue réellement affichée (en-tête Accept-Language) sur un profil neuf
  puis relancé.
- **Téléchargement automatique** : `winlux-browsers --install <id>` (brave, chrome, edge, firefox, opera, operagx, tor, vivaldi)
  lance `/usr/lib/winlux/winlux-browser-fetch` (copie de
  `scripts/prepare-browser-packages.py`). Il lit l'index du dépôt officiel de
  l'éditeur en HTTPS (packages.microsoft.com, dl.google.com,
  packages.mozilla.org) et télécharge le `.deb` dans `/var/cache/winlux/browsers`.
  Il vérifie ensuite le SHA-256 donné par l'index, le nom du paquet et
  l'architecture. Sans réseau, l'installation s'arrête avec « Téléchargement
  impossible » et ne laisse ni fichier partiel ni verrou.
- **Installation isolée** : l'archive est extraite dans
  `/opt/winlux/browsers/<nom>`, sans exécuter les scripts du paquet. Le
  navigateur tourne sous le compte dédié `winlux-web`, avec sa sandbox.
- **Contrôle de construction** : `scripts/check-image-redistributable.sh` fait
  échouer la construction si l'image contient un logiciel d'éditeur tiers
  propriétaire : Edge, Chrome, VS Code, Teams, Zoom, Discord, Spotify, Brave,
  Vivaldi, Opera… Il cherche leurs dossiers d'installation, leurs paquets dpkg
  ou des `.deb` en cache. `stage-browser-packages.sh` refuse en plus tout
  paquet de navigateur (Firefox compris), et `build-live-iso.sh` refuse
  `WINLUX_EDGE_DEB`.

## Vérifications

- `sh scripts/test-browsers.sh` : aucun navigateur par défaut, écran de choix,
  téléchargement à la demande (faux éditeur), échec du téléchargement sans
  installation partielle, refus des archives altérées.
- `sh scripts/test-browser-links.sh` : lien ouvert dans le navigateur
  installé même non choisi par défaut, écran de choix seulement sans
  navigateur, lien gardé puis ouvert après le choix.
- VM de validation (`test-browsers-vm.sh`, `test-native-session-vm.sh`) : aucun
  `.deb` de navigateur dans l'image, les trois proposés au téléchargement, et
  un lien ouvert sans navigateur mène à l'écran de choix.
- `python3 scripts/test-image-identity.py` : branchement des contrôles dans la
  construction et la publication.
