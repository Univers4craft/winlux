#!/usr/bin/python3
"""Real mixed Linux/PE window lifecycle and screensaver, in a disposable guest."""
import os
import json
import signal
from pathlib import Path
import re
import subprocess
import time

def run(*args, **kwargs):
    kwargs.setdefault("timeout", 20)
    try:
        return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, **kwargs)
    except subprocess.CalledProcessError as error:
        print(error.output, flush=True)
        raise

def windows():
    result = {}
    try: rows = run("wmctrl", "-lG").splitlines()
    except subprocess.CalledProcessError as error:
        if "client list" in error.output.lower(): return result
        # A window closed between the client list and its property reads
        # (e.g. right after wmctrl -ic): list again.
        if "BadWindow" in error.output:
            time.sleep(0.1)
            return windows()
        if not error.output.strip():
            # wmctrl returns 1 for a zero-length client list after the last
            # app closes. Require the WM to remain alive before accepting it.
            run("wmctrl", "-m")
            return result
        raise
    for row in rows:
        fields = row.split(None, 7)
        if len(fields) == 8:
            result[fields[0]] = (tuple(map(int, fields[2:6])), fields[7])
    return result

# The Settings window title follows the interface language since #145.
SETTINGS_TITLES = ("Paramètres Winlux", "Parametres Winlux", "Winlux Settings",
                   "Configuración de Winlux", "Winlux-Einstellungen")

def wait_title(title, timeout=90):
    titles = (title,) if isinstance(title, str) else tuple(title)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for window, (_, text) in windows().items():
            if any(name in text for name in titles): return window
        time.sleep(0.2)
    raise AssertionError(f"Window not mapped: {title}; {windows()}")

def wait_for(predicate, label):
    for _ in range(50):
        if predicate(): return
        time.sleep(0.1)
    raise AssertionError(label)

def capture(name):
    destination = Path("/var/lib/winlux") / (name + ".xwd")
    destination.parent.mkdir(parents=True, exist_ok=True)
    run("xwd", "-root", "-silent", "-out", str(destination))
    return destination.read_bytes()

def client_ids():
    return {int(value, 16) for value in re.findall(r"0x[0-9a-fA-F]+", run("xprop", "-root", "_NET_CLIENT_LIST"))}

os.environ.update(DISPLAY=":99", WINLUX_NATIVE_SHELL="1", XDG_CURRENT_DESKTOP="XFCE",
                  XDG_MENU_PREFIX="xfce-", XDG_RUNTIME_DIR="/run/user/0",
                  DBUS_SESSION_BUS_ADDRESS="unix:path=/run/winlux/session-bus",
                  XDG_CONFIG_DIRS="/etc/xdg", XDG_DATA_DIRS="/usr/local/share:/usr/share",
                  LANG="fr_FR.UTF-8" if Path("/usr/lib/locale/fr_FR.utf8").is_dir() else "C.UTF-8", WINLUX_UI_SCALE="1")
Path("/var/lib/winlux").mkdir(parents=True, exist_ok=True)
if Path("/usr/share/winlux/update-backend").exists():
    print(run("python3", "/opt/winlux/scripts/test-apt-native-guest.py", timeout=300), flush=True)
log = Path("/var/lib/winlux/native-programs.log").open("w")
server = subprocess.Popen(["Xvfb", ":99", "-ac", "-screen", "0", "1920x1080x24", "-nolisten", "tcp"], stdout=log, stderr=log)
time.sleep(1)
assert server.poll() is None
def start_session(timeout=240):
    # Trace the session script into its own log and wait for the script
    # itself, not for EOF on a pipe that background services could inherit.
    # On a hang, print where it stopped and what is still running.
    trace = Path("/run/winlux/native-session.log")
    trace.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    with trace.open("w") as output:
        session = subprocess.Popen(["sh", "-x", "/usr/bin/winlux-native-session"],
                                   stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        try:
            status = session.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            status = None
    elapsed = time.monotonic() - started
    if status == 0:
        print(f"WINLUX_NATIVE_SESSION_SECONDS={elapsed:.1f}", flush=True)
        return
    print(f"winlux-native-session: {'blocked after' if status is None else f'exit {status} after'} {elapsed:.0f}s; trace:", flush=True)
    print("\n".join(trace.read_text(errors="replace").splitlines()[-80:]), flush=True)
    try: subprocess.run(["ps", "-eo", "pid,ppid,stat,etime,args"], check=False, timeout=10)
    except (OSError, subprocess.TimeoutExpired): subprocess.run(["/bin/busybox", "ps"], check=False)
    for service_log in sorted(Path("/run/winlux").glob("*.log")):
        if service_log == trace: continue
        lines = service_log.read_text(errors="replace").splitlines()[-15:]
        if lines: print(f"--- {service_log.name}\n" + "\n".join(lines), flush=True)
    if status is None: session.kill()
    raise AssertionError("winlux-native-session did not complete")

start_session()
file_manager = run("gdbus", "introspect", "--session", "--dest", "org.xfce.FileManager",
                   "--object-path", "/org/xfce/FileManager", timeout=60)
assert "Launch" in file_manager, "Desktop file manager cannot launch files"
print("WINLUX_NATIVE_DESKTOP_FILE_SERVICE=0", flush=True)
for service,object_path in [("org.freedesktop.NetworkManager","/org/freedesktop/NetworkManager"),
                            ("org.freedesktop.UDisks2","/org/freedesktop/UDisks2")]:
    print(run("gdbus","call","--system","--dest",service,"--object-path",object_path,
              "--method","org.freedesktop.DBus.Peer.Ping",timeout=40),flush=True)
print("WINLUX_NATIVE_HARDWARE_SERVICES=0",flush=True)
print(run("wmctrl", "-m"), flush=True)
subprocess.Popen(["xfdesktop", "--disable-wm-check"], stdout=log, stderr=log)
run("winlux-control", "status")
subprocess.Popen(["winlux-shell-app", "explorer"], stdout=log, stderr=log)
explorer = wait_title("Explorer Winlux")
subprocess.Popen(["winlux-shell-app", "settings", "11"], stdout=log, stderr=log)
settings = wait_title(SETTINGS_TITLES)
def settings_page(window):
    try:
        pid = int(run("xprop", "-id", str(window), "_NET_WM_PID").split("=")[-1].strip())
        environment=Path(f"/proc/{pid}/environ").read_bytes().split(b"WINLUX_SETTINGS_PAGE=",1)
        return b"WINLUX_SETTINGS_PAGE="+environment[1].split(b"\0",1)[0] if len(environment)==2 else b""
    except (OSError,ValueError,subprocess.CalledProcessError):return b""

assert settings_page(settings) == b"WINLUX_SETTINGS_PAGE=11"
from gi.repository import Gio
previous = client_ids()
Gio.DesktopAppInfo.new_from_filename("/usr/share/applications/winlux-updates.desktop").launch([], None)
def update_windows():
    return [int(identifier,16) for identifier,(_,title) in windows().items()
            if int(identifier,16) not in previous and any(name in title for name in SETTINGS_TITLES) and
            settings_page(identifier)==b"WINLUX_SETTINGS_PAGE=7"]
wait_for(lambda: len(update_windows())==1,"updates launcher opens its own settings window")
updates=update_windows()[0]
assert settings_page(updates) == b"WINLUX_SETTINGS_PAGE=7"
capture("native-settings-updates")
run("wmctrl", "-ic", hex(updates))
print("WINLUX_NATIVE_SETTINGS_ROUTES=0", flush=True)
document = Path("/tmp/winlux-native-note.txt")
document.write_text("Winlux : vrai éditeur de texte.\n")
subprocess.Popen(["mousepad", "--disable-server", str(document)], stdout=log, stderr=log)
editor = wait_title("winlux-native-note.txt")
subprocess.Popen(["winlux-terminal"], stdout=log, stderr=log)
terminal = wait_title("Terminal Winlux")

for window in [explorer, settings, editor, terminal]:
    state = run("xprop", "-id", window, "_NET_WM_STATE")
    assert "_NET_WM_STATE_FULLSCREEN" not in state, state
    run("wmctrl", "-ir", window, "-e", "0,120,90,800,500")
    wait_for(lambda: all(abs(actual - expected) < 32 for actual, expected in zip(windows().get(window, ((0, 0, 0, 0), ""))[0][2:], (800, 500))), "resize")
    run("xdotool", "windowminimize", window)
    wait_for(lambda: "Iconic" in run("xprop", "-id", window, "WM_STATE"), "minimize")
    run("wmctrl", "-ia", window)
    wait_for(lambda: "Normal" in run("xprop", "-id", window, "WM_STATE"), "restore")
    run("wmctrl", "-ir", window, "-b", "add,maximized_vert,maximized_horz")
    wait_for(lambda: "_NET_WM_STATE_MAXIMIZED_VERT" in run("xprop", "-id", window, "_NET_WM_STATE"), "maximize")
    run("wmctrl", "-ir", window, "-b", "remove,maximized_vert,maximized_horz")
print("WINLUX_NATIVE_WINDOW_CONTROLS=0", flush=True)

# Exercise the delivered Openbox bindings, not just manual wmctrl placement.
run("xdotool", "windowactivate", "--sync", terminal)
run("xdotool", "key", "--clearmodifiers", "super+Left")
wait_for(lambda: abs(windows()[terminal][0][0]) < 32 and abs(windows()[terminal][0][2] - 960) < 32, "snap left")
run("xdotool", "key", "--clearmodifiers", "super+Right")
wait_for(lambda: abs(windows()[terminal][0][0] - 960) < 32 and abs(windows()[terminal][0][2] - 960) < 32, "snap right")
print("WINLUX_NATIVE_WINDOW_SNAPPING=0", flush=True)

# End-to-end geometry acknowledgement latency. This includes subprocess and
# X11 round trips; it is not a measurement of the visible display frame rate.
timings = {"move_ms": [], "resize_ms": []}
for i in range(12):
    started = time.monotonic()
    x, y = 80 + i * 8, 80 + i * 4
    run("xdotool", "windowmove", "--sync", terminal, str(x), str(y))
    timings["move_ms"].append((time.monotonic() - started) * 1000)
    started = time.monotonic()
    width, height = 700 + i * 8, 400 + i * 4
    run("xdotool", "windowsize", "--sync", terminal, str(width), str(height))
    timings["resize_ms"].append((time.monotonic() - started) * 1000)
report = {key: {"median": round(sorted(values)[len(values)//2], 2),
                "p95": round(sorted(values)[-1], 2),
                "maximum": round(max(values), 2), "samples": len(values)}
          for key, values in timings.items()}
Path("/var/lib/winlux/window-latency.json").write_text(json.dumps(report, indent=2))
print("WINLUX_WINDOW_LATENCY=" + json.dumps(report), flush=True)

run("wmctrl", "-ia", editor)
time.sleep(0.2)
run("xdotool", "key", "ctrl+End")
run("xdotool", "type", "Texte sauvegarde dans Mousepad.")
run("xdotool", "key", "ctrl+s")
wait_for(lambda: "Texte sauvegarde" in document.read_text(), "editor save")
print("WINLUX_NATIVE_EDITOR_SAVE=0", flush=True)

run("winlux-applications", "register", "/root/Téléchargements/Bonjour.exe")
assert any("Bonjour" in entry.read_text() for entry in Path("/root/.local/share/applications").glob("winlux-pe-*.desktop"))
html = Path("/tmp/winlux-native-browser.html")
html.write_text("<!doctype html><title>Winlux browser window test</title><h1>Winlux</h1>")
html.chmod(0o644)
# Since #155 the init installs Edge in the background. Make sure it is
# really installed before launching it, and show why if it is not.
if "browser_edge=installed" not in run("winlux-browsers", "--status"):
    setup_log = Path("/run/winlux/browser-setup.log")
    print("Edge absent au lancement ; journal d'installation :\n" +
          (setup_log.read_text(errors="replace") if setup_log.exists() else "(aucun)"), flush=True)
    print(run("winlux-browsers", "--install", "edge", timeout=600), flush=True)
subprocess.Popen(["winlux-browsers", "--launch", "edge", "--disable-gpu", html.as_uri()], stdout=log, stderr=log)
browser = wait_title("Winlux browser window test", 90)
assert "_NET_WM_STATE_FULLSCREEN" not in run("xprop", "-id", browser, "_NET_WM_STATE")
geometry = windows()[browser][0]
assert geometry[2] < 1920 and geometry[3] < 1040, geometry
print("WINLUX_NATIVE_EDGE_WINDOWED=0", flush=True)
clients = client_ids()
for window in [explorer, settings, editor, terminal, browser]:
    assert int(window, 16) in clients, window
run("winlux-applications", "window-icons")
for window in [explorer, settings]:
    icon = run("xprop", "-id", window, "_NET_WM_ICON")
    assert "_NET_WM_ICON(CARDINAL)" in icon, icon[:200]
for window, geometry in [(browser, "0,40,40,1100,700"), (settings, "0,1030,60,830,620"),
                         (explorer, "0,150,120,900,580"), (editor, "0,100,750,800,240"), (terminal, "0,1040,740,780,260")]:
    run("wmctrl", "-ir", window, "-e", geometry)
time.sleep(1)
capture("native-desktop")
assert len(list(Path("/run/winlux/tray").glob("*.svg"))) >= 3, "Panel device indicators did not execute"
subprocess.Popen(["winlux-start-menu"], stdout=log, stderr=log)
start = wait_title("Démarrer Winlux")
assert windows()[start][0][2] >= 640 and windows()[start][0][3] >= 600, "Start size/layout collapsed"
time.sleep(1)
capture("native-start-menu")
sx, sy, sw, sh = windows()[start][0]
run("xdotool", "mousemove", str(sx + sw - 100), str(sy + 116), "click", "1")
time.sleep(0.3)
capture("native-start-all")
run("xdotool", "mousemove", str(sx + 120), str(sy + 50), "click", "1")
run("xdotool", "type", "Bonjour")
time.sleep(0.4)
capture("native-start-search")
run("xdotool", "key", "Escape")
print("WINLUX_NATIVE_TASKBAR_AND_MENU=0", flush=True)

# Optional vendor executable, injected only into the disposable diagnostic disk.
# No proprietary application is copied into the released OS image.
external = Path("/opt/winlux-test/ChatGPT.exe")
name = Path("/opt/winlux-test/executable-name.txt")
if name.is_file():
    value = name.read_text().strip()
    assert Path(value).name == value and value not in (".", "..")
    external = external.parent / value
cases = [("external", external)] if external.is_file() else []
for label, filename in [("chatgpt", "ChatGPT.exe"), ("claude", "claude.exe")]:
    application = Path("/opt/winlux-vendor") / label / filename
    if application.is_file(): cases.append((label, application))
reports = []
for label, application in cases:
    started = time.monotonic()
    process = subprocess.Popen(["winlux", str(application)], cwd=application.parent,
                               env=dict(os.environ, WINLUX_TRACE="1"),
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               text=True, errors="replace", start_new_session=True)
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=20)
    except subprocess.TimeoutExpired:
        timed_out = True
        capture("native-vendor-" + label)
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate(timeout=5)
    else:
        capture("native-vendor-" + label)
    report = f"exit={process.returncode}\ntimeout={timed_out}\n{stdout}\n{stderr}"
    Path("/var/lib/winlux/chatgpt-result.txt").write_text(report)
    Path("/var/lib/winlux/vendor-result.txt").write_text(f"application={application.name}\n{report}")
    record = dict(application=application.name, path=str(application), exit_code=process.returncode,
                  timed_out=timed_out, seconds=round(time.monotonic() - started, 2), stdout=stdout, stderr=stderr)
    reports.append(record)
    print("WINLUX_VENDOR_DIAGNOSTIC=" + json.dumps(record, ensure_ascii=False), flush=True)
if reports: Path("/var/lib/winlux/vendor-results.json").write_text(json.dumps(reports, ensure_ascii=False, indent=2))

run("xscreensaver-command", "-activate")
time.sleep(3)
print(run("xscreensaver-command", "-time"), flush=True)
capture("native-pipes-1")
run("pidof", "pipes")
first = capture("native-pipes-1")
time.sleep(1)
second = capture("native-pipes-2")
assert first != second, "Screensaver animation did not change"
run("xscreensaver-command", "-deactivate")
run("xdotool", "mousemove_relative", "2", "2")
print("WINLUX_NATIVE_PIPES_ANIMATED=0", flush=True)

for window in [explorer, settings, editor, terminal, browser]:
    run("wmctrl", "-ic", window)
    # Query the stable root property rather than reading the title/geometry of
    # clients that can disappear halfway through wmctrl's enumeration.
    wait_for(lambda: int(window, 16) not in client_ids(), "close")
run("wmctrl", "-m")
print("WINLUX_NATIVE_CLOSE=0", flush=True)

def system_services():
    """v0.2.13: CUPS answers, PipeWire serves Pulse clients, SMB/NFS modules ship."""
    deadline = time.monotonic() + 30
    while not Path("/run/cups/cups.sock").exists():
        assert time.monotonic() < deadline, "cupsd not started: " + Path("/run/winlux/cups.log").read_text(errors="replace")[-2000:]
        time.sleep(0.5)
    print(run("lpstat", "-r"), flush=True)
    deadline = time.monotonic() + 30
    while True:
        info = subprocess.run(["pactl", "info"], capture_output=True, text=True)
        if info.returncode == 0 and "PipeWire" in info.stdout: break
        if time.monotonic() > deadline:
            raise AssertionError("PipeWire not serving Pulse clients: " + info.stdout + info.stderr
                                 + Path("/run/winlux/pipewire.log").read_text(errors="replace")[-1500:])
        time.sleep(0.5)
    release = os.uname().release
    for module in ["cifs", "nfs", "nfsv4"]:
        run("modprobe", "-n", "-S", release, module)

system_services()
print("WINLUX_NATIVE_SYSTEM_SERVICES=0", flush=True)

def network():
    """The VM's DHCP DNS server (not QEMU's usual 10.0.2.3) must resolve names."""
    deadline = time.monotonic() + 90
    while True:
        probe = subprocess.run(["getent", "hosts", "github.com"], capture_output=True, text=True)
        if probe.returncode == 0: break
        if time.monotonic() > deadline:
            for command in (["cat", "/etc/resolv.conf"], ["ls", "-l", "/etc/resolv.conf"], ["ip", "-4", "addr"],
                            ["ip", "route"], ["nmcli", "device"], ["nmcli", "general"]):
                print("$", " ".join(command), flush=True)
                print(subprocess.run(command, capture_output=True, text=True).stdout, flush=True)
            for name in ("network-manager.log", "network-fallback.log"):
                path = Path("/run/winlux") / name
                if path.exists():
                    print(f"--- {path}", flush=True)
                    print("\n".join(path.read_text(errors="replace").splitlines()[-40:]), flush=True)
            raise AssertionError("DNS does not resolve github.com")
        time.sleep(1)
    print(probe.stdout.strip(), flush=True)

network()
print("WINLUX_NATIVE_NETWORK_DNS=0", flush=True)

def installer_shortcut():
    """The live ISO's « Installer Winlux » shortcut opens the installer window."""
    before = client_ids()
    Gio.DesktopAppInfo.new_from_filename("/usr/share/applications/winlux-installer.desktop").launch([], None)
    deadline = time.monotonic() + 90
    found = None
    while found is None and time.monotonic() < deadline:
        for window in client_ids() - before:
            klass = subprocess.run(["xprop", "-id", hex(window), "WM_CLASS"], capture_output=True, text=True).stdout
            if "installer" in klass.lower(): found = window
        time.sleep(0.5)
    if found is None:
        print(subprocess.run(["ps", "-eo", "pid,args"], capture_output=True, text=True).stdout[-3000:], flush=True)
        raise AssertionError("installer window not mapped from its shortcut")
    capture("native-installer")
    run("wmctrl", "-ic", hex(found))
    subprocess.run(["pkill", "-f", "/usr/bin/winlux-installer"])

installer_shortcut()
print("WINLUX_NATIVE_INSTALLER_SHORTCUT=0", flush=True)

def fast_user_switch():
    """#119: a second account gets its own real Xorg on its own VT, then logs off."""
    guest = "invite"
    if subprocess.run(["id", guest], capture_output=True).returncode != 0:
        run("useradd", "-m", "-s", "/bin/bash", guest)
    users = Path("/etc/winlux/users.conf")
    if users.exists() and not any(line.split(":", 1)[0] == guest for line in users.read_text().splitlines()):
        with users.open("a") as handle: handle.write(guest + ":user:\n")
    run("winlux-sessions", "switch", guest, timeout=60)
    entry = Path("/run/winlux/sessions") / guest
    fields = dict(line.split("=", 1) for line in entry.read_text().splitlines() if "=" in line)
    display, vt = fields["display"], fields["vt"]
    assert Path("/sys/class/tty/tty0/active").read_text().strip() == "tty" + vt, "VT not activated"
    # The second desktop must come up on that server, with its own WM.
    env = dict(os.environ, DISPLAY=display)
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        probe = subprocess.run(["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"], env=env, capture_output=True, text=True)
        if "window id" in probe.stdout: break
        time.sleep(1)
    else:
        raise AssertionError("no window manager on " + display)
    listed = Path("/run/winlux/sessions.txt").read_text()
    assert f"session={guest}|{display}|{vt}|active" in listed, listed
    # The first desktop kept running while the other user was active.
    run("wmctrl", "-m")
    run("winlux-sessions", "logoff", guest, timeout=30)
    wait_for(lambda: not entry.exists(), "session still registered after logoff")
    deadline = time.monotonic() + 20
    while Path("/tmp/.X11-unix/X" + display.lstrip(":")).exists() and time.monotonic() < deadline:
        time.sleep(0.5)
    assert not Path("/tmp/.X11-unix/X" + display.lstrip(":")).exists(), "X server left running"

try:
    fast_user_switch()
except Exception:
    logs = sorted(Path("/run/winlux").glob("session-invite.log")) + sorted(Path("/run/user").glob("*/winlux/*.log"))
    for path in logs + sorted(Path("/run/winlux").glob("Xorg.[1-9].log")):
        print(f"--- {path}", flush=True)
        print("\n".join(path.read_text(errors="replace").splitlines()[-25:]), flush=True)
    raise
print("WINLUX_NATIVE_FAST_USER_SWITCH=0", flush=True)
print("WINLUX_NATIVE_RESULT=0", flush=True)
