# #2681 : CoreApplication sert sa fabrique, ICoreApplication et Properties.
# Le comportement est gardé par le test unitaire nommé.
ca = (runtime_dlls / "winrt_coreapp.rs").read_text(encoding="utf-8")
ok("fn core_application_proprietes_2681" in ca, "test de CoreApplication.Properties (#2681)")
