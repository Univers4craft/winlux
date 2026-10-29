# Fonctions du bureau

## Recherche

Démarrer recherche les applications XDG, les catégories de Paramètres et les
noms de fichiers visibles du dossier personnel. La recherche de fichiers est
asynchrone, ne suit pas les dossiers symboliques et est limitée à 30 000
entrées et 30 résultats. Elle ne parcourt pas les fichiers système et ne
recherche pas le contenu des documents. Entrée ouvre le premier résultat.

## Réglages rapides et matériel

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

## Fichiers

« Fichiers et périphériques » complète l'Explorer PE existant avec Thunar :
favoris GTK, miniatures via Tumbler, accès aux volumes via GVfs/UDisks et
éjection via la barre latérale ou l'outil Disques. Un aperçu intégré affiche
les textes (64 Kio maximum) et les images jusqu'à 20 Mio et 40 mégapixels.
Les autres formats ne sont pas interprétés. Aucun support de fichiers
exécutables n'est requis pour cet aperçu.

## Applications

« Applications et démarrage » donne accès aux navigateurs intégrés et aux
applications Flatpak de l'utilisateur. Le catalogue Flathub est activé par
une action explicite ; l'installation et la désinstallation utilisent des
identifiants d'application validés et nécessitent Internet. Ce gestionnaire
ne transforme pas la racine Winlux en distribution APT et n'exécute pas les
scripts de maintenance de paquets DEB arbitraires.

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
