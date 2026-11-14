# #2362 : System.Configuration.ConfigurationManager (GetSection,
# AppSettings, ConnectionStrings) dans notre mscorlib ; Corsair.Service.exe
# (iCUE) appelait ConfigurationManager.GetSection.
src = (ROOT / "bcl" / "mscorlib" / "Configuration.cs").read_text(encoding="utf-8")
ok("public static class ConfigurationManager" in src
   and "public static object GetSection(string sectionName)" in src
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Configuration.cs").read_text(encoding="utf-8"),
   "CLR : ConfigurationManager.GetSection, AppSettings, ConnectionStrings (#2362)")
