# Architecture

Winlux est empilé en quatre couches. Chacune a un rôle précis et un emplacement dans le dépôt.

```
┌──────────────────────────────────────────────────────────────────────────┐
│ 4. BUREAU                                                                │
│   a) Session native habillée Windows 11 (scripts/, integration/native/)  │
│      xfdesktop · xfce4-panel · Openbox · Thunar                          │
│      menu Démarrer GTK · Paramètres GTK · verrouillage · UAC · Bienvenue │
│   b) Shell PE interne (testexe/src/shell/) : vrais .exe dessinés par le  │
│      runtime : Explorateur, Gestionnaire des tâches, Regedit, Store…     │
├──────────────────────────────────────────────────────────────────────────┤
│ 3. RUNTIME WINLUX (runtime/, Rust, maison)                               │
│   chargeur PE32+ · ~100 DLL Windows · CLR .NET · registre · chemins      │
│   msiexec · NSIS/Inno · Direct3D→OpenGL/Vulkan · services système        │
├──────────────────────────────────────────────────────────────────────────┤
│ 2. BASE LINUX : Ubuntu 26.04 LTS, apt/dpkg natifs                        │
│   Xorg (modesetting) · PipeWire/ALSA · NetworkManager · BlueZ · CUPS     │
│   Samba · FFmpeg · init Winlux + shim systemctl                          │
├──────────────────────────────────────────────────────────────────────────┤
│ 1. NOYAU LINUX : un seul noyau, aucune VM Windows                        │
└──────────────────────────────────────────────────────────────────────────┘
```

## 1. Le noyau Linux

Winlux utilise un noyau Ubuntu standard, signé (donc compatible Secure Boot). Les modules sont choisis à la construction de l'image : stockage, systèmes de fichiers (ext4, NTFS `ntfs3`, FAT, exFAT, `cifs`, `nfs`), et affichage virtuel `bochs`, `qxl` et `vmwgfx` pour que QEMU, VirtualBox et VMware affichent une image sans paquet `xserver-xorg-video-*` (Xorg utilise le pilote `modesetting`).

Le démarrage se fait en deux temps :

1. un **petit initramfs** (BusyBox, `scripts/winlux-live-init.sh`) charge les pilotes de stockage, vérifie au besoin l'intégrité du média, déverrouille un disque **LUKS**, puis bascule vers la vraie racine ;
2. la racine est soit la **SquashFS** de l'ISO avec une couche d'écriture en mémoire (session Live), soit la partition installée `WINLUX_ROOT`.

## 2. La base Linux

Depuis la v0.2.12, la racine est une **vraie base Ubuntu 26.04 LTS** (« Resolute ») avec `apt-get` et `dpkg`, préparée à partir de l'image officielle Ubuntu Base dont le SHA-256 est fixé dans `scripts/prepare-apt-base.py`.

- Tout ce qui est propre à Winlux est regroupé dans le paquet Debian **`winlux-desktop`**, marqué *Essential* et *Protected*, et le noyau dans **`winlux-kernel`**. Les bibliothèques Ubuntu restent gérées par leurs paquets d'origine.
- Winlux garde **son propre init** et non systemd. Un shim `systemctl` (`scripts/winlux-systemctl.sh`) traduit les commandes systemd pour que les paquets installés par APT puissent démarrer leurs services.
- Les services lourds ne démarrent qu'au besoin : par exemple Samba et wsdd restent masqués tant qu'aucun dossier n'est partagé.

Détails : [docs/NATIVE-APT.md](https://github.com/Univers4craft/winlux/blob/master/docs/NATIVE-APT.md).

## 3. Le runtime Winlux

C'est le cœur du projet : un seul binaire Rust, `/usr/bin/winlux` (crate `winlux`, dossier `runtime/`), qui charge et exécute les programmes Windows 64 bits.

### Le chargeur PE

| Fichier | Rôle |
|---|---|
| `runtime/src/pe.rs` | Lecture du format PE32+ : en-têtes, sections, répertoires de données. Un PE 32 bits est reconnu et refusé avec un message explicite. |
| `runtime/src/loader.rs` | Mappage des sections, relocations, imports et imports différés, chargement des DLL réelles livrées avec le programme, TLS et `DllMain`. |
| `runtime/src/thunks.rs` | Passerelles d'appel entre la convention Windows x64 et le code Rust. |
| `runtime/src/process.rs`, `launch.rs` | Processus, lancement, environnement, ligne de commande, PEB/TEB (accessibles par le registre `GS`). |
| `runtime/src/dlls/seh.rs`, `cxx_eh.rs` | Exceptions structurées x64 (SEH) et exceptions C++ MSVC. |
| `runtime/src/crash.rs`, `log.rs` | Écran bleu de diagnostic, journal structuré, signalement d'un import manquant. |

### Les DLL Windows réimplémentées

Le dossier `runtime/src/dlls/` contient une centaine de modules. Quand un programme importe `kernel32.dll!CreateFileW`, le chargeur le relie directement à la fonction Rust correspondante. Les grandes familles :

| Famille | Modules principaux |
|---|---|
| Noyau et fichiers | `ntdll`, `kernel32` (+ `_fs`, `_ext`, `_misc`, `_cpu`), `winfile`, `mmap`, `knownfolders` |
| Fenêtres et dessin | `user32`, `gdi32` (+ polices), `gui_x11`, `gui_cairo`, `comctl32`, `comdlg32`, `uxtheme`, `dwmapi` |
| 2D, texte et images | `d2d1` + `d2d_raster` (Direct2D logiciel), `dwrite` + `sfnt` (DirectWrite), `windowscodecs` (WIC) |
| 3D et vidéo | `d3d9`, `d3d11`, `d3d12`, `dxgi`, `d3d_shader`, `spirv`, `vk_backend`, `gl_legacy`, `dxva2` |
| Son et périphériques | `winmm`, `dsound`, `mmdevapi`, `xaudio2`, `xinput`, `dinput8`, `mfplat`, `mfcapture`, `v4l2`, `evdev` |
| Système et sécurité | `advapi32` (+ services), `security`, `bcrypt`, `crypt32`, `wintrust`, `userenv`, `powrprof`, `setupapi`, `taskschd` |
| Shell et COM | `shell32`, `shlwapi`, `ole32`, `oleaut32`, `propsys`, `comobj`, `dataobject`, `notifyicon`, `xdnd`, `clipboard` |
| Réseau | `ws2_32`, `winhttp`, `iphlpapi`, `rpcrt4` (UUID seulement) |
| Bibliothèques C/C++ | `msvcrt`, `crt_*` (ucrt), `libgcc`, `libstdcpp` |
| Impression | `winspool`, `print_pdf`, `ipp` |
| .NET | `mscoree` |

### Les autres sous-systèmes du runtime

| Dossier | Rôle | Page du wiki |
|---|---|---|
| `clr/` | CLR .NET maison : métadonnées, interpréteur CIL, ramasse-miettes, exceptions, P/Invoke | [Compatibilité](Compatibilite-des-applications) |
| `registry/` | Registre Windows persistant, `reg.exe`, `regedit`, fichiers `.reg` | [Registre Windows](Registre-Windows) |
| `paths.rs`, `paths/` | Traduction `C:\…` ↔ chemins Linux | [Chemins et droits](Chemins-et-droits) |
| `uac.rs`, `permissions.rs`, `admin.rs` | UAC, autorisations des applications | [Chemins et droits](Chemins-et-droits) |
| `msi/`, `setup/` | `msiexec` intégré (CAB, LZX), installateurs NSIS et Inno Setup | [Compatibilité](Compatibilite-des-applications) |
| `archive/` | ZIP, gzip, tar ; 7z et RAR par `bsdtar` | [Bureau](Bureau-et-applications) |
| `imaging/` | Décodeurs d'images maison (PNG, JPEG, GIF, TIFF, WebP, PSD, XCF, EXR, DDS, SVG…) | [Bureau](Bureau-et-applications) |
| `audio/` | Mixage, PipeWire ou ALSA | [Compatibilité](Compatibilite-des-applications) |
| `intl/` | Pages de code, claviers XKB → HKL, fuseaux horaires, locales | [Compatibilité](Compatibilite-des-applications) |
| `volumes.rs`, `mounts.rs`, `netdrive/`, `diskimage.rs` | Clés USB, partitions Windows, lecteurs réseau, images disque | [Réseau et disques](Reseau-et-disques) |
| `firewall.rs` | Pare-feu et alerte de connexion entrante | [Réseau et disques](Reseau-et-disques) |
| `remote/` | Serveur de bureau à distance RFB (VNC) écrit en Rust | [Bureau](Bureau-et-applications) |
| `power/`, `boottime.rs`, `gamemode.rs`, `cpu.rs`, `gpu.rs` | Veille, temps de démarrage, mode jeu, réglages matériels | [Paramètres](Parametres) |
| `tasks.rs` | Service du Planificateur de tâches | [Bureau](Bureau-et-applications) |

Le même binaire sert aussi d'outil système : `winlux --volumes daemon`, `winlux --tasks daemon`, `winlux --gamemode daemon`, `winlux --boot-time`, `winlux --drives` (lecteurs réseau), `winlux --text`… Des liens comme `winlux-drives` pointent vers lui.

## 4. Le bureau

Winlux a **deux piles d'interface distinctes**, à ne pas confondre :

| | Session native | Shell PE interne |
|---|---|---|
| **Ce que c'est** | Des composants Linux habillés Windows 11 | De vrais programmes Windows écrits en Rust `no_std`, compilés en `.exe` et exécutés par le runtime |
| **Code** | `scripts/winlux-start-menu.py`, `winlux-settings.py`, `winlux-settings-pages.py`, `winlux-lock.py`, `winlux-uac.py`, `winlux-welcome.py`… ; configuration dans `integration/native/` | `testexe/src/shell/` : `explorer.rs`, `taskmgr.rs`, `regedit.rs`, `store.rs`, `settings.rs`, `desktop.rs`, `wm.rs`… |
| **Composants** | xfdesktop (bureau), xfce4-panel (barre des tâches), Openbox (fenêtres), Thunar (service de fichiers D-Bus), menu Démarrer et Paramètres en GTK | Explorateur, Gestionnaire des tâches, Éditeur du Registre, Store, Observateur d'événements, Pare-feu, Planificateur… |
| **Rôle** | C'est la session de bureau par défaut, installée et en Live | Applications ouvertes depuis la session native ; l'ancien bureau entièrement PE (`gui_demo.exe`) reste disponible pour les tests |

Le shell PE est aussi la meilleure preuve que la couche de compatibilité fonctionne : l'Explorateur que l'utilisateur ouvre tous les jours est un `.exe` qui passe par `user32`, `gdi32` et le backend X11 du runtime.

Détails : [Bureau et applications](Bureau-et-applications).

## Le trajet d'un double-clic sur un `.exe`

```
 Double-clic sur setup.exe (bureau, Thunar, Explorateur)
        │
        ▼
 Type MIME application/x-ms-dos-executable ──► winlux-open      (integration/)
        │      (binfmt_misc l'enregistre aussi pour ./setup.exe en terminal)
        ▼
 winlux-open : console ? ──oui──► Terminal Winlux (xterm + Bash)
        │ non
        ▼
 /usr/bin/winlux setup.exe
        │
        ├─ pe.rs      lit les en-têtes, refuse un PE 32 bits proprement
        ├─ loader.rs  mappe les sections, relocations, imports, DLL réelles
        ├─ manifeste  requireAdministrator ? ──► fenêtre UAC (winlux-uac)
        ├─ process.rs PEB/TEB, environnement (USERPROFILE, APPDATA…), TLS
        ▼
 Point d'entrée du programme
        │  chaque appel Win32 ─► fonction Rust ─► appel Linux
        │     CreateFileW("C:\Users\Léa\Documents\a.txt") ─► open("/home/lea/Documents/a.txt")
        │     CreateWindowExW ─► fenêtre X11      RegSetValueExW ─► registry.tsv
        │     IDirect3DDevice9::Present ─► Vulkan  waveOutWrite ─► PipeWire
        ▼
 Import non implémenté ? ─► arrêt propre + journal + proposition de signalement
```

## Technologies

| Domaine | Choix |
|---|---|
| Langage du runtime et du shell PE | Rust (stable, `no_std` pour les `.exe` du shell) |
| Interface de la session native | Python 3 + GTK, XFCE, Openbox |
| Affichage | Xorg (pilote modesetting), X11 via le runtime, Xft/Fontconfig pour le texte |
| 3D | OpenGL et Vulkan (traduction maison de Direct3D) |
| Son | PipeWire, repli ALSA |
| Base système | Ubuntu 26.04 LTS, apt/dpkg, init Winlux |
| Image | SquashFS xz, GRUB (BIOS + UEFI), shim signé pour Secure Boot |
| CI | GitHub Actions (runners Ubuntu 26.04, KVM pour les tests de VM) |
| Licence | GPL 3 ou ultérieure |

Voir aussi : [Compatibilité des applications](Compatibilite-des-applications) · [Construire l'ISO](Construire-l-ISO)
