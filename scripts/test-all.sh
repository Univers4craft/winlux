#!/bin/sh
# Suite complète de tests locaux en une seule commande :
#   sh scripts/test-all.sh            tout ce que la CI lance sur l'hôte
#   sh scripts/test-all.sh --rapide   sans compilation ni tests PE (quelques minutes)
#   sh scripts/test-all.sh site       seulement les étapes dont le nom contient « site »
#                                     (filtre combinable avec --rapide)
#
# Les étapes sont lues dans .github/workflows/ci.yml : une vérification ajoutée
# à la CI est aussi lancée ici, sans liste à tenir à jour. Les étapes qui
# demandent sudo ou un outil absent (xvfb-run, mingw) sont sautées et listées
# à la fin. Les tests en VM (ISO complète) restent ceux de release.yml.
set -u
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root" || exit 1
quick=0 filter=
for argument in "$@"; do
    case "$argument" in --rapide) quick=1 ;; *) filter=$argument ;; esac
done
logs=$(mktemp -d "${TMPDIR:-/tmp}/winlux-tests.XXXXXX")
passed=0 failed=0 skipped=0
failures= skips=

step() {
    name=$1; shift
    case "$name $*" in *"$filter"*) ;; *) return 0 ;; esac
    log="$logs/$(printf '%s' "$name" | tr -c 'A-Za-z0-9._-' '_').log"
    printf '%-60s' "$name"
    start=$(date +%s)
    if "$@" >"$log" 2>&1; then
        passed=$((passed + 1)); printf ' OK (%ss)\n' "$(( $(date +%s) - start ))"
    else
        failed=$((failed + 1)); failures="$failures
  - $name (journal : $log)"
        printf ' ÉCHEC\n'; tail -n 15 "$log" | sed 's/^/      /'
    fi
}
skip() { case "$1" in *"$filter"*) ;; *) return 0 ;; esac; skipped=$((skipped + 1)); skips="$skips
  - $1 ($2)"; }

if test "$quick" = 0 && test -z "$filter"; then
    step "Compilation du runtime" cargo build --release -p winlux
    step "Tests unitaires Rust (cargo test)" cargo test -p winlux
    if command -v cargo >/dev/null && test -f scripts/build-tests.sh; then
        step "Programmes Windows de test (build-tests)" bash scripts/build-tests.sh
    fi
fi

# Noms courts acceptés par le filtre (« contraste », « site »…) : le filtre
# porte sur le nom de l'étape et sa commande.
# Commandes « scripts/test-… » de la CI (ci.yml, et les tests d'hôte de
# release.yml), une par ligne ; la boucle des fenêtres surgissantes est dépliée.
grep -hE '^\s*(run: )?(sudo |xvfb-run [^/]*)?(/usr/bin/)?(python3|sh|bash|dbus-run-session)[^#]*(scripts/test-|installation/tests/test)[A-Za-z0-9_.-]+' \
    .github/workflows/ci.yml .github/workflows/release.yml .github/workflows/installation.yml |
    sed -E 's/^\s*(run: )?//' | awk '!seen[$0]++' |
    while IFS= read -r line; do
        case "$line" in
            *'"$popup"'*) for popup in start volume power calendar; do printf '%s\n' "$line" | sed "s/\"\$popup\"/$popup/"; done ;;
            *) printf '%s\n' "$line" ;;
        esac
    done >"$logs/steps.txt"
while IFS= read -r command; do
    script=$(printf '%s\n' "$command" | grep -oE '(scripts/test-|installation/tests/test)[A-Za-z0-9_-]+\.(py|sh)( [a-z]+$)?' | tail -n 1)
    case "$command" in
        sudo*) skip "$script" "demande sudo"; continue ;;
        *-vm.sh*|*qemu*|*vm-guest*) skip "$script" "VM et ISO complète (release.yml)"; continue ;;
        *xvfb-run*) command -v xvfb-run >/dev/null 2>&1 || { skip "$script" "xvfb-run absent"; continue; } ;;
    esac
    case "$script" in
        scripts/test-runtime.sh|scripts/test-runtime-parallel.sh|scripts/test-image-memory.py|scripts/test-loadlibrary-personal-cwd.sh|scripts/test-integration.sh|scripts/test-mingw.sh)
            if test "$quick" = 1; then skip "$script" "mode rapide"; continue; fi
            test -d target/x86_64-pc-windows-msvc/release || { skip "$script" "programmes PE non compilés"; continue; } ;;
    esac
    if test "$script" = scripts/test-mingw.sh; then
        command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 || { skip "$script" "MinGW-w64 absent"; continue; }
        step "scripts/build-mingw-tests.sh" sh scripts/build-mingw-tests.sh
    fi
    step "$script" sh -c "$command"
done <"$logs/steps.txt"

# Fautes matérielles en exceptions SEH et suspension de threads (#269) :
# lancés en CI par test-mingw.sh ;
# ici aussi sans MinGW, dès que clang et le runtime compilé sont là.
if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 && command -v clang >/dev/null 2>&1 && test -x target/release/winlux; then
    step "scripts/test-hw-exceptions.sh" sh scripts/test-hw-exceptions.sh
    step "scripts/test-io-cancel.sh" sh scripts/test-io-cancel.sh
    step "scripts/test-thread-control.sh" sh scripts/test-thread-control.sh
fi
# DuplicateHandle et mémoire entre processus (Chromium, Electron, #236) :
# lancé en CI par test-mingw.sh ; ici aussi dès que clang est là.
if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 && command -v clang >/dev/null 2>&1 && test -x target/release/winlux; then
    step "scripts/test-duplication.sh" sh scripts/test-duplication.sh
    step "scripts/test-webview2.sh" sh scripts/test-webview2.sh
fi

# Fenêtre encore connue pendant WM_DESTROY et WM_NCDESTROY (#1079, Paint.NET) :
# test unitaire ciblé, lancé aussi en mode rapide.
step "Fenêtre vivante pendant WM_DESTROY (#1079)" cargo test -q -p winlux la_fenetre_existe_encore_pendant_wm_destroy

# Diagnostic et boîte d'erreur de Paint.NET (#1112, #1113) : nom de volume
# d'un lecteur, chemin et attributs des DLL système intégrées.
step "Volume et DLL intégrées vues par Paint.NET (#1112, #1113)" cargo test -q -p winlux -- nom_de_volume_d_une_racine_de_lecteur dll_integree_a_un_chemin_et_un_fichier_visible

# Langue des applications wxWidgets (#1125) : « dossier\*.* » liste les
# dossiers sans point, locale par nom de la session, SetThread*UILanguage(s).
step "Langue des applications wxWidgets : *.* et locale de la session (#1125)" cargo test -q -p winlux -- etoile_point_etoile_liste_aussi_les_noms_sans_point locale_par_defaut_suit_la_langue_de_la_session langue_d_interface_du_thread_et_langues_preferees

# Programme Windows absent (#1133) : CreateProcess rend FALSE et
# ERROR_FILE_NOT_FOUND, sans runtime enfant qui écrit sur stderr.
step "CreateProcess d'un .exe absent (#1133)" cargo test -q -p winlux -- programme_windows_absent_echoue_sans_lancer_de_runtime

# AltGr (#1151) : Ctrl gauche + Alt droit comme sous Windows, sans quoi
# AltGr+3 (« # ») déclenche le raccourci Alt+3 de Notepad++.
step "AltGr vu comme Ctrl+Alt, caractères AZERTY tapés (#1151)" cargo test -q -p winlux -- altgr_est_ctrl_gauche_plus_alt_droit

# Sockets .NET (#1146) : port de complétion, ConnectEx, WSASend/WSARecv avec
# OVERLAPPED, vrais codes Winsock (jamais -1) par WSAGetOverlappedResult.
step "Sockets avec OVERLAPPED et ConnectEx (#1146)" cargo test -q -p winlux -- connectex_wsasend_wsarecv_terminent_par_le_port_de_completion connexion_refusee_rend_wsaeconnrefused_et_jamais_moins_un fermeture_et_cancelioex_annulent_les_receptions_en_attente wsagetlasterror_et_getlasterror_partagent_la_meme_valeur

# Reconnaissance de gestes de l'encre (#1132) : mshwgst.dll intégrée et clé
# TPG\System Recognizers lue par WPF (Éditeur de ScreenToGif, InkCanvas).
step "Gestes de l'encre pour WPF InkCanvas (#1132)" cargo test -q -p winlux -- reconnaissance_de_gestes_disponible_sans_geste_reconnu moteur_de_gestes_declare_pour_wpf

# Glisser-déposer depuis une liste ou une arborescence (#1138, FileZilla) :
# DragDetect, LVN_BEGINDRAG, TVN_BEGINDRAGW.
step "Glisser depuis une liste ou une arborescence (#1138)" cargo test -q -p winlux -- glisser_depuis_une_liste_ou_une_arborescence perte_du_focus_garde_les_boutons_de_souris

# Utilisation du MTA par jetons (#1147, Flow Launcher, CsWinRT .NET) :
# CoIncrementMTAUsage / CoDecrementMTAUsage, aussi via combase et l'API set.
step "CoIncrementMTAUsage / CoDecrementMTAUsage (#1147)" cargo test -q -p winlux -- utilisation_du_mta_par_jetons api_sets_reutilisent_leurs_veritables_exports

# Direct3D 12 : shaders DXBC traduits sur OpenGL, signature racine et
# tampons de constantes (#1177). Le rendu demande un affichage : xvfb-run.
step "Direct3D 12 : signature racine et tampons de constantes (#1177)" cargo test -q -p winlux -- root_signature_round_trip root_arguments_become_constant_buffers echantillonneurs_des_tables_et_statiques
if command -v xvfb-run >/dev/null 2>&1; then
    step "Direct3D 12 : shaders traduits sur OpenGL (#1177)" xvfb-run -a cargo test -q -p winlux -- shaders_traduits_et_constantes_racine_opengl texture_et_echantillonneur_statique_opengl
fi

# Barre de titre dessinée par l'appli (#1131) : WM_NCCALCSIZE traité sans
# DefWindowProc retire le cadre du gestionnaire (WindowChrome de WPF).
step "Cadre retiré pour une barre de titre maison (#1131)" cargo test -q -p winlux -- wm_nccalcsize_traite_par_l_appli_retire_le_cadre
# Liste des processus pour .NET (#1162) : NtQuerySystemInformation 5 et 88,
# d'où Process.GetProcesses et le contrôle d'instance unique.
step "Liste des processus pour .NET (#1162)" cargo test -q -p winlux -- liste_des_processus_comme_la_lit_dotnet

# HTTPS des applications .NET (#1183) : normaliz.dll, IdnToAscii et
# IdnToUnicode (RFC 3490 / 3492) pour le nom d'hôte de SslStream.
step "normaliz.dll pour le HTTPS .NET (#1183)" cargo test -q -p winlux -- noms_de_domaine_pour_le_tls_de_dotnet decodage_punycode_exemples_de_la_rfc_3492

# Accent violet Prisme par défaut (#343), pas encore dans ci.yml.
step "scripts/test-avahi-printers.py" python3 -B scripts/test-avahi-printers.py
step "scripts/test-control-status-cost.py" python3 -B scripts/test-control-status-cost.py
step "scripts/test-notices.py" python3 -B scripts/test-notices.py
step "scripts/test-native-catalogue.py (raccourcis du menu, #992)" python3 -B scripts/test-native-catalogue.py
step "scripts/test-update-signature.py" python3 -B scripts/test-update-signature.py
step "scripts/test-ab-update.py (mises à jour A/B)" python3 -B scripts/test-ab-update.py

# Applis WinForms et WPF .NET autonomes (#664–#666, #268) : télécharge une fois le runtime
# .NET 9 de Paint.NET portable (132 Mo), donc pas en mode rapide.
if test "$quick" = 0; then
    step "scripts/test-winforms-bonjour.sh" sh scripts/test-winforms-bonjour.sh
    step "scripts/test-wpf-bonjour.sh" sh scripts/test-wpf-bonjour.sh
fi

# Site web et wiki (site.yml) : rendu Chromium sauté sans Node.js ni
# Playwright, sauf WINLUX_SITE_RENDER=required.
step "scripts/test-site.py (site web)" python3 -B scripts/test-site.py --render
step "wiki : liens entre pages" python3 -B -c '
import pathlib, re, sys
pages = {p.stem for p in pathlib.Path("wiki").glob("*.md")}
bad = 0
for page in sorted(pathlib.Path("wiki").glob("*.md")):
    text = re.sub(r"`[^`]*`", "", page.read_text(encoding="utf-8"))
    for target in re.findall(r"\]\(([A-Za-z0-9_-]+)(?:#[^)]*)?\)", text):
        if target not in pages:
            print(f"{page}: lien vers une page absente : {target}")
            bad = 1
sys.exit(bad)
'

total=$((passed + failed))
echo
echo "Résultat : $passed réussis, $failed échoués sur $total, $skipped sautés."
test -z "$skips" || printf 'Sautés :%s\n' "$skips"
test -z "$failures" || printf 'Échecs :%s\n' "$failures"
test "$failed" = 0
