# #1554 : GetDevice sur la vue contexte d'une cible 1.0 (Paint.NET).
step "Direct2D : GetDevice d'une cible fenêtre (#1554)" cargo test -q -p winlux -- old_target_answers_device_context6_with_a_view_on_itself
