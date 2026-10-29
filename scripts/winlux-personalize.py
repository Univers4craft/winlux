#!/usr/bin/python3
"""Persist appearance choices and apply them to GTK, Openbox and XFCE."""
import json
import configparser
from pathlib import Path
import re
import subprocess
import sys
import xml.etree.ElementTree as ET

HOME=Path.home()
PREF=HOME/".config/winlux/appearance.json"

def apply(kind,value):
    try: state=json.loads(PREF.read_text())
    except (OSError,ValueError): state={"theme":"dark","accent":"#8178b7"}
    if kind=="theme" and value not in ("dark","light"): raise ValueError("Thème inconnu")
    if kind=="accent" and not re.fullmatch(r"#[0-9a-fA-F]{6}",value): raise ValueError("Couleur invalide")
    if kind=="wallpaper":
        path=Path(value).resolve(strict=True)
        if not path.is_file(): raise ValueError("Image invalide")
        from gi.repository import GdkPixbuf
        if not GdkPixbuf.Pixbuf.get_file_info(str(path)): raise ValueError("Format d’image non reconnu")
        value=str(path)
    if kind=="panel-size" and value not in ("32","40","48","56","64"): raise ValueError("Taille invalide")
    if kind not in ("theme","accent","wallpaper","panel-size"): raise ValueError("Réglage inconnu")
    def query(channel,prop,value,typ):
        args=["xfconf-query","-c",channel,"-p",prop]
        exists=subprocess.run(args,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0
        subprocess.run(args+([] if exists else ["-n","-t",typ])+["-s",value],check=True)
    if kind=="wallpaper":
        props=subprocess.check_output(["xfconf-query","-c","xfce4-desktop","-l"],text=True).splitlines()
        targets=[p for p in props if p.endswith("/last-image")]
        if not targets: raise ValueError("Le service de fond d’écran n’est pas initialisé")
        for prop in targets: query("xfce4-desktop",prop,value,"string")
    elif kind=="panel-size": query("xfce4-panel","/panels/panel-1/size",value,"uint")
    state[kind]=value
    PREF.parent.mkdir(parents=True,exist_ok=True)
    temp=PREF.with_suffix(".tmp");temp.write_text(json.dumps(state));temp.replace(PREF)
    dark=state.get("theme","dark")=="dark"
    accent=state.get("accent","#8178b7")
    bg,fg,surface=("#181b24","#ecedf5","#242a38") if dark else ("#f3f5fa","#202430","#ffffff")
    css=f"window, window box, list, viewport {{background-color:{bg}; color:{fg};}} #sidebar, #sidebar list {{background-color:{surface}; color:{fg};}} #sidebar row:selected, selection {{background-color:{accent};}} button {{background:{surface}; color:{fg}; border-color:{accent};}} entry {{background:{surface}; color:{fg};}} .card {{background:{surface};color:{fg};}}"
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
    gtk_config.set("Settings","gtk-theme-name","Adwaita-dark" if dark else "Adwaita")
    with gtk_preferences.open("w") as stream:gtk_config.write(stream)
    query("xsettings","/Net/ThemeName","Adwaita-dark" if dark else "Adwaita","string")
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
    print("Préférence appliquée. Rouvrez les applications pour actualiser leur thème.")

if __name__=="__main__":
    try:
        if len(sys.argv)!=3: raise ValueError("usage : winlux-personalize theme|accent|wallpaper|panel-size VALEUR")
        apply(sys.argv[1],sys.argv[2])
    except (OSError,ValueError,subprocess.SubprocessError) as error:
        print(str(error),file=sys.stderr);raise SystemExit(1)
