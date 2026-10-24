#!/bin/sh
set -eu

repository=${WINLUX_UPDATE_REPOSITORY:-Univers4craft/winlux}
target_root=${WINLUX_UPDATE_ROOT:-/}
manifest_source=${WINLUX_UPDATE_MANIFEST:-https://github.com/$repository/releases/latest/download/winlux-update-manifest.txt}
update_token=${WINLUX_UPDATE_TOKEN:-${GH_TOKEN:-}}
state_dir="$target_root/var/lib/winlux"
backup_dir="$state_dir/update-backup"

usage() {
    echo "usage: winlux-update --check | --apply | --rollback"
}

download() {
    source=$1
    destination=$2
    accept=${3:-application/octet-stream}
    case "$source" in
        file://*) cp "${source#file://}" "$destination" ;;
        https://github.com/*|https://objects.githubusercontent.com/*|https://api.github.com/*)
            command -v curl >/dev/null 2>&1 || { echo "curl est requis" >&2; exit 1; }
            set -- curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
                --header "Accept: $accept" --header "X-GitHub-Api-Version: 2022-11-28" \
                --output "$destination"
            if test -n "$update_token"; then
                set -- "$@" --header "Authorization: Bearer $update_token"
            fi
            if ! "$@" "$source"; then
                echo "téléchargement GitHub impossible; pour un dépôt privé, définissez WINLUX_UPDATE_TOKEN" >&2
                exit 1
            fi
            ;;
        *) echo "source de mise à jour refusée : $source" >&2; exit 1 ;;
    esac
}

asset_api_url() {
    name=$1
    metadata=$2
    tr -d '\n\r' <"$metadata" |
        sed -n "s#.*\"url\"[[:space:]]*:[[:space:]]*\"\\(https://api.github.com/repos/$repository/releases/assets/[0-9][0-9]*\\)\"[^{}]*\"name\"[[:space:]]*:[[:space:]]*\"$name\".*#\\1#p"
}

manifest_value() {
    key=$1
    file=$2
    value=$(sed -n "s/^$key=//p" "$file")
    test "$(printf '%s\n' "$value" | wc -l)" -eq 1 || return 1
    printf '%s' "$value"
}

fetch_manifest() {
    manifest=$1
    if test -n "$update_token" && test -z "${WINLUX_UPDATE_MANIFEST:-}"; then
        release_metadata="$work/release.json"
        download "https://api.github.com/repos/$repository/releases/latest" "$release_metadata" "application/vnd.github+json"
        manifest_api=$(asset_api_url winlux-update-manifest.txt "$release_metadata")
        test -n "$manifest_api" || { echo "manifeste absent de la dernière release" >&2; exit 1; }
        download "$manifest_api" "$manifest"
    else
        download "$manifest_source" "$manifest"
    fi
    test "$(manifest_value format "$manifest")" = 1 || { echo "format de manifeste inconnu" >&2; exit 1; }
    version=$(manifest_value version "$manifest")
    asset=$(manifest_value asset "$manifest")
    checksum=$(manifest_value sha256 "$manifest")
    case "$version" in *[!A-Za-z0-9._-]*|'') echo "version de manifeste invalide" >&2; exit 1 ;; esac
    case "$asset" in *[!A-Za-z0-9._-]*|'') echo "nom d'actif invalide" >&2; exit 1 ;; esac
    case "$checksum" in *[!0-9a-f]*|'') echo "somme SHA-256 invalide" >&2; exit 1 ;; esac
    test "${#checksum}" -eq 64 || { echo "somme SHA-256 invalide" >&2; exit 1; }
}

current_version() {
    if test -f "$target_root/usr/share/winlux/version"; then
        sed -n '1p' "$target_root/usr/share/winlux/version"
    else
        echo none
    fi
}

mode=${1:-}
case "$mode" in
    --check|--apply|--rollback) ;;
    *) usage >&2; exit 2 ;;
esac

if test "$mode" = --rollback; then
    test -d "$backup_dir" || { echo "aucune sauvegarde Winlux" >&2; exit 1; }
    test -f "$backup_dir/usr/bin/winlux" || { echo "sauvegarde incomplète" >&2; exit 1; }
    install -Dm755 "$backup_dir/usr/bin/winlux" "$target_root/usr/bin/winlux.rollback"
    mv "$target_root/usr/bin/winlux.rollback" "$target_root/usr/bin/winlux"
    if test -f "$backup_dir/usr/bin/winlux-update"; then
        install -Dm755 "$backup_dir/usr/bin/winlux-update" "$target_root/usr/bin/winlux-update.rollback"
        mv "$target_root/usr/bin/winlux-update.rollback" "$target_root/usr/bin/winlux-update"
    fi
    install -Dm644 "$backup_dir/usr/share/winlux/version" "$target_root/usr/share/winlux/version.rollback"
    mv "$target_root/usr/share/winlux/version.rollback" "$target_root/usr/share/winlux/version"
    echo "Retour à Winlux $(current_version) effectué."
    exit 0
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-update.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
manifest="$work/manifest.txt"
fetch_manifest "$manifest"
installed=$(current_version)
echo "Version installée : $installed"
echo "Version disponible : $version"
if test "$installed" = "$version"; then
    echo "Winlux est à jour."
    exit 0
fi
test "$mode" = --apply || exit 0

if test -n "${WINLUX_UPDATE_ASSET_DIR:-}"; then
    asset_source="file://$WINLUX_UPDATE_ASSET_DIR/$asset"
elif test -n "$update_token" && test -n "${release_metadata:-}"; then
    asset_source=$(asset_api_url "$asset" "$release_metadata")
    test -n "$asset_source" || { echo "actif $asset absent de la dernière release" >&2; exit 1; }
else
    asset_source="https://github.com/$repository/releases/download/$version/$asset"
fi
bundle="$work/$asset"
download "$asset_source" "$bundle"
actual=$(sha256sum "$bundle" | cut -d' ' -f1)
test "$actual" = "$checksum" || { echo "échec de la vérification SHA-256" >&2; exit 1; }

tar -tzf "$bundle" >"$work/files.txt"
while IFS= read -r entry; do
    case "$entry" in
        /*|*../*|../*|*'/..') echo "chemin dangereux dans le paquet : $entry" >&2; exit 1 ;;
    esac
done <"$work/files.txt"
mkdir -p "$work/payload"
tar -xzf "$bundle" -C "$work/payload"
test -x "$work/payload/usr/bin/winlux" || { echo "runtime absent du paquet" >&2; exit 1; }
test -x "$work/payload/usr/bin/winlux-update" || { echo "outil de mise à jour absent du paquet" >&2; exit 1; }
test -f "$work/payload/usr/share/winlux/version" || { echo "version absente du paquet" >&2; exit 1; }
test "$(sed -n '1p' "$work/payload/usr/share/winlux/version")" = "$version" || { echo "version du paquet incohérente" >&2; exit 1; }

mkdir -p "$backup_dir/usr/bin" "$backup_dir/usr/share/winlux" "$target_root/usr/bin" "$target_root/usr/share/winlux"
if test -f "$target_root/usr/bin/winlux"; then cp "$target_root/usr/bin/winlux" "$backup_dir/usr/bin/winlux"; fi
if test -f "$target_root/usr/bin/winlux-update"; then cp "$target_root/usr/bin/winlux-update" "$backup_dir/usr/bin/winlux-update"; fi
if test -f "$target_root/usr/share/winlux/version"; then cp "$target_root/usr/share/winlux/version" "$backup_dir/usr/share/winlux/version"; else printf 'none\n' >"$backup_dir/usr/share/winlux/version"; fi

install -Dm755 "$work/payload/usr/bin/winlux" "$target_root/usr/bin/winlux.new"
install -Dm755 "$work/payload/usr/bin/winlux-update" "$target_root/usr/bin/winlux-update.new"
install -Dm644 "$work/payload/usr/share/winlux/version" "$target_root/usr/share/winlux/version.new"
mv "$target_root/usr/bin/winlux.new" "$target_root/usr/bin/winlux"
mv "$target_root/usr/bin/winlux-update.new" "$target_root/usr/bin/winlux-update"
mv "$target_root/usr/share/winlux/version.new" "$target_root/usr/share/winlux/version"
echo "Winlux $version installé. Une sauvegarde restaurable est disponible avec --rollback."
