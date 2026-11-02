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

## Objectif 10 s (novembre 2026)

Mesures réelles sur la VM de damien (v0.2.13 installée, libvirt i440fx,
4 vCPU, 4 Go) : 21 s jusqu'au bureau complet, dont 8 s de menu GRUB
(`set timeout=8` écrit par l'installateur), ~4 s de messages du noyau à
l'écran (pas de `quiet`) et 7,6 s du noyau au bureau. Le premier démarrage
après l'installation durait 105 s : les logiciels choisis dans l'installateur
s'installaient pendant le boot, sans message.

Corrigé :

- Menu GRUB des installations : caché 1 s (Échap ou Maj l'affiche), visible
  5 s seulement quand Windows est proposé ; entrée « Winlux Desktop » en
  `quiet loglevel=3 vt.global_cursor_default=0`. Les installations existantes
  sont mises à niveau une fois par le postinst (`/usr/share/winlux/boot-menu`).
- `winlux-hardware display` (GPU et configuration Xorg) est le seul inventaire
  avant Xorg ; `winlux-hardware rest`, `winlux-firmware` et le profil
  d'utilisation passent en arrière-plan. Plus de chargement aveugle d'une
  soixantaine de pilotes ni de second `udevadm trigger` + `settle --timeout=10` :
  les pilotes sont chargés d'après les modalias du matériel présent.
- `winlux-native-session --early` (bus D-Bus, polkit, udisks, NetworkManager,
  CUPS, PipeWire) démarre en même temps que Xorg ; la session graphique
  attend seulement cette partie.
- Gestionnaire de fenêtres lancé d'emblée, Thunar attendu jusqu'à la prise de
  son nom D-Bus (la réponse complète seulement en test automatique),
  catalogue du menu Démarrer, raccourcis du Bureau, programmes « au
  démarrage », alignement de la barre, souris et processeur en arrière-plan ;
  `winlux-control` ne refait plus son rapport d'état complet (une vingtaine de
  sondes) après chaque réglage restauré au démarrage.
- `winlux-update --boot` ne lance plus Python quand il n'y a rien à vérifier.
- Fond d'écran fixé avant la barre des tâches ; clavier et pavé tactile
  appliqués pendant que le bureau se dessine ; fenêtre Bienvenue ouverte
  après le bureau (elle restait parfois noire).
- Paquets du premier démarrage : après l'arrivée du bureau, en `nice 19` +
  `ionice -c 3`, avec une notification au début et à la fin
  (`winlux-first-boot-notice`, 4 langues).

Banc de mesure (VM QEMU émulée sans KVM, 4 vCPU, disque « installé » fait de
la racine v0.2.13 et des scripts de la branche, deuxième démarrage, repère
`desktop`) ; l'émulation est environ 11 fois plus lente que KVM, seules les
proportions comptent :

| Itération | Bureau | Étape la plus lourde |
|---|---|---|
| v0.2.13 | 89,7 s | détection matérielle 25,6 s, session 32,2 s |
| 1 : pilotes d'après le matériel, alignement sans rapport d'état | ~90 s (bruit) | détection 20,8 s |
| 2 : inventaire en arrière-plan, Thunar en parallèle | 42,7 s | session 14,5 s |
| 3-4 : GPU seul avant Xorg sans `cat`, catalogue en parallèle | 38,6 s | session 15,5 s |
| 5 : services sans écran pendant Xorg, `--boot` sans Python | 33,3 s | session 10,4 s |
| 6-7 : WM d'emblée, Thunar et raccourcis hors chemin critique | 31,6 s | session 8,2 s |
| 8-11 : Xorg sans glamor en VM sans 3D, relevé d'état allégé (#259) | 31,3 s | Xorg 7,1 → 5,5 s |

Attention : jusqu'ici le repère `desktop` est le lancement de xfdesktop, pas
son affichage. À partir de l'itération 12 le banc note aussi
`wm.xfdesktop`, l'instant où la fenêtre du bureau est visible
(`xdotool search --onlyvisible`) : c'est le chiffre qui compte pour
l'utilisateur.

Depuis #258, un système installé ouvre le bureau sous le compte de
l'utilisateur (`winlux-user-session`), qui reprenait l'ancien ordre en série.
Même banc, compte installé avec connexion automatique, bureau visible :

| Itération | Bureau visible | Changement |
|---|---|---|
| master après #258 | 71,3 s | Thunar attendu (Ping) avant tout, programmes au démarrage et raccourcis en série |
| 12 : même traitement que #254 dans la session du compte | 61,3 s | WM d'emblée, raccourcis, programmes au démarrage, clavier en arrière-plan |
| 13 : démarrage décalé | 55,3 s | agents de la zone de notification, presse-papiers, lecteurs réseau, économiseur, verrouillage, catalogue, relevé d'état et suivi des écrans attendent le bureau visible |
| 14-15 : Thunar non attendu, barre non reconstruite | 48,5 s | `align-*` ne réécrit plus `plugin-ids` identique |

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
