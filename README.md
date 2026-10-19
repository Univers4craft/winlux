# Winlux

Winlux est un runtime expérimental Linux qui charge des exécutables **Windows PE32+ x86-64** et fournit un sous-ensemble d’API Win32 écrit en Rust. Le projet vise une interopérabilité progressive avec Linux; il ne s’appuie pas sur Wine.

> **État expérimental :** la compatibilité est partielle. Ce projet ne garantit ni l’exécution de tout logiciel Windows, ni l’absence de crash. Les applications GUI complexes et les exécutables .NET ne sont pas encore pris en charge.

## État testé

Les exécutables de test incluent `hello.exe`, `exitcode.exe`, `p1_suite.exe`, des tests de DLL réelle/import différé, `findfiles.exe` et `crt_suite.exe` (chaînes, conversions, mathématiques, tri). Ils sont compilés en PE Windows x64 avec Rust et `rust-lld`, puis exécutés directement sous Winlux.

Le runtime contient notamment un chargeur PE, les relocations et imports, un PEB/TEB minimal, du TLS, des DLL intégrées partielles, le registre Winlux persistant, des fichiers mappés vers Linux (énumération, fichiers temporaires, métadonnées) et une passerelle Winsock/ALSA expérimentale.

Le sous-ensemble de processus implémente `CreateProcessA/W` pour lancer un programme Linux ou un PE enfant via Winlux, avec attente, code de sortie, blocs d’environnement A/W et `CREATE_SUSPENDED`/`ResumeThread`. L’héritage des handles et les attributs étendus ne sont pas encore implémentés. Les fichiers prennent en charge des E/S overlapped élémentaires via `ReadFile`/`WriteFile`, `GetOverlappedResult`, une annulation best-effort avec `CancelIo/CancelIoEx` et un sous-ensemble IOCP.

## Prérequis

- Linux x86-64
- Rust stable et Cargo
- Cible Rust `x86_64-pc-windows-msvc` et `rust-lld` pour compiler les exécutables de test
- `timeout` pour `scripts/test-runtime.sh`
- ALSA (`libasound.so.2`) seulement pour tenter la lecture audio WAVE

Installer la cible de test, si nécessaire :

```sh
rustup target add x86_64-pc-windows-msvc
```

## Compiler et tester

Depuis la racine du dépôt :

```sh
cargo test -p winlux
scripts/build-tests.sh
cargo build --release -p winlux
scripts/test-runtime.sh
```

Lancement manuel d’un PE :

```sh
target/release/winlux chemin/vers/programme.exe [arguments...]
```

`WINLUX_TRACE=1` active les traces du chargeur. Par défaut, l’appel d’un import non implémenté arrête le programme et l’enregistre dans le journal Winlux. `WINLUX_MISSING=ignore` permet de continuer en renvoyant zéro, avec les risques de résultats incorrects.

## Arborescence

- `runtime/` : runtime Linux et chargeur PE.
- `testexe/` : exécutables Windows de test.
- `testdll/` : DLL Windows de test.
- `integration/` : fichiers préparatoires binfmt, MIME, `.desktop` et script d’installation.
- `scripts/` : compilation, tests et première ébauche du pipeline ISO.
- `theme/` : palette CSS expérimentale, non raccordée à un shell graphique.
- `TODO.md` : feuille de route maintenue.
- `COMPATIBILITY.md` : état détaillé de compatibilité et limites connues.

## Intégration Linux et ISO

La configuration binfmt et le fichier `.desktop` sont des préparatifs, pas une intégration hôte validée. `integration/install.sh` modifie des emplacements système et nécessite une revue ainsi que des privilèges d’administration pour être exécuté. Le script ISO requiert un répertoire staging Linux **déjà bootable** et `xorriso`; il ne construit pas encore l’OS de l’ISO.

## Limites importantes

- PE32+ x86-64 seulement; le runtime n’est pas un noyau Windows.
- `user32`/`gdi32` sont très partiels : aucune pile GUI utilisable n’est fournie.
- `mscoree` émet un diagnostic; aucun CLR n’est intégré. `paint.net` ne fonctionne donc pas actuellement.
- Les codecs compressés, DirectX, les pilotes, le registre Windows complet, les services SCM complets, les E/S overlapped et de nombreuses API Win32 ne sont pas implémentés.
- `FreeLibrary` ne démonte pas encore une DLL chargée.
- Le pont audio WAV/ALSA dépend de l’environnement Linux et n’a pas été validé sur plusieurs périphériques.

Voir [COMPATIBILITY.md](COMPATIBILITY.md) pour les détails et [TODO.md](TODO.md) pour les travaux restants.

## Licence

Winlux est distribué sous GNU GPL version 3 ou, à votre choix, toute version ultérieure. Voir [LICENSE](LICENSE).
