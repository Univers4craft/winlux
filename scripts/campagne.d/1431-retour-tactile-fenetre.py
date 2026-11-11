# #1431 : SetWindowFeedbackSetting et GetWindowFeedbackSetting (Paint.NET).
src = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
ok('"SetWindowFeedbackSetting" => fn SetWindowFeedbackSetting' in src
   and '"GetWindowFeedbackSetting" => fn GetWindowFeedbackSetting' in src,
   "user32 : réglages de retour tactile par fenêtre (#1431)")
