# Compilation et installation de Winlux

## Prérequis

| Outil | Version minimale | Usage |
|-------|-----------------|-------|
| Linux x86-64 | Noyau 5.4+ | Hôte d'exécution |
| Rust stable | 1.75+ | Compilation du runtime |
| Cargo | (inclus avec Rust) | Gestionnaire de paquets |
| `x86_64-pc-windows-msvc` | cible Rust | Exécutables PE de test |
| `rust-lld` | (inclus avec Rust) | Éditeur de liens PE |
| `gcc-mingw-w64-x86-64` | — | Tests C MinGW (optionnel) |
| `timeout` (coreutils) | — | Scripts de test |
| `libasound2-dev` | — | Lecture audio WAVE (optionnel) |

### Installer Rust

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add x86_64-pc-windows-msvc
```

## Compilation depuis zéro

```sh
git clone https://github.com/Univers4craft/winlux.git
cd winlux

# Compiler le runtime
cargo build --release -p winlux

# Compiler les exécutables PE de test
bash scripts/build-tests.sh
```

Le binaire se trouve dans `target/release/winlux`.

## Lancer les tests

```sh
# Tests unitaires (81 tests)
cargo test -p winlux

# Tests runtime séquentiels
bash scripts/test-runtime.sh

# Tests runtime en parallèle (les 10 PE simultanés)
bash scripts/test-runtime-parallel.sh

# Tests réseau et commandes système
sh scripts/test-network.sh
sh scripts/test-control.sh

# Tests du système de mise à jour
bash scripts/test-updater.sh

# Tests MinGW (nécessite gcc-mingw-w64-x86-64)
sh scripts/build-mingw-tests.sh
sh scripts/test-mingw.sh
```

## Installation sur une machine réelle

```sh
# Compiler en release
cargo build --release -p winlux

# Installer (nécessite les droits d'administration)
sudo integration/install.sh

# Cela installe :
#   /usr/bin/winlux          — le runtime
#   /usr/bin/winlux-open     — lanceur de double-clic
#   /usr/bin/winlux-update   — outil de mise à jour
#   /usr/bin/winlux-control  — commandes système
#   Enregistrement binfmt pour les .exe
#   Association MIME des fichiers .exe
#   Fichier .desktop pour l'ouverture graphique

# Désinstaller
sudo integration/uninstall.sh
```

Après installation, un double-clic sur un `.exe` l'ouvre via Winlux.
Les programmes console s'ouvrent dans un terminal ; les programmes
graphiques sont lancés directement.

## Construction de l'ISO

L'ISO de test contient un système Linux minimal avec Winlux préinstallé.

### Prérequis supplémentaires pour l'ISO

```sh
sudo apt-get install -y \
  xorriso grub-pc-bin grub-efi-amd64-bin \
  linux-image-generic xserver-xorg-core \
  x11-xserver-utils qemu-system-x86
```

### Construire l'ISO

```sh
# Le noyau doit être lisible par l'utilisateur
WINLUX_KERNEL=/boot/vmlinuz-$(uname -r) bash scripts/build-live-iso.sh
```

L'image se trouve dans `build/winlux-live-test.iso` avec sa somme
SHA-256 dans `build/winlux-live-test.iso.sha256`.

### Tester l'ISO dans QEMU

```sh
# Test BIOS
bash scripts/test-live-iso-qemu.sh build/winlux-live-test.iso bios

# Test UEFI
bash scripts/test-live-iso-qemu.sh build/winlux-live-test.iso uefi
```

Configuration QEMU par défaut : 2 Gio RAM, 2 CPU. Configurable avec
`WINLUX_QEMU_MEMORY` et `WINLUX_QEMU_CPUS`.

### VM graphique persistante

```sh
# Créer le disque QCOW2 (24 Gio, 4 Gio RAM, 4 CPU)
scripts/create-test-vm.sh --create-only

# Démarrer la VM
scripts/create-test-vm.sh

# Après modification des sources, reconstruire
scripts/create-test-vm.sh --refresh
```

### Embarquer Microsoft Edge dans l'ISO

```sh
WINLUX_EDGE_DEB=/chemin/microsoft-edge-stable.deb \
  bash scripts/build-live-iso.sh
```

## Dépannage

### Le programme plante avec « import manquant »

L'import Windows appelé n'est pas encore implémenté dans Winlux.
Par défaut, Winlux arrête le programme et enregistre l'import manquant
dans son journal (`~/.local/share/winlux/`).

`WINLUX_MISSING=ignore` permet de continuer en renvoyant 0 (risque de
résultats incorrects).

### « lecture impossible » ou « en-tête PE invalide »

Le fichier n'est pas un exécutable PE32+ x86-64 valide. Winlux ne prend
en charge que les binaires 64 bits.

### « DLL, pas un exécutable »

Winlux ne peut pas lancer directement une DLL. Lancez l'exécutable qui
l'utilise.

### Le test ISO échoue sous UEFI

Augmentez la RAM : `WINLUX_QEMU_MEMORY=4096`. L'initramfs enrichi
nécessite plus de mémoire pour la décompression UEFI.

### Pas de son

Vérifiez que `libasound.so.2` est installée. Le pont ALSA n'a pas été
validé sur tous les périphériques audio.

### Le bureau ne s'affiche pas

Le shell graphique nécessite un serveur X11 fonctionnel. Vérifiez que
Xorg ou Xvfb est démarré. `WINLUX_GUI=headless` active le backend de
test sans affichage.

### Les logs sont trop verbeux / pas assez

Voir la section suivante sur la configuration des logs.

## Logs et verbosité

Winlux utilise un système de logs structurés configurable.

| Variable d'environnement | Effet |
|--------------------------|-------|
| `WINLUX_LOG=error` | Erreurs uniquement |
| `WINLUX_LOG=warn` | Avertissements et erreurs (défaut) |
| `WINLUX_LOG=info` | Messages informatifs |
| `WINLUX_LOG=debug` ou `--verbose` | Détails de débogage |
| `WINLUX_LOG=trace` ou `WINLUX_TRACE=1` | Traces complètes du chargeur |
| `WINLUX_LOG=debug,json` | Format JSON structuré |
| `WINLUX_LOG=json` | JSON en mode debug |

En mode JSON, chaque ligne est un objet :
```json
{"level":"debug","target":"loader","msg":"12 relocations appliquées (décalage 0x400000)"}
```

En mode texte (défaut) :
```
winlux[debug/loader]: 12 relocations appliquées (décalage 0x400000)
```
