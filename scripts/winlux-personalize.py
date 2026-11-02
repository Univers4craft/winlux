#!/usr/bin/python3
"""Persist appearance choices and apply them to GTK, Openbox and XFCE.

    winlux-personalize theme|accent|wallpaper|panel-size|start-icon VALEUR
    winlux-personalize reset default
    winlux-personalize export FICHIER.wlxtheme   thème complet en un fichier
    winlux-personalize import FICHIER.wlxtheme   applique un thème partagé

Un thème Winlux (.wlxtheme) est une archive zip : theme.json (mode de couleur,
couleur d'accent, taille de la barre, sons système) et l'image du fond.
"""
import json
import configparser
from pathlib import Path
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
import os


# ── Langue de l'interface (Paramètres > Langue) : fr, en, es ou de ──────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    'Thème inconnu': ('Unknown theme', 'Tema desconocido', 'Unbekanntes Design'),
    'Couleur invalide': ('Invalid color', 'Color no válido', 'Ungültige Farbe'),
    'Image invalide': ('Invalid image', 'Imagen no válida', 'Ungültiges Bild'),
    'Format d’image non reconnu': ('Image format not recognized', 'Formato de imagen no reconocido', 'Bildformat nicht erkannt'),
    'Taille invalide': ('Invalid size', 'Tamaño no válido', 'Ungültige Größe'),
    'Réglage inconnu': ('Unknown setting', 'Ajuste desconocido', 'Unbekannte Einstellung'),
    'Image trop volumineuse': ('Image too large', 'Imagen demasiado grande', 'Bild zu groß'),
    'Le service de fond d’écran n’est pas initialisé': ('The wallpaper service is not initialized', 'El servicio de fondo de pantalla no está inicializado', 'Der Hintergrunddienst ist nicht initialisiert'),
    'Préférence appliquée. Rouvrez les applications pour actualiser leur thème.': ('Preference applied. Reopen your apps to refresh their theme.', 'Preferencia aplicada. Vuelva a abrir las aplicaciones para actualizar su tema.', 'Einstellung übernommen. Öffnen Sie die Apps erneut, um ihr Design zu aktualisieren.'),
    'Fichier de thème invalide': ('Invalid theme file', 'Archivo de tema no válido', 'Ungültige Designdatei'),
    'Thème enregistré.': ('Theme saved.', 'Tema guardado.', 'Design gespeichert.'),
    'Thème appliqué.': ('Theme applied.', 'Tema aplicado.', 'Design übernommen.'),
    'usage : winlux-personalize theme|accent|wallpaper|panel-size VALEUR': ('usage: winlux-personalize theme|accent|wallpaper|panel-size VALUE', 'uso: winlux-personalize theme|accent|wallpaper|panel-size VALOR', 'Verwendung: winlux-personalize theme|accent|wallpaper|panel-size WERT'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


HOME=Path.home()
PREF=HOME/".config/winlux/appearance.json"

def text_on(color):
    """Texte noir ou blanc, celui qui se lit le mieux sur cette couleur (WCAG)."""
    def channel(value):
        value/=255
        return value/12.92 if value<=0.03928 else ((value+0.055)/1.055)**2.4
    r,g,b=(int(color[i:i+2],16) for i in (1,3,5))
    lum=0.2126*channel(r)+0.7152*channel(g)+0.0722*channel(b)
    return "#000000" if (lum+0.05)/0.05>=1.05/(lum+0.05) else "#ffffff"

def appearance_css(dark,accent):
    """gtk.css commun à toutes les applications GTK (priorité « utilisateur »)."""
    bg,fg,surface=("#181b24","#ecedf5","#242a38") if dark else ("#f3f5fa","#202430","#ffffff")
    # Pas de « window box » : les fenêtres qui ont leurs propres couleurs (UAC,
    # autorisations, signalement) gardaient des boîtes repeintes sous leur texte.
    on_accent=text_on(accent)
    return f"window, list, viewport {{background-color:{bg}; color:{fg};}} #sidebar, #sidebar list {{background-color:{surface}; color:{fg};}} #sidebar row:selected, #sidebar row:selected label, selection {{background-color:{accent}; color:{on_accent};}} button {{background:{surface}; color:{fg}; border-color:{accent};}} entry {{background:{surface}; color:{fg};}} .card {{background:{surface};color:{fg};}}"

def apply(kind,value):
    try: state=json.loads(PREF.read_text())
    except (OSError,ValueError): state={"theme":"dark","accent":"#8178b7"}
    if kind=="theme" and value not in ("dark","light"): raise ValueError(_t("Thème inconnu"))
    if kind=="accent" and not re.fullmatch(r"#[0-9a-fA-F]{6}",value): raise ValueError(_t("Couleur invalide"))
    if kind in ("wallpaper", "start-icon") and not (kind == "start-icon" and value == "default"):
        path=Path(value).resolve(strict=True)
        if not path.is_file(): raise ValueError(_t("Image invalide"))
        from gi.repository import GdkPixbuf
        if not GdkPixbuf.Pixbuf.get_file_info(str(path)): raise ValueError(_t("Format d’image non reconnu"))
        value=str(path)
    if kind=="panel-size" and value not in ("32","40","48","56","64"): raise ValueError(_t("Taille invalide"))
    if kind not in ("theme","accent","wallpaper","panel-size","start-icon"): raise ValueError(_t("Réglage inconnu"))
    def query(channel,prop,value,typ):
        args=["xfconf-query","-c",channel,"-p",prop]
        exists=subprocess.run(args,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0
        subprocess.run(args+([] if exists else ["-n","-t",typ])+["-s",value],check=True)
    if kind=="start-icon":
        folder=HOME/".local/share/winlux/start"
        folder.mkdir(parents=True,exist_ok=True)
        icon="/usr/share/winlux/native/start.svg"
        if value != "default":
            image=Path(value)
            if image.stat().st_size > 20 * 1024 * 1024: raise ValueError(_t("Image trop volumineuse"))
            suffix=image.suffix.lower() if re.fullmatch(r"\.[a-z0-9]{1,8}", image.suffix.lower()) else ".img"
            icon=str(folder/("icon"+suffix))
            if image != Path(icon): shutil.copyfile(image,icon)
        launcher=folder/"winlux-start.desktop"
        launcher.write_text("[Desktop Entry]\nType=Application\nName=Démarrer\nExec=/usr/bin/winlux-start-menu\nIcon="+icon+"\nTerminal=false\n")
        subprocess.run(["xfconf-query","-c","xfce4-panel","-p","/plugins/plugin-1/items",
                        "-a","-t","string","-s",str(launcher)],check=True)
    elif kind=="wallpaper":
        props=subprocess.check_output(["xfconf-query","-c","xfce4-desktop","-l"],text=True).splitlines()
        targets=[p for p in props if p.endswith("/last-image")]
        if not targets: raise ValueError(_t("Le service de fond d’écran n’est pas initialisé"))
        for prop in targets: query("xfce4-desktop",prop,value,"string")
    elif kind=="panel-size": query("xfce4-panel","/panels/panel-1/size",value,"uint")
    state[kind]=value
    PREF.parent.mkdir(parents=True,exist_ok=True)
    temp=PREF.with_suffix(".tmp");temp.write_text(json.dumps(state));temp.replace(PREF)
    dark=state.get("theme","dark")=="dark"
    accent=state.get("accent","#8178b7")
    css=appearance_css(dark,accent)
    style=HOME/".config/gtk-3.0/gtk.css";style.parent.mkdir(parents=True,exist_ok=True)
    marker="/* Winlux appearance */"
    old=style.read_text() if style.exists() else ""
    end_marker="/* End Winlux appearance */"
    old=re.sub(re.escape(marker)+r".*?"+re.escape(end_marker),"",old,flags=re.S)
    style.write_text(old+"\n"+marker+"\n"+css+"\n"+end_marker+"\n")
    gtk_preferences=HOME/".config/gtk-3.0/settings.ini"
    gtk_config=configparser.ConfigParser(interpolation=None)
    gtk_config.optionxform=str
    gtk_config.read(gtk_preferences)
    if not gtk_config.has_section("Settings"):gtk_config.add_section("Settings")
    gtk_config.set("Settings","gtk-application-prefer-dark-theme","1" if dark else "0")
    # « Adwaita-dark » n'existe pas comme thème GTK 3 installé : GTK retombait
    # sur Adwaita clair. Le sombre vient de gtk-application-prefer-dark-theme.
    gtk_config.set("Settings","gtk-theme-name","Adwaita")
    with gtk_preferences.open("w") as stream:gtk_config.write(stream)
    query("xsettings","/Net/ThemeName","Adwaita","string")
    source=Path("/usr/share/themes/Winlux/openbox-3/themerc")
    if source.is_file():
        replacements={"window.active.title.bg.color":surface,"window.active.client.color":surface,
                      "window.active.border.color":accent,"window.active.label.text.color":fg,
                      "window.inactive.title.bg.color":bg,"window.inactive.label.text.color":fg}
        lines=[]
        for line in source.read_text().splitlines():
            key=line.split(":",1)[0]
            lines.append(f"{key}: {replacements[key]}" if key in replacements else line)
        target=HOME/".local/share/themes/Winlux-user/openbox-3/themerc"
        target.parent.mkdir(parents=True,exist_ok=True);target.write_text("\n".join(lines)+"\n")
        rc=HOME/".config/openbox/rc.xml"
        if rc.exists():
            ns="http://openbox.org/3.4/rc";ET.register_namespace("",ns)
            tree=ET.parse(rc);name=tree.getroot().find(f"{{{ns}}}theme/{{{ns}}}name")
            if name is not None:
                name.text="Winlux-user";tree.write(rc,encoding="utf-8",xml_declaration=True)
                subprocess.run(["openbox","--reconfigure"],check=True)
    print(_t("Préférence appliquée. Rouvrez les applications pour actualiser leur thème."))

# ── Thèmes partageables (.wlxtheme) ─────────────────────────────────────────

THEME_FORMAT = "winlux-theme-1"
THEME_MAX = 25 * 1024 * 1024
SOUNDS_OFF = HOME / ".config/winlux/system-sounds-off"
WALL_SUFFIXES = (".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif")


def theme_settings(state, sounds):
    """Réglages d'un thème, validés (valeurs inconnues ignorées)."""
    out = {"format": THEME_FORMAT}
    if state.get("theme") in ("dark", "light"): out["theme"] = state["theme"]
    if re.fullmatch(r"#[0-9a-fA-F]{6}", str(state.get("accent", ""))): out["accent"] = state["accent"].lower()
    if str(state.get("panel-size", "")) in ("32", "40", "48", "56", "64"): out["panel-size"] = str(state["panel-size"])
    if isinstance(sounds, bool): out["sounds"] = sounds
    return out


def export_theme(state, target, sounds=True):
    import zipfile
    settings = theme_settings(state, sounds)
    settings["name"] = Path(target).stem[:64]
    wallpaper = Path(state.get("wallpaper") or "")
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as archive:
        if wallpaper.is_file() and wallpaper.suffix.lower() in WALL_SUFFIXES and wallpaper.stat().st_size <= THEME_MAX - 65536:
            settings["wallpaper"] = "wallpaper" + wallpaper.suffix.lower()
            archive.write(wallpaper, settings["wallpaper"])
        archive.writestr("theme.json", json.dumps(settings, ensure_ascii=False, indent=1))
    return settings


def read_theme(source):
    """(réglages, image du fond ou None, extension) d'un .wlxtheme ; ValueError s'il est invalide."""
    import zipfile
    try:
        archive = zipfile.ZipFile(source)
    except (OSError, zipfile.BadZipFile):
        raise ValueError(_t("Fichier de thème invalide"))
    with archive:
        members = {info.filename: info for info in archive.infolist()}
        if "theme.json" not in members or sum(i.file_size for i in members.values()) > THEME_MAX:
            raise ValueError(_t("Fichier de thème invalide"))
        if any(name != "theme.json" and not re.fullmatch(r"wallpaper\.[a-z]{3,4}", name) for name in members):
            raise ValueError(_t("Fichier de thème invalide"))
        try: raw = json.loads(archive.read("theme.json").decode("utf-8"))
        except (ValueError, UnicodeDecodeError): raise ValueError(_t("Fichier de thème invalide"))
        if not isinstance(raw, dict) or raw.get("format") != THEME_FORMAT:
            raise ValueError(_t("Fichier de thème invalide"))
        settings = theme_settings(raw, raw.get("sounds"))
        settings["name"] = re.sub(r"[^\w .-]", "", str(raw.get("name", "")))[:64].strip() or "theme"
        name = raw.get("wallpaper")
        if name in members and Path(name).suffix in WALL_SUFFIXES:
            return settings, archive.read(name), Path(name).suffix
    return settings, None, ""


def import_theme(source):
    settings, picture, suffix = read_theme(source)
    if picture:
        folder = HOME / ".local/share/winlux/themes" / settings["name"]
        folder.mkdir(parents=True, exist_ok=True)
        target = folder / ("wallpaper" + suffix)
        target.write_bytes(picture)
        apply("wallpaper", str(target))
    for kind in ("theme", "accent", "panel-size"):
        if kind in settings: apply(kind, settings[kind])
    if settings.get("sounds") is True:
        try: SOUNDS_OFF.unlink()
        except OSError: pass
    elif settings.get("sounds") is False:
        SOUNDS_OFF.parent.mkdir(parents=True, exist_ok=True)
        SOUNDS_OFF.touch()
    return settings


if __name__=="__main__":
    if len(sys.argv) == 3 and sys.argv[1] in ("export", "import"):
        try:
            if sys.argv[1] == "export":
                try: current = json.loads(PREF.read_text())
                except (OSError, ValueError): current = {"theme": "dark", "accent": "#8178b7"}
                export_theme(current, sys.argv[2], not SOUNDS_OFF.exists())
                print(_t("Thème enregistré."))
            else:
                import_theme(sys.argv[2])
                print(_t("Thème appliqué."))
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            print(str(error), file=sys.stderr)
            raise SystemExit(1)
        raise SystemExit(0)
    try:
        if len(sys.argv)!=3: raise ValueError(_t("usage : winlux-personalize theme|accent|wallpaper|panel-size VALEUR"))
        if sys.argv[1] == "reset" and sys.argv[2] == "default":
            for kind, value in (("theme", "dark"), ("accent", "#8178b7"), ("panel-size", "40"),
                                ("start-icon", "default"), ("wallpaper", "/usr/share/winlux/wallpaper.bmp")):
                apply(kind, value)
        else: apply(sys.argv[1],sys.argv[2])
    except (OSError,ValueError,subprocess.SubprocessError) as error:
        print(str(error),file=sys.stderr);raise SystemExit(1)
