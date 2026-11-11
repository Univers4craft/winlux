# #1654 : le crochet WH_CBT reçoit HCBT_CREATEWND avant WM_NCCREATE
# (MFC y attache ses fenêtres ; Pythonwin plantait dans mfc140u).
src = (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
ok("matches!(id, WH_GETMESSAGE | WH_CBT)" in src
   and src.find("cbt_create_window(hwnd, &mut cs)") < src.find("WM_NCCREATE,\n        0,\n        &cs"),
   "user32 : crochet WH_CBT appelé à la création, avant WM_NCCREATE (#1654)")
