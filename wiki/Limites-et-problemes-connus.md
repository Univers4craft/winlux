# Limites et problèmes connus

Cette page dit franchement ce qui ne marche pas encore. Les problèmes actifs sont suivis dans les [issues](https://github.com/Univers4craft/winlux/issues) et dans [Suivi.md](https://github.com/Univers4craft/winlux/blob/master/Suivi.md).

## Mises à jour

| Situation | Conséquence | Statut |
|---|---|---|
| Installation v0.2.6 ou plus ancienne | Ne peut pas passer à la base APT en place : **réinstallation unique** avec une ISO récente. | Voulu (changement de base) |
| Installation v0.2.12 | Son outil de mise à jour ne peut installer aucune mise à jour (APT refuse le fichier temporaire qu'il lui passe) : **réinstallation unique** avec l'ISO v0.2.13. | Corrigé dans v0.2.13 (#200) |
| Notes publiées de la v0.2.12 | Elles promettent à tort une mise à jour sans réinstallation. | Corrigées à la publication de v0.2.13 |
| Mises à jour non signées | Vérifiées par SHA-256 et taille, mais sans signature cryptographique. | À faire (#31) |
| Pas de partitions A/B | Une coupure de courant pendant une mise à jour reste un risque (APT et le mode récupération limitent les dégâts). | À faire (#31) |

## Programmes Windows

- **64 bits uniquement** : un `.exe` 32 bits est refusé avec un message explicite (le code .NET managé 32 bits passe par le CLR maison).
- **Paint.NET** ne s'ouvre pas encore ; **WPF** et **WinForms** ne sont pas disponibles.
- **CLR maison** : pas de génériques de la BCL, de délégués ni de LINQ.
- **ChatGPT et Claude pour Windows** ne démarrent pas encore.
- **Photoshop**, les anti-triche et tout ce qui demande un pilote Windows (`.sys`) : hors de portée actuellement.
- **COM** : pas d'activation depuis le registre, pas de serveurs hors processus ni d'automatisation.
- **Media Foundation / DirectShow** : pas de décodage vidéo pour les `.exe`.
- **Processus** : pas d'héritage des handles ; `FreeLibrary` laisse les DLL en mémoire jusqu'à la fin du processus ; services sans `StartServiceCtrlDispatcher`.
- **msiexec** : pas d'actions personnalisées DLL/EXE/script, pas de `.mst` ni de `.msp`.
- **Paquets MSIX** : non pris en charge.

## Système et bureau

- Le bureau de l'ISO doit encore passer en **utilisateur non privilégié** (#13).
- Corbeille incomplète (restaurer, vider), pas de barre de progression des copies (#10, #11).
- Navigation au clavier à compléter dans les fenêtres du shell.
- Certaines applications ne sont encore qu'en une langue (#139) ; l'installateur en anglais ne traduit pas encore tout (#140).
- Identité Prisme à terminer : écran de chargement du noyau, sons système, icônes (#170, PR #187).
- Le registre stocke HKLM dans le profil de chaque utilisateur, pas dans un magasin machine commun.

## Matériel

- Testé surtout en **machine virtuelle**. La veille prolongée, le capot de portable, la luminosité, les écrans multiples et les pavés tactiles sont en cours de validation (PR #149).
- Direct3D n'a pas encore été validé sur une gamme de vrais GPU.
- BitLocker : les volumes chiffrés par BitLocker ne sont jamais montés.

## Pièges rencontrés pendant le développement

Ces problèmes sont corrigés, mais méritent d'être connus de ceux qui touchent au système :

- **Panneau XFCE** : `xfce4-genmon-plugin` 4.3 sur Ubuntu 26.04 ignore ses fichiers `.rc` et lit Xfconf, ce qui laissait les icônes de la barre des tâches vides ; `winlux-native-config` recopie la configuration dans `xfce4-panel.xml`. `winlux-control align-left/center` doit garder le plugin de zone de notification dans la liste.
- **BusyBox de l'image** : pas de commande `nice`, l'init appelle `/usr/bin/nice`.
- **Affichage en VM** : aucun paquet `xserver-xorg-video-*` ; Xorg en `modesetting` avec les modules `bochs`, `qxl`, `vmwgfx` (et `ttm`, `drm_ttm_helper`, `drm_exec`).
- **Publication** : noms de paquets Ubuntu 26.04 changés, BusyBox du runner utilisant AVX absent de QEMU, ISO au-delà de 2 Gio, disque du runner plein. Voir [Publication et mises à jour](Publication-et-mises-a-jour).
- **Clippy** : la CI utilise le dernier Rust stable, avec des lints absents des versions plus anciennes. Mettez Rust à jour avant de vous fier à un Clippy local.

Voir aussi : [Feuille de route](Feuille-de-route) · [FAQ](FAQ)
