# Winlux

[![CI](https://github.com/Univers4craft/winlux/actions/workflows/ci.yml/badge.svg)](https://github.com/Univers4craft/winlux/actions/workflows/ci.yml)

**Un environnement Linux au bureau familier, avec un runtime Windows développé en Rust, sans Wine.**

Winlux réunit un chargeur d’exécutables **Windows PE32+ x86-64**, un sous-ensemble d’API Win32 et un prototype de bureau X11. Une ISO Linux expérimentale permet de tester l’ensemble en machine virtuelle.

> **Préversion technique, pas un remplacement de Windows prêt à l’emploi.**
> Tous les `.exe` ne fonctionnent pas. Photoshop, WPF et WinForms ne sont pas garantis compatibles. Privilégiez les tests en VM : l’installation généraliste sur SSD reste à développer.

[Compatibilité](COMPATIBILITY.md) · [Interface](INTERFACE.md) · [Feuille de route](TODO.md) · [Installation SSD : objectifs](ROADMAP-SSD.md) · [Releases](https://github.com/Univers4craft/winlux/releases)

## Ce qui existe aujourd’hui

### Bureau

- Bureau X11 avec Openbox, xfdesktop et panneau Xfce : menu Démarrer inspiré de Windows 11, barre des tâches pleine largeur et accents violets. L'ancien bureau GDI reste dans les tests du runtime.
- Applications Linux et fenêtres PE prises en charge déplaçables, redimensionnables, minimisables et fermables ; les fenêtres actives apparaissent dans la barre des tâches.
- Menu des applications Linux installées et des exécutables Windows enregistrés au premier lancement ou découverts dans leurs raccourcis et le registre. Le catalogue ne garantit pas leur compatibilité d'exécution.
- Clic droit natif sur le bureau et les lanceurs ; éditeur de texte **Mousepad** maintenu par Xfce, terminal xterm/Bash et écran de veille animé **Pipes** de XScreenSaver.
- Explorer : onglets, double volet, recherche, tri, copie/déplacement et corbeille.
- Paramètres Linux/Windows, réseau, audio/batterie, clavier/trackpad, apparence, écran, mises à jour et **À propos**.

Les indicateurs utilisent les informations exposées par Linux. Une VM ne fournit pas nécessairement la batterie ou le trackpad physiques de l’hôte. La définition, l’échelle du bureau et le zoom du visualiseur sont des réglages distincts. La validation visuelle et matérielle reste à élargir.

### Compatibilité Windows

- Chargement PE, imports, relocations, DLL réelles, TLS et structures PEB/TEB minimales.
- API partielles pour fichiers, processus, synchronisation, registre persistant, réseau et graphismes GDI/X11.
- CRT, fondations COM/OLE, flux `IStream` et fabriques COM multi-usages enregistrées dans le processus.
- Audio WAVE, **MP3 et AAC ADTS mono/stéréo** via bibliothèques Linux, avec limites mémoire et tests réels.
- Détection .NET et délégation possible à `dotnet` ou Mono installés sur Linux ; aucun CLR Windows complet intégré.

Les codecs audio ne fournissent pas la lecture vidéo ni une implémentation complète de Media Foundation/DirectShow. Voir [le périmètre audio](docs/AUDIO-CODECS.md).

## Démarrage rapide sur Linux

Prérequis : **Linux x86-64**, Rust stable et Cargo. La compilation des PE de test nécessite la cible Windows et `rust-lld` fourni par Rust.

```sh
git clone https://github.com/Univers4craft/winlux.git
cd winlux
rustup target add x86_64-pc-windows-msvc
cargo build --release -p winlux
target/release/winlux chemin/vers/programme.exe [arguments...]
```

Un dépôt privé exige une authentification. Pour l’affichage, prévoyez un serveur X11 et les bibliothèques Xft/Fontconfig. L’audio utilise ALSA ; MP3/AAC nécessitent `libmpg123.so.0` et `libfaad.so.2`. Les bibliothèques natives ne sont pas des DLL Windows.

`WINLUX_TRACE=1` active les traces du chargeur. Un import non implémenté arrête normalement le programme. `WINLUX_DOTNET` et `WINLUX_MONO` précisent les pilotes .NET Linux ; ils ne rendent pas WPF/WinForms compatibles.

### Intégration au bureau Linux

```sh
sudo integration/install.sh
# Pour retirer cette intégration :
sudo integration/uninstall.sh
```

Ces commandes modifient des emplacements système. Le lanceur `winlux-open` distingue programmes console et graphiques et affiche les erreurs. L’intégration sur différentes sessions KDE/GNOME/XFCE reste à tester.

## ISO et machine virtuelle

Les [releases](https://github.com/Univers4craft/winlux/releases) contiennent les livrables publiés. **Le code de master peut être plus récent que la dernière ISO.** Un commit ne reconstruit pas automatiquement une image existante. La version système de référence est dans [VERSION](VERSION).

Sur une machine Linux équipée des dépendances du constructeur :

```sh
WINLUX_KERNEL=/chemin/vers/vmlinuz scripts/build-live-iso.sh
scripts/test-live-iso-qemu.sh build/winlux-live-test.iso bios
scripts/test-live-iso-qemu.sh build/winlux-live-test.iso uefi
```

Le script nécessite les outils initramfs/GRUB, Xorg, polices, bibliothèques et modules attendus. Le noyau doit correspondre aux modules sélectionnés par `WINLUX_KERNEL_RELEASE` (par défaut celui de l’hôte). Ce n’est pas encore un constructeur universel indépendant de la distribution. Les bibliothèques audio sont explicitement copiées dans l’image.

Pour préparer une VM persistante :

```sh
scripts/create-test-vm.sh --create-only
scripts/create-test-vm.sh
```

Valeurs par défaut : disque QCOW2 extensible de 24 Gio, 4 Gio de RAM, 4 CPU, affichage 1920×1080. `WINLUX_VM_WIDTH` et `WINLUX_VM_HEIGHT` règlent la définition. Après reconstruction, `--refresh` remplace explicitement la racine de test et sauvegarde l’ancien disque ; **il ne migre pas ses données**.

Un disque préparé portant le label `WINLUX_ROOT` permet la persistance. Sans lui, la session Live fonctionne en mémoire et perd ses modifications à l’arrêt. Ces scripts ne constituent pas un installateur SSD généraliste.

Un installateur disque expérimental existe séparément : il efface un disque
entier après contrôles et confirmation explicite. Voir [ses protections et les
tests requis](docs/INSTALLER-SAFETY.md) avant toute utilisation ; ne pas le lancer
sur un SSD contenant des données.

Dans Explorer, **C:** correspond au dossier personnel Linux, **Z:** à la racine Linux. Les images préparées avec le catalogue hors ligne installent Edge au premier démarrage. **Paramètres > Navigateurs** permet d’installer Chrome ou Firefox et de choisir le navigateur par défaut, sans téléchargement supplémentaire. Voir [Navigateurs](docs/BROWSERS.md) pour la préparation des paquets et les limites.

L’image système inclut **Terminal Winlux** (xterm), avec Bash interactif, texte
anticrénelé, UTF-8 et défilement. Il s’ouvre depuis le menu Démarrer, depuis
**Ouvrir dans le terminal** dans Explorer (dans le dossier courant), ou
automatiquement pour les `.exe` console. Les commandes `winlux-terminal` et
`x-terminal-emulator` sont disponibles pour les applications.

## Mises à jour GitHub

L’outil recherche une **release publiée**, pas les derniers commits :

```sh
winlux-update --check
winlux-update --list
winlux-update --apply
winlux-update --rollback
winlux-update --auto enable   # disable pour désactiver
```

| Paquet | Contenu |
| --- | --- |
| `--system` | Bureau, runtime, applications embarquées, scripts et bibliothèques depuis un stage vérifié. Noyau, modules et boot uniquement avec `WINLUX_UPDATE_INCLUDE_BOOT=1`. |
| `--runtime` | Runtime et outil de mise à jour uniquement ; ne fournit pas les nouvelles bibliothèques système. |

**GitHub Actions teste le code ; le workflow de release construit une ISO et impose les tests Live et disque installé en BIOS/UEFI avant publication.** Cette chaîne de release doit encore être exécutée sur GitHub pour être validée sur ses runners. Le paquet de mise à jour publié par ce workflow reste de portée **runtime**. Les tags `v*-preview.*` en sont exclus pour préserver les releases système manuelles ; les autres tags doivent correspondre à `VERSION`.

Après construction et tests de l’ISO, `scripts/build-update-bundle.sh --system` prépare le paquet complet. Il faut ensuite publier sur la même release l’ISO, ses sommes de contrôle, le paquet, le manifeste et les notes référencées. Les notes viennent de `docs/releases/VERSION.md`.

L’outil utilise `/releases/latest` : les brouillons et releases marquées « prerelease » ne sont pas sélectionnés. Une préversion système publiée par ce mécanisme doit rester clairement identifiée comme expérimentale dans son titre et ses notes.

Pour un dépôt privé, l’outil accepte notamment `WINLUX_UPDATE_TOKEN` ou `GH_TOKEN`. **Ne commitez jamais de jeton et ne l’intégrez pas à une ISO ou une release.**

Les mises à jour vérifient inventaires et SHA-256 et conservent un retour arrière. Les arrêts normaux sont bloqués pendant l’installation ; un redémarrage requis reste signalé jusqu’au démarrage suivant. L’OS invité ne peut pas empêcher l’arrêt forcé de l’hyperviseur. Ce mécanisme n’est pas encore une architecture A/B résistante à toute coupure.

### Identifier la version installée

Les nouvelles constructions incluent `/usr/share/winlux/BUILD-INFO`, visible dans **Paramètres → À propos** : version, commit, date UTC, état des sources suivies et périmètre système/runtime.

La date décrit la construction du livrable, pas nécessairement la compilation d’un binaire réutilisé. `SOURCE_DATE_EPOCH` permet de la fixer. Sans dépôt Git, le commit est `unknown` ; les anciennes images peuvent ne pas fournir ce fichier.

## Compiler et tester

```sh
cargo test -p winlux
scripts/build-tests.sh
cargo build --release -p winlux
scripts/test-runtime.sh
scripts/test-runtime-parallel.sh
scripts/test-integration.sh
scripts/test-network.sh
scripts/test-control.sh
scripts/test-display.sh
scripts/test-updater.sh
scripts/test-update-notes.sh
```

Les dix PE de test sont exécutés séquentiellement et en parallèle. Les tests C tiers nécessitent MinGW-w64 ; les tests parallèles utilisent `timeout` :

```sh
scripts/build-mingw-tests.sh
scripts/test-mingw.sh
```

Avec les bibliothèques audio installées, les tests de décodage réels et PlaySound utilisent une sortie ALSA silencieuse :

```sh
ALSA_CONFIG_PATH="$PWD/runtime/tests/audio/alsa-null.conf" cargo test -p winlux winmm::codecs -- --include-ignored
```

**Une CI verte ne certifie pas tous les logiciels Windows ni le matériel réel.** Pour signaler un problème, indiquez version/commit, logiciel concerné, étapes de reproduction et journal des imports manquants.

## Limites et prochaines étapes

- Exécution native ciblée sur PE32+ x86-64 ; pas de prise en charge générale des programmes 32 bits.
- Exceptions Windows/C++, déchargement sûr des DLL et plusieurs API système à compléter.
- WPF, WinForms, Photoshop, DirectX et logiciels Windows complexes non garantis compatibles.
- COM, services, héritage des handles et sémantiques avancées des E/S encore partiels.
- Lecture vidéo et validation audio sur des périphériques physiques à poursuivre.
- Installateur disque encore expérimental, chaîne ISO à valider sur les runners GitHub et validation matérielle élargie à poursuivre.

Les parties écartées après revue ne sont pas présentées comme fonctionnelles : voir [la revue runtime](REVIEW-INTEGRATION.md) et [la revue graphique](REVIEW-GRAPHICS.md).

## Organisation du dépôt

| Dossier | Rôle |
| --- | --- |
| `runtime/` | Chargeur PE et API Win32 en Rust. |
| `testexe/`, `testdll/`, `testc/` | Exécutables et DLL de test Windows, suites C MinGW. |
| `integration/` | Lanceurs, associations MIME et intégration Linux. |
| `scripts/` | Compilation, tests, ISO, VM et mises à jour. |
| `docs/` | Documentation technique et notes de version. |

## Licence

[GNU GPL version 3 ou ultérieure](LICENSE). Les bibliothèques et logiciels Linux embarqués conservent leurs licences et obligations de distribution respectives.
