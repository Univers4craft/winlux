# #3266 : au repos, le démon du pare-feu ne relit les fd de tous les
# processus que si une socket en écoute apparaît ou disparaît (repli une
# fois par minute). Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/firewall.rs").read_text(encoding="utf-8")
ok(
    "fn parcours_des_fd_seulement_si_les_sockets_changent_3266" in t,
    "pare-feu au repos : parcours des fd seulement si les sockets en écoute changent (#3266)",
)
ok(
    "fn proprietaire_d_une_socket_qui_fait_exec_ou_se_termine_3266" in t,
    "pare-feu au repos : exec ou fin d'un processus en écoute vus au tour suivant (#3266)",
)
