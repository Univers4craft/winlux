"""Interface graphique (Tk, présente avec Python sous Windows comme sous Linux)."""

import json
import os
import queue
import shutil
import subprocess
import sys
import threading
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox, simpledialog, ttk

from . import disks, image, version, writer
from .disks import DiskError
from .i18n import error_text, human_size, tr
from .image import Cancelled, ImageError

ACCENT = "#6d4aff"       # violet du logo Prisme
ACCENT_DARK = "#5536d6"
BG = "#f6f6fb"
CARD = "#ffffff"
TEXT = "#1b1b29"
MUTED = "#5d5d75"
MIN_STICK = 4 * 1000 ** 3


def _logo_path():
    here = Path(__file__).resolve().parent
    for candidate in (here / "logo.png", here.parents[1] / "assets/brand/winlux-logo-64.png"):
        if candidate.is_file():
            return candidate
    return None


def _launcher():
    """Commande qui relance cet outil (pour l'écriture avec droits administrateur)."""
    if getattr(sys, "frozen", False):
        return [sys.executable]
    script = os.path.realpath(sys.argv[0]) if sys.argv and sys.argv[0] else ""
    if script.endswith(("winlux-usb-creator", "winlux-usb-creator.py")) and os.path.isfile(script):
        if os.access(script, os.X_OK) and script.startswith("/usr/"):
            return [script]  # chemin annoncé dans la règle polkit du paquet
        return [os.path.realpath(sys.executable), script]
    return [os.path.realpath(sys.executable), "-m", "winlux_usb"]


class Job:
    """Création de la clé dans un fil séparé ; les événements passent par une file."""

    def __init__(self, app, source, disk, verify):
        self.app = app
        self.events = app.events
        self.source = source      # ("latest", release) ou ("file", chemin)
        self.disk = disk
        self.verify = verify
        self.cancel = threading.Event()
        self.proc = None
        self.speed = writer.Speed()

    def post(self, *event):
        self.events.put(event)

    def status(self, text, fraction=None):
        self.post("status", text, fraction)

    def run(self):
        try:
            path = self.obtain_image()
            self.write(path)
            self.post("done")
        except (ImageError, DiskError) as exc:
            self.post("error", exc.key, exc.fields)
        except Cancelled:
            self.post("error", "err_cancelled", {})
        except Exception as exc:  # noqa: BLE001 - toujours rendre la main à l'interface
            self.post("error", "err_internal", {"error": repr(exc)})

    def checksum_progress(self, done, total):
        self.status(tr("phase_checksum", pct=done * 100 // max(total, 1)), done / max(total, 1))

    def obtain_image(self):
        kind, value = self.source
        if kind == "file":
            image.check_image(value)
            expected = image.sidecar_sha256(value)
            if expected and image.sha256_file(value, self.checksum_progress, self.cancel) != expected:
                raise ImageError("err_checksum")
            return value
        release = value
        if release is None:
            self.status(tr("phase_release"))
            release = image.latest_release()

        def dl(done, total):
            self.status(tr("phase_download", done=human_size(done), total=human_size(total)),
                        done / max(total, 1))
        path = image.download(release, None, dl, self.cancel, self.checksum_progress)
        image.check_image(path)
        return str(path)

    def progress(self, phase, done, total):
        frac = done / max(total, 1)
        if phase == "prepare":
            self.status(tr("phase_prepare"), 0)
        elif phase == "write":
            rate = self.speed.update(done)
            self.status(tr("phase_write", done=human_size(done), total=human_size(total),
                           speed=human_size(rate) if rate else "…"), frac)
        elif phase == "verify":
            self.status(tr("phase_verify", pct=int(frac * 100)), frac)

    def write(self, path):
        if writer.is_admin():
            writer.write_image(path, self.disk.path, self.progress, self.cancel, self.verify)
            return
        if os.name == "nt":
            raise DiskError("err_admin")
        self.write_elevated(path)

    def elevation(self):
        """Préfixe sudo/pkexec, ou mot de passe à fournir à `sudo -S`."""
        if shutil.which("sudo") and subprocess.run(["sudo", "-n", "true"], capture_output=True).returncode == 0:
            return ["sudo", "-n"], None
        if shutil.which("pkexec") and (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")):
            return ["pkexec"], None
        if shutil.which("sudo"):
            answer = self.app.ask_password()
            if answer is None:
                raise DiskError("err_auth")
            return ["sudo", "-S", "-k", "-p", ""], answer
        raise DiskError("err_admin")

    def write_elevated(self, path):
        prefix, password = self.elevation()
        cmd = prefix + _launcher() + ["write", "--image", os.path.abspath(path), "--device", self.disk.path,
                                      "--machine", "--yes"]
        if not self.verify:
            cmd.append("--no-verify")
        self.status(tr("phase_auth"), 0)
        self.proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     text=True, bufsize=1)
        if password is not None:
            self.proc.stdin.write(password + "\n")
            self.proc.stdin.flush()
        watcher = threading.Thread(target=self._forward_cancel, daemon=True)
        watcher.start()
        error = None
        finished = False
        for line in self.proc.stdout:
            parts = line.strip().split(" ", 3)
            if parts[0] == "@progress" and len(parts) == 4:
                self.progress(parts[1], int(parts[2]), int(parts[3]))
            elif parts[0] == "@done":
                finished = True
            elif parts[0] == "@error":
                fields = {}
                if len(parts) > 2:
                    try:
                        fields = json.loads(line.strip().split(" ", 2)[2])
                    except ValueError:
                        pass
                error = (parts[1] if len(parts) > 1 else "err_internal", fields)
        code = self.proc.wait()
        stderr = self.proc.stderr.read()
        if error:
            if error[0] == "err_cancelled":
                raise Cancelled()
            raise DiskError(error[0], **error[1])
        if not finished:
            if code in (126, 127) or (password is not None and code == 1):
                raise DiskError("err_auth")
            raise DiskError("err_internal", error=stderr.strip()[-300:] or f"code {code}")

    def _forward_cancel(self):
        self.cancel.wait()
        proc = self.proc
        if proc and proc.poll() is None:
            try:
                proc.stdin.write("cancel\n")
                proc.stdin.flush()
            except OSError:
                pass


class App(tk.Tk):
    def __init__(self):
        super().__init__()
        self.events = queue.Queue()
        self.release = None
        self.disks = []
        self.job = None
        self.title(tr("title"))
        self.configure(bg=BG)
        self.minsize(560, 520)
        self.resizable(True, False)
        self._style()
        logo = _logo_path()
        self.logo = tk.PhotoImage(file=str(logo)) if logo else None
        if self.logo:
            self.iconphoto(True, self.logo)
        self._build()
        self.refresh_disks()
        threading.Thread(target=self._fetch_release, daemon=True).start()
        self.after(100, self._pump)
        self.after(2000, self._auto_refresh)
        self.protocol("WM_DELETE_WINDOW", self.on_close)

    # --- apparence -------------------------------------------------------
    def _style(self):
        style = ttk.Style(self)
        if os.name == "nt" and "vista" in style.theme_names():
            style.theme_use("vista")
        else:
            style.theme_use("clam")
        family = "Segoe UI" if os.name == "nt" else "Noto Sans"
        from tkinter import font
        if family not in font.families(self):
            family = font.nametofont("TkDefaultFont").actual("family")
        self.family = family
        for name in ("TkDefaultFont", "TkTextFont", "TkMenuFont"):
            font.nametofont(name).configure(family=family, size=10)
        style.configure("Card.TFrame", background=CARD)
        style.configure("Card.TLabel", background=CARD, foreground=TEXT)
        style.configure("Muted.TLabel", background=CARD, foreground=MUTED)
        style.configure("Head.TLabel", background=CARD, foreground=TEXT, font=(family, 11, "bold"))
        style.configure("Card.TRadiobutton", background=CARD, foreground=TEXT)
        style.configure("Card.TCheckbutton", background=CARD, foreground=TEXT)
        style.map("Card.TRadiobutton", background=[("active", CARD)])
        style.map("Card.TCheckbutton", background=[("active", CARD)])
        style.configure("Accent.TButton", background=ACCENT, foreground="#ffffff", bordercolor=ACCENT,
                        focuscolor=ACCENT, padding=(18, 8), font=(family, 10, "bold"))
        style.map("Accent.TButton", background=[("disabled", "#b9b0e8"), ("active", ACCENT_DARK)],
                  foreground=[("disabled", "#f2f0ff")])
        style.configure("Danger.TButton", background="#c42b1c", foreground="#ffffff", bordercolor="#c42b1c",
                        focuscolor="#c42b1c", padding=(18, 8), font=(family, 10, "bold"))
        style.map("Danger.TButton", background=[("active", "#a32315")])
        style.configure("Accent.Horizontal.TProgressbar", background=ACCENT, troughcolor="#e6e3f5",
                        bordercolor="#e6e3f5", lightcolor=ACCENT, darkcolor=ACCENT)

    def _card(self, parent):
        frame = ttk.Frame(parent, style="Card.TFrame", padding=16)
        frame.pack(fill="x", padx=16, pady=(0, 12))
        return frame

    def _build(self):
        head = tk.Frame(self, bg=BG)
        head.pack(fill="x", padx=16, pady=16)
        if self.logo:
            tk.Label(head, image=self.logo, bg=BG).pack(side="left", padx=(0, 12))
        titles = tk.Frame(head, bg=BG)
        titles.pack(side="left", fill="x", expand=True)
        tk.Label(titles, text=tr("title"), bg=BG, fg=TEXT, font=(self.family, 16, "bold"),
                 anchor="w").pack(fill="x")
        tk.Label(titles, text=tr("subtitle"), bg=BG, fg=MUTED, anchor="w", justify="left",
                 wraplength=440).pack(fill="x")

        card = self._card(self)
        ttk.Label(card, text=tr("step_image"), style="Head.TLabel").pack(anchor="w", pady=(0, 6))
        self.mode = tk.StringVar(value="latest")
        ttk.Radiobutton(card, text=tr("img_latest"), value="latest", variable=self.mode,
                        style="Card.TRadiobutton", command=self.update_state).pack(anchor="w")
        self.release_label = ttk.Label(card, text=tr("img_checking"), style="Muted.TLabel")
        self.release_label.pack(anchor="w", padx=(24, 0), pady=(0, 6))
        ttk.Radiobutton(card, text=tr("img_file"), value="file", variable=self.mode,
                        style="Card.TRadiobutton", command=self.update_state).pack(anchor="w")
        row = ttk.Frame(card, style="Card.TFrame")
        row.pack(fill="x", padx=(24, 0))
        self.file_var = tk.StringVar(value="")
        self.file_label = ttk.Label(row, text=tr("no_file"), style="Muted.TLabel")
        self.file_label.pack(side="left", fill="x", expand=True)
        self.browse_btn = ttk.Button(row, text=tr("browse"), command=self.browse)
        self.browse_btn.pack(side="right")

        card = self._card(self)
        ttk.Label(card, text=tr("step_usb"), style="Head.TLabel").pack(anchor="w", pady=(0, 6))
        row = ttk.Frame(card, style="Card.TFrame")
        row.pack(fill="x")
        self.disk_var = tk.StringVar()
        self.disk_box = ttk.Combobox(row, textvariable=self.disk_var, state="readonly")
        self.disk_box.pack(side="left", fill="x", expand=True)
        self.disk_box.bind("<<ComboboxSelected>>", lambda _e: self.update_state())
        self.refresh_btn = ttk.Button(row, text=tr("refresh"), command=self.refresh_disks)
        self.refresh_btn.pack(side="right", padx=(8, 0))
        self.disk_hint = ttk.Label(card, text="", style="Muted.TLabel", wraplength=480, justify="left")
        self.verify_var = tk.BooleanVar(value=True)
        self.verify_box = ttk.Checkbutton(card, text=tr("verify_option"), variable=self.verify_var,
                                          style="Card.TCheckbutton")
        self.verify_box.pack(anchor="w", pady=(8, 0))

        bottom = tk.Frame(self, bg=BG)
        bottom.pack(fill="x", padx=16, pady=(4, 16))
        self.progress = ttk.Progressbar(bottom, maximum=1000, style="Accent.Horizontal.TProgressbar")
        self.progress.pack(fill="x")
        self.status = tk.Label(bottom, text=tr("ready"), bg=BG, fg=MUTED, anchor="w")
        self.status.pack(fill="x", pady=(4, 10))
        buttons = tk.Frame(bottom, bg=BG)
        buttons.pack(fill="x")
        tk.Label(buttons, text=f"v{version().lstrip('v')}", bg=BG, fg=MUTED).pack(side="left")
        self.close_btn = ttk.Button(buttons, text=tr("close"), command=self.on_close)
        self.close_btn.pack(side="right")
        self.start_btn = ttk.Button(buttons, text=tr("start"), style="Accent.TButton", command=self.start)
        self.start_btn.pack(side="right", padx=(0, 8))

    # --- données ---------------------------------------------------------
    def _fetch_release(self):
        try:
            self.events.put(("release", image.latest_release()))
        except ImageError:
            self.events.put(("release", None))

    def browse(self):
        path = filedialog.askopenfilename(title=tr("img_file"),
                                          filetypes=[(tr("iso_files"), "*.iso"), ("*", "*")])
        if path:
            self.file_var.set(path)
            self.file_label.configure(text=os.path.basename(path))
            self.mode.set("file")
            self.update_state()

    def refresh_disks(self):
        try:
            found = [d for d in disks.list_disks() if d.candidate]
        except Exception:  # noqa: BLE001 - lecture du matériel : liste vide plutôt qu'un plantage
            found = []
        if [d.path for d in found] == [d.path for d in self.disks] and self.disks:
            return
        current = self.selected_disk()
        self.disks = found
        labels = [self._disk_label(d) for d in found]
        self.disk_box.configure(values=labels)
        if current and current.path in [d.path for d in found]:
            self.disk_box.current([d.path for d in found].index(current.path))
        elif found:
            self.disk_box.current(0)
        else:
            self.disk_var.set("")
        self.update_state()

    def _auto_refresh(self):
        if self.job is None:
            self.refresh_disks()
        self.after(2000, self._auto_refresh)

    @staticmethod
    def _disk_label(d):
        where = f" — {', '.join(d.mountpoints)}" if d.mountpoints else ""
        return f"{d.model or tr('step_usb')} ({human_size(d.size)}) {d.path}{where}"

    def selected_disk(self):
        try:
            index = self.disk_box.current()
        except tk.TclError:
            return None
        return self.disks[index] if 0 <= index < len(self.disks) else None

    def update_state(self):
        busy = self.job is not None
        disk = self.selected_disk()
        if not self.disks:
            self.disk_hint.configure(text=tr("no_usb", min=human_size(MIN_STICK)))
            self.disk_hint.pack(anchor="w", pady=(6, 0), before=self.verify_box)
        else:
            self.disk_hint.pack_forget()
        has_image = (self.mode.get() == "file" and self.file_var.get()) or \
                    (self.mode.get() == "latest" and self.release is not False)
        ok = not busy and disk is not None and has_image
        self.start_btn.configure(state="normal" if ok else "disabled")
        for widget in (self.browse_btn, self.refresh_btn):
            widget.configure(state="disabled" if busy else "normal")
        self.disk_box.configure(state="disabled" if busy else "readonly")
        self.close_btn.configure(text=tr("cancel") if busy else tr("close"))

    # --- actions ---------------------------------------------------------
    def start(self):
        disk = self.selected_disk()
        if disk is None:
            return
        if self.mode.get() == "file":
            source = ("file", self.file_var.get())
            try:
                image.check_image(source[1])
                disks_err = disks.image_on_disk(source[1], disk)
            except ImageError as exc:
                messagebox.showerror(tr("err_title"), error_text(exc.key, exc.fields), parent=self)
                return
            if disks_err:
                messagebox.showerror(tr("err_title"), error_text("err_source_on_disk"), parent=self)
                return
            size = os.path.getsize(source[1])
        else:
            source = ("latest", self.release or None)
            size = self.release["size"] if self.release else 0
        if size and disk.size < size:
            messagebox.showerror(tr("err_title"), error_text("err_too_small", {"need": size, "have": disk.size}),
                                 parent=self)
            return
        if not self.confirm(disk):
            return
        self.job = Job(self, source, disk, self.verify_var.get())
        self.progress.configure(value=0)
        self.update_state()
        threading.Thread(target=self.job.run, daemon=True).start()

    def confirm(self, disk):
        """Confirmation explicite : bouton « Annuler » par défaut, nom et taille de la clé."""
        win = tk.Toplevel(self)
        win.title(tr("confirm_title"))
        win.configure(bg=CARD)
        win.transient(self)
        win.resizable(False, False)
        result = {"ok": False}
        tk.Label(win, text="⚠", fg="#c42b1c", bg=CARD, font=(self.family, 28)).grid(row=0, column=0, rowspan=2,
                                                                                padx=(20, 8), pady=20, sticky="n")
        tk.Label(win, text=tr("confirm_title"), bg=CARD, fg=TEXT, font=(self.family, 13, "bold"),
                 anchor="w").grid(row=0, column=1, sticky="w", padx=(0, 20), pady=(20, 4))
        body = tr("confirm_body", disk=f"{disk.model} — {disk.path}", size=human_size(disk.size))
        tk.Label(win, text=body, bg=CARD, fg=TEXT, justify="left", wraplength=380,
                 anchor="w").grid(row=1, column=1, sticky="w", padx=(0, 20))
        row = tk.Frame(win, bg=CARD)
        row.grid(row=2, column=0, columnspan=2, sticky="e", padx=20, pady=20)

        def accept():
            result["ok"] = True
            win.destroy()
        cancel = ttk.Button(row, text=tr("cancel"), command=win.destroy)
        ttk.Button(row, text=tr("confirm_ok"), style="Danger.TButton", command=accept).pack(side="right")
        cancel.pack(side="right", padx=(0, 8))
        cancel.focus_set()
        win.bind("<Escape>", lambda _e: win.destroy())
        win.update_idletasks()
        x = self.winfo_rootx() + (self.winfo_width() - win.winfo_reqwidth()) // 2
        y = self.winfo_rooty() + (self.winfo_height() - win.winfo_reqheight()) // 3
        win.geometry(f"+{max(x, 0)}+{max(y, 0)}")
        win.grab_set()
        self.wait_window(win)
        return result["ok"]

    def ask_password(self):
        """Mot de passe pour `sudo -S`, demandé depuis le fil de travail."""
        answer = {}
        done = threading.Event()

        def ask():
            answer["value"] = simpledialog.askstring(tr("title"), tr("password_prompt"), show="•", parent=self)
            done.set()
        self.after(0, ask)
        done.wait()
        return answer.get("value")

    def on_close(self):
        if self.job is not None:
            if messagebox.askyesno(tr("cancel"), tr("cancel_confirm"), parent=self, icon="warning"):
                self.job.cancel.set()
            return
        self.destroy()

    def _pump(self):
        try:
            while True:
                event = self.events.get_nowait()
                self._handle(event)
        except queue.Empty:
            pass
        self.after(100, self._pump)

    def _handle(self, event):
        kind = event[0]
        if kind == "release":
            self.release = event[1] if event[1] else False
            if self.release:
                self.release_label.configure(text=f"{self.release['tag']} — {human_size(self.release['size'])}")
            else:
                self.release_label.configure(text=tr("img_offline"))
                if self.mode.get() == "latest":
                    self.mode.set("file")
            self.update_state()
        elif kind == "status":
            self.status.configure(text=event[1])
            if event[2] is not None:
                self.progress.configure(value=int(event[2] * 1000))
        elif kind == "done":
            self.job = None
            self.progress.configure(value=1000)
            self.status.configure(text=tr("done_title"))
            self.update_state()
            messagebox.showinfo(tr("done_title"), tr("done_body"), parent=self)
        elif kind == "error":
            self.job = None
            self.progress.configure(value=0)
            text = error_text(event[1], event[2])
            self.status.configure(text=text)
            self.disks = []
            self.refresh_disks()
            messagebox.showerror(tr("err_title"), text, parent=self)


def _relaunch_as_admin_windows():
    """Sous Windows, relance avec les droits administrateur (invite UAC) si besoin."""
    import ctypes
    if writer.is_admin():
        return False
    if getattr(sys, "frozen", False):
        exe, params = sys.executable, subprocess.list2cmdline(sys.argv[1:])
    else:
        exe, params = sys.executable, subprocess.list2cmdline([os.path.abspath(sys.argv[0])] + sys.argv[1:])
    ret = ctypes.windll.shell32.ShellExecuteW(None, "runas", exe, params, None, 1)
    return ret > 32


def main():
    if os.name == "nt" and _relaunch_as_admin_windows():
        return 0
    app = App()
    app.mainloop()
    return 0
