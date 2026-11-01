# Registre Windows

Beaucoup de programmes Windows lisent et écrivent leur configuration dans le registre. Winlux a son **propre registre persistant, écrit de zéro**, partagé par toutes les applications `.exe` d'un même utilisateur, par `reg.exe`, `regedit.exe`, `msiexec` et l'Éditeur du Registre. La référence complète est [docs/REGISTRY.md](https://github.com/Univers4craft/winlux/blob/master/docs/REGISTRY.md).

## Où il est stocké

| | |
|---|---|
| Fichier | `~/.local/share/winlux/registry.tsv` (un par utilisateur) |
| Sauvegarde | `registry.tsv.bak`, copiée avant chaque lancement d'application |
| Code | magasin dans `runtime/src/registry/`, API Win32 dans `runtime/src/dlls/advapi32.rs`, raccourcis shlwapi dans `shlwapi_reg.rs`, accès NT dans `ntdll.rs` |
| Tests | `WINLUX_REGISTRY_PATH` choisit un fichier isolé |

Ce n'est pas un format de ruche Windows : c'est un fichier texte simple, lisible depuis Linux, qui se répare et se versionne facilement.

## Le format `WINLUX-REGISTRY-2`

Une ligne d'en-tête `WINLUX-REGISTRY-2`, puis des lignes séparées par des tabulations :

```
K  <chemin en hexadécimal>  <date FILETIME>                       ← une clé
V  <chemin en hexadécimal>  <nom en hexadécimal>  <type>  <données hex>   ← une valeur
```

- Chemins et noms en UTF-8 avec **leur casse d'origine**, comparés sans tenir compte de la casse (un lecteur externe doit les passer en minuscules pour comparer).
- Les chaînes (`REG_SZ`, `REG_EXPAND_SZ`, `REG_MULTI_SZ`, types 1, 2 et 7) sont stockées en **UTF-16LE**, comme sous Windows ; les API « A » les convertissent en Windows-1252.
- Un ancien fichier `WINLUX-REGISTRY-1` est relu et converti à la première écriture.

### Racines

Racines stockées : `hkcu`, `hklm`, `hku`, `hkcc`.

**HKEY_CLASSES_ROOT n'est pas stocké** : c'est une **vue fusionnée** de `HKCU\Software\Classes` (prioritaire) puis `HKLM\Software\Classes`, comme sous Windows. Une clé créée sous HKCR va dans HKLM, sauf si elle existe déjà dans HKCU.

### Écritures concurrentes

Chaque écriture relit le fichier sous verrou (`flock`), applique la modification, puis remplace le fichier de façon atomique : deux programmes qui écrivent en même temps ne s'écrasent pas. Les lectures gardent une copie en mémoire tant que le fichier n'a pas changé.

## Les API prises en charge

- **Win32** (versions A et W) : `RegOpenKey[Ex]`, `RegCreateKey[Ex]`, `RegSetValue[Ex]`, `RegSetKeyValue`, `RegQueryValue[Ex]`, `RegGetValue` (filtres de type, développement de `REG_EXPAND_SZ`), `RegDeleteValue`, `RegDeleteKey[Ex]`, `RegDeleteTree`, `RegCopyTree`, `RegRenameKey`, `RegEnumKey[Ex]`, `RegEnumValue`, `RegQueryInfoKey`, `RegNotifyChangeKeyValue`, `RegLoadMUIStringW` (chaînes simples).
- **shlwapi** : `SHGetValue`, `SHSetValue`, `SHDeleteKey`, `SHCopyKey`, les fonctions « utilisateur puis machine » (`SHRegGetUSValue`…), `SHOpenRegStream2`, et les associations de fichiers `AssocQueryString`/`AssocQueryKey` lues dans HKCR.
- **NT** : `NtOpenKey[Ex]`, `NtCreateKey`, `NtQueryValueKey` (formats Basic, Full, Partial), `NtClose`. `\Registry\Machine` correspond à HKLM, `\Registry\User\S-1-22-1-<uid>` à HKCU.

## Fichiers `.reg`, `reg.exe` et `regedit`

Ces outils sont **intégrés au runtime** : un installateur qui appelle `reg.exe` ou `regedit.exe` n'a besoin d'aucun fichier externe.

```sh
winlux reg.exe query "HKCU\Software\MonAppli" /s
winlux reg.exe add "HKCU\Software\MonAppli" /v Langue /t REG_SZ /d fr /f
winlux regedit.exe /s reglages.reg        # import silencieux
winlux regedit.exe /e export.reg "HKCU\Software\MonAppli"
winlux --reg …                            # même chose sans passer par un .exe
```

- Formats lus : « Windows Registry Editor Version 5.00 » (UTF-16 ou UTF-8) et `REGEDIT4` (ANSI), avec `[-clé]`, `@=`, `dword:`, `hex:`, `hex(n):`, `"valeur"=-` et lignes continuées.
- L'export écrit la version 5.00 en UTF-16 avec BOM, comme regedit.
- Un double-clic sur un `.reg` ouvre l'Éditeur du Registre, qui demande confirmation avant l'import.

## L'Éditeur du Registre

Application maison du shell (`regedit` dans Démarrer, ou `winlux --regedit`) qui reprend l'éditeur de Windows 11 : arborescence et barre d'adresse, création de tous les types de valeurs, modification DWORD/QWORD en hexadécimal ou décimal, renommage (F2), suppression avec confirmation, recherche (Ctrl+F, F3), import et export `.reg`. Traduit en français, anglais, espagnol et allemand.

## Utilisation par Winlux lui-même

Le registre n'est pas seulement pour les programmes Windows :

- les **associations de fichiers** des programmes Windows (ProgID, `OpenWithProgids`, `UserChoice`…) sont lues dans HKCR pour créer les lanceurs du bureau et le menu « Ouvrir avec » ;
- la clé **`Uninstall`** alimente le menu Démarrer et la fenêtre « Applications installées » ;
- les réglages de l'Explorateur (`Explorer\Advanced`) sont écrits par Paramètres et relus par l'Explorateur.

Pour lire le registre depuis du code Rust du runtime :

```rust
let key = crate::registry::canonical(key);
let exists = crate::registry::read(|d| Ok(d.exists(&key)));
// écriture : crate::registry::write(...) ou apply_registry_edits(...)
```

## Ce qui n'est pas encore géré

- Les ACL du registre, la redirection WOW64 (`/reg:32`), les clés volatiles, `RegLoadKey`/`RegSaveKey`.
- La résolution des références `@fichier.dll,-id` dans `RegLoadMUIStringW`.
- HKLM est stocké dans le profil de chaque utilisateur : ce n'est pas encore un magasin machine commun à tous les comptes.
- Une synchronisation explicite de certains réglages Linux avec leurs clés Windows (les clés dynamiques de langue et de clavier, par exemple).

Voir aussi : [Chemins et droits](Chemins-et-droits) · [Compatibilité des applications](Compatibilite-des-applications)
