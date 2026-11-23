# user32 exporte RegisterShellHookWindow et DeregisterShellHookWindow
# (Learn) : Spotify les importe en différé et mourait en 0xC06D007F. La
# fenêtre inscrite reçoit les HSHELL_* de création, titre et destruction
# (#3330).
u32_3330 = (ROOT / "runtime/src/dlls/user32_more.rs").read_text(encoding="utf-8")
t32_3330 = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
ok('"RegisterShellHookWindow" => fn register_shell_hook_window' in u32_3330
   and '"DeregisterShellHookWindow" => fn deregister_shell_hook_window' in u32_3330
   and "fn crochet_du_shell_recoit_les_hshell_3330" in t32_3330,
   "user32 : crochet du shell servi et testé (#3330)")
