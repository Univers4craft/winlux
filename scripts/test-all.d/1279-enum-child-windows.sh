# EnumChildWindows (#1279) : chaque descendant est énuméré, et une invalidation
# n'est plus perdue après ValidateRect (accueil de Sumatra PDF).
step "EnumChildWindows parcourt les descendants (#1279)" cargo test -q -p winlux -- enum_child_windows_parcourt_tous_les_descendants
step "Invalidation après ValidateRect repeinte (#1279)" cargo test -q -p winlux -- invalidation_apres_validation_repeint
