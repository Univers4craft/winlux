# RegNotifyChangeKeyValue (#1300) : réarmer ne crée plus de thread, et
# l'événement est signalé au changement suivant.
step "registre : surveillance sans thread par appel (#1300)" cargo test -q -p winlux -- surveillance_du_registre_sans_thread_par_appel
