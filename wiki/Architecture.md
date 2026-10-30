# Architecture de Winlux

## Vue d'ensemble

Winlux est un système complet qui tourne sur un **noyau Linux standard** (pas de VM Windows cachée). L'objectif est que les fichiers `.exe` s'exécutent nativement par double-clic, avec un accès complet au système de fichiers Linux.

```
+--------------------------------------------------+
|                Bureau X11 (Xorg)                  |
|  Openbox + xfdesktop + panneau Xfce              |
|  Style Windows 11 : menu Démarrer, barre,        |
|  notifications, Explorer                          |
+--------------------------------------------------+
|            Runtime Winlux (Rust)                  |
|  Chargeur PE32+ | API Win32 | GDI/X11            |
|  kernel32 | user32 | gdi32 | ntdll | msvcrt      |
|  advapi32 | ole32 | ws2_32 | shell32 | ...       |
+--------------------------------------------------+
|           Intégration Linux                       |
|  binfmt_misc | MIME .exe | .desktop               |
|  Lanceur winlux-open | Terminal                   |
+--------------------------------------------------+
|              Noyau Linux                          |
|  Ubuntu LTS | Xorg | ALSA | Réseau POSIX         |
+--------------------------------------------------+
```

## Composants principaux

### 1. Runtime PE (`runtime/`)

Le coeur du projet. Écrit en Rust, il gère :

- **Chargeur PE32+ x86-64** : parsing des headers PE, application des relocations, résolution des imports
- **Structures NT** : PEB/TEB minimaux via segment `GS`
- **API Win32 implémentées** :
  - `kernel32` : fichiers, processus, threads, mémoire virtuelle, synchronisation, TLS, horloge, console, pages de code
  - `user32` / `gdi32` : fenêtres, messages, dessin GDI, polices, pinceaux, bitmaps, DIB
  - `ntdll` : primitives Rtl*, registre NT, redirections API-set
  - `advapi32` : registre persistant, SCM (Service Control Manager)
  - `ole32` : fondation COM/OLE, appartements, fabriques, IStream
  - `ws2_32` : sockets TCP/UDP, résolveur DNS
  - `msvcrt` : CRT complète (stdio, chaînes, maths, qsort, scanf...)
  - `winmm` / `mfplat` : audio WAVE, MP3, AAC ADTS
  - `bcrypt`, `version`, `shell32`, `rpcrt4`, `winhttp`...

### 2. Bureau X11

Deux implémentations coexistent :
- **Bureau natif** (production) : Openbox + xfdesktop + panneau Xfce, stylisé Windows 11
- **Bureau GDI** (prototype dans testexe) : `gui_demo.exe` avec `WINLUX_DESKTOP_DEMO=1`, rendu direct via le runtime

Le bureau natif inclut :
- Barre des tâches centrée ou à gauche
- Menu Démarrer flottant avec recherche, épinglés, recommandés
- Explorer avec onglets, double volet, copie/déplacement, corbeille
- Paramètres complets (Linux/Windows, réseau, audio, affichage, mises à jour)
- Terminal (xterm), éditeur (Mousepad), navigateurs (Edge, Chrome, Firefox)

### 3. Intégration Linux (`integration/`)

- `winlux-binfmt.conf` : enregistrement binfmt_misc pour les `.exe`
- `x-ms-dos-executable.xml` : type MIME
- `winlux.desktop` : association fichier
- `winlux-open` : lanceur intelligent (console vs graphique)
- `install.sh` / `uninstall.sh` : installation/désinstallation propre

### 4. ISO et VM (`scripts/`)

- `build-live-iso.sh` : construction d'une ISO bootable (BIOS + UEFI via GRUB)
- `create-test-vm.sh` : VM persistante QCOW2
- `test-live-iso-qemu.sh` : tests automatisés en VM
- `build-update-bundle.sh` : paquets de mise à jour avec manifeste SHA-256

### 5. Tests (`testexe/`, `testdll/`, `testc/`)

- **testexe** : 10 PE de test Rust (hello, exitcode, p1_suite, dll_import, delay_import, findfiles, crt_suite, gui_suite, gui_demo, mingw_suite)
- **testdll** : DLL de test pour le chargement dynamique
- **testc** : Suite MinGW-w64 (40 contrôles) pour valider la CRT msvcrt

## Flux d'exécution d'un .exe

```
1. Double-clic sur fichier.exe
2. binfmt_misc → appelle /usr/bin/winlux
3. Winlux parse les headers PE32+
4. Applique les relocations
5. Résout les imports (kernel32, user32, etc.)
6. Initialise PEB/TEB via GS
7. Appelle le point d'entrée
8. Les appels API Win32 sont interceptés et traduits en appels Linux/POSIX
9. Les fenêtres sont créées via X11 (gui_x11.rs)
10. Audio via ALSA, réseau via sockets POSIX
```

## Technologies

| Composant | Technologie |
|-----------|-------------|
| Runtime | Rust |
| GUI backend | x11rb (Rust) + Xft |
| Bureau | Openbox + Xfce + Xorg |
| Audio | ALSA + libmpg123 + libfaad |
| Build | Cargo + MinGW-w64 (tests C) |
| ISO | xorriso + GRUB + SquashFS |
| CI | GitHub Actions |
| Licence | GPL-3.0-or-later |
