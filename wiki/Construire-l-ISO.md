# Construire l'ISO

> En temps normal, **l'ISO est construite par le workflow de publication** (voir [Publication et mises à jour](Publication-et-mises-a-jour)), sur un runner GitHub Ubuntu 26.04 avec KVM. Cette page explique comment le faire à la main, pour comprendre ou déboguer la chaîne. Références : [docs/BUILD.md](https://github.com/Univers4craft/winlux/blob/master/docs/BUILD.md), [docs/NATIVE-APT.md](https://github.com/Univers4craft/winlux/blob/master/docs/NATIVE-APT.md), [docs/NATIVE-DESKTOP.md](https://github.com/Univers4craft/winlux/blob/master/docs/NATIVE-DESKTOP.md).

## Vue d'ensemble de la chaîne

```
 cargo build --release -p winlux          → runtime (target/release/winlux)
 scripts/build-tests.sh                   → .exe du shell et des tests (testexe/)
 scripts/prepare-desktop-packages.py      → composants du bureau, préparés hors de l'hôte
 scripts/prepare-browser-packages.py      → navigateurs hors ligne
 scripts/prepare-apt-base.py              → base Ubuntu 26.04 configurée dans une VM jetable
 scripts/build-live-iso.sh                → paquet winlux-desktop + racine SquashFS + initramfs + GRUB → ISO
 scripts/build-update-bundle.sh --system  → .deb de mise à jour + manifeste
```

## Prérequis

- Un hôte **Ubuntu 26.04 (Resolute) amd64** ou dérivé compatible, pour garder la même ABI que la base.
- **KVM** (`/dev/kvm`) : la base est préparée dans une VM QEMU.
- Rust stable avec la cible `x86_64-pc-windows-msvc`.
- Les outils de construction : `xorriso`, `grub-pc-bin`, `grub-efi-amd64-bin`, `mtools`, `dosfstools`, `squashfs-tools`, `python3`, `curl`, `dpkg-deb`, `fakeroot`, `mkfs.ext4`, `debugfs`, `qemu-system-x86`, un noyau `linux-image-*` lisible et les modules correspondants. La liste exacte est vérifiée par les précontrôles de `scripts/build-live-iso.sh` et installée par le workflow `release.yml`.

Les conteneurs sans KVM, ou sur une autre version d'Ubuntu, ne peuvent pas préparer la base : utilisez alors le workflow.

## Étapes

```sh
# 1. Runtime et .exe maison
rustup target add x86_64-pc-windows-msvc
cargo build --release -p winlux
bash scripts/build-tests.sh

# 2. Composants du bureau (dans build/, sans rien installer sur l'hôte)
python3 scripts/prepare-desktop-packages.py build/desktop-components

# 3. Base Ubuntu native (télécharge Ubuntu Base 26.04.1 au SHA-256 fixé,
#    installe les paquets dans une VM jetable ; journal : build/apt-base/bootstrap.log)
python3 scripts/prepare-apt-base.py \
  --kernel /boot/vmlinuz-<version> --kernel-release <version>

# 4. Image
WINLUX_KERNEL=/boot/vmlinuz-<version> WINLUX_KERNEL_RELEASE=<version> \
  sh scripts/build-live-iso.sh build/winlux-live-test.iso
```

L'ISO et sa somme SHA-256 sont produites dans `build/`.

### Ce que contient l'image

- un **petit initramfs** BusyBox (`scripts/winlux-live-init.sh`) : pilotes de stockage, vérification du média, déverrouillage LUKS, bascule vers la racine ;
- la **racine SquashFS compressée en xz**, avec une couche d'écriture en mémoire en Live ;
- les paquets **`winlux-desktop`** (tout ce qui est propre à Winlux, *Essential* et *Protected*) et **`winlux-kernel`** ;
- un menu **GRUB** au thème Prisme : session Live, installation, récupération, vérification du média ;
- un fichier `winlux.sha256` qui liste tous les fichiers de l'ISO pour la vérification d'intégrité.

### Variables utiles

| Variable | Effet |
|---|---|
| `WINLUX_KERNEL`, `WINLUX_KERNEL_RELEASE` | Noyau et version des modules à embarquer. |
| `WINLUX_SECUREBOOT=1` | Utilise le shim et le GRUB signés (`shim-signed`, `grub-efi-amd64-signed`) pour démarrer avec Secure Boot. Voir [docs/SECURE-BOOT.md](https://github.com/Univers4craft/winlux/blob/master/docs/SECURE-BOOT.md). |
| `WINLUX_AUTOINSTALL=fichier.yaml` | Intègre un fichier de réponses pour une installation silencieuse. |
| `SOURCE_DATE_EPOCH` | Fixe la date de construction écrite dans `BUILD-INFO`. |
| `WINLUX_LEGACY_BASE=1` | Reconstruit l'ancienne base sans APT, pour diagnostic seulement. |

## Tester l'ISO

```sh
sh scripts/test-live-iso-qemu.sh build/winlux-live-test.iso bios
sh scripts/test-live-iso-qemu.sh build/winlux-live-test.iso uefi
WINLUX_QEMU_MEMORY=6144 sh scripts/test-install-vm-guest.sh build/winlux-live-test.iso   # installation sur disque
sh scripts/test-native-session-vm.sh      # bureau natif sur un disque jetable, captures dans build/native-vm.*
sh scripts/test-browsers-vm.sh            # navigateurs
sh scripts/test-secureboot-qemu.sh        # chaîne Secure Boot
```

`WINLUX_QEMU_MEMORY` et `WINLUX_QEMU_CPUS` règlent la VM de test. Le test d'installation demande 6 Gio car il embarque une seconde copie de l'ISO.

### VM graphique persistante pour développer

```sh
scripts/create-test-vm.sh --create-only   # disque QCOW2 de 24 Gio
scripts/create-test-vm.sh                 # démarre (4 Gio, 4 CPU, 1920×1080)
scripts/create-test-vm.sh --refresh       # remplace la racine après reconstruction (sauvegarde l'ancien disque, ne migre pas ses données)
```

`WINLUX_VM_WIDTH` et `WINLUX_VM_HEIGHT` changent la définition.

## Fichiers sensibles

`release.yml`, `scripts/build-live-iso.sh`, `scripts/prepare-apt-base.py` et `scripts/test-live-iso-qemu.sh` forment la chaîne de publication : une erreur n'y apparaît souvent qu'au moment de taguer. Toute modification doit être coordonnée avec la préparation de la version en cours, et validée par les workflows `apt-base.yml` et `release.yml` (qui s'exécutent en mode validation sur les pull requests qui les touchent).

## Pièges connus

- Les noms de paquets changent d'une version d'Ubuntu à l'autre : vérifiez-les dans l'index de Resolute avant d'ajouter un paquet.
- Les fichiers de release GitHub doivent faire **moins de 2 Gio** : la taille de l'ISO est contrôlée à la construction.
- Le runner peut manquer de disque : le workflow libère de l'espace avant de construire.
- Les scripts GRUB n'acceptent pas `&&`.
- L'image n'a pas de commande `nice` dans son BusyBox : l'init appelle `/usr/bin/nice`.

Voir aussi : [Tests et CI](Tests-et-CI) · [Architecture](Architecture)
