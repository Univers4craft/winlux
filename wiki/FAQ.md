# FAQ

### Winlux, c'est Linux avec Wine ?

Non. Winlux n'utilise **ni Wine, ni Proton, ni DXVK**. La couche qui exécute les `.exe` (chargeur PE, `kernel32`, `user32`, Direct3D, CLR .NET, registre…) est écrite de zéro en Rust dans ce dépôt. Voir [But et principes](But-et-principes).

### Y a-t-il une machine virtuelle Windows cachée ?

Non. Il n'y a qu'un noyau, Linux. Un `.exe` est un processus Linux ordinaire, visible dans le Gestionnaire des tâches et dans `ps`.

### Faut-il une licence Windows ?

Non. Aucun composant de Windows n'est nécessaire ni fourni.

### Mon programme va-t-il marcher ?

Ça dépend des API qu'il utilise. Les programmes console et Win32 simples, les installateurs MSI, NSIS et Inno Setup marchent souvent ; les gros logiciels (WPF, WinForms, Photoshop, jeux récents) pas encore, et les programmes 32 bits sont refusés. Le plus simple est d'essayer dans la session Live et de [signaler](Comment-contribuer) ce qui manque. Détails : [Compatibilité des applications](Compatibilite-des-applications).

### Où sont mes fichiers ?

`C:\Users\<nom>\Documents` est votre vrai dossier Documents Linux, `C:\` votre dossier personnel, `Z:\` la racine Linux, et les clés USB et lecteurs réseau reçoivent les lettres `E:`, `F:`… Voir [Chemins et droits](Chemins-et-droits).

### Puis-je installer des logiciels Linux ?

Oui : Winlux est une vraie base Ubuntu 26.04. `apt`, Flatpak, Snap et AppImage fonctionnent, et le Store réunit toutes ces sources avec les logiciels Windows.

### Faut-il un compte Microsoft ou un compte en ligne ?

Non. Winlux n'utilise que des **comptes locaux**, et ne propose aucune application Microsoft par défaut.

### Pourquoi le logo n'est-il pas celui de Windows ?

Winlux reprend l'ergonomie de Windows 11 mais pas la marque. Son logo est **Prisme**, un losange violet et cyan.

### Winlux démarre-t-il avec Secure Boot ?

Oui, grâce au shim signé des distributions Linux. On peut aussi utiliser ses propres clés. Voir [docs/SECURE-BOOT.md](https://github.com/Univers4craft/winlux/blob/master/docs/SECURE-BOOT.md).

### Puis-je l'installer à côté de Windows ?

Oui, l'installateur sait réduire la partition Windows et proposer les deux systèmes au démarrage. Sauvegardez vos données avant. Winlux voit ensuite la partition Windows comme un lecteur, en lecture seule si Windows est en veille prolongée ou en démarrage rapide.

### Comment mettre à jour ?

Paramètres › Mises à jour. Attention : depuis la v0.2.6 ou la v0.2.12, une réinstallation unique est nécessaire. Voir [Publication et mises à jour](Publication-et-mises-a-jour).

### Winlux envoie-t-il des données ?

Non. Il n'y a pas de télémétrie. Les mises à jour lisent seulement la page publique des releases GitHub, et un signalement de bug ne part que si vous le validez, données personnelles masquées.

### Y a-t-il un antivirus ?

Non, par choix. Les autorisations des applications (caméra, micro, écran, fichiers) et le pare-feu limitent ce qu'un programme peut faire, mais aucun fichier n'est analysé.

### Comment aider ?

Tester, signaler, traduire, coder : voir [Comment contribuer](Comment-contribuer).
