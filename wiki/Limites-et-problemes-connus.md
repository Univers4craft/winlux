# Limites et problèmes connus

Cette page dit franchement ce qui ne marche pas encore. Les problèmes actifs sont suivis dans les [issues](https://github.com/Univers4craft/winlux/issues) et dans [Suivi.md](https://github.com/Univers4craft/winlux/blob/master/Suivi.md).

État au 22 novembre 2026, sur la dernière version publiée (v0.2.21) et sur `master`.

## Mises à jour

| Situation | Conséquence | Statut |
|---|---|---|
| Installation v0.2.6 ou plus ancienne | Ne peut pas passer à la base APT en place : **réinstallation unique** avec une ISO récente. | Voulu (changement de base) |
| Installation v0.2.12 | Son outil de mise à jour ne peut installer aucune mise à jour : **réinstallation unique** avec une ISO récente. | Corrigé depuis la v0.2.13 (#200) |
| Installation v0.2.13 ou plus récente | Se met à jour depuis Paramètres, manifeste signé, retour automatique à la version précédente si la nouvelle ne démarre pas. | En service (#31) |

## Programmes Windows

- **Gros logiciels** : Office ne s'installe pas encore jusqu'au bout (#2740), Teams n'atteint pas son écran de connexion (#2511, #2726), Acrobat Reader s'installe mais ne s'utilise pas encore (#236), l'installeur de Creative Cloud ne montre pas sa fenêtre (#2784), OneDrive ne démarre pas (#2851), Steam ne se connecte pas encore (#285).
- **Applications Chromium** (Electron, CEF, WebView2) : en cours (#236).
- **.NET de bureau** : WPF reste incomplet (#268) ; WinForms progresse dans notre moteur .NET, mur par mur ; l'installeur web de Paint.NET n'aboutit pas encore (#2752).
- **Programmes 32 bits** : beaucoup démarrent, pas tous. Des interfaces COM rendues au code 32 bits n'ont pas encore de mandataire (#2428).
- **COM** : un serveur hors processus déclaré comme service n'est pas encore lancé (#2575).
- **Media Foundation / DirectShow** : toute la vidéo n'est pas encore décodée pour les `.exe`.
- **Anti-triche noyau** et tout ce qui demande un pilote Windows (`.sys`) : hors périmètre. Un anti-triche noyau est refusé à l'installation, avec un message clair.

## Système et bureau

- La connexion automatique ne se désactive pas encore après l'installation (#2135).
- Aucun journal système persistant : un gel est difficile à diagnostiquer après redémarrage (#2133).
- Plusieurs logiciels Linux de la Boutique s'installent sans leurs dépendances ou sans leurs traductions (#2081 à #2103).
- Le registre stocke HKLM dans le profil de chaque utilisateur, pas dans un magasin machine commun.

## Matériel

- **Tablette en UEFI 32 bits** : après installation sur le disque interne, aucun support amorçable n'est trouvé (#2747) ; la clé USB tombe parfois en « grub rescue » (#2748).
- **Écran tactile** : un appui sur Démarrer n'ouvre parfois pas le menu (#2749).
- Direct3D n'a pas encore été validé sur une large gamme de vrais GPU, ni Winlux sur une large gamme de PC.
- BitLocker : les volumes chiffrés par BitLocker ne sont jamais montés.

## Pièges rencontrés pendant le développement

Ces problèmes sont corrigés, mais méritent d'être connus de ceux qui touchent au système :

- **Panneau XFCE** : `xfce4-genmon-plugin` 4.3 sur Ubuntu 26.04 ignore ses fichiers `.rc` et lit Xfconf, ce qui laissait les icônes de la barre des tâches vides ; `winlux-native-config` recopie la configuration dans `xfce4-panel.xml`. `winlux-control align-left/center` doit garder le plugin de zone de notification dans la liste.
- **BusyBox de l'image** : pas de commande `nice`, l'init appelle `/usr/bin/nice`.
- **Affichage en VM** : aucun paquet `xserver-xorg-video-*` ; Xorg en `modesetting` avec les modules `bochs`, `qxl`, `vmwgfx` (et `ttm`, `drm_ttm_helper`, `drm_exec`).
- **Publication** : noms de paquets Ubuntu 26.04 changés, BusyBox du runner utilisant AVX absent de QEMU, ISO au-delà de 2 Gio, disque du runner plein. Voir [Publication et mises à jour](Publication-et-mises-a-jour).
- **Clippy** : la CI utilise le dernier Rust stable, avec des lints absents des versions plus anciennes. Mettez Rust à jour avant de vous fier à un Clippy local.
- **Codes de sortie** : Linux ne garde que l'octet bas d'un code de sortie ; un code Windows dont l'octet bas vaut 0 ne sort plus en succès (#2758).

Voir aussi : [Feuille de route](Feuille-de-route) · [FAQ](FAQ)
