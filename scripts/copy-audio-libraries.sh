#!/bin/sh
# Bibliothèques chargées par dlopen : elles ne figurent pas dans ldd(winlux).
set -eu
stage=${1:?répertoire racine de destination requis}
case "$stage" in /*) ;; *) echo "destination absolue requise" >&2; exit 1 ;; esac
test "$stage" != / && test -d "$stage" || { echo "destination invalide" >&2; exit 1; }
cache=$(ldconfig -p)
for soname in libmpg123.so.0 libfaad.so.2 libasound.so.2; do
    library=$(printf '%s\n' "$cache" | awk -v name="$soname" '$1 == name && /x86-64/ { print $NF; exit }')
    test -n "$library" && test -r "$library" || {
        echo "bibliothèque audio requise absente : $soname (libmpg123-0, libfaad2, libasound2)" >&2
        exit 1
    }
    dependencies=$(ldd "$library")
    case "$dependencies" in *'not found'*) echo "dépendance audio absente : $library" >&2; exit 1 ;; esac
    {
        printf '%s\n' "$library"
        printf '%s\n' "$dependencies" | awk '$2 == "=>" && $3 ~ /^\// { print $3 } $1 ~ /^\// { print $1 }'
    } | sort -u | while IFS= read -r path; do
        mkdir -p "$stage$(dirname -- "$path")"
        cp -L "$path" "$stage$path"
    done
done

# Notices des paquets distribués (noms avec/sans transition t64).
for notice in /usr/share/doc/libmpg123*/copyright /usr/share/doc/libfaad2/copyright /usr/share/doc/libasound2*/copyright; do
    test -f "$notice" || continue
    mkdir -p "$stage$(dirname -- "$notice")"
    cp -L "$notice" "$stage$notice"
done
