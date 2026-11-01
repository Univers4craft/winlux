# Performances : démarrage et mode jeu

Suivi : issue #153.

## Temps de démarrage

L'init (`scripts/build-live-iso.sh`) note l'instant de chaque étape, en
secondes depuis l'allumage du noyau, dans `/run/winlux/boot-times` :

| Repère | Fin de l'étape |
|---|---|
| `init` | noyau et initramfs chargés (le micrologiciel n'est pas compté) |
| `recovery` | vérification/reprise des mises à jour (`winlux-update --boot`) |
| `drivers` | udev, modules, `udevadm settle` |
| `hardware` | `winlux-hardware`, `winlux-firmware`, profil d'utilisation |
| `services` | services APT, pare-feu, planificateur, mode jeu |
| `xorg` | Xorg répond |
| `session` | `winlux-native-session` terminé |
| `desktop` | bureau lancé : fin du démarrage |

Au repère `desktop`, `winlux --boot-time record` garde le total dans
`/var/lib/winlux/boot-history` (20 derniers démarrages) et la console série
affiche `WINLUX_BOOT_PHASES=nom:ms,…`. `winlux --boot-time` donne le rapport
(`boot_total_ms`, `boot_phases`, `boot_slowest`, `boot_previous_ms`,
`boot_best_ms`) que `winlux-control status` ajoute à `/run/winlux/cpu.txt`.
Paramètres > Processeur l'affiche ; la section est absente tant qu'aucun
démarrage complet n'a été mesuré.

## Mesure et GRUB

Mesure du 31/10/2026 en VM KVM (i5-1235U, QEMU `-cpu host -smp 4`, BIOS),
médiane de 3 démarrages jusqu'à `WINLUX_DESKTOP_READY` : 53,0 s avant #155,
40,3 s après. Après le noyau, les étapes totalisent 9,4 s (session 3,4 s,
Xorg 1,9 s, vérification système 1,2 s, pilotes 1,1 s).

Le reste était dans GRUB : la recherche de `WINLUX_ROOT` faite dans l'entrée
« Winlux Desktop » échouait en live et GRUB attendait alors 10 s sur « Press
any key to continue ». Elle est faite une fois avant le menu, et le menu
attend 2 s au lieu de 5. Les 14 s de la recherche elle-même viennent du
lecteur de disquette émulé de la machine QEMU `pc` (i440fx) : en `q35`, comme
sur un PC réel, elle prend une demi-seconde.

## Mode jeu

Réglage : Paramètres > Processeur > Mode jeu (`GAME_MODE=auto|off` dans
`/etc/winlux/cpu.conf`, automatique par défaut).

1. Le runtime inscrit tout `.exe` qui importe Direct3D, DXGI, OpenGL, Vulkan
   ou XInput (`cpu::is_heavy`) dans `/run/winlux/gamemode/<pid>` (numéro et
   instant de création du processus : un numéro réutilisé ne compte pas).
2. Le service `winlux --gamemode daemon` (root, lancé par l'init) vérifie les
   inscriptions toutes les 2 s. Tant qu'un jeu vit :
   - gouverneur `performance`, seulement s'il existe et sur secteur ;
   - priorité du jeu à `nice -5`, seulement si elle était normale (un choix
     fait dans « Réglages de l'exécutable » est respecté) ;
   - sauvegarde, mises à jour, apt/dpkg et leurs enfants en `nice 19` et
     priorité disque « idle » ; les boucles de l'init ne relancent ni
     sauvegarde ni mise à jour automatique (`/run/winlux/gamemode.active`) ;
   - `vm.swappiness` à 10 et mémoire compactée, si `/proc/sys/vm` est
     modifiable ;
   - veille et extinction de l'écran bloquées (`power-inhibit`).
3. À la fermeture du dernier jeu, tout revient à l'état d'avant, pour les
   processus encore présents.

L'état (`/run/winlux/gamemode.txt`, ajouté à `cpu.txt`) publie
`cap_game_governor`, `cap_game_memory`, `cap_game_priority` : les Paramètres
ne citent que les actions possibles sur la machine.
