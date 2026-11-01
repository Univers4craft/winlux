# But et principes

## Le but

Beaucoup de gens restent sous Windows pour une seule raison : leurs programmes. Linux est libre, rapide et sûr, mais un `.exe` ne s'y ouvre pas d'un double-clic, et les solutions existantes demandent de comprendre ce qu'est un « préfixe », une « bouteille » ou une machine virtuelle.

**Winlux veut supprimer cette barrière.** C'est un système d'exploitation complet, livré sous forme d'ISO installable, dont l'objectif est simple à énoncer :

- **un `.exe` se lance d'un double-clic, où qu'il soit** : sur le bureau, dans Téléchargements, sur une clé USB, sur un partage réseau ;
- **il voit les mêmes fichiers que le reste du système** : `C:\Users\<nom>\Documents` est le vrai dossier Documents de l'utilisateur Linux, et `Z:\` est la racine Linux ;
- **l'utilisateur retrouve ses repères** : menu Démarrer, barre des tâches, Explorateur, Paramètres, Gestionnaire des tâches, Éditeur du Registre, raccourcis clavier, tout ressemble à Windows 11 ;
- **il n'y a qu'un seul noyau, Linux** : aucune machine virtuelle Windows ne tourne en arrière-plan, aucune licence Windows n'est nécessaire, et les applications Linux s'installent normalement avec `apt`, Flatpak ou Snap à côté des programmes Windows.

## Ce que Winlux n'est pas

- **Ce n'est pas une distribution qui embarque Wine.** La couche de compatibilité est écrite de zéro (voir plus bas).
- **Ce n'est pas une machine virtuelle.** Un `.exe` tourne comme un processus Linux ordinaire, visible dans `ps` et dans le Gestionnaire des tâches.
- **Ce n'est pas un clone de Windows.** L'ergonomie est reprise, mais pas la marque : aucun logo Windows, aucune application Microsoft, aucun compte Microsoft.
- **Ce n'est pas (encore) un remplaçant de Windows prêt pour tout le monde.** C'est une préversion technique. Voir [Limites et problèmes connus](Limites-et-problemes-connus).

## Les principes

### 1. Tout est fait maison

Décision du 14 octobre 2026 : la couche de compatibilité Windows n'utilise **ni Wine, ni Proton, ni DXVK, ni aucune couche tierce**. Le chargeur PE, `ntdll`, `kernel32`, `user32`, `gdi32`, Direct3D, le CLR .NET, `msiexec`, le registre : tout est écrit dans le dépôt, en Rust.

Pourquoi : maîtriser chaque ligne qui fait tourner un `.exe`, pouvoir diagnostiquer chaque échec, et construire un système cohérent plutôt qu'un assemblage de composants téléchargés.

Ce principe a des limites claires, pour ne pas réinventer ce que Linux fait déjà très bien. **Les briques Linux standard sont permises** :

| Besoin | Brique Linux utilisée |
|---|---|
| Décodage audio/vidéo du lecteur multimédia | FFmpeg |
| Impression | CUPS (par IPP, sans libcups) |
| Partage de dossiers vers Windows et Mac | Samba, wsdd, Avahi |
| Lecteurs réseau | `mount.cifs`, `mount.nfs`, `sshfs`, `rclone` |
| Son | PipeWire, ALSA |
| Bluetooth, réseau | BlueZ, NetworkManager |
| Base du système | paquets Ubuntu 26.04 LTS signés, gérés par `dpkg` |

Quand un de ces outils est absent, la fonction correspondante est **masquée** dans Paramètres. La CI vérifie à chaque push que rien ne dépend de Wine.

### 2. Aucun lien avec Microsoft au-delà de la compatibilité

Winlux exécute des programmes Windows, mais n'est pas associé à Microsoft :

- **comptes locaux uniquement**, créés à l'installation ou au premier démarrage, sans compte en ligne ni synchronisation dans un nuage ;
- **aucune application Microsoft proposée par défaut** (par exemple, l'éditeur Geany est proposé au premier démarrage, pas VS Code) ;
- pas de télémétrie : le signalement de bugs ne part que sur action volontaire, avec les données personnelles masquées.

**Seule exception, assumée** : pour démarrer avec Secure Boot activé sur un PC du commerce sans aucun réglage, Winlux utilise le *shim* des distributions Linux, contresigné par l'autorité UEFI tierce présente dans les micrologiciels. Ceux qui veulent s'en passer peuvent enregistrer leur propre clé MOK ou leurs propres clés Secure Boot ([docs/SECURE-BOOT.md](https://github.com/Univers4craft/winlux/blob/master/docs/SECURE-BOOT.md)).

### 3. Une identité visuelle propre : « Prisme »

L'ergonomie est celle de Windows 11, mais **aucun logo Windows n'apparaît nulle part**. Le logo de Winlux est **Prisme** : un losange taillé en quatre facettes, violet à gauche (Linux, la nuit) et cyan à droite (*lux*, la lumière). Accent de marque `#7C4DFF`, fond de nuit `#070A1F`.

Prisme apparaît sur le bouton Démarrer, le menu GRUB, l'écran de démarrage, l'installateur, l'assistant de bienvenue et la page À propos. Les fonds d'écran « Winlux Nuit » (par défaut), « Winlux Aube » et « Winlux classique » et le son de démarrage « Prisme » sont aussi créés dans le dépôt. Tout est régénérable avec `scripts/render-brand.py` ([docs/IDENTITE-VISUELLE.md](https://github.com/Univers4craft/winlux/blob/master/docs/IDENTITE-VISUELLE.md)).

### 4. Ressembler à Windows 11, pour de vrai

Le menu Démarrer peut être centré ou aligné à gauche, comme sous Windows 11. Les raccourcis habituels existent (Windows+L, Windows+V, Windows+S, Ctrl+Maj+Échap, Windows+flèches). Les fenêtres « système » portent les noms que l'utilisateur connaît : Explorateur, Paramètres, Gestionnaire des tâches, Observateur d'événements, Planificateur de tâches, Éditeur du Registre.

### 5. Des Paramètres honnêtes

- **Une section qui ne correspond à rien sur la machine est masquée**, jamais grisée ni vide : pas de page Bluetooth sans adaptateur, pas de réglage de fréquence CPU si le matériel ne le permet pas, pas de « Suivre la taille de la VM » sur un vrai PC.
- **Les réglages avancés du matériel** (processeur, GPU, alimentation) vivent dans Paramètres, et seulement là, après détection réelle du matériel.

### 6. Ne rien promettre qui n'existe pas

- Un import Windows non implémenté **arrête le programme avec un message clair** et le note dans le journal, au lieu de renvoyer un faux succès qui ferait planter le programme plus loin.
- La documentation distingue toujours ce qui est fait, partiel et prévu.
- Une CI verte veut dire « rien n'a régressé », pas « tous les logiciels Windows marchent ».

### 7. Multilingue dès le départ

Le bureau, l'Explorateur, les Paramètres, l'écran de verrouillage et les applications intégrées sont traduits en **français, anglais, espagnol et allemand** ; l'installateur ajoute l'**italien** et le **portugais**. Aucune langue ni disposition de clavier (AZERTY, QWERTY, QWERTZ…) ne doit être perdue lors d'une modification.

### 8. Facile à installer, facile à réparer

- Installation automatique sur tout le disque par défaut, partitionnement manuel derrière un mode avancé.
- L'installateur s'adapte à l'écran réel, de 1366×768 à la 4K.
- Point de restauration avant chaque mise à jour, retour arrière, menu de récupération dans GRUB.

## Pour qui ?

- **Aujourd'hui** : les curieux et les contributeurs, qui testent dans une machine virtuelle et signalent ce qui ne marche pas.
- **Demain** : toute personne qui veut quitter Windows sans abandonner ses programmes, et qui ne veut pas avoir à comprendre comment ça marche.

Voir aussi : [Architecture](Architecture) · [Feuille de route](Feuille-de-route)
