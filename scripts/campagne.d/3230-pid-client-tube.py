# #3230 : GetNamedPipeClientProcessId rend le pid du processus client
# (SO_PEERCRED), sans quoi crashpad refuse chaque enfant de Chromium.
# Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/dlls/kernel32_node.rs").read_text(encoding="utf-8")
ok(
    "fn pid_du_client_d_un_tube_venu_d_un_autre_processus_3230" in t,
    "GetNamedPipeClientProcessId rend le pid du client d'un autre processus (#3230)",
)
