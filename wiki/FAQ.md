# FAQ

### Winlux, c'est Linux avec Wine ?

Non. Winlux n'utilise **ni Wine, ni Proton, ni DXVK**. La couche qui exécute les `.exe` (chargeur PE, `kernel32`, `user32`, Direct3D, CLR .NET, registre…) est écrite de zéro en Rust dans ce dépôt. Voir [But et principes](But-et-principes).

### Y a-t-il une machine virtuelle Windows cachée ?

Non. Il n'y a qu'un noyau, Linux. Un `.exe` est un processus Linux ordinaire, visible dans le Gestionnaire des tâches et dans `ps`.

### Faut-il une licence Windows ?

Non. Aucun composant de Windows n'est nécessaire ni fourni.

### Mon programme va-t-il marcher ?

Ça dépend des API qu'il utilise. Les programmes console et Win32 simples, en 64 comme en 32 bits, et les installateurs MSI, NSIS et Inno Setup marchent souvent ; OBS Studio et Paint.NET s'ouvrent. Les gros logiciels comme Office, Teams, Acrobat ou Creative Cloud, les applications Chromium et WPF ne vont pas encore au bout. Le plus simple est d'essayer dans la session Live et de [signaler](Comment-contribuer) ce qui manque. Détails : [Compatibilité des applications](Compatibilite-des-applications).

### Où sont mes fichiers ?

`C:\Users\<nom>\Documents` est votre vrai dossier Documents Linux, `C:\` votre dossier personnel, `Z:\` la racine Linux, et les clés USB et lecteurs réseau reçoivent les lettres `E:`, `F:`… Voir [Chemins et droits](Chemins-et-droits).

### Puis-je installer des logiciels Linux ?

Oui : Winlux est une vraie base Ubuntu 26.04. `apt`, Flatpak, Snap et AppImage fonctionnent, et le Store réunit toutes ces sources avec les logiciels Windows.

### Faut-il un compte Microsoft ou un compte en ligne ?

Non. Winlux n'utilise que des **comptes locaux**, et ne propose aucune application Microsoft par défaut.

### Pourquoi le logo n'est-il pas celui de Windows ?

Winlux offre une ergonomie familière, avec sa propre identité : il n'est ni affilié à Microsoft, ni approuvé par elle. Son logo est **Prisme**, un losange violet et cyan.

### Winlux démarre-t-il avec Secure Boot ?

Oui, grâce au shim signé des distributions Linux. On peut aussi utiliser ses propres clés. Voir [docs/SECURE-BOOT.md](https://github.com/Univers4craft/winlux/blob/master/docs/SECURE-BOOT.md).

### Puis-je l'installer à côté de Windows ?

Oui, l'installateur sait réduire la partition Windows et proposer les deux systèmes au démarrage. Sauvegardez vos données avant. Winlux voit ensuite la partition Windows comme un lecteur, en lecture seule si Windows est en veille prolongée ou en démarrage rapide.

### Comment mettre à jour ?

Paramètres › Mises à jour › Mise à jour OS. Une installation v0.2.13 ou plus récente se met à jour sans réinstaller ; une installation v0.2.12 ou plus ancienne doit être réinstallée une fois avec une ISO récente. Voir [Publication et mises à jour](Publication-et-mises-a-jour).

### Winlux envoie-t-il des données ?

Non. Il n'y a pas de télémétrie. Les mises à jour lisent seulement la page publique des releases GitHub, et un signalement de bug ne part que si vous le validez, données personnelles masquées.

### Y a-t-il un antivirus ?

Non, par choix. Les autorisations des applications (caméra, micro, écran, fichiers) et le pare-feu limitent ce qu'un programme peut faire, mais aucun fichier n'est analysé.

### Comment aider ?

Tester, signaler, traduire, coder : voir [Comment contribuer](Comment-contribuer).
