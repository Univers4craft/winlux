# Paquets MSI de test : origine

Ces fichiers sont produits par l'équipe Winlux. Aucun ne vient de Microsoft ni d'un logiciel tiers.

| Fichier | Origine |
|---|---|
| `sample.wxs`, `tables.wxs` | Descriptions écrites pour Winlux (produits fictifs « Winlux Exemple » et « Winlux Tables »). |
| `tables/*.idt` | Tables écrites pour Winlux : Environment, IniFile, RemoveFile, RemoveRegistry. |
| `sample.msi`, `tables.msi` | Construits par `generer.sh` avec msitools (`wixl`, `msibuild`). msitools est un logiciel libre du projet GNOME, sous licence LGPL. |

Les fichiers que contiennent les paquets (`app.exe`, `Lisez-moi.txt`, `données.bin`) sont de simples textes de test, créés par `generer.sh`. `app.exe` contient seulement `MZ-winlux-test` : ce n'est pas un vrai programme.

Pour régénérer les paquets : `sh runtime/src/msi/testdata/generer.sh`. Il faut les paquets `wixl` et `msitools`. Les tests de `runtime/src/msi/tests.rs` passent avec les paquets régénérés.
