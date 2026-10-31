#!/usr/bin/python3
"""XDG launchers for real installed browsers, registered Windows apps and PEs.

No shell commands from registry/shortcut files are evaluated. Windows arguments
are encoded as separate Desktop Entry Exec arguments. Only our own generated
launchers are replaced or removed.
"""
import configparser
import ctypes
import fcntl
import hashlib
import json
import os
import re
from pathlib import Path
import struct
import subprocess
import sys

HOME = Path.home()
DATA = Path(os.environ.get("XDG_DATA_HOME", HOME / ".local/share"))
APPS = DATA / "applications"
STATE = DATA / "winlux"
EXE_LAUNCHER = "/usr/bin/winlux-launch-exe"

def escaped(value):
    return str(value).replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")

def argument(value):
    value = str(value).replace("%", "%%")
    for char in ["\\", '"', "`", "$"]:
        value = value.replace(char, "\\" + char)
    # Exec has both Desktop Entry value escaping and command-line quoting.
    return escaped('"' + value + '"')

CHANGED = []

def save(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_file() and not path.is_symlink() and path.read_text(errors="replace") == text:
        return
    CHANGED.append(path)
    temporary = path.with_name(path.name + f".tmp.{os.getpid()}")
    with temporary.open("x", encoding="utf-8") as stream:
        stream.write(text)
    temporary.chmod(0o644)
    temporary.replace(path)

def managed_desktop(path, text):
    if path.is_file():
        current = path.read_text(errors="replace")
        marker = next((line.split("=", 1)[1] for line in current.splitlines() if line.startswith("X-Winlux-Generated-Hash=")), None)
        original = "\n".join(line for line in current.splitlines() if not line.startswith("X-Winlux-Generated-Hash=")) + "\n"
        if marker != hashlib.sha256(original.encode()).hexdigest():
            return  # The user edited or hid this launcher: retain their changes.
    digest = hashlib.sha256(text.encode()).hexdigest()
    text = text.replace("[Desktop Entry]\n", f"[Desktop Entry]\nX-Winlux-Generated-Hash={digest}\n", 1)
    save(path, text)

def windows_path(value, home):
    value = value.strip().strip('"').replace("\\", "/")
    if value[:2].lower() == "c:":
        return home / value[2:].lstrip("/")
    if value[:2].lower() == "z:":
        return Path("/") / value[2:].lstrip("/")
    return Path(value) if value.startswith("/") else home / value

def pe_icon(path, index=0):
    """Rebuild a resource icon as ICO (Windows icon index: N-th group, or -id); do not execute the PE."""
    data = path.read_bytes() if path.stat().st_size <= 256 * 1024 * 1024 else b""
    if len(data) < 64 or data[:2] != b"MZ":
        return None
    def u16(offset): return struct.unpack_from("<H", data, offset)[0]
    def u32(offset): return struct.unpack_from("<I", data, offset)[0]
    pe = u32(60)
    if data[pe:pe + 4] != b"PE\0\0" or u16(pe + 22) & 0x2000:
        return None
    optional = pe + 24
    directory = optional + (112 if u16(optional) == 0x20b else 96)
    rva = u32(directory + 16)
    sections = optional + u16(pe + 20)
    def file_offset(address):
        for index in range(min(u16(pe + 6), 96)):
            section = sections + index * 40
            start = u32(section + 12)
            if start <= address < start + max(u32(section + 8), u32(section + 16)):
                offset = u32(section + 20) + address - start
                if offset < len(data): return offset
        raise ValueError("Resource outside PE sections")
    base = file_offset(rva)
    def children(relative):
        offset = base + relative
        count = u16(offset + 12) + u16(offset + 14)
        if count > 4096: raise ValueError("Too many resource entries")
        return [(u32(offset + 16 + i * 8), u32(offset + 20 + i * 8)) for i in range(count)]
    def resources(kind):
        found = {}
        for resource_type, target in children(0):
            if resource_type != kind or not target & 0x80000000: continue
            for name, branch in children(target & 0x7fffffff):
                if not branch & 0x80000000: continue
                for _, leaf in children(branch & 0x7fffffff):
                    if leaf & 0x80000000: continue
                    address = base + leaf
                    start, size = file_offset(u32(address)), u32(address + 4)
                    if size > 4 * 1024 * 1024 or start + size > len(data): raise ValueError("Invalid icon size")
                    found[name] = data[start:start + size]
                    break
        return found
    groups, images = resources(14), resources(3)
    if not groups: return None
    if index < 0 and -index in groups: group = groups[-index]
    elif 0 <= index < len(groups): group = list(groups.values())[index]
    else: group = next(iter(groups.values()))
    count = struct.unpack_from("<H", group, 4)[0]
    if not 0 < count <= 64 or len(group) < 6 + 14 * count: return None
    entries, payload, offset = [], [], 6 + 16 * count
    for i in range(count):
        entry = group[6 + i * 14:20 + i * 14]
        image = images[struct.unpack_from("<H", entry, 12)[0]]
        entries.append(entry[:8] + struct.pack("<II", len(image), offset))
        payload.append(image)
        offset += len(image)
    return struct.pack("<HHH", 0, 1, count) + b"".join(entries + payload)

def valid_exe(path):
    try:
        if not path.is_file(): return False
        with path.open("rb") as stream:
            header = stream.read(64)
            if len(header) < 64 or header[:2] != b"MZ": return False
            offset = struct.unpack_from("<I", header, 60)[0]
            if offset > 16 * 1024 * 1024: return False
            stream.seek(offset)
            header = stream.read(24)
            return len(header) == 24 and header[:4] == b"PE\0\0" and not struct.unpack_from("<H", header, 22)[0] & 0x2000
    except (OSError, ValueError, struct.error): return False

def icon_for(path, identity):
    output = HOME / ".cache/winlux/icons" / (identity + ".ico")
    try:
        stamp = output.with_suffix(".stamp")
        metadata = f"{path.stat().st_mtime_ns}:{path.stat().st_size}"
        if stamp.is_file() and stamp.read_text() == metadata:
            return str(output) if output.is_file() else "application-x-executable"
        if output.is_file() and output.stat().st_mtime_ns >= path.stat().st_mtime_ns:
            return str(output)
        icon = pe_icon(path)
        output.parent.mkdir(parents=True, exist_ok=True)
        stamp.write_text(metadata)
        if icon:
            output.write_bytes(icon)
            return str(output)
    except (OSError, ValueError, struct.error, KeyError): pass
    return "application-x-executable"

def window_icons():
    """Publish extracted PE icons to EWMH, where the native tasklist reads them."""
    try:
        windows = subprocess.check_output(["wmctrl", "-lp"], text=True, timeout=2)
    except (OSError, subprocess.SubprocessError): return
    cache = Path("/run/winlux/window-icons.json")
    try: known = json.loads(cache.read_text())
    except (OSError, ValueError): known = {}
    current = {}
    for line in windows.splitlines():
        fields = line.split(None, 4)
        if len(fields) < 3: continue
        window, pid = fields[0], fields[2]
        if not pid.isdigit() or pid == "0": continue
        try:
            args = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
            if Path(os.fsdecode(args[0])).name != "winlux": continue
            target = next((Path(os.fsdecode(arg)) for arg in args[1:] if arg.lower().endswith(b".exe")), None)
            if target is None or not target.is_file(): continue
            identity = hashlib.sha256(str(target).encode()).hexdigest()[:24]
            key = f"{pid}:{target.stat().st_mtime_ns}"
            current[window] = key
            if known.get(window) == key: continue
            icon = icon_for(target, identity)
            if icon == "application-x-executable":
                environment = Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
                mode = next((item.split(b"=", 1)[1].decode() for item in environment if item.startswith(b"WINLUX_SHELL_APP=")), "")
                if mode not in ("explorer", "settings"): continue
                icon = f"/usr/share/winlux/native/icons/{mode}.svg"
            pix = ctypes.CDLL("libgdk_pixbuf-2.0.so.0")
            obj = ctypes.CDLL("libgobject-2.0.so.0")
            pix.gdk_pixbuf_new_from_file_at_scale.argtypes = [ctypes.c_char_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
            pix.gdk_pixbuf_new_from_file_at_scale.restype = ctypes.c_void_p
            pointer = pix.gdk_pixbuf_new_from_file_at_scale(os.fsencode(icon), 48, 48, 1, None)
            if not pointer: continue
            try:
                values = []
                for name in ["width", "height", "rowstride", "n_channels"]:
                    fn = getattr(pix, "gdk_pixbuf_get_" + name)
                    fn.argtypes, fn.restype = [ctypes.c_void_p], ctypes.c_int
                    values.append(fn(pointer))
                width, height, stride, channels = values
                pix.gdk_pixbuf_get_pixels.argtypes, pix.gdk_pixbuf_get_pixels.restype = [ctypes.c_void_p], ctypes.c_void_p
                pixels = ctypes.string_at(pix.gdk_pixbuf_get_pixels(pointer), stride * (height - 1) + width * channels)
                argb = [width, height]
                for y in range(height):
                    for x in range(width):
                        offset = y * stride + x * channels
                        r, g, b = pixels[offset:offset + 3]
                        alpha = pixels[offset + 3] if channels == 4 else 255
                        argb.append((alpha << 24) | (r << 16) | (g << 8) | b)
                publish_icon(window, argb)
            finally:
                obj.g_object_unref.argtypes = [ctypes.c_void_p]
                obj.g_object_unref(pointer)
        except (OSError, ValueError, StopIteration, subprocess.SubprocessError) as error:
            current.pop(window, None)
            detail = getattr(error, "stderr", b"")
            if isinstance(detail, bytes): detail = detail.decode(errors="replace")
            print(f"Window icon {window}: {type(error).__name__} {detail[:200]}", file=sys.stderr)
            continue
    cache.parent.mkdir(parents=True, exist_ok=True)
    cache.write_text(json.dumps(current))

def publish_icon(window, values):
    # Xlib expects native unsigned longs for a 32-bit CARDINAL property on LP64.
    xlib = ctypes.CDLL("libX11.so.6")
    xlib.XOpenDisplay.argtypes, xlib.XOpenDisplay.restype = [ctypes.c_char_p], ctypes.c_void_p
    xlib.XInternAtom.argtypes, xlib.XInternAtom.restype = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int], ctypes.c_ulong
    xlib.XChangeProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_void_p, ctypes.c_int]
    xlib.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
    xlib.XCloseDisplay.argtypes = [ctypes.c_void_p]
    xlib.XSetErrorHandler.argtypes, xlib.XSetErrorHandler.restype = [ctypes.c_void_p], ctypes.c_void_p
    display = xlib.XOpenDisplay(None)
    if not display: raise OSError("No X display for icon")
    errors = []
    callback_type = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)
    def ignore_closed_window(_display, _event):
        errors.append(True)
        return 0
    callback = callback_type(ignore_closed_window)
    previous = xlib.XSetErrorHandler(ctypes.cast(callback, ctypes.c_void_p))
    try:
        atom = xlib.XInternAtom(display, b"_NET_WM_ICON", 0)
        pixels = (ctypes.c_ulong * len(values))(*values)
        xlib.XChangeProperty(display, int(window, 16), atom, 6, 32, 0, ctypes.cast(pixels, ctypes.c_void_p), len(values))
        xlib.XSync(display, 0)
    finally:
        xlib.XCloseDisplay(display)
        xlib.XSetErrorHandler(previous)
    if errors: raise OSError("Window closed while setting icon")

def launcher(path, name, profile="desktop", extra=()):
    path = path.resolve()
    if not valid_exe(path): return None
    identity = hashlib.sha256((str(path) + "\0" + profile + "\0".join(extra)).encode()).hexdigest()[:24]
    filename = f"winlux-pe-{identity}.desktop"
    icon = icon_for(path, identity)
    command = " ".join(argument(value) for value in [EXE_LAUNCHER, profile, path, *extra])
    name = name.replace("\n", " ").replace("\r", " ").replace("\0", " ")[:200]
    managed_desktop(APPS / filename, f"[Desktop Entry]\nType=Application\nName={escaped(name)}\nExec={command}\nIcon={escaped(icon)}\nTerminal=false\nCategories=Utility;WinluxWindows;\nX-Winlux-Executable={escaped(path)}\nActions=Location;\n\n[Desktop Action Location]\nName=Ouvrir l’emplacement\nExec=/usr/bin/winlux-shell-app explorer {argument(path.parent)}\n")
    return filename

def records(home):
    path = home / ".local/share/winlux/launchers.json"
    try:
        if path.stat().st_size > 4 * 1024 * 1024: return []
        value = json.loads(path.read_text())
        return value if isinstance(value, list) else []
    except (OSError, ValueError): return []

def register(path):
    path = path.resolve()
    if not valid_exe(path): return
    STATE.mkdir(parents=True, exist_ok=True)
    with (STATE / "launchers.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        entries = [item for item in records(HOME) if isinstance(item, dict) and item.get("path") != str(path)]
        entries.append({"path": str(path), "name": path.stem})
        save(STATE / "launchers.json", json.dumps(entries[-4096:], ensure_ascii=False))

def command_arguments(command):
    """CommandLineToArgvW-style quotes/backslashes, without invoking a shell."""
    args, current, quoted, index, started = [], [], False, 0, False
    while index < len(command):
        char = command[index]
        if char in " \t" and not quoted:
            if started: args.append("".join(current)); current, started = [], False
            index += 1
            continue
        if char == "\\":
            end = index
            while end < len(command) and command[end] == "\\": end += 1
            count = end - index
            if end < len(command) and command[end] == '"':
                current.extend("\\" * (count // 2))
                if count % 2: current.append('"')
                else: quoted = not quoted
                index = end + 1
            else: current.extend("\\" * count); index = end
        elif char == '"': quoted = not quoted; index += 1
        else: current.append(char); index += 1
        started = True
    if started: args.append("".join(current))
    return args

def registry_strings(home):
    """Every string value of the Winlux registry: lower-case key -> {lower-case name: text}."""
    path = home / ".local/share/winlux/registry.tsv"
    keys = {}
    try:
        if path.stat().st_size > 16 * 1024 * 1024: return keys
        lines = path.read_text(errors="replace").splitlines()
    except OSError: return keys
    for line in lines:
        parts = line.split("\t")
        if len(parts) < 5 or parts[0] != "V": continue
        try:
            key, name = (bytes.fromhex(value).decode("utf-8").lower() for value in parts[1:3])
            raw = bytes.fromhex(parts[4])
            if parts[3] in ("1", "2"):
                value = raw.decode("utf-16-le").rstrip("\0") if len(raw) % 2 == 0 and b"\0" in raw else raw.rstrip(b"\0").decode("cp1252")
            else: value = ""
        except (ValueError, UnicodeError): continue
        keys.setdefault(key, {})[name] = value
    return keys

def installed_registry(home, registry=None):
    registry = registry_strings(home) if registry is None else registry
    entries = []
    for key, values in registry.items():
        if "\\uninstall\\" not in key: continue
        name = values.get("displayname")
        icon = values.get("displayicon", "").rsplit(",", 1)[0]
        if name and icon:
            target = windows_path(expand(icon), home)
            if valid_exe(target): entries.append((target, name))
    return entries

# Variables d'environnement Windows des commandes du registre (REG_EXPAND_SZ),
# avec la même disposition de C: que le runtime et msiexec.
ENVIRONMENT = {"programfiles": "C:\\Program Files", "programw6432": "C:\\Program Files",
               "programfiles(x86)": "C:\\Program Files (x86)", "commonprogramfiles": "C:\\Program Files\\Common Files",
               "commonprogramfiles(x86)": "C:\\Program Files (x86)\\Common Files", "systemroot": "C:\\windows",
               "windir": "C:\\windows", "systemdrive": "C:", "userprofile": "C:", "homedrive": "C:",
               "appdata": "C:\\AppData\\Roaming", "localappdata": "C:\\AppData\\Local", "programdata": "C:\\ProgramData",
               "allusersprofile": "C:\\ProgramData", "public": "C:\\Users\\Public"}

def expand(value):
    def variable(match):
        return ENVIRONMENT.get(match.group(1).lower(), match.group(0))
    return re.sub(r"%([A-Za-z0-9_()]+)%", variable, value)

CLASS_ROOTS = ("hkcr\\", "hklm\\software\\classes\\", "hkcu\\software\\classes\\")
USER_CHOICE = "hkcu\\software\\microsoft\\windows\\currentversion\\explorer\\fileexts\\"
FILE_TOKENS = ("%1", "%l", "%v")

def classes(registry):
    """HKEY_CLASSES_ROOT as Windows shows it: HKCU over HKLM over the stored HKCR."""
    view = {}
    for prefix in CLASS_ROOTS:
        for key, values in registry.items():
            if key.startswith(prefix): view.setdefault(key[len(prefix):], {}).update(values)
    return view

def verb_command(view, name):
    shell = name + "\\shell\\"
    verbs = [verb.strip().lower() for verb in view.get(name + "\\shell", {}).get("", "").split(",") if verb.strip()]
    verbs.append("open")
    verbs += sorted(key[len(shell):-8] for key in view
                    if key.startswith(shell) and key.endswith("\\command") and "\\" not in key[len(shell):-8])
    for verb in verbs:
        command = view.get(f"{shell}{verb}\\command", {}).get("", "").strip()
        if command: return command
    return None

def handler(command, home):
    """Registry command -> (executable, template); %1 becomes the opened file (None)."""
    words = command_arguments(expand(command))
    if not words: return None
    executable = windows_path(words[0], home)
    if not valid_exe(executable): return None
    template, placed = [], False
    for word in words[1:]:
        lower = word.lower()
        if any(token in lower for token in FILE_TOKENS):
            if not placed: template.append(None)
            placed = True
        elif lower == "%*": continue
        elif "%" in word and re.search(r"%[0-9]", word): continue
        else: template.append(word)
    if not placed: template.append(None)  # ShellExecute appends the file.
    return executable, tuple(template)

def associations(home, registry=None):
    """{".ext": {"handlers": [(exe, template, progid)], "default": index or None, "label": text}}"""
    registry = registry_strings(home) if registry is None else registry
    view = classes(registry)
    applications = {key.split("\\")[1]: key for key in view if key.startswith("applications\\") and key.count("\\") == 1}
    found = {}
    for key in sorted({name.split("\\", 1)[0] for name in view if name.startswith(".")}):
        if len(key) > 32 or not re.fullmatch(r"\.[a-z0-9_+~-]+", key): continue
        values = view.get(key, {})
        progids, default = [], values.get("", "").strip().lower()
        choice = registry.get(USER_CHOICE + key + "\\userchoice", {}).get("progid", "").strip().lower()
        for name in [choice, default] + sorted(view.get(key + "\\openwithprogids", {})):
            if name and name not in progids: progids.append(name)
        label, chosen = "", choice or default
        entry = {"handlers": [], "default": None, "label": ""}
        for progid in progids:
            command = verb_command(view, progid)
            resolved = handler(command, home) if command else None
            if not resolved: continue
            label = label or view.get(progid, {}).get("", "").strip()
            if progid == chosen: entry["default"] = len(entry["handlers"])
            entry["handlers"].append((resolved[0], resolved[1], progid))
        for executable in sorted(view.get(key + "\\openwithlist", {}).values()) + sorted(
                name for name in view.get(key + "\\openwithlist", {}) if name.endswith(".exe")):
            branch = applications.get(executable.strip().lower())
            command = verb_command(view, branch) if branch else None
            resolved = handler(command, home) if command else None
            if resolved: entry["handlers"].append((resolved[0], resolved[1], branch))
        for name, branch in applications.items():
            if key in view.get(branch + "\\supportedtypes", {}):
                command = verb_command(view, branch)
                resolved = handler(command, home) if command else None
                if resolved: entry["handlers"].append((resolved[0], resolved[1], branch))
        unique, seen = [], {}
        for index, item in enumerate(entry["handlers"]):
            identity = (item[0], item[1])
            if identity in seen:
                if entry["default"] == index: entry["default"] = seen[identity]
                continue
            if entry["default"] == index: entry["default"] = len(unique)
            seen[identity] = len(unique)
            unique.append(item)
        entry["handlers"], entry["label"] = unique, label[:120]
        if unique: found[key] = entry
    return found

def mime_globs():
    """Extension -> MIME type known to the shared MIME database (highest weight wins)."""
    known = {}
    directories = [DATA] + [Path(item) for item in os.environ.get("XDG_DATA_DIRS", "/usr/local/share:/usr/share").split(":") if item]
    for directory in reversed(directories):
        if directory == DATA: continue  # Our own custom types are recomputed below.
        try: lines = (directory / "mime/globs2").read_text(errors="replace").splitlines()
        except OSError: continue
        for line in lines:
            parts = line.split(":")
            if len(parts) < 3 or line.startswith("#"): continue
            weight, kind, pattern = parts[0], parts[1], parts[2]
            if not pattern.startswith("*.") or any(char in pattern[2:] for char in "*?[") or "cs" in parts[3:4]: continue
            extension = pattern[1:].lower()
            try: weight = int(weight)
            except ValueError: continue
            if extension not in known or weight > known[extension][0]: known[extension] = (weight, kind)
    return {extension: kind for extension, (weight, kind) in known.items()}

def custom_types(entries, known):
    """Declare a MIME type for extensions Linux does not know, so they can be opened."""
    types, declared = {}, []
    for extension, entry in sorted(entries.items()):
        if extension in known:
            types[extension] = known[extension]
            continue
        kind = "application/x-winlux-" + extension[1:]
        types[extension] = kind
        comment = entry["label"] or f"Fichier {extension[1:].upper()}"
        declared.append(f'  <mime-type type="{xml_escape(kind)}">\n    <comment>{xml_escape(comment)}</comment>\n'
                        f'    <glob pattern="*{xml_escape(extension)}"/>\n  </mime-type>\n')
    package = DATA / "mime/packages/winlux-associations.xml"
    text = ('<?xml version="1.0" encoding="UTF-8"?>\n<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">\n'
            + "".join(declared) + "</mime-info>\n")
    current = package.read_text(errors="replace") if package.is_file() else None
    if declared and current != text:
        save(package, text)
        update_database("update-mime-database", DATA / "mime")
    elif not declared and current is not None:
        package.unlink()
        update_database("update-mime-database", DATA / "mime")
    return types

def xml_escape(value):
    return str(value).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")

def update_database(tool, directory):
    try: subprocess.run([tool, str(directory)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=60, check=False)
    except (OSError, subprocess.SubprocessError): pass

def association_launcher(executable, template, name, kinds, profile="desktop"):
    """Hidden launcher listed by "Open with" and used for double-click; %f is the file."""
    identity = hashlib.sha256(("\0".join([str(executable), profile, "assoc"] + [item or "\0" for item in template])).encode()).hexdigest()[:24]
    filename = f"winlux-assoc-{identity}.desktop"
    icon = icon_for(executable, hashlib.sha256(str(executable).encode()).hexdigest()[:24])
    words = [argument(EXE_LAUNCHER), argument(profile + "-open"), argument(executable)]
    words += ["%f" if item is None else argument(item) for item in template]
    name = name.replace("\n", " ").replace("\r", " ").replace("\0", " ")[:200]
    managed_desktop(APPS / filename, f"[Desktop Entry]\nType=Application\nName={escaped(name)}\nExec={' '.join(words)}\n"
                    f"Icon={escaped(icon)}\nTerminal=false\nNoDisplay=true\nMimeType={''.join(kind + ';' for kind in sorted(kinds))}\n"
                    f"X-Winlux-Executable={escaped(executable)}\n")
    return filename

MIMEAPPS = Path(os.environ.get("XDG_CONFIG_HOME", HOME / ".config")) / "mimeapps.list"

def read_mimeapps(path=None):
    path = path or MIMEAPPS
    parser = configparser.ConfigParser(interpolation=None, strict=False, delimiters=("=",))
    parser.optionxform = str
    try: parser.read_string(path.read_text(errors="replace"))
    except (OSError, configparser.Error): pass
    return parser

def write_mimeapps(parser, path=None):
    path = path or MIMEAPPS
    lines = []
    for section in parser.sections():
        lines.append(f"[{section}]")
        lines += [f"{key}={value}" for key, value in parser.items(section)]
        lines.append("")
    save(path, "\n".join(lines))

def default_of(parser, kind):
    if not parser.has_option("Default Applications", kind): return None
    return next((item for item in parser.get("Default Applications", kind).split(";") if item.strip()), None)

def promote(parser, kind, desktop):
    """Put desktop first for kind; the other existing choices stay behind it."""
    if not parser.has_section("Default Applications"): parser.add_section("Default Applications")
    current = parser.get("Default Applications", kind) if parser.has_option("Default Applications", kind) else ""
    others = [item for item in current.split(";") if item.strip() and item != desktop]
    parser.set("Default Applications", kind, ";".join([desktop] + others) + ";")

def demote(parser, kind, desktop):
    if not parser.has_option("Default Applications", kind): return
    rest = [item for item in parser.get("Default Applications", kind).split(";") if item.strip() and item != desktop]
    if rest: parser.set("Default Applications", kind, ";".join(rest) + ";")
    else: parser.remove_option("Default Applications", kind)

def refresh_associations(names):
    """Registry file associations -> hidden launchers, MIME types and new defaults."""
    keep, registry = set(), registry_strings(HOME)
    entries = associations(HOME, registry)
    types = custom_types(entries, mime_globs())
    grouped = {}
    for extension, entry in entries.items():
        for executable, template, _ in entry["handlers"]:
            grouped.setdefault((executable, template), set()).add(types[extension])
    files = {}
    for (executable, template), kinds in grouped.items():
        name = names.get(str(executable)) or executable.stem
        files[(executable, template)] = association_launcher(executable, template, name, kinds)
        keep.add(files[(executable, template)])
    # An installer that registers itself as the default wins, as on Windows,
    # unless the user picked another application for that type since then.
    state_path = STATE / "associations.json"
    try: claimed = json.loads(state_path.read_text())
    except (OSError, ValueError): claimed = {}
    if not isinstance(claimed, dict): claimed = {}
    parser = read_mimeapps()
    before = {kind: default_of(parser, kind) for kind in claimed}
    wanted = {}
    for extension, entry in sorted(entries.items()):
        if entry["default"] is not None:
            executable, template, _ = entry["handlers"][entry["default"]]
            wanted[types[extension]] = files[(executable, template)]
    changed = False
    for kind, desktop in wanted.items():
        current = default_of(parser, kind)
        if current == desktop:
            claimed[kind] = desktop
            continue
        if current is not None and current != claimed.get(kind): continue  # The user's own choice.
        promote(parser, kind, desktop)
        claimed[kind], changed = desktop, True
    for kind, desktop in list(claimed.items()):
        if kind in wanted and claimed[kind] == wanted[kind]: continue
        if desktop not in keep and before.get(kind) == desktop:
            demote(parser, kind, desktop)  # Uninstalled: the previous choice applies again.
            changed = True
        if kind not in wanted: del claimed[kind]
    if changed: write_mimeapps(parser)
    STATE.mkdir(parents=True, exist_ok=True)
    save(state_path, json.dumps(claimed, ensure_ascii=False, sort_keys=True))
    return keep

def lnk_strings(data):
    """Parse a Shell Link (.lnk): target, arguments, working folder and icon."""
    if len(data) < 76 or struct.unpack_from("<I", data)[0] != 76: raise ValueError("Not a shortcut")
    flags = struct.unpack_from("<I", data, 20)[0]
    icon_index = struct.unpack_from("<i", data, 56)[0]
    cursor, target, values = 76, None, {}
    if flags & 1: cursor += 2 + struct.unpack_from("<H", data, cursor)[0]
    if flags & 2:
        size, header = struct.unpack_from("<II", data, cursor)
        offset = struct.unpack_from("<I", data, cursor + 28)[0] if header >= 36 else 0
        if offset:
            raw = data[cursor + offset:cursor + size]
            end = next((i for i in range(0, len(raw) - 1, 2) if raw[i:i + 2] == b"\0\0"), len(raw))
            target = raw[:end].decode("utf-16-le")
        else:
            offset = struct.unpack_from("<I", data, cursor + 16)[0]
            if offset: target = data[cursor + offset:cursor + size].split(b"\0", 1)[0].decode("cp1252")
        cursor += size
    for bit, name in [(4, "description"), (8, "relative"), (16, "workdir"), (32, "arguments"), (64, "icon")]:
        if not flags & bit: continue
        length = struct.unpack_from("<H", data, cursor)[0]
        cursor += 2
        width = 2 if flags & 128 else 1
        raw = data[cursor:cursor + length * width]
        if len(raw) != length * width: raise ValueError("Truncated shortcut")
        values[name] = raw.decode("utf-16-le" if width == 2 else "cp1252")
        cursor += length * width
    values["target"], values["icon_index"] = target, icon_index
    return values

def lnk_resolve(path, home=None):
    """Linux target, arguments, working folder and icon file of a .lnk."""
    home = home or HOME
    if path.stat().st_size > 1024 * 1024: raise ValueError("Shortcut too large")
    values = lnk_strings(path.read_bytes())
    target = None
    if values.get("target"): target = windows_path(expand(values["target"]), home)
    elif values.get("relative"):
        relative = expand(values["relative"])
        target = windows_path(relative, home) if relative[1:2] == ":" else (path.parent / relative.replace("\\", "/")).resolve()
    if target is None: raise ValueError("Shortcut without file target")
    workdir = windows_path(expand(values["workdir"]), home) if values.get("workdir") else None
    icon = windows_path(expand(values["icon"]), home) if values.get("icon") else None
    return target, command_arguments(expand(values.get("arguments", ""))), workdir, icon, values["icon_index"]

def shortcuts(home):
    roots = [home / "AppData/Roaming/Microsoft/Windows/Start Menu/Programs",
             home / "ProgramData/Microsoft/Windows/Start Menu/Programs", home / "Start Menu/Programs"]
    count = 0
    for root in roots:
        if not root.is_dir(): continue
        for path in root.rglob("*.lnk"):
            count += 1
            if count > 4096: return
            try:
                executable, extra, _, _, _ = lnk_resolve(path, home)
                if valid_exe(executable): yield executable, path.stem, extra
            except (OSError, ValueError, UnicodeError, struct.error): continue

def thumbnail(source, output, size):
    """Tumbler thumbnailer: the icon of a .exe/.dll, or of the target of a .lnk, as PNG."""
    index, icon_file = 0, source
    with source.open("rb") as stream: magic = stream.read(4)
    if source.suffix.lower() == ".lnk" or magic == b"L\0\0\0":
        target, _, _, icon, icon_index = lnk_resolve(source)
        icon_file, index = (icon, icon_index) if icon is not None and icon.is_file() else (target, 0)
    if icon_file.suffix.lower() == ".ico": data = icon_file.read_bytes()[:4 * 1024 * 1024]
    else: data = pe_icon(icon_file, index)
    if not data: raise SystemExit(1)
    gi = __import__("gi")
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf
    loader = GdkPixbuf.PixbufLoader.new_with_type("ico")
    loader.write(data)
    loader.close()
    pixbuf = loader.get_pixbuf()
    scale = min(1.0, size / max(pixbuf.get_width(), pixbuf.get_height())) if size > 0 else 1.0
    if scale < 1.0:
        pixbuf = pixbuf.scale_simple(max(1, int(pixbuf.get_width() * scale)), max(1, int(pixbuf.get_height() * scale)), GdkPixbuf.InterpType.BILINEAR)
    pixbuf.savev(str(output), "png", [], [])

def browsers():
    values = [("edge", "Microsoft Edge", "opt/microsoft/msedge/microsoft-edge", "opt/microsoft/msedge/product_logo_48.png"),
              ("chrome", "Google Chrome", "opt/google/chrome/google-chrome", "opt/google/chrome/product_logo_48.png"),
              ("firefox", "Mozilla Firefox", "usr/lib/firefox/firefox", "usr/lib/firefox/browser/chrome/icons/default/default48.png")]
    keep = set()
    for key, name, binary, icon in values:
        root = Path("/opt/winlux/browsers") / key / "root"
        if (root / binary).is_file():
            filename = f"winlux-browser-{key}.desktop"
            keep.add(filename)
            managed_desktop(APPS / filename, f"[Desktop Entry]\nType=Application\nName={name}\nExec=/usr/bin/winlux-browsers --launch {key} %U\nIcon={root / icon}\nTerminal=false\nCategories=Network;WebBrowser;\n")
    return keep

def refresh():
    APPS.mkdir(parents=True, exist_ok=True)
    keep = browsers()
    homes = [(HOME, "desktop")]
    if os.getuid() == 0 and Path("/home/winlux-web").is_dir(): homes.append((Path("/home/winlux-web"), "browser"))
    names = {}
    for home, profile in homes:
        entries = [(Path(item["path"]), item.get("name", Path(item["path"]).stem)) for item in records(home)
                   if isinstance(item, dict) and isinstance(item.get("path"), str) and isinstance(item.get("name", ""), str)]
        entries += installed_registry(home)
        for target, name in entries:
            filename = launcher(target, name, profile)
            if filename: keep.add(filename)
            if home == HOME: names[str(target.resolve())] = name
        for target, name, extra in shortcuts(home):
            filename = launcher(target, name, profile, extra)
            if filename: keep.add(filename)
            if home == HOME: names.setdefault(str(target.resolve()), name)
    keep |= refresh_associations(names)
    for pattern in ["winlux-pe-*.desktop", "winlux-browser-*.desktop", "winlux-assoc-*.desktop"]:
        for path in APPS.glob(pattern):
            if path.name not in keep:
                path.unlink()
                CHANGED.append(path)
    # GIO reads MimeType= from this cache: without it, "Open with" stays empty.
    if any(path.parent == APPS for path in CHANGED) or (
            any(APPS.glob("winlux-assoc-*.desktop")) and not (APPS / "mimeinfo.cache").is_file()):
        update_database("update-desktop-database", APPS)

def print_shortcut(path):
    """Target, working folder then one argument per line, for winlux-open (sh)."""
    target, arguments, workdir, _, _ = lnk_resolve(path)
    values = [str(target), str(workdir) if workdir else ""] + arguments
    if any("\n" in value or "\0" in value for value in values): raise SystemExit(1)
    sys.stdout.write("".join(value + "\n" for value in values))

def main():
    command = sys.argv[1] if len(sys.argv) > 1 else "refresh"
    if command == "window-icons": window_icons()
    elif command == "register" and len(sys.argv) > 2:
        register(Path(sys.argv[2]))
        refresh()
    elif command == "resolve-lnk" and len(sys.argv) > 2:
        try: print_shortcut(Path(sys.argv[2]))
        except (OSError, ValueError, UnicodeError, struct.error) as error:
            print(f"Raccourci illisible : {error}", file=sys.stderr)
            raise SystemExit(1)
    elif command == "thumbnail" and len(sys.argv) > 4:
        try: thumbnail(Path(sys.argv[2]), Path(sys.argv[3]), int(sys.argv[4]))
        except (OSError, ValueError, UnicodeError, struct.error, KeyError): raise SystemExit(1)
    elif command == "refresh": refresh()
    else: raise SystemExit(2)

if __name__ == "__main__": main()
