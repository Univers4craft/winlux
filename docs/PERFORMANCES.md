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
