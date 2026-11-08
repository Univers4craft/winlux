# Mémoire au lancement d'un .exe (#997)

## Règle

Le runtime ne copie jamais le fichier d'un .exe en mémoire pour le lancer.
- Il le **projette** (`loader::ImageFile`, mmap en lecture seule), que ce soit pour :
  - la détection d'installateur (`setup::detect_file`) ;
  - la détection d'un bundle WiX Burn (`setup::burn::is_bundle_file`) ;
  - l'en-tête PE lu par `main.rs` ;
  - le chargeur (`loader::load_unlocked`).
- Une section d'au moins 1 Mio, alignée sur la page dans le fichier, est projetée depuis le disque en copie privée (MAP_PRIVATE), comme le fait Windows pour une image. Une écriture, par exemple une relocation, en fait une copie privée, et le fichier ne change jamais.
- Les autres sections sont copiées dans l'image.

## Pourquoi c'est obligatoire

Claude Code 2.1.311 (`claude.exe`, 298 870 432 octets) est un programme JavaScript empaqueté. Son code tient dans une section `.bun` de 170 Mo, alignée sur la page dans le fichier.

Avant le correctif, lancer `claude.exe --version` lisait le fichier entier quatre fois :
1. détection Burn ;
2. détection d'installateur ;
3. `main.rs`, dont la copie restait en mémoire jusqu'à la fin du programme ;
4. le chargeur.

Mesures dans le cloud, Linux x86-64 :

| Mesure | Avant | Après |
|---|---|---|
| Pic de mémoire résidente, `claude.exe --version` | 863 Mo | 291 Mo, page cache du fichier compris |
| Sous une limite cgroup de 700 Mo | tué par le système (SIGKILL, « code -9 » dans l'assistant) | répond en 0,9 s |
| Pic de mémoire anonyme, hello.exe + section de 192 Mio (`scripts/test-image-memory.py`) | 576 Mio, trois copies | 2 Mio |

Dans la VM de test (4 Go, bureau ouvert), l'activation de Claude dans l'Assistant IA échouait sur « code -9 ». `winlux claude.exe --version` ne répondait plus en 30 s, faute de mémoire.

## Contrôle en CI

`scripts/test-image-memory.py`, dans le job « Tests runtime (PE) » et dans `scripts/test-all.sh`, procède ainsi :
1. il ajoute à `hello.exe` une section de 192 Mio, alignée comme `.bun` ;
2. il relève le pic de RssAnon du lancement toutes les millisecondes ;
3. il échoue au-delà de 48 Mio de plus que `hello.exe` seul.

Toute nouvelle lecture complète du .exe au lancement (`std::fs::read`) fait échouer ce test. Pour lire un .exe, il faut passer par `ImageFile`.
