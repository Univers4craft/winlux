# #3087 : le titre du Lecteur multimédia garde le nom du fichier en UTF-16
# (apostrophe ’ et tout caractère hors Latin-1), sans passer par les octets
# Windows-1252 du shell.
_m_3087 = (ROOT / "testexe/src/shell/mediaplayer.rs").read_text(encoding="utf-8")
ok("fn file_name_wide" in _m_3087 and "unsafe fn set_window_title(name: &[u16])" in _m_3087,
   "Lecteur multimédia : titre de fenêtre en UTF-16, ’ conservée (#3087)")
