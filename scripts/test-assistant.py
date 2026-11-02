#!/usr/bin/env python3
"""Assistant Claude (#286) : désactivé par défaut, rien n'apparaît ni ne tourne
avant l'activation ; la conversation part par l'entrée standard, sans outil ni
télémétrie ; les lectures et actions sont limitées et montrées avant ; la
connexion reste lisible par le seul propriétaire ; le bouton de la barre est
ajouté, gardé par winlux-control align-*, puis retiré."""
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent
work = Path(tempfile.mkdtemp(prefix="winlux-assistant-test-"))
home = work / "home"
home.mkdir()
os.environ.update(HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"), XDG_DATA_HOME=str(home / ".local/share"),
                  WINLUX_JOURNAL_DIR=str(work / "journal"), WINLUX_LANG="fr")
os.environ.pop("CLAUDE_CONFIG_DIR", None)
bin_dir = work / "bin"
bin_dir.mkdir()
os.environ["PATH"] = f"{bin_dir}:{os.environ['PATH']}"

spec = importlib.util.spec_from_file_location("assistant", project / "scripts/winlux-assistant.py")
assistant = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assistant)

# ── Désactivé par défaut : Windows+C et le bouton ne lancent rien ──
assert not assistant.is_enabled()
assistant.run_chat = lambda: (_ for _ in ()).throw(AssertionError("fenêtre ouverte alors que l'assistant est désactivé"))
assert assistant.main(["winlux-assistant"]) == 0
assert not (home / ".config/winlux/assistant.conf").exists()
desktop = (project / "integration/native/applications/winlux-assistant.desktop").read_text(encoding="utf-8")
assert "NoDisplay=true" in desktop, "absent du menu Démarrer : on l'ouvre depuis la barre une fois activé"
for lang in ("en", "es", "de"):
    assert f"Name[{lang}]=" in desktop and f"Comment[{lang}]=" in desktop
rc = (project / "integration/native/config/openbox/rc.xml").read_text(encoding="utf-8")
assert '<keybind key="W-c"><action name="Execute"><command>winlux-assistant</command>' in rc
panel = (project / "integration/native/config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml").read_text(encoding="utf-8")
assert "winlux-assistant" not in panel, "aucun bouton dans la barre livrée"

# ── Réglages lisibles par le seul propriétaire ──
assistant.write_config(enabled=1)
assert stat.S_IMODE((home / ".config/winlux/assistant.conf").stat().st_mode) == 0o600
assert assistant.is_enabled()
assistant.write_config(enabled=0)

# ── Identifiants de Claude Code remis en 700/600 ──
claude_dir = home / ".claude"
claude_dir.mkdir(mode=0o755)
(claude_dir / ".credentials.json").write_text('{"claudeAiOauth": {"accessToken": "x"}}')
(claude_dir / ".credentials.json").chmod(0o644)
(home / ".claude.json").write_text("{}")
(home / ".claude.json").chmod(0o644)
fixed = assistant.protect_credentials()
assert len(fixed) == 3, fixed
assert stat.S_IMODE(claude_dir.stat().st_mode) == 0o700
assert stat.S_IMODE((claude_dir / ".credentials.json").stat().st_mode) == 0o600
assert stat.S_IMODE((home / ".claude.json").stat().st_mode) == 0o600
assert assistant.protect_credentials() == []

# ── Conversation : faux claude qui note ses arguments, son entrée et son environnement ──
fake = bin_dir / "claude"
record = work / "claude-call.json"
fake.write_text(f"""#!{sys.executable}
import json, os, sys
mode = os.environ.get("FAKE_CLAUDE", "ok")
if sys.argv[1:3] == ["auth", "status"]:
    print(json.dumps({{"loggedIn": mode != "logout", "authMethod": "claude.ai", "email": "damien@example.org"}}))
    sys.exit(0)
data = sys.stdin.read()
json.dump({{"argv": sys.argv[1:], "stdin": data, "cwd": os.getcwd(),
           "env": {{k: os.environ.get(k) for k in ("DISABLE_TELEMETRY", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")}}}},
          open({str(record)!r}, "w"))
if mode == "logout":
    print(json.dumps({{"type": "result", "is_error": True, "result": "Not logged in · Please run /login"}}))
    sys.exit(1)
answer = {{"reponse": "Je regarde le journal de putty.exe.", "demande": "lire_journal", "argument": "putty.exe"}}
print(json.dumps({{"type": "result", "is_error": False, "result": json.dumps(answer), "structured_output": answer}}))
""")
fake.chmod(0o755)
os.environ["WINLUX_ASSISTANT_CLAUDE"] = str(fake)
status = assistant.auth_status()
assert status["loggedIn"] and assistant.account_name(status) == "damien@example.org"

conversation = assistant.Conversation()
question = "Pourquoi putty.exe ne s'ouvre pas ? Mon dossier secret-perso"
conversation.entries.append(("user", question))
reply, kind, argument = conversation.ask(timeout=60)
assert (kind, argument) == ("lire_journal", "putty.exe") and reply.startswith("Je regarde")
call = json.loads(record.read_text())
argv = call["argv"]
assert question in call["stdin"], "la conversation part par l'entrée standard"
assert not any("secret-perso" in arg for arg in argv), "jamais en argument (visible par ps)"
assert argv[argv.index("--tools") + 1] == "", "Claude n'a aucun outil"
for flag in ("-p", "--no-session-persistence", "--strict-mcp-config", "--disable-slash-commands", "--json-schema"):
    assert flag in argv, flag
assert argv[argv.index("--setting-sources") + 1] == "", "ni crochets ni réglages personnels de Claude Code"
assert "--dangerously-skip-permissions" not in argv and "--bare" not in argv
assert call["env"] == {"DISABLE_TELEMETRY": "1", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1"}
assert Path(call["cwd"]) == home / ".local/share/winlux/assistant"
assert stat.S_IMODE(Path(call["cwd"]).stat().st_mode) == 0o700
schema = json.loads(argv[argv.index("--json-schema") + 1])
assert set(schema["properties"]["demande"]["enum"]) == set(assistant.REQUESTS)
system = argv[argv.index("--system-prompt") + 1]
assert "pas Wine" in system and "français" in system

os.environ["FAKE_CLAUDE"] = "logout"
try:
    conversation.ask(timeout=60)
    raise AssertionError("non connecté non détecté")
except assistant.NotSignedIn:
    pass
os.environ["FAKE_CLAUDE"] = "ok"

# ── Lectures : seulement ce qui est demandé, chemins personnels masqués ──
journal = work / "journal"
journal.mkdir()
(journal / "winlux.log").write_text(
    f"2026-11-02 [info] calc.exe[10] loader: ok\n"
    f"2026-11-02 [error] PuTTY.exe[42] loader: DLL introuvable : imm32.dll ({home}/Bureau/putty.exe)\n"
    f"2026-11-02 [warn] notepad.exe[7] gdi: police absente\n")
(journal / "crash.log").write_text("[CRASH] ts=1 sig=11 exe=putty.exe module=ntdll pid=42\n  rip=0x1\n"
                                   "[CRASH] ts=2 sig=11 exe=calc.exe module=x pid=10\n")
what, reader = assistant.describe_request("lire_journal", "C:\\Programmes\\putty.exe")
assert "putty.exe" in what
text = reader()
assert "imm32.dll" in text and "~/Bureau/putty.exe" in text and str(home) not in text
assert "calc.exe" not in text and "exe=putty.exe" in text
errors = assistant.describe_request("lire_journal", "")[1]()
assert "PuTTY.exe" in errors and "notepad.exe" in errors and "calc.exe" not in errors
assert "calc.exe" in assistant.describe_request("lire_plantages", "")[1]()
assert "Linux " in assistant.describe_request("lire_systeme", "")[1]()
(home / "Documents/Impôts").mkdir(parents=True)
(home / "Documents/Impôts/Déclaration 2026.pdf").write_text("x")
(home / ".cache").mkdir()
(home / ".cache/declaration-cache.pdf").write_text("x")
found = assistant.describe_request("chercher_fichier", "declaration")[1]()
assert found == "~/Documents/Impôts/Déclaration 2026.pdf", found
assert assistant.describe_request("inconnue", "x") is None

# ── Actions : liste fermée, commande exacte montrée avant ──
label, command = assistant.describe_action("reglage", "theme_sombre")
assert label == "Thème sombre" and command == ["winlux-control", "theme-dark"]
assert assistant.describe_action("reglage", "rm -rf /") is None
assert assistant.describe_action("reglage", "poweroff") is None
assert assistant.describe_action("ouvrir_parametres", "29")[1] == ["winlux-shell-app", "settings", "29"]
assert assistant.describe_action("ouvrir_parametres", "7; reboot") is None
assert assistant.describe_action("ouvrir_dossier", "~/Documents")[1] == ["winlux-shell-app", "explorer", str(home / "Documents")]
assert assistant.describe_action("ouvrir_dossier", "~/introuvable") is None
assert assistant.describe_action("lire_journal", "x") is None
control = (project / "scripts/winlux-control.sh").read_text(encoding="utf-8")
for _, action in assistant.SETTINGS.values():
    assert action in control, f"action inconnue de winlux-control : {action}"

# ── Bouton de la barre : faux xfconf-query avec état ──
store = work / "xfconf.json"
store.write_text(json.dumps({"/panels/panel-1/plugin-ids": [3, 1, 2, 8, 10, 4, 5, 6, 9, 7],
                             "/plugins/plugin-10": "systray", "/plugins/plugin-3": "separator",
                             "/plugins/plugin-3/expand": "true"}))
(bin_dir / "xfconf-query").write_text(f"""#!{sys.executable}
import json, sys
store = {str(store)!r}
data = json.load(open(store))
args = sys.argv[1:]
prop = args[args.index("-p") + 1]
values = [args[i + 1] for i, a in enumerate(args) if a == "-s"]
if "-r" in args:
    for key in [k for k in data if k == prop or ("-R" in args and k.startswith(prop + "/"))]:
        del data[key]
elif values:
    if prop not in data and "-n" not in args:
        sys.exit(1)
    data[prop] = [int(v) if v.isdigit() else v for v in values] if "-a" in args else values[0]
else:
    if prop not in data:
        sys.exit(1)
    value = data[prop]
    if isinstance(value, list):
        print(f"Value is an array with {{len(value)}} items:\\n")
        for item in value:
            print(item)
    else:
        print(value)
    sys.exit(0)
json.dump(data, open(store, "w"))
""")
(bin_dir / "xfconf-query").chmod(0o755)

assistant.enable()
data = json.loads(store.read_text())
assert data["/plugins/plugin-30"] == "launcher"
assert data["/plugins/plugin-30/items"] == ["/usr/share/applications/winlux-assistant.desktop"]
assert data["/panels/panel-1/plugin-ids"] == [3, 1, 2, 8, 30, 10, 4, 5, 6, 9, 7]
assert assistant.read_config() == {"enabled": "1", "panel_plugin": "30"}
assistant.enable()  # deuxième activation : rien en double
assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"].count(30) == 1

# winlux-control align-* réécrit l'ordre de la barre : le bouton reste.
root = work / "root"
env = dict(os.environ, WINLUX_CONTROL_ROOT=str(root), WINLUX_CONTROL_TEST_LOG=str(work / "commands"))
for alignment in ("align-left", "align-center"):
    data = json.loads(store.read_text())
    data["/panels/panel-1/plugin-ids"] = [3, 1, 2, 8, 10, 4, 5, 6, 9, 7]
    store.write_text(json.dumps(data))
    subprocess.run(["sh", str(project / "scripts/winlux-control.sh"), alignment], check=True, env=env,
                   stdout=subprocess.DEVNULL)
    assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"] == [3, 1, 2, 8, 30, 10, 4, 5, 6, 9, 7], alignment

# Avec le bouton Widgets (greffon 11, #293) : les deux restent.
data = json.loads(store.read_text())
data["/plugins/plugin-11"] = "genmon"
store.write_text(json.dumps(data))
subprocess.run(["sh", str(project / "scripts/winlux-control.sh"), "align-center"], check=True, env=env,
               stdout=subprocess.DEVNULL)
assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"] == [11, 3, 1, 2, 8, 30, 10, 4, 5, 6, 9, 7]
data = json.loads(store.read_text())
del data["/plugins/plugin-11"]
data["/panels/panel-1/plugin-ids"].remove(11)
store.write_text(json.dumps(data))

assistant.disable()
data = json.loads(store.read_text())
assert data["/panels/panel-1/plugin-ids"] == [3, 1, 2, 8, 10, 4, 5, 6, 9, 7]
assert not any(key.startswith("/plugins/plugin-30") for key in data)
assert assistant.read_config() == {"enabled": "0"}
subprocess.run(["sh", str(project / "scripts/winlux-control.sh"), "align-center"], check=True, env=env,
               stdout=subprocess.DEVNULL)
assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"] == [3, 1, 2, 8, 10, 4, 5, 6, 9, 7]

# ── Quatre langues : chaque texte de l'interface est traduit ──
for text, row in assistant._TR.items():
    assert len(row) == 3 and all(row), text
source = (project / "scripts/winlux-assistant.py").read_text(encoding="utf-8")
import re
for text in re.findall(r'_t\("((?:[^"\\]|\\.)+)"\s*[,)]', source):
    assert text in assistant._TR, f"texte non traduit : {text}"
for label, _ in assistant.SETTINGS.values():
    assert label in assistant._TR, label

import shutil
shutil.rmtree(work, ignore_errors=True)
print("OK: assistant Claude désactivé par défaut, conversation sans outil par l'entrée standard, "
      "lectures et actions limitées, identifiants protégés, bouton de barre ajouté puis retiré")
