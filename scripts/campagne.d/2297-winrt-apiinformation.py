# #2297 : ApiInformation sert IApiInformationStatics, d'après ce que Winlux sert.
# Le comportement est gardé par le test unitaire nommé.
ai = (runtime_dlls / "winrt_apiinfo.rs").read_text(encoding="utf-8")
ok("fn api_information_2297" in ai, "test d'ApiInformation (#2297)")
