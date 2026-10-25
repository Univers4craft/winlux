#!/bin/sh
# Métadonnées de construction, sans information personnelle ni jeton.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=${1:-$(cat "$root/VERSION")}
scope=${2:-system}
case "$version" in ''|*[!A-Za-z0-9._+-]*) echo 'version invalide' >&2; exit 1 ;; esac
case "$scope" in system|runtime) ;; *) echo 'périmètre invalide' >&2; exit 1 ;; esac
commit=unknown
state=unknown
if git -C "$root" rev-parse --verify HEAD >/dev/null 2>&1; then
    commit=$(git -C "$root" rev-parse HEAD)
    state=clean
    git -C "$root" diff --quiet HEAD -- || state=modified
fi
if test -n "${SOURCE_DATE_EPOCH:-}"; then
    built_at=$(date -u -d "@$SOURCE_DATE_EPOCH" '+%Y-%m-%dT%H:%M:%SZ')
else
    built_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
fi
printf 'version=%s\ncommit=%s\ntracked_sources=%s\nbuilt_at_utc=%s\nscope=%s\n' \
    "$version" "$commit" "$state" "$built_at" "$scope"
