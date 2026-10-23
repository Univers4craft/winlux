# Thème Winlux

Cette base fournit une palette claire et des composants visuels inspirés de
Windows 11, sans reprendre de fichiers Microsoft. La palette est maintenant
reliée au backend X11 et la démonstration PE affiche un centre de paramètres
unifié : réglages Linux d'un côté, compatibilité Windows de l'autre, puis les
fonctions partagées entre les deux environnements. Une seconde vue
« Applications » présente au même endroit les paquets Linux APT/Flatpak et les
programmes Windows EXE/MSI, avec les actions d'installation et de
désinstallation pour chaque environnement.

## État et travaux restants

Le fichier CSS reste une référence pour le futur shell GTK/Qt. Le runtime X11
utilise sa propre palette Rust afin de conserver un rendu autonome des
applications Windows exécutées par Winlux.

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
