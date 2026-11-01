# Tests et CI

Winlux est testé à trois niveaux : **tests unitaires Rust**, **vrais programmes Windows** exécutés par le runtime, et **machines virtuelles** qui démarrent l'ISO. Une CI verte veut dire que rien de ce qui marchait n'a régressé ; elle ne prouve pas que tous les logiciels Windows fonctionnent.

## Les workflows GitHub Actions

### `ci.yml` : à chaque push et pull request

| Job | Contenu |
|---|---|
| **Hygiène** | `cargo fmt --check -p winlux` ; `cargo clippy -p winlux -- -D warnings` (aucun avertissement toléré, avec la dernière version stable de Rust) ; **refus de toute dépendance à Wine** ; scripts en fins de ligne LF et exécutables ; chaque `.exe` de test présent dans `testexe/src/bin` doit être listé dans `scripts/test-runtime.sh`. |
| **Tests unitaires** | `cargo build --release` et `cargo test -p winlux` ; navigateurs hors ligne ; catalogue natif Linux et Windows ; installateur (benchmark, profils, migration, fichiers de réponses, partitionnement manuel, échelle d'écran) ; signalement de problèmes ; écran de verrouillage ; autorisations ; UAC ; « Coller » sur le bureau ; grandes images X11 en 1080p ; codecs MP3/AAC réels ; moteur multimédia avec un vrai FFmpeg ; `cargo check --all-targets` ; mise à jour et retour arrière ; réseau et commandes des Paramètres ; sauvegarde, sessions et bureau à distance ; **partage Samba face à des clients Windows 11 et macOS simulés**. |
| **Tests runtime (PE)** | Compile les `.exe` de `testexe/` et les exécute un par un puis en parallèle ; intégration du double-clic ; installe MinGW-w64, clang et lld, compile et exécute les suites C/C++ (MinGW et MSVC, dont les exceptions C++) ; rapport des PE générés. |
| **Captures d'écran** | Sous Xvfb : contraste du menu Démarrer en thèmes clair et sombre (`test-shell-contrast.py`), puis `capture-ui.py` photographie toutes les pages des Paramètres, le menu Démarrer, les fenêtres et les applications maison ; `compare-captures.py` les compare à la dernière version verte de `master` (artefacts `captures-ui` et `captures-ui-diff`, alerte au-delà de 2 % de pixels changés). |

### `apt-base.yml` : sur les pull requests qui touchent la base

Reconstruit la base Ubuntu exactement comme la publication (sans rien publier), dès que `prepare-apt-base.py`, `prepare-desktop-packages.py` ou les workflows changent. Une base cassée est ainsi vue avant de taguer.

### `release.yml` : sur un tag `vX.Y.Z`

Construit l'ISO et la valide en VM avant de publier : navigateurs, bureau natif, ISO en BIOS et UEFI, installation sur disque, mise à jour depuis la version précédente. Détails dans [Publication et mises à jour](Publication-et-mises-a-jour). Il tourne aussi en mode validation sur les pull requests qui le modifient.

## Lancer les tests en local

### Le minimum avant une pull request

```sh
cargo fmt --check -p winlux
cargo clippy -p winlux -- -D warnings     # faites d'abord `rustup update` : la CI utilise le dernier Rust stable
cargo test -p winlux
bash scripts/build-tests.sh && bash scripts/test-runtime.sh
```

### Selon ce que vous avez modifié

| Domaine | Commande |
|---|---|
| Runtime, PE | `bash scripts/test-runtime.sh`, `bash scripts/test-runtime-parallel.sh` |
| C/C++ MinGW | `sh scripts/build-mingw-tests.sh && sh scripts/test-mingw.sh` |
| Double-clic, intégration | `sh scripts/test-integration.sh`, `sh scripts/test-binfmt-desktops.sh` |
| Réseau, Paramètres | `sh scripts/test-network.sh`, `sh scripts/test-control.sh`, `xvfb-run -a python3 -B scripts/test-settings-native.py` |
| Affichage | `sh scripts/test-display.sh` |
| Mises à jour | `bash scripts/test-updater.sh`, `sh scripts/test-update-notes.sh`, `python3 -B scripts/test-update-guard.py` |
| Installateur | `python3 -B scripts/test-installer-oobe.py`, `test-installer-partitioning.py`, `test-autoinstall.py` |
| Verrouillage, UAC, autorisations | `python3 -B scripts/test-lock.py`, `test-uac.py`, `test-permission.py` |
| Partage, transfert | `sh scripts/test-share.sh`, `sh scripts/test-share-clients.sh`, `sh scripts/test-transfer.sh` |
| Sessions, sauvegarde, bureau à distance | `sh scripts/test-sessions.sh`, `sh scripts/test-backup.sh`, `sh scripts/test-remote.sh` |
| Store | `python3 -B scripts/test-store.py` |
| Menu Démarrer, fenêtres | `python3 -B scripts/test-start-icon.py`, `test-shell-popups.py`, `test-window-performance.py` |
| Codecs audio réels | `ALSA_CONFIG_PATH="$PWD/runtime/tests/audio/alsa-null.conf" cargo test -p winlux winmm::codecs -- --include-ignored` |
| ISO et VM | voir [Construire l'ISO](Construire-l-ISO) |

## Les programmes de test

- **`testexe/src/bin/`** : programmes Windows en Rust `no_std`, compilés pour `x86_64-pc-windows-msvc` avec `rust-lld` : `hello`, `exitcode`, `p1_suite`, `dll_import`, `delay_import`, `findfiles`, `crt_suite`, `gui_suite`, `audio_suite`, `d3d_suite`, `print_suite`, `power_api`, `uac_suite`, `winpaths`, `explorer_regression`, ainsi que les applications maison (`calc`, `taskmgr`, `mediaplayer`…). Chacun doit apparaître dans `scripts/test-runtime.sh`.
- **`testdll/`** : DLL pour tester `LoadLibrary`, `GetProcAddress`, TLS et `DllMain`.
- **`testc/`** : suites C (`mingw_suite.c`), C++ (`cpp_exceptions.cpp`) et MSVC (`msvc/`, exceptions C++ et `__try/__except`).

## Bonnes pratiques

- Ajouter un test qui échoue avant le correctif et passe après.
- Ne jamais désactiver ni ignorer un test pour obtenir une CI verte.
- Un nouveau `.exe` de test doit être listé dans `scripts/test-runtime.sh`, sinon le job Hygiène échoue.
- Les scripts doivent être en LF et exécutables (`chmod +x`).

Voir aussi : [Comment contribuer](Comment-contribuer)
