#!/usr/bin/python3
"""Run only XScreenSaver's graphical worker without administrator privileges.

The unmodified main daemon retains its root-session no-lock policy. No password
authentication is overridden. Non-root desktops use the original worker as-is.
"""
import os
from pathlib import Path
import pwd
import subprocess
import sys
import tempfile

binary = "/usr/libexec/xscreensaver/xscreensaver-gfx.real"
if Path("/usr/libexec/xscreensaver/xscreensaver-gfx.distribution").is_file():
    binary = "/usr/libexec/xscreensaver/xscreensaver-gfx.distribution"
if os.geteuid() == 0:
    account = pwd.getpwnam("winlux-saver")
    home = Path(account.pw_dir)
    configuration = Path.home() / ".xscreensaver"
    if not configuration.is_file(): configuration = Path("/usr/share/winlux/native/xscreensaver")
    # O_EXCL creation and fd-based ownership avoid following user-supplied links.
    # Keep the temporary on the destination filesystem for atomic replacement.
    fd, temporary = tempfile.mkstemp(prefix=".saver-config-", dir=home)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(configuration.read_bytes())
            os.fchmod(stream.fileno(), 0o600)
            os.fchown(stream.fileno(), account.pw_uid, account.pw_gid)
        os.replace(temporary, home / ".xscreensaver")
    finally:
        if os.path.exists(temporary): os.unlink(temporary)
    subprocess.run(["/usr/bin/xhost", "+SI:localuser:winlux-saver"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    os.setgroups([])
    os.setgid(account.pw_gid)
    os.setuid(account.pw_uid)
    os.environ.update(HOME=str(home), USER=account.pw_name, LOGNAME=account.pw_name, LIBGL_ALWAYS_SOFTWARE="1")
    for variable in ["XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS"]: os.environ.pop(variable, None)
os.execv(binary, ["xscreensaver-gfx", *sys.argv[1:]])
