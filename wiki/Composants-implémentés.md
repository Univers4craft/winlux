# Composants implémentés

> État au 30 octobre 2026. Basé sur l'analyse du code et de `Suivi.md`.

## Runtime PE (chargeur)

| Fonctionnalité | État |
|----------------|------|
| Chargement PE32+ x86-64 | Fait |
| Relocations et imports | Fait |
| PEB/TEB via GS | Fait |
| Imports différés (delay imports) | Fait |
| LoadLibrary/GetProcAddress | Fait |
| Exports par nom et ordinal | Fait |
| Callbacks TLS et DllMain | Fait |
| Politique imports manquants (WINLUX_MISSING) | Fait |
| Vérifications de bornes PE/IAT | Fait |
| Gestionnaire de crash sans allocation | Fait |
| FreeLibrary (vrai déchargement) | **À faire** |
| Pile de secours pour stack overflow | **À faire** |

## API Win32 — kernel32

| Fonctionnalité | État |
|----------------|------|
| Fichiers (CreateFile, ReadFile, WriteFile, ...) | Fait |
| Fichiers Unicode (variantes W) | Fait |
| Répertoires (CreateDirectory, RemoveDirectory, ...) | Fait |
| FindFirstFile/FindNextFile A/W | Fait |
| GetFullPathName, CopyFile, MoveFile | Fait |
| GetTempFileName, GetFileAttributesEx, SetEndOfFile | Fait |
| VirtualAlloc/VirtualFree (alignement 64 Ko) | Fait |
| VirtualProtect, MEM_DECOMMIT | Fait |
| VirtualQuery | Fait |
| TLS (indices, valeurs par thread, TlsFree) | Fait |
| Synchronisation (Event, Mutex, WaitForSingle/Multiple) | Fait |
| Sections critiques récursives | Fait |
| I/O overlapped + GetOverlappedResult | Fait |
| CancelIo/CancelIoEx (best-effort) | Fait |
| IOCP (GetQueuedCompletionStatus[Ex]) | Fait |
| CreateProcess A/W + handles processus | Fait |
| CREATE_SUSPENDED / ResumeThread | Fait |
| Environnement ANSI/Unicode pour enfants | Fait |
| GetTickCount (horloge monotone) | Fait |
| GetSystemInfo, version OS, FILETIME | Fait |
| Console, pages de code, FormatMessage | Fait |
| Héritage de handles complet | **À faire** |
| Annulation I/O exhaustive | **À faire** |

## API Win32 — user32 / gdi32

| Fonctionnalité | État |
|----------------|------|
| Classes de fenêtres, table HWND | Fait |
| File de messages par thread | Fait |
| GetMessage/PeekMessage/DispatchMessage A/W | Fait |
| PostMessage/SendMessage A/W | Fait |
| WndProc invité | Fait |
| SetWindowPos / MoveWindow | Fait |
| Focus clavier (GetFocus/SetFocus) | Fait |
| GetKeyState / GetAsyncKeyState / GetKeyboardState | Fait |
| Set/GetWindowText W | Fait |
| Backend X11 réel (x11rb) | Fait |
| Backend headless pour tests | Fait |
| FillRect, FrameRect, Ellipse, Rectangle | Fait |
| BitBlt, StretchBlt, PatBlt (tous ROPs) | Fait |
| CreateSolidBrush, CreatePen, CreateFont A/W | Fait |
| SelectObject, DeleteObject, GetStockObject | Fait |
| SetTextColor, SetBkColor, SetBkMode, TextOutA | Fait |
| GetSysColor / GetSysColorBrush | Fait |
| CreateCompatibleBitmap / CreateDIBSection 32bpp | Fait |
| SetDIBitsToDevice / StretchDIBits (1/4/8/16/24/32 bits) | Fait |
| GetDC / ReleaseDC | Fait |
| Décorations/titre maison si pas de WM | Fait |

## API Win32 — autres DLL

| DLL | Fonctionnalités | État |
|-----|-----------------|------|
| `ntdll` | Primitives Rtl*, registre NT, API-set | Partiel |
| `advapi32` | Registre persistant Win32 | Fait |
| `ole32` | COM/OLE fondation, appartements, fabriques, IStream | Fait |
| `ws2_32` | TCP/UDP, WSA, getaddrinfo A/W | Fait |
| `msvcrt` | CRT complète (stdio, chaînes, maths, scanf, ...) | Fait |
| `shell32` | Fondation | Partiel |
| `bcrypt` | Fondation | Partiel |
| `version` | Fondation | Partiel |
| `winmm` | WAVE PCM/float, MP3, AAC ADTS | Fait |
| `mfplat` | Points d'entrée | Partiel |
| `winhttp` | HTTP/HTTPS synchrone via curl | Fait |
| `rpcrt4` | UuidCreate (UUID v4) | Fait |

## Bureau / Shell

| Fonctionnalité | État |
|----------------|------|
| Barre des tâches Windows 11 (centrée/gauche) | Fait |
| Menu Démarrer flottant (recherche, épinglés, recommandés) | Fait |
| Zone de notification (réseau, volume, batterie, heure) | Fait |
| Explorer (onglets, double volet, recherche, tri) | Fait |
| Copie/déplacement de fichiers | Fait |
| Corbeille dans Explorer | **À faire** |
| Paramètres (Linux/Windows, réseau, audio, affichage, mises à jour) | Fait |
| Terminal intégré (xterm) | Fait |
| Éditeur de texte (Mousepad) | Fait |
| Navigateurs (Edge, Chrome, Firefox) | Fait |
| Icônes du bureau déplaçables | Fait |
| Thème clair/sombre + échelle | Fait |
| Hotkeys globales | Fait |
| Calculatrice native | Fait |
| Capture d'écran native | Fait |
| Visionneuse d'images native | Fait |
| Post-it natif | Fait |
| Infos système | Fait |
| Lecteur PDF natif | Fait |
| Lecteur audio natif | Fait |
| Raccourcis clavier (Ctrl+L, Win, AltGr, XKB) | Fait |
| Résolution adaptative (suit la fenêtre QEMU) | Fait |
| Pointeur flèche visible (SWcursor) | Fait |

## Intégration Linux

| Fonctionnalité | État |
|----------------|------|
| binfmt_misc pour .exe | Fait |
| Type MIME et association .desktop | Fait |
| Installation dans /usr/bin | Fait |
| Désinstallation propre | Fait |
| Lanceur console/graphique intelligent | Fait |
| Chemins avec espaces et URI file:// | Fait |
| Messages d'erreur graphiques | Fait |
| Test sur machine Linux réelle | **À faire** |
| Double-clic GNOME/KDE/XFCE vérifié | **À faire** |

## ISO et installation

| Fonctionnalité | État |
|----------------|------|
| ISO bootable BIOS + UEFI (GRUB) | Fait |
| SquashFS + initramfs | Fait |
| VM persistante QCOW2 | Fait |
| Tests automatisés QEMU | Fait |
| Installateur disque expérimental | Fait |
| Mises à jour GitHub (SHA-256, rollback) | Fait |
| BUILD-INFO dans Paramètres > À propos | Fait |
| Signature Ed25519 des mises à jour | **À faire** |
| Système A/B réel | **À faire** |
| Workflow release validé sur GitHub | **À faire** |

## CI

| Vérification | État |
|--------------|------|
| cargo check | Fait |
| cargo test | Fait (84+ tests) |
| rustfmt | Fait (bloquant) |
| Clippy | Fait (bloquant) |
| Build des 10 PE de test | Fait |
| Tests séquentiels et parallèles | Fait |
| Tests MinGW-w64 (40 contrôles) | Fait |
| Tests réseau, paramètres, mise à jour | Fait |
| Tests ISO QEMU BIOS/UEFI | Fait |
