# Thème Winlux

Cette base fournit une palette claire et des composants visuels inspirés de
Windows 11, sans reprendre de fichiers Microsoft. La palette est maintenant
reliée au backend X11 et la démonstration PE affiche un centre de paramètres
unifié : réglages Linux d'un côté, compatibilité Windows de l'autre, puis les
fonctions partagées entre les deux environnements. Une seconde vue
« Applications » présente au même endroit les paquets Linux APT/Flatpak et les
programmes Windows EXE/MSI, avec les actions d'installation et de
désinstallation pour chaque environnement.

Le démonstrateur fournit également une première vue « Winlux Desktop » en
1280×720 logique : fond abstrait, raccourcis, barre des tâches centrée, menu
Démarrer avec recherche/applications/recommandations et zone système
Wi-Fi-volume-batterie-heure. Le bouton Démarrer ouvre et ferme réellement le
menu via les messages souris Win32. La mise à l'échelle DPI produit la même vue
en 1920×1080 à 150 % et 2560×1440 à 200 %.

## État et travaux restants

Le fichier CSS reste une référence pour le futur shell GTK/Qt. Le runtime X11
utilise sa propre palette Rust afin de conserver un rendu autonome des
applications Windows exécutées par Winlux. Le backend calcule maintenant une
échelle depuis le DPI déclaré par l'écran (1× à 3×), applique la même échelle
aux fenêtres, formes, textes et entrées souris, et accepte
`WINLUX_UI_SCALE` comme surcharge de diagnostic. Les textes utilisent
Xft/Fontconfig pour obtenir des glyphes vectoriels antialiasés ; les anciennes
polices X11 « core » restent le repli quand Xft est indisponible.

- Choisir et construire un shell graphique Linux libre.
- Créer barre des tâches, lanceur/menu, explorateur et centre de notifications.
- Relier le shell au runtime : lancement, liste des fenêtres, focus, fermeture,
  presse-papiers et chemins Windows/Linux.
- Relier la vue Applications aux gestionnaires APT/Flatpak et au suivi des
  installateurs/désinstallateurs EXE/MSI ; l'écran actuel est le prototype
  visuel exécuté par `gui_demo.exe`.
- Ajouter les variantes clair/sombre, le clavier, l'accessibilité et les tests
  sur plusieurs résolutions.
- Intégrer ces composants à une image ISO Linux réellement amorçable.
