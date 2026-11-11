# LookupAccountSid résout LOCAL SERVICE, NETWORK SERVICE, Guests, Performance Log Users ; CreateWellKnownSid 58 (#1639).
sec = (runtime_dlls / "security.rs").read_text(encoding="utf-8")
ok('"S-1-5-19" => ("LOCAL SERVICE"' in sec and '58 => "S-1-5-32-559"' in sec,
   "Sécurité : comptes bien connus résolus par LookupAccountSid (#1639)")
