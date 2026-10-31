# Comment contribuer

## Prérequis

- **Linux x86-64**
- **Rust stable** et Cargo
- **Cible Windows** : `rustup target add x86_64-pc-windows-msvc`
- **MinGW-w64** (pour les tests C) : `apt install gcc-mingw-w64-x86-64`
- **Xorg** et bibliothèques pour les tests graphiques : Xvfb, Xft, Fontconfig
- **Audio** : `libmpg123-dev`, `libfaad-dev` (pour les tests audio)

## Compilation

```sh
git clone https://github.com/Univers4craft/winlux.git
cd winlux
cargo build --release -p winlux
```

## Tests

```sh
# Tests unitaires (84+)
cargo test -p winlux

# Build des PE de test
scripts/build-tests.sh

# Tests d'exécution (séquentiel et parallèle)
scripts/test-runtime.sh
scripts/test-runtime-parallel.sh

# Tests C MinGW-w64
scripts/build-mingw-tests.sh
scripts/test-mingw.sh

# Tests d'intégration
scripts/test-integration.sh

# Tests réseau, paramètres, mises à jour
scripts/test-network.sh
scripts/test-control.sh
scripts/test-update-notes.sh

# Tests audio (nécessite libmpg123 et libfaad)
ALSA_CONFIG_PATH="$PWD/runtime/tests/audio/alsa-null.conf" \
  cargo test -p winlux winmm::codecs -- --include-ignored
```

## Générer l'ISO

```sh
# Prérequis : outils initramfs/GRUB, Xorg, polices, squashfs-tools
WINLUX_KERNEL=/boot/vmlinuz-$(uname -r) scripts/build-live-iso.sh

# Test en VM
scripts/test-live-iso-qemu.sh build/winlux-live-test.iso bios
scripts/test-live-iso-qemu.sh build/winlux-live-test.iso uefi
```

## VM persistante

```sh
scripts/create-test-vm.sh --create-only  # Crée le disque
scripts/create-test-vm.sh                # Lance la VM
# Après reconstruction : --refresh remplace la racine
```

## Organisation du dépôt

| Dossier | Rôle |
|---------|------|
| `runtime/` | Chargeur PE et API Win32 en Rust |
| `runtime/src/` | Code source principal du runtime |
| `testexe/` | 10 PE de test Rust |
| `testdll/` | DLL de test |
| `testc/` | Suite MinGW-w64 (40 contrôles) |
| `integration/` | Lanceurs, MIME, binfmt, .desktop |
| `scripts/` | Build, tests, ISO, VM, mises à jour |
| `docs/` | Documentation technique et notes de version |
| `theme/` | Thème CSS de référence |
| `wiki/` | Pages wiki du projet |

## Fichiers de suivi

| Fichier | Rôle |
|---------|------|
| `Suivi.md` | **Source unique de vérité** pour les tâches |
| `TODO.md` | Feuille de route détaillée |
| `TRACKING.md` | Agrégation de toutes les tâches de tous les .md |
| `COMPATIBILITY.md` | Périmètre de compatibilité Windows |
| `INTERFACE.md` | Spécifications de l'interface bureau |
| `ROADMAP-SSD.md` | Objectifs installation SSD |

## Règles

1. **Suivi.md est la source de vérité** : chaque action concrète doit y être reflétée
2. **Tests avant commit** : `cargo test -p winlux` + `scripts/build-tests.sh` minimum
3. **rustfmt et Clippy bloquants** en CI
4. **Pas de Wine/Proton** : tout est développé en interne
5. **GPL-3.0-or-later** : voir `LICENSE`
