#!/usr/bin/python3
"""Full-screen shutdown/reboot overlay with a spinner, Windows-style."""
import os
import signal
import subprocess
import sys
import time

DISPLAY = os.environ.get("DISPLAY", ":0")

def have(cmd):
    for d in os.environ.get("PATH", "/usr/bin:/bin").split(":"):
        if os.path.isfile(os.path.join(d, cmd)):
            return True
    return False

def screen_size():
    try:
        out = subprocess.check_output(
            ["xrandr", "--query"],
            env={**os.environ, "DISPLAY": DISPLAY},
            text=True, stderr=subprocess.DEVNULL,
        )
        for line in out.splitlines():
            if "*" in line:
                mode = line.split()[0]
                w, h = mode.split("x")
                return int(w), int(h)
    except Exception:
        pass
    return 1280, 720

def main():
    action = sys.argv[1] if len(sys.argv) > 1 else "poweroff"
    if action == "reboot":
        message = "Redémarrage en cours..."
    elif action == "restart-session":
        message = "Redémarrage de la session..."
    else:
        message = "Fermeture en cours..."

    if not have("python3"):
        return

    w, h = screen_size()

    # Use xdotool + xprop to create a simple fullscreen overlay via a
    # temporary X window.  We draw with a tiny Python Tkinter UI if
    # available, otherwise fall back to xmessage.
    try:
        import tkinter as tk
    except ImportError:
        # Fallback: use xmessage centered on screen.
        if have("xmessage"):
            env = {**os.environ, "DISPLAY": DISPLAY}
            proc = subprocess.Popen(
                ["xmessage", "-center", "-timeout", "30", message],
                env=env,
            )
            return proc.pid
        return None

    root = tk.Tk()
    root.title("Winlux")
    root.attributes("-fullscreen", True)
    root.configure(bg="#000000")
    root.overrideredirect(True)

    # Keep above everything.
    root.attributes("-topmost", True)

    canvas = tk.Canvas(root, bg="#000000", highlightthickness=0,
                       width=w, height=h)
    canvas.pack(fill="both", expand=True)

    # Message text.
    canvas.create_text(
        w // 2, h // 2 - 30,
        text=message,
        fill="#FFFFFF",
        font=("Segoe UI", 18, "normal"),
        anchor="center",
    )

    # Spinner: rotating dots.
    dots = []
    cx, cy = w // 2, h // 2 + 30
    import math
    n_dots = 8
    radius = 20
    dot_r = 3
    for i in range(n_dots):
        angle = 2 * math.pi * i / n_dots - math.pi / 2
        dx = cx + radius * math.cos(angle)
        dy = cy + radius * math.sin(angle)
        dot = canvas.create_oval(
            dx - dot_r, dy - dot_r, dx + dot_r, dy + dot_r,
            fill="#333333", outline="",
        )
        dots.append(dot)

    frame = [0]

    def animate():
        active = frame[0] % n_dots
        for i, dot in enumerate(dots):
            dist = (i - active) % n_dots
            if dist == 0:
                color = "#FFFFFF"
            elif dist == 1 or dist == n_dots - 1:
                color = "#AAAAAA"
            elif dist == 2 or dist == n_dots - 2:
                color = "#666666"
            else:
                color = "#333333"
            canvas.itemconfigure(dot, fill=color)
        frame[0] += 1
        root.after(120, animate)

    animate()

    # Auto-close after 30 seconds in case shutdown stalls.
    root.after(30000, root.destroy)

    # Don't block: run in a subprocess.
    root.mainloop()


if __name__ == "__main__":
    # Fork so the caller doesn't block.
    pid = os.fork()
    if pid > 0:
        print(pid)
        sys.exit(0)
    # Child: detach from terminal.
    os.setsid()
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    try:
        main()
    except Exception:
        pass
