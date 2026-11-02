# Assistant IA (#286)

Un panneau de discussion ouvert par un bouton de la barre des tâches ou par
Windows+C, qui connaît Winlux : « pourquoi ce .exe ne s'ouvre pas ? », « où
est mon fichier ? », « passe au thème sombre ». Trois assistants au choix,
sur la page 30 des Paramètres (Applications > Assistant IA) :

| Assistant | Ce qui tourne | Compte | Téléchargé à l'activation |
|---|---|---|---|
| **Claude** | Claude Code (`claude.exe` à travers la couche PE, même installation que l'application du menu Démarrer) | compte Claude (`claude auth login`, Google ou e-mail) | Claude Code, ≈ 300 Mo, SHA-256 du manifeste |
| **ChatGPT** | Codex, l'outil officiel d'OpenAI (programme Linux natif, `x86_64` ou `aarch64`) | compte ChatGPT (`codex login`, Google ou e-mail), jamais une clé d'API | paquet npm `@openai/codex` de la plateforme, ≈ 170 Mo, SHA-512 du registre npm |
| **IA locale** | `llama-server` de llama.cpp + **Qwen2.5 1,5B Instruct Q4_K_M** | aucun | moteur ≈ 30 Mo (version publiée sur GitHub, SHA-256 de l'API) et modèle ≈ 1,1 Go (Hugging Face, SHA-256 du fichier LFS) |

Le modèle local est choisi parce qu'il parle correctement français (et les
trois autres langues), produit du JSON fiable, tient en 1,1 Go et répond en
quelques secondes sur un processeur ordinaire avec 4 Go de mémoire (licence
Apache 2.0). L'option IA locale est cachée sur un PC qui n'est pas `x86_64`
ou qui a moins de 3 Gio de mémoire.

## Règles

- **Tout est désactivé par défaut.** Tant qu'aucun assistant n'est activé,
  aucun bouton dans la barre, rien ne tourne, Windows+C ne fait rien. Rien
  n'est dans l'ISO.
- Chaque interrupteur ouvre une fenêtre qui télécharge ce qui manque, connecte
  le compte si besoin, puis ajoute **un seul** bouton à la barre (greffon
  lanceur xfce4-panel numéroté à partir de 30, la barre redémarre une fois).
  Plusieurs assistants activés : on choisit dans l'en-tête du panneau, le
  choix est retenu. Le bouton part quand le dernier est désactivé.
  `winlux-control align-left/center` garde le bouton.
- **Claude et ChatGPT n'ont aucun outil.** Claude tourne en `claude -p
  --tools "" --setting-sources "" --strict-mcp-config
  --no-session-persistence` ; Codex en `codex exec --ephemeral
  --ignore-user-config --sandbox read-only` dans un dossier vide, avec les
  commandes (`features.shell_tool`, `features.unified_exec`), la recherche web
  et les statistiques coupées. Une réponse de Codex qui contient malgré tout
  une commande, un fichier modifié, un outil MCP ou une recherche web est
  refusée. La conversation part par l'entrée standard, jamais en argument
  visible par `ps`.
- Pour lire le journal d'un programme, les plantages, la description du PC ou
  chercher un fichier, Claude et ChatGPT le *demandent* : le texte exact est
  montré et ne part qu'après « Envoyer à … ».
- **IA locale** : `llama-server` démarre quand le panneau s'ouvre et s'arrête à
  sa fermeture (et meurt avec le panneau, `PR_SET_PDEATHSIG`). Il n'écoute que
  `127.0.0.1`, sur un port libre, protégé par une clé tirée au hasard passée
  par l'environnement. Rien ne quitte le PC ; ce qu'elle lit est montré
  (« Lu sur ce PC, rien n'est envoyé »).
- **Trois agents au plus** pour l'IA locale. Un aiguilleur choisit l'agent ;
  chaque agent ne remplit que des champs fermés (schéma JSON imposé au modèle
  par la grammaire de llama.cpp), le code fait le reste :
  - *fichiers* : le terme cherché ou un dossier connu ; la recherche se fait
    sur le PC et chaque résultat propose « ouvrir l'emplacement » dans
    l'Explorateur ;
  - *réglages* : un réglage de la liste fermée de `winlux-control` ou une page
    des Paramètres ;
  - *aide* : réponse courte ; le journal d'un `.exe` nommé dans la question est
    lu sur le PC.
- **Pour les trois, chaque action** (réglage, dossier, page des Paramètres)
  vient d'une liste fermée, montre la commande exacte et attend « Faire ».
- **Connexions propres à l'assistant**, dans
  `~/.local/share/winlux/assistant-comptes/` (`claude/` passé à Claude Code
  par `CLAUDE_CONFIG_DIR`, `chatgpt/` passé à Codex par `CODEX_HOME`), remises
  en 700/600 à chaque lancement. `~/.claude` et `~/.codex` (Claude Code ou Codex
  installés à part par l'utilisateur) ne sont **jamais lus ni réutilisés** :
  l'assistant demande sa propre connexion. Les clés d'API éventuellement
  présentes dans l'environnement (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) sont
  retirées : seul le compte de l'utilisateur sert.
- **Langue** : l'assistant répond toujours dans la langue choisie dans
  Paramètres > Langue (`/etc/winlux/locale.conf`), quelle que soit la langue
  de la question ; toute l'interface suit la même langue (fr, en, es, de).

## Nom, marques et conditions d'utilisation

- Le produit s'appelle **Assistant IA** dans les quatre langues (« AI
  assistant », « Asistente de IA », « KI-Assistent ») et a sa propre icône
  (`winlux-assistant.svg`). Les noms Claude (marque d'Anthropic), ChatGPT et
  Codex (marques d'OpenAI) ne servent qu'à désigner le service que
  l'utilisateur choisit, jamais à nommer le produit ; Winlux n'est ni affilié
  à Anthropic ou OpenAI, ni approuvé par eux. L'icône de l'application Claude
  Code du menu Démarrer (`winlux-claude-code.svg`) est neutre (gris ardoise).
- **Codex** (OpenAI) : le programme est publié sous licence Apache 2.0. Winlux
  ne le redistribue pas : chaque utilisateur le télécharge depuis le registre
  npm au moment de l'activation. Son usage avec un compte ChatGPT relève des
  conditions d'utilisation d'OpenAI
  (<https://openai.com/policies/terms-of-use/>) et de ses politiques d'usage
  (<https://openai.com/policies/usage-policies/>) ; l'accès à Codex et ses
  limites dépendent de l'offre ChatGPT du compte. L'utilisateur accepte ces
  conditions lui-même, à la connexion, sur la page d'OpenAI.
- **Claude Code** (Anthropic) n'est pas un logiciel libre ; il est lui aussi
  téléchargé par l'utilisateur (voir [CLAUDE-CODE.md](CLAUDE-CODE.md)), et son
  usage avec un compte Claude relève des conditions d'Anthropic
  (<https://www.anthropic.com/legal/consumer-terms>) et de sa politique
  d'usage (<https://www.anthropic.com/legal/aup>).
- **IA locale** : llama.cpp est sous licence MIT, Qwen2.5 1,5B Instruct sous
  Apache 2.0 ; les deux sont téléchargés par l'utilisateur et ne sont pas
  dans l'ISO.

## Commandes

`winlux-assistant` ouvre le panneau (rien si aucun assistant n'est activé).
`--configurer`, `--activer`, `--desactiver`, `--deconnecter` suivis de
`claude`, `chatgpt` ou `local` ; `--supprimer local` libère la place du
modèle ; `--etat` donne l'état des trois en JSON.

## Fichiers et tests

`scripts/winlux-assistant.py` (installé en `/usr/bin/winlux-assistant`),
`integration/native/applications/winlux-assistant.desktop` (masquée du menu
Démarrer), icône `winlux-assistant.svg`, page 30 de
`scripts/winlux-settings-pages.py`. Session native seulement : le bureau PE de
l'ISO live ne l'affiche pas.

`scripts/test-assistant.py` (dans `scripts/test-all.sh` et la CI) : désactivé
par défaut, faux `claude` et faux `codex` qui notent leurs arguments, leur
entrée et leur environnement, refus d'une réponse de Codex qui utilise un
outil, faux registre npm, fausse API GitHub et faux Hugging Face qui servent
des fichiers altérés (refusés) puis bons, faux `llama-server` qui vérifie la
clé, les trois agents, la liste fermée des actions, les droits des
identifiants, et le bouton de la barre avec un xfconf à état.

Non vérifié ici : les vraies connexions `claude auth login` (à travers la
couche PE) et `codex login`, et le vrai modèle Qwen (Hugging Face et GitHub
sont fermés depuis l'environnement de construction) ; à tester sur la machine
de damien.
