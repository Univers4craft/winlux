#!/usr/bin/env python3
"""Assistant IA (#286) : Claude, ChatGPT et IA locale, tous désactivés par
défaut, rien n'apparaît ni ne tourne avant l'activation ; la conversation part
par l'entrée standard, sans outil ni télémétrie ; les lectures et actions sont
limitées et montrées avant ; les connexions restent lisibles par le seul
propriétaire ; les téléchargements (Codex, llama.cpp, modèle) sont refusés si
l'empreinte ne correspond pas ; l'IA locale n'écoute que 127.0.0.1 et ses
trois agents ne font rien sans clic ; le bouton de la barre est ajouté (un
seul), gardé par winlux-control align-*, puis retiré."""
import base64
import hashlib
import http.server
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tarfile
import tempfile
import threading
import zipfile

project = Path(__file__).resolve().parent.parent
work = Path(tempfile.mkdtemp(prefix="winlux-assistant-test-"))
home = work / "home"
home.mkdir()
os.environ.update(HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"), XDG_DATA_HOME=str(home / ".local/share"),
                  WINLUX_JOURNAL_DIR=str(work / "journal"), WINLUX_LANG="fr")
for name in ("CLAUDE_CONFIG_DIR", "CODEX_HOME", "WINLUX_ASSISTANT_CODEX", "WINLUX_ASSISTANT_LLAMA"):
    os.environ.pop(name, None)
bin_dir = work / "bin"
bin_dir.mkdir()
os.environ["PATH"] = f"{bin_dir}:{os.environ['PATH']}"

spec = importlib.util.spec_from_file_location("assistant", project / "scripts/winlux-assistant.py")
assistant = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assistant)

# ── Désactivé par défaut : Windows+C et le bouton ne lancent rien ──
assert not assistant.is_enabled()
assert all(not assistant.is_enabled(key) for key in assistant.PROVIDERS)
assert assistant.current_provider() is None
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

# ── Réglages lisibles par le seul propriétaire ; « enabled=1 » de #298 = Claude ──
(home / ".config/winlux").mkdir(parents=True)
(home / ".config/winlux/assistant.conf").write_text("enabled=1\n")
assert assistant.is_enabled("claude") and not assistant.is_enabled("chatgpt")
assert assistant.current_provider() == "claude"
assistant.write_config(enabled_claude=0)
assert stat.S_IMODE((home / ".config/winlux/assistant.conf").stat().st_mode) == 0o600
assert assistant.read_config() == {"enabled_claude": "0"}
assert not assistant.is_enabled()

# ── Connexions propres à l'assistant, remises en 700/600 ; ~/.claude et ~/.codex jamais touchés ──
others = []
for name, token in ((".claude", ".credentials.json"), (".codex", "auth.json")):
    (home / name).mkdir(mode=0o755)
    (home / name / token).write_text("{}")
    (home / name / token).chmod(0o644)
    others += [home / name, home / name / token]
accounts = home / ".local/share/winlux/assistant-comptes"
claude_dir, codex_home = accounts / "claude", accounts / "chatgpt"
for folder in (claude_dir, codex_home):
    folder.mkdir(parents=True, mode=0o755)
for path in (claude_dir / ".credentials.json", claude_dir / ".claude.json", codex_home / "auth.json"):
    path.write_text("{}")
    path.chmod(0o644)
accounts.chmod(0o755)
assert len(assistant.protect_credentials()) == 6
for path in (accounts, claude_dir, codex_home):
    assert stat.S_IMODE(path.stat().st_mode) == 0o700, path
for path in (claude_dir / ".credentials.json", claude_dir / ".claude.json", codex_home / "auth.json"):
    assert stat.S_IMODE(path.stat().st_mode) == 0o600, path
assert assistant.protect_credentials() == []
for path in others:
    assert stat.S_IMODE(path.stat().st_mode) in (0o644, 0o755), f"autre outil touché : {path}"
assert ".claude" not in assistant.claude_env()["CLAUDE_CONFIG_DIR"]
pe_launcher = work / "claude-pe"
pe_launcher.write_text('#!/bin/sh\nexec winlux "/x/claude.exe" "$@"\n')
assert assistant.claude_env(pe_launcher)["CLAUDE_CONFIG_DIR"] == "Z:" + str(claude_dir).replace("/", "\\")
os.environ["ANTHROPIC_API_KEY"] = "sk-ant-ne-doit-pas-partir"
assert "ANTHROPIC_API_KEY" not in assistant.claude_env(), "le compte Claude, jamais une clé d'API"

# ── Conversation : faux claude qui note ses arguments, son entrée et son environnement ──
fake = bin_dir / "claude"
record = work / "claude-call.json"
fake.write_text(f"""#!{sys.executable}
import json, os, sys
mode = os.environ.get("FAKE_CLAUDE", "ok")
if sys.argv[1:3] == ["auth", "status"]:
    open({str(work / "claude-status-cwd.txt")!r}, "w").write(os.getcwd())
    print(json.dumps({{"loggedIn": mode != "logout", "authMethod": "claude.ai", "email": "damien@example.org"}}))
    sys.exit(0)
data = sys.stdin.read()
json.dump({{"argv": sys.argv[1:], "stdin": data, "cwd": os.getcwd(),
           "env": {{k: os.environ.get(k) for k in ("DISABLE_TELEMETRY", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
                                                     "CLAUDE_CONFIG_DIR", "ANTHROPIC_API_KEY")}}}},
          open({str(record)!r}, "w"))
if mode == "logout":
    print(json.dumps({{"type": "result", "is_error": True, "result": "Not logged in · Please run /login"}}))
    sys.exit(1)
answer = {{"reponse": "Je regarde le journal de putty.exe.", "demande": "lire_journal", "argument": "putty.exe"}}
print(json.dumps({{"type": "result", "is_error": False, "result": json.dumps(answer), "structured_output": answer}}))
""")
fake.chmod(0o755)
os.environ["WINLUX_ASSISTANT_CLAUDE"] = str(fake)
(home / "Bureau").mkdir()
previous_cwd = os.getcwd()
os.chdir(home / "Bureau")  # Paramètres lancés depuis le Bureau (#913)
status = assistant.auth_status()
os.chdir(previous_cwd)
assert status["loggedIn"] and assistant.account_name(status) == "damien@example.org"
assert (work / "claude-status-cwd.txt").read_text() == str(assistant.WORK), \
    "claude.exe jamais lancé dans un dossier personnel : la couche PE demanderait l'accès aux fichiers (#913)"

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
assert call["env"] == {"DISABLE_TELEMETRY": "1", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
                       "CLAUDE_CONFIG_DIR": str(claude_dir), "ANTHROPIC_API_KEY": None}
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
assert assistant.describe_action("ouvrir_parametres", "30")[1] == ["winlux-shell-app", "settings", "30"]
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
assert assistant.read_config() == {"enabled_claude": "1", "current": "claude", "panel_plugin": "30"}
assistant.enable()  # deuxième activation : rien en double
assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"].count(30) == 1
# Un deuxième assistant : toujours un seul bouton, et il reste tant qu'un assistant est activé.
assistant.enable("chatgpt")
assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"].count(30) == 1
assert assistant.current_provider() == "chatgpt" and assistant.enabled_providers() == ["claude", "chatgpt"]
assistant.disable("chatgpt")
assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"].count(30) == 1
assert assistant.current_provider() == "claude"

# winlux-control align-* réécrit l'ordre de la barre : le bouton reste.
root = work / "root"
env = dict(os.environ, WINLUX_CONTROL_ROOT=str(root), WINLUX_CONTROL_TEST_LOG=str(work / "commands"))
for alignment in ("align-left", "align-center"):
    data = json.loads(store.read_text())
    data["/panels/panel-1/plugin-ids"] = [3, 1, 2, 8, 10, 4, 7, 9]  # ordre d'avant #703
    store.write_text(json.dumps(data))
    subprocess.run(["sh", str(project / "scripts/winlux-control.sh"), alignment], check=True, env=env,
                   stdout=subprocess.DEVNULL)
    # Greffon 13 : contrepoids juste après le séparateur 3 en barre centrée, au bout sinon.
    # Depuis #703 : réseau/son/batterie = genmon 4 seul (5 et 6 absents), l'heure avant la cloche.
    expected = [3, 13, 1, 2, 8, 30, 10, 4, 7, 9] if alignment == "align-center" else [3, 1, 2, 8, 30, 10, 4, 7, 9, 13]
    assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"] == expected, alignment

# Avec le bouton Widgets (greffon 11, #293) : les deux restent.
data = json.loads(store.read_text())
data["/plugins/plugin-11"] = "genmon"
store.write_text(json.dumps(data))
subprocess.run(["sh", str(project / "scripts/winlux-control.sh"), "align-center"], check=True, env=env,
               stdout=subprocess.DEVNULL)
assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"] == [11, 3, 13, 1, 2, 8, 30, 10, 4, 7, 9]
data = json.loads(store.read_text())
del data["/plugins/plugin-11"]
data["/panels/panel-1/plugin-ids"].remove(11)
store.write_text(json.dumps(data))

# Bouton Vue des tâches (#459, numéro libre de 14 à 29) : juste après Démarrer.
data = json.loads(store.read_text())
data["/plugins/plugin-15"] = "launcher"
data["/plugins/plugin-15/items"] = ["/usr/share/applications/winlux-taskview.desktop"]
store.write_text(json.dumps(data))
for alignment, expected in (("align-left", [3, 1, 15, 2, 8, 30, 10, 4, 7, 9, 13]),
                            ("align-center", [3, 13, 1, 15, 2, 8, 30, 10, 4, 7, 9])):
    subprocess.run(["sh", str(project / "scripts/winlux-control.sh"), alignment], check=True, env=env,
                   stdout=subprocess.DEVNULL)
    assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"] == expected, alignment
# Rechercher (#461) avant Vue des tâches, quel que soit son numéro.
data = json.loads(store.read_text())
data["/plugins/plugin-16"] = "launcher"
data["/plugins/plugin-16/items"] = ["/usr/share/applications/winlux-search.desktop"]
store.write_text(json.dumps(data))
subprocess.run(["sh", str(project / "scripts/winlux-control.sh"), "align-center"], check=True, env=env, stdout=subprocess.DEVNULL)
assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"] == [3, 13, 1, 16, 15, 2, 8, 30, 10, 4, 7, 9]
data = json.loads(store.read_text())
for number in (15, 16):
    del data[f"/plugins/plugin-{number}"], data[f"/plugins/plugin-{number}/items"]
    data["/panels/panel-1/plugin-ids"].remove(number)
store.write_text(json.dumps(data))

assistant.disable()
data = json.loads(store.read_text())
assert data["/panels/panel-1/plugin-ids"] == [3, 13, 1, 2, 8, 10, 4, 7, 9]
assert not any(key.startswith("/plugins/plugin-30") for key in data)
assert assistant.read_config() == {"enabled_claude": "0", "enabled_chatgpt": "0"}
assert assistant.main(["winlux-assistant"]) == 0  # plus aucun assistant : rien ne s'ouvre
subprocess.run(["sh", str(project / "scripts/winlux-control.sh"), "align-center"], check=True, env=env,
               stdout=subprocess.DEVNULL)
assert json.loads(store.read_text())["/panels/panel-1/plugin-ids"] == [3, 13, 1, 2, 8, 10, 4, 7, 9]

# ── Serveur HTTP de test : faux registre npm, fausse API GitHub, faux Hugging Face ──
routes = {}


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = routes.get(self.path)
        if body is None:
            self.send_response(404)
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
threading.Thread(target=httpd.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{httpd.server_address[1]}"
assistant._machine = lambda: "x86_64"


def tar_bytes(files):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as tar:
        for name, content in files.items():
            info = tarfile.TarInfo(name)
            info.size, info.mode = len(content), 0o644
            tar.addfile(info, io.BytesIO(content))
    return buffer.getvalue()


# ── ChatGPT : Codex téléchargé depuis npm, empreinte SHA-512 vérifiée ──
chatgpt = assistant.provider("chatgpt")
assert chatgpt.supported() and chatgpt.launcher() is None
codex_tgz = tar_bytes({"package/package.json": b"{}",
                       "package/vendor/x86_64-unknown-linux-musl/bin/codex": b"#!/bin/sh\necho codex\n",
                       "package/vendor/x86_64-unknown-linux-musl/codex-path/rg": b"rg",
                       "package/vendor/x86_64-unknown-linux-musl/codex-resources/voice/big.bin": b"v" * 1000,
                       "package/vendor/aarch64-unknown-linux-musl/bin/codex": b"arm"})
# Version figée et empreinte écrite dans le code, jamais « latest » du registre (#911).
assert assistant.CODEX_VERSION == "0.167.0" and set(assistant.CODEX_SHA512) == {"linux-x64", "linux-arm64"}
assistant.CODEX_VERSION = "9.9.9"
assistant.CODEX_SHA512 = dict(assistant.CODEX_SHA512, **{"linux-x64": base64.b64encode(hashlib.sha512(codex_tgz).digest()).decode()})
assistant.NPM_REGISTRY = base
codex_path = "/@openai/codex/-/codex-9.9.9-linux-x64.tgz"
routes[codex_path] = codex_tgz[:-1] + bytes([codex_tgz[-1] ^ 1])  # archive altérée
try:
    chatgpt.install(None, None, lambda index: None)
    raise AssertionError("archive Codex altérée acceptée")
except RuntimeError as error:
    assert "empreinte" in str(error)
assert chatgpt.launcher() is None and not assistant.CODEX_DIR.exists()
assert not list(assistant.DATA.glob("codex*")), "aucun reste du téléchargement refusé"
routes[codex_path] = codex_tgz
steps = []
launcher = chatgpt.install(None, None, steps.append)
assert steps == [0] and launcher == assistant.CODEX_DIR / "bin/codex" and os.access(launcher, os.X_OK)
assert (assistant.CODEX_DIR / "codex-path/rg").is_file()
assert not (assistant.CODEX_DIR / "codex-resources/voice").exists(), "la voix n'est pas utile ici"
assert not (assistant.CODEX_DIR / "aarch64-unknown-linux-musl").exists()
assert not (assistant.DATA / "codex.tgz").exists()

# Faux codex : note ses arguments, son entrée et son environnement.
codex = bin_dir / "codex"
codex_record = work / "codex-call.json"
codex.write_text(f"""#!{sys.executable}
import json, os, sys
mode = os.environ.get("FAKE_CODEX", "ok")
args = sys.argv[1:]
if args[:2] == ["login", "status"]:
    print("Not logged in" if mode == "logout" else "Logged in using ChatGPT", file=sys.stderr)
    sys.exit(1 if mode == "logout" else 0)
data = sys.stdin.read()
json.dump({{"argv": args, "stdin": data, "cwd": os.getcwd(), "openai_key": os.environ.get("OPENAI_API_KEY"),
           "codex_home": os.environ.get("CODEX_HOME"),
           "schema": json.load(open(args[args.index("--output-schema") + 1]))}}, open({str(codex_record)!r}, "w"))
if mode == "logout":
    print(json.dumps({{"type": "turn.failed", "error": {{"message": "401 Unauthorized: please log in"}}}}))
    sys.exit(1)
print(json.dumps({{"type": "thread.started", "thread_id": "t"}}))
if mode == "tool":
    print(json.dumps({{"type": "item.completed", "item": {{"type": "command_execution", "command": "cat ~/.ssh/id_rsa"}}}}))
answer = {{"reponse": "Je peux passer au thème sombre.", "demande": "reglage", "argument": "theme_sombre"}}
open(args[args.index("-o") + 1], "w").write(json.dumps(answer))
print(json.dumps({{"type": "item.completed", "item": {{"type": "agent_message", "text": json.dumps(answer)}}}}))
print(json.dumps({{"type": "turn.completed"}}))
""")
codex.chmod(0o755)
os.environ["WINLUX_ASSISTANT_CODEX"] = str(codex)
os.environ["OPENAI_API_KEY"] = "sk-ne-doit-pas-partir"
assert chatgpt.status() == {"installed": True, "loggedIn": True, "account": "ChatGPT"}
conversation = assistant.Conversation("chatgpt")
question = "Mets le thème sombre, mon dossier secret-perso"
conversation.entries.append(("user", question))
reply, kind, argument = conversation.ask(timeout=60)
assert (kind, argument) == ("reglage", "theme_sombre"), (kind, argument)
call = json.loads(codex_record.read_text())
argv = call["argv"]
assert question in call["stdin"] and "assistant ChatGPT intégré à Winlux" in call["stdin"]
assert not any("secret-perso" in arg for arg in argv), "jamais en argument (visible par ps)"
assert argv[0] == "exec" and argv[-1] == "-", "la question arrive par l'entrée standard"
for flag in ("--ephemeral", "--ignore-user-config", "--skip-git-repo-check", "--json"):
    assert flag in argv, flag
assert argv[argv.index("--sandbox") + 1] == "read-only"
for setting in ("features.shell_tool=false", "features.unified_exec=false", 'web_search="disabled"',
                "analytics.enabled=false"):
    assert setting in argv, setting
assert "--dangerously-bypass-approvals-and-sandbox" not in argv and "--full-auto" not in argv
assert call["openai_key"] is None, "le compte ChatGPT, jamais une clé d'API de l'environnement"
assert call["codex_home"] == str(codex_home), "connexion propre à l'assistant, jamais ~/.codex"
assert Path(call["cwd"]) == home / ".local/share/winlux/assistant"
assert set(call["schema"]["required"]) == set(call["schema"]["properties"]), "format strict d'OpenAI"
assert not (home / ".local/share/winlux/assistant/reponse.txt").exists(), "rien de gardé sur le disque"
os.environ["FAKE_CODEX"] = "tool"
try:
    conversation.ask(timeout=60)
    raise AssertionError("utilisation d'un outil non refusée")
except RuntimeError as error:
    assert "outil" in str(error)
os.environ["FAKE_CODEX"] = "logout"
assert not chatgpt.status()["loggedIn"]
try:
    conversation.ask(timeout=60)
    raise AssertionError("non connecté non détecté")
except assistant.NotSignedIn:
    pass
os.environ["FAKE_CODEX"] = "ok"
assert chatgpt.login_command(codex) == [str(codex), "login"] and not chatgpt.open_login_page
del os.environ["OPENAI_API_KEY"]

# ── IA locale : cachée sur un PC trop petit, téléchargements vérifiés ──
local = assistant.provider("local")
assistant._mem_total_gib = lambda: 2.0
assert not local.supported(), "moins de 3 Gio de mémoire : option cachée"
assistant._mem_total_gib = lambda: 3.8
assert local.supported() and local.launcher() is None and not local.remote
# #911 : révisions figées, empreintes écrites dans le code (jamais « latest » de GitHub ni « main »).
assert assistant.LLAMA_TAG == "b12202" and "/releases/download/b12202/" in assistant.LLAMA_URL
assert len(assistant.LLAMA_SHA256) == 64 and len(assistant.LOCAL_MODEL_SHA256) == 64
assert len(assistant.LOCAL_MODEL_REVISION) == 40
engine_tgz = io.BytesIO()
with tarfile.open(fileobj=engine_tgz, mode="w:gz") as tar:
    for name, content in (("llama-b9999/llama-server", b"#!/bin/sh\n"), ("llama-b9999/libllama.so.0.9.0", b"so")):
        info = tarfile.TarInfo(name)
        info.size, info.mode = len(content), 0o644
        tar.addfile(info, io.BytesIO(content))
    link = tarfile.TarInfo("llama-b9999/libllama.so.0")
    link.type, link.linkname = tarfile.SYMTYPE, "libllama.so.0.9.0"
    tar.addfile(link)
engine_tgz = engine_tgz.getvalue()
model = b"GGUF" + b"\0" * 4096
assistant.LLAMA_URL = f"{base}/engine.tar.gz"
assistant.LLAMA_SHA256 = hashlib.sha256(engine_tgz).hexdigest()
assistant.LOCAL_MODEL_SHA256 = hashlib.sha256(model).hexdigest()
model_path = f"/{assistant.LOCAL_MODEL_REPO}/resolve/{assistant.LOCAL_MODEL_REVISION}/{assistant.LOCAL_MODEL_FILE}"
routes["/engine.tar.gz"] = engine_tgz
routes[model_path] = model[:-1] + b"X"  # altéré
assistant.HF_BASE = base
progress = []
try:
    local.install(lambda f, t: progress.append(t), None, lambda index: None)
    raise AssertionError("modèle altéré accepté")
except RuntimeError as error:
    assert "empreinte" in str(error)
assert local.engine() is not None and os.access(local.engine(), os.X_OK), "le moteur, lui, était bon"
assert local.model() is None and not list(assistant.LOCAL_DIR.glob("*.part"))
routes[model_path] = model
steps = []
assert local.install(lambda f, t: progress.append(t), None, steps.append) == local.engine()
assert steps == [0, 1] and local.model().read_bytes() == model and progress
assert (local.engine().parent / "libllama.so.0").is_symlink(), "liens des bibliothèques gardés"
assert local.status() == {"installed": True, "loggedIn": True}

# Faux llama-server : vérifie la clé, répond selon le schéma demandé (comme la grammaire JSON).
llama = bin_dir / "llama-server"
llama_record = work / "llama-argv.json"
llama.write_text(f"""#!{sys.executable}
import http.server, json, os, sys
args = sys.argv[1:]
json.dump({{"argv": args, "key": os.environ.get("LLAMA_API_KEY")}}, open({str(llama_record)!r}, "w"))
host, port = args[args.index("--host") + 1], int(args[args.index("--port") + 1])
key = os.environ["LLAMA_API_KEY"]

def answer(schema, text):
    props = schema["properties"]
    low = text.lower()
    if "agent" in props:
        return {{"agent": "fichiers" if "trouve" in low or "ouvre mes" in low else
                 "reglages" if "thème" in low or "theme" in low else "aide"}}
    if "terme" in props:
        if "ouvre mes images" in low:
            return {{"terme": "", "dossier": "images"}}
        return {{"terme": low.split()[-1], "dossier": "aucun"}}
    if "reglage" in props:
        return {{"reglage": "theme_sombre", "page": "aucune"}}
    return {{"reponse": "Il manque imm32.dll." if "imm32" in text else "Je ne sais pas.", "page": "10"}}

class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200 if self.path == "/health" else 404); self.end_headers(); self.wfile.write(b"{{}}")
    def do_POST(self):
        if self.headers.get("Authorization") != "Bearer " + key:
            self.send_response(401); self.end_headers(); return
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        schema = body["response_format"]["json_schema"]["schema"]
        content = json.dumps(answer(schema, body["messages"][-1]["content"]))
        data = json.dumps({{"choices": [{{"message": {{"content": content}}}}]}}).encode()
        self.send_response(200); self.send_header("Content-Length", str(len(data))); self.end_headers()
        self.wfile.write(data)
    def log_message(self, *a): pass

http.server.HTTPServer((host, port), H).serve_forever()
""")
llama.chmod(0o755)
os.environ["WINLUX_ASSISTANT_LLAMA"] = str(llama)
server = assistant.LocalServer()
server.start(timeout=30)
try:
    started = json.loads(llama_record.read_text())
    assert started["argv"][started["argv"].index("--host") + 1] == "127.0.0.1", "jamais ouvert au réseau"
    assert started["key"] == server.key and server.key not in " ".join(started["argv"]), "clé hors de ps"
    assert started["argv"][started["argv"].index("-m") + 1] == str(local.model())
    helper = assistant.LocalAssistant(server)
    assert len(assistant.AGENTS) <= assistant.MAX_AGENTS == 3
    # Agent fichiers : cherche sur ce PC, propose d'ouvrir l'emplacement, ne fait rien seul.
    result = helper.handle("Trouve ma declaration")
    assert result["agent"] == "fichiers"
    assert "Déclaration 2026.pdf" in result["reads"][0][1]
    assert result["actions"] == [("ouvrir l'emplacement de ~/Documents/Impôts/Déclaration 2026.pdf",
                                  ["winlux-shell-app", "explorer", str(home / "Documents/Impôts")])], result
    (home / "Images").mkdir()
    result = helper.handle("Ouvre mes images")
    assert result["actions"] == [("ouvrir le dossier ~/Images", ["winlux-shell-app", "explorer", str(home / "Images")])]
    result = helper.handle("Trouve introuvablexyz")
    assert result["actions"] == [] and "aucun fichier" in result["reply"]
    # Agent réglages : un réglage de la liste fermée, à confirmer.
    result = helper.handle("Passe au thème sombre")
    assert result["agent"] == "reglages" and result["actions"] == [("Thème sombre", ["winlux-control", "theme-dark"])]
    # Agent aide : le journal du .exe nommé est lu sur ce PC.
    result = helper.handle("Pourquoi putty.exe ne s'ouvre pas ?")
    assert result["agent"] == "aide" and result["reply"] == "Il manque imm32.dll."
    assert "le journal du programme putty.exe" == result["reads"][0][0] and "imm32.dll" in result["reads"][0][1]
    assert result["actions"][0][1] == ["winlux-shell-app", "settings", "10"]
finally:
    server.stop()
assert not server.running()

# Désactivée : fichiers gardés ; « Supprimer » libère la place.
assistant.enable("local")
assert assistant.main(["winlux-assistant", "--etat"]) == 0
assistant.disable("local")
assert local.model() is not None
os.environ.pop("WINLUX_ASSISTANT_LLAMA")
assert assistant.main(["winlux-assistant", "--supprimer", "local"]) == 0
assert not assistant.LOCAL_DIR.exists() and not assistant.is_enabled()
assert assistant.main(["winlux-assistant", "--activer", "inconnu"]) == 2

# Moteur à qui il manque une bibliothèque : dit à l'installation, pas à l'ouverture du panneau.
broken = work / "broken-llama-server"
broken.write_text("#!/bin/sh\necho 'llama-server: error while loading shared libraries: libgomp.so.1: cannot open "
                  "shared object file: No such file or directory' >&2\nexit 127\n")
broken.chmod(0o755)
assert assistant.engine_problem(broken) == "libgomp.so.1"

# ── Sans navigateur sur ce PC (#912) : lien pour un autre appareil, rien n'est ouvert ici ──
browsers = work / "navigateurs"
assistant.BROWSERS_DIR = browsers
saved_commands = assistant.BROWSER_COMMANDS
assistant.BROWSER_COMMANDS = ("navigateur-inexistant-winlux",)
assert not assistant.has_browser()
codex_provider = assistant.provider("chatgpt")
assert codex_provider.login_command(Path("/x/codex"), False) == ["/x/codex", "login", "--device-auth"]
assert codex_provider.login_command(Path("/x/codex"), True) == ["/x/codex", "login"]
assert assistant.provider("claude").login_command(Path("/x/claude"), False)[1:] == ["auth", "login", "--claudeai"]
firefox = browsers / "firefox/root/usr/lib/firefox/firefox"
firefox.parent.mkdir(parents=True)
firefox.write_text("#!/bin/sh\n")
firefox.chmod(0o755)
assert assistant.has_browser(), "Firefox du catalogue Winlux"
assistant.BROWSER_COMMANDS = saved_commands
setup = (project / "scripts/winlux-assistant.py").read_text(encoding="utf-8")
assert 'WINLUX_BROWSER="true"' in setup and "os.killpg" in setup and "start_new_session=True" in setup
assert '"settings", "11"' in setup, "Installer un navigateur : page Navigateurs, choix explicite"
for text in ("Copier le lien", "Installer un navigateur", "Lien copié."):
    assert text in assistant._TR and len(assistant._TR[text]) == 3, text

# ── Une fenêtre d'activation par assistant (#910), unique : un 2e clic la ramène ──
assert 'org.winlux.AssistantSetup.{key}' in setup and "app.get_is_remote()" in setup
pages = (project / "scripts/winlux-settings-pages.py").read_text(encoding="utf-8")
toggle = pages[pages.index("    def assistant_toggle"):pages.index("    # ── Partage et transfert")]
assert "assistant_setups" in toggle and "if getattr(self, \"assistant_setup\", None)" not in toggle
httpd.shutdown()

# ── L'IA parle la langue choisie pour Winlux (Paramètres > Langue, /etc/winlux/locale.conf) ──
for lang, name, page in (("en", "English", "Display"), ("es", "español", "Pantalla"), ("de", "Deutsch", "Bildschirm")):
    assistant._LANG = lang
    assert f"Réponds toujours en {name}" in assistant.system_prompt("ChatGPT"), lang
    label = assistant.describe_action("ouvrir_parametres", "8")[0]
    assert page in label and "Écran" not in label, label
    assert assistant.display_name("local") != "IA locale"
    assert "Processeur" not in assistant.read_system() and "Mémoire" not in assistant.read_system()
    assert assistant.describe_action("reglage", "theme_sombre")[0] != "Thème sombre"
assistant._LANG = "fr"
for page_name in assistant.SETTINGS_PAGES.values():
    assert page_name in assistant._TR, f"page non traduite : {page_name}"

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
print("OK: assistants Claude, ChatGPT et IA locale désactivés par défaut, conversations sans outil par l'entrée "
      "standard, téléchargements vérifiés, IA locale sur 127.0.0.1 à trois agents, actions confirmées, "
      "identifiants protégés, bouton de barre unique ajouté puis retiré")
