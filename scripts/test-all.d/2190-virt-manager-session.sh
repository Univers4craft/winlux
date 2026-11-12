# #2190 : la surcharge GSettings de virt-manager est acceptée par glib-compile-schemas.
if command -v glib-compile-schemas >/dev/null; then
    step "Surcharge GSettings de virt-manager valide (#2190)" sh -c 'd=$(mktemp -d) && cp integration/native/glib-schemas/*.gschema.override "$d"/ && glib-compile-schemas --strict --dry-run "$d"; r=$?; rm -rf "$d"; exit $r'
else
    skip "Surcharge GSettings de virt-manager valide (#2190)" "glib-compile-schemas absent"
fi
