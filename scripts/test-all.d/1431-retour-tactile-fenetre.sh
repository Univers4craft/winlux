# #1431 : Paint.NET coupe les retours tactiles de son canevas.
step "user32 : SetWindowFeedbackSetting et GetWindowFeedbackSetting (#1431)" cargo test -q -p winlux -- retour_tactile_par_fenetre
