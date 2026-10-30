# Fonctions du bureau

## Mises à jour

Les paramètres natifs affichent une fenêtre en cas d'échec d'une recherche,
installation ou restauration : elle indique l'étape connue, le code de retour
et les dernières lignes du journal, sélectionnables pour les copier. Le journal
complet de la session reste consultable dans `/run/winlux/update.log`.
Les tailles annoncées et l'action prévue sont indiquées avant installation.
L'espace nécessaire est contrôlé avant le téléchargement et la préparation.
Le téléchargement et le SHA-256 affichent une progression mesurée ; les
étapes dont la durée ne peut pas être mesurée restent indéterminées. Un paquet
partiel est conservé pour reprendre un téléchargement interrompu lorsque le
serveur le permet, puis vérifié intégralement avant toute installation.
Les paquets runtime et système existants restent distincts ; les mises à jour
différentielles fichier par fichier ne sont pas encore prises en charge.

## Recherche

Démarrer recherche les applications XDG, les catégories de Paramètres et les
noms de fichiers visibles du dossier personnel. La recherche de fichiers est
asynchrone, ne suit pas les dossiers symboliques et est limitée à 30 000
entrées et 30 résultats. Elle ne parcourt pas les fichiers système et ne
recherche pas le contenu des documents. Entrée ouvre le premier résultat.

L'accueil propose des raccourcis vers le clavier, le réseau, l'apparence et
le secours lors de la première session. « Terminer » enregistre le choix dans
le profil ; fermer la fenêtre permet de le reprendre plus tard.

## Réglages rapides et matériel

Les commandes Arrêter et Redémarrer demandent d'abord aux fenêtres de se
fermer normalement. Si une fenêtre reste ouverte pendant 30 secondes,
l'opération est annulée pour permettre d'enregistrer le travail. Une mise à
jour en cours bloque aussi l'arrêt. Une fois les fenêtres fermées, les
services reçoivent une demande d'arrêt avant la synchronisation des disques.

Démarrer se ferme au clic extérieur ou sur Échap. Le clic sur le son ouvre
un panneau compact avec volume réel, curseur 0–100 %, silence et accès aux
réglages. Le clic sur l'alimentation affiche batterie/secteur et un accès aux
options d'alimentation. Ces deux panneaux sont à instance unique, se placent
près de la barre des tâches du moniteur courant et se ferment aussi sur Échap
ou au clic extérieur. Les écritures du curseur sont regroupées sur 120 ms.

Cliquer sur les indicateurs réseau, son ou alimentation ouvre les réglages
rapides. NetworkManager et son agent gèrent les réseaux et les demandes de
mot de passe ; BlueZ et Blueman gèrent le Bluetooth. `brightnessctl` utilise
les périphériques de rétroéclairage réellement disponibles. L'absence d'un
adaptateur, d'un pilote, d'un rétroéclairage ou d'un service est une erreur
explicite, pas un changement d'état simulé. La présence de ces outils ne
garantit pas la prise en charge de tous les chipsets physiques.

La route Ethernet est préférée au Wi-Fi par défaut, sans désactiver la radio
ni supprimer ses profils. Dans une VM, l'accès fourni par l'hôte apparaît
généralement sous forme de carte Ethernet. Si aucune route n'existe et que
NetworkManager ne gère pas une carte filaire connectée, un client DHCP de
secours tente d'obtenir une adresse. Les réglages permettent de lister les
réseaux, gérer la radio et ouvrir l'éditeur de connexions ; l'agent réseau
demande le mot de passe. Le WPS dépend de l'agent, de l'adaptateur et du point
d'accès ; il n'est pas garanti sur tous les matériels.

## Notifications

Le service de session implémente `org.freedesktop.Notifications`. La cloche du
panneau ouvre les 100 dernières notifications, sur un historique conservé de
200 entrées dans `$XDG_STATE_HOME/winlux/notifications.json` (par défaut
`~/.local/state`). Ne pas déranger masque les bannières mais conserve
l'historique. Les actions sont transmises aux applications encore présentes
dans la session ; elles ne relancent pas une commande arbitraire enregistrée
dans un ancien message. L'historique peut être effacé depuis ce panneau.

## Processus

Le gestionnaire de tâches affiche les processus, leur PID, la RAM résidente
et le CPU par cœur, à partir de `/proc`. La première mesure CPU est zéro et
les suivantes utilisent les différences de temps processeur. La fermeture
envoie SIGTERM ; l'arrêt forcé envoie SIGKILL après confirmation. PID 1 et le
gestionnaire lui-même sont protégés, et le temps de création est revérifié
pour éviter d'agir sur un PID réutilisé.

## Apparence

Paramètres > Personnalisation ouvre les choix clair/sombre, accent, image et
hauteur de panneau. GTK, XFCE et les décorations Openbox sont configurés dans
le profil. Les applications ouvertes peuvent nécessiter d'être rouvertes pour
charger leur nouveau style. Les CSS utilisateur extérieurs au bloc généré
Winlux sont conservés.
Une réinitialisation explicite propose de revenir au thème, à l'accent, au
fond, à l'icône Démarrer et à la taille de barre par défaut.

Une image peut aussi remplacer l'icône Démarrer ; un bouton rétablit celle
livrée avec Winlux. Les raccourcis créés volontairement depuis Démarrer et les
icônes ajoutées par Paramètres sont marqués comme fiables lorsque le service
de métadonnées du bureau le permet ; les autres fichiers ne sont pas approuvés
automatiquement.

## Secours au démarrage

Une nouvelle installation depuis une image intégrant ce changement propose
« Winlux secours » dans GRUB. L'initramfs monte l'installation et donne accès
à une restauration de la sauvegarde de mise à jour ou à une console. La
restauration dépend d'une sauvegarde valide et des outils présents sur le
disque ; elle ne répare pas tous les problèmes de démarrage ou matériels.
Conserver une clé USB d'installation Winlux permet de démarrer si le disque,
GRUB, le noyau ou l'initramfs ne fonctionne plus. Cette entrée n'est pas
ajoutée automatiquement aux installations déjà présentes.

## Fichiers

« Fichiers et périphériques » complète l'Explorer PE existant avec Thunar :
favoris GTK, miniatures via Tumbler, accès aux volumes via GVfs/UDisks et
éjection via la barre latérale ou l'outil Disques. Un aperçu intégré affiche
les textes (64 Kio maximum) et les images jusqu'à 20 Mio et 40 mégapixels.
Les autres formats ne sont pas interprétés. Aucun support de fichiers
exécutables n'est requis pour cet aperçu.
La page Fichiers et le menu du bureau donnent accès à Thunar pour ses menus
contextuels, favoris, onglets et volumes ; l'Explorer Winlux reste disponible.

## Fenêtres

Windows + flèches gauche/droite placent une fenêtre sur une moitié d'écran.
Windows + Ctrl + flèche gauche/droite la place dans un coin supérieur ;
Windows + Maj + flèche gauche/droite choisit le coin inférieur. Ces raccourcis
ne remplacent pas un aperçu d'ancrage à la souris, qui reste à développer.

## Applications

« Applications et démarrage » donne accès aux navigateurs intégrés et aux
applications Flatpak de l'utilisateur. Le catalogue Flathub est activé par
une action explicite ; l'installation et la désinstallation utilisent des
identifiants d'application validés et nécessitent Internet. Ce gestionnaire
ne transforme pas la racine Winlux en distribution APT et n'exécute pas les
scripts de maintenance de paquets DEB arbitraires.

La future image avec [APT natif](NATIVE-APT.md) utilise une base Ubuntu
configurée et un paquet Winlux protégé. Elle ajoute les applications APT
dans ce même panneau ; ce changement ne convertit pas une installation
existante et n’est pas inclus dans la release 0.2.6 déjà lancée.

Les applications par défaut couvrent texte, PNG, JPEG, PDF, HTML et dossiers.
Les choix de démarrage produisent des fichiers XDG dans `~/.config/autostart`,
lancés à la connexion et désactivables depuis la même interface.

## Petits gestes et raccourcis

Démarrer, le volume et l’alimentation se ferment au clic extérieur ou sur
Échap. Cliquer à nouveau sur leur icône referme leur panneau. Le volume
dispose d’un curseur de 0 à 100 %, utilisable aussi avec les flèches du
clavier, et d’un bouton muet. L’alimentation affiche la batterie ou le secteur
et propose l’arrêt et le redémarrage avec confirmation.

Ctrl+Maj+Échap ouvre les tâches, Windows+A les réglages rapides et Windows+N
les notifications. Les raccourcis personnalisés existants sont conservés.
