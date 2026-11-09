# Threads de réserve (#1361) : RegisterWaitForSingleObject, QueueUserWorkItem
# et minuteries rendent leur TEB et leurs blocs TLS en finissant.
step "Un thread de réserve libère son TEB (#1361)" cargo test -q -p winlux -- thread_de_reserve_libere_son_teb
