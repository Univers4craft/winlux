"""Éditeur graphique de partitions (mode avancé de l'installateur Winlux)."""
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk  # noqa: E402

import winlux_partitioner as wp  # noqa: E402

CSS = b"""
window.installer-window, dialog.pm-dialog {
    font-size: 13px;
}
dialog.pm-dialog, dialog.pm-dialog box, dialog.pm-dialog grid {
    background-color: #1a1a2e;
    color: #ffffff;
}
dialog.pm-dialog label {
    color: #ffffff;
}
dialog.pm-dialog .pm-error, .pm-error {
    color: #e74856;
    font-size: 12px;
}
.pm-ok {
    color: #2ecc71;
    font-size: 12px;
}
.pm-ops {
    color: #b4b4cc;
    font-size: 12px;
    font-family: monospace;
}
treeview.pm-tree {
    background-color: #2d2d44;
    color: #ffffff;
    font-size: 12px;
}
treeview.pm-tree:selected {
    background-color: #0078D4;
    color: #ffffff;
}
treeview.pm-tree header button {
    background-image: none;
    background-color: #363654;
    color: #b4b4cc;
    border: none;
    padding: 4px 8px;
}
radio, check {
    min-width: 16px;
    min-height: 16px;
}
.pm-toolbar button {
    padding: 6px 12px;
    min-height: 28px;
}
"""

COL_DEVICE, COL_FS, COL_SIZE, COL_MOUNT, COL_ACTION, COL_INDEX = range(6)


def fs_display(tr, fs):
    if fs in ("none", "bios_grub", "swap", "vfat"):
        return tr(f"pm_fs_{fs}")
    return fs or "?"


class ManualPartitionEditor(Gtk.Box):
    def __init__(self, tr, bar, on_changed, scale=1.0):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.tr = tr
        self.on_changed = on_changed
        self.scale = scale
        self.disk = None
        self.plan = None
        self.rows = []

        self.title = Gtk.Label(xalign=0)
        self.title.get_style_context().add_class("section-label")
        self.pack_start(self.title, False, False, 0)
        self.hint = Gtk.Label(xalign=0)
        self.hint.set_line_wrap(True)
        self.hint.get_style_context().add_class("info-text")
        self.pack_start(self.hint, False, False, 0)

        self.bar = bar
        self.pack_start(self.bar, False, False, 0)

        self.store = Gtk.ListStore(str, str, str, str, str, int)
        self.tree = Gtk.TreeView(model=self.store)
        self.tree.get_style_context().add_class("pm-tree")
        self.columns = []
        for col in range(5):
            renderer = Gtk.CellRendererText()
            column = Gtk.TreeViewColumn("", renderer, text=col)
            column.set_resizable(True)
            column.set_expand(col in (COL_DEVICE, COL_ACTION))
            self.tree.append_column(column)
            self.columns.append(column)
        self.tree.get_selection().connect("changed", lambda _s: self.update_buttons())
        self.tree.connect("row-activated", lambda *_a: self.on_edit_or_create())
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        scroll.set_min_content_height(int(150 * scale))
        scroll.add(self.tree)
        self.pack_start(scroll, False, False, 0)

        self.toolbar = Gtk.FlowBox()
        self.toolbar.set_selection_mode(Gtk.SelectionMode.NONE)
        self.toolbar.set_max_children_per_line(6)
        self.toolbar.set_column_spacing(6)
        self.toolbar.set_row_spacing(6)
        self.toolbar.get_style_context().add_class("pm-toolbar")
        self.buttons = {}
        for key, handler in (("pm_new_table", self.on_new_table), ("pm_create", self.on_create),
                             ("pm_edit", self.on_edit), ("pm_delete", self.on_delete),
                             ("pm_auto_boot", self.on_auto_boot), ("pm_revert", self.on_revert)):
            btn = Gtk.Button()
            btn.get_style_context().add_class("btn-secondary")
            btn.connect("clicked", lambda _b, h=handler: h())
            self.buttons[key] = btn
            self.toolbar.add(btn)
        self.pack_start(self.toolbar, False, False, 0)

        self.status = Gtk.Label(xalign=0)
        self.status.set_line_wrap(True)
        self.pack_start(self.status, False, False, 0)
        self.retranslate()

    # ── État ─────────────────────────────────────────────────────

    def retranslate(self):
        tr = self.tr
        self.title.set_text(tr("pm_title"))
        self.hint.set_text(tr("pm_hint"))
        for column, key in zip(self.columns, ("pm_col_device", "pm_col_type", "pm_col_size",
                                               "pm_col_mount", "pm_col_action")):
            column.set_title(tr(key))
        for key, btn in self.buttons.items():
            btn.set_label(tr(key))

    def set_disk(self, disk):
        if disk is None:
            self.disk = self.plan = None
        elif self.disk is None or self.disk["path"] != disk["path"] or self.plan is None:
            self.disk = disk
            self.plan = wp.read_disk(disk["path"], disk["size"])
        self.refresh()

    def errors(self):
        if self.plan is None:
            return [("pm_no_disk", {})]
        return self.plan.validate()

    def is_valid(self):
        return self.plan is not None and not self.errors()

    def refresh(self):
        self.retranslate()
        self.store.clear()
        self.rows = []
        tr = self.tr
        if self.plan is None:
            self.bar.set_data(0, [])
        else:
            bar_items = []
            for kind, item in self.plan.layout():
                idx = len(self.rows)
                self.rows.append((kind, item))
                if kind == "free":
                    start, size = item
                    self.store.append([tr("pm_free"), "", wp.human_mib(size), "", "", idx])
                    bar_items.append({"name": "free", "size": size * wp.MIB, "fstype": "free",
                                      "label": tr("pm_free")})
                    continue
                part = item
                if part.busy:
                    action = tr("pm_status_busy")
                elif not part.existing:
                    action = tr("pm_status_new")
                else:
                    acts = []
                    if part.resized():
                        acts.append(tr("pm_status_resize"))
                    if part.format:
                        acts.append(tr("pm_status_format"))
                    action = ", ".join(acts) or tr("pm_status_keep")
                path = self.plan.path_of(part)
                self.store.append([
                    path + (f"  ({part.label})" if part.label else ""),
                    fs_display(tr, part.fstype), wp.human_mib(part.size),
                    part.mountpoint, action, idx])
                bar_items.append({"name": path.rsplit("/", 1)[-1], "size": part.size * wp.MIB,
                                  "fstype": part.fstype, "label": part.mountpoint or part.label})
            self.bar.set_data(self.plan.disk_bytes, bar_items)
        errors = self.errors()
        ctx = self.status.get_style_context()
        ctx.remove_class("pm-error")
        ctx.remove_class("pm-ok")
        if errors:
            self.status.set_text("\n".join("• " + tr(k, **kw) for k, kw in errors))
            ctx.add_class("pm-error")
        else:
            self.status.set_text(tr("pm_valid"))
            ctx.add_class("pm-ok")
        self.update_buttons()
        self.on_changed()

    def selected(self):
        model, it = self.tree.get_selection().get_selected()
        if it is None:
            return None, None
        return self.rows[model[it][COL_INDEX]]

    def update_buttons(self):
        kind, item = self.selected()
        has_plan = self.plan is not None
        has_table = has_plan and self.plan.label is not None
        self.buttons["pm_new_table"].set_sensitive(has_plan)
        self.buttons["pm_create"].set_sensitive(has_table and bool(self.plan.free_regions()))
        self.buttons["pm_edit"].set_sensitive(kind == "part" and not item.busy)
        self.buttons["pm_delete"].set_sensitive(kind == "part" and not item.busy)
        self.buttons["pm_auto_boot"].set_sensitive(has_plan)
        self.buttons["pm_revert"].set_sensitive(has_plan and self.plan.has_changes())

    # ── Actions ──────────────────────────────────────────────────

    def _dialog(self, title, ok_label):
        dlg = Gtk.Dialog(title=title, transient_for=self.get_toplevel(), modal=True)
        dlg.get_style_context().add_class("pm-dialog")
        dlg.set_position(Gtk.WindowPosition.CENTER_ON_PARENT)
        cancel = dlg.add_button(self.tr("pm_dlg_cancel"), Gtk.ResponseType.CANCEL)
        cancel.get_style_context().add_class("btn-secondary")
        ok = dlg.add_button(ok_label, Gtk.ResponseType.OK)
        ok.get_style_context().add_class("btn-primary")
        dlg.set_default_response(Gtk.ResponseType.OK)
        area = dlg.get_content_area()
        pad = int(16 * self.scale)
        area.set_margin_start(pad)
        area.set_margin_end(pad)
        area.set_margin_top(pad)
        area.set_spacing(int(8 * self.scale))
        return dlg, area, ok

    def _run(self, dlg, apply):
        """Rejoue le dialogue tant que apply() lève une PlanError."""
        error = Gtk.Label(xalign=0)
        error.set_line_wrap(True)
        error.get_style_context().add_class("pm-error")
        dlg.get_content_area().pack_end(error, False, False, 0)
        dlg.show_all()
        try:
            while dlg.run() == Gtk.ResponseType.OK:
                try:
                    apply()
                    return True
                except wp.PlanError as e:
                    error.set_text(self.tr(e.key, **e.kwargs))
            return False
        finally:
            dlg.destroy()

    def _form(self, area, part=None, region=None):
        tr = self.tr
        grid = Gtk.Grid(column_spacing=int(12 * self.scale), row_spacing=int(8 * self.scale))
        area.pack_start(grid, False, False, 0)
        if part is not None:
            max_mib = int(self.plan.max_size(part))
            current = int(round(part.size))
        else:
            max_mib = int(region[1])
            current = max_mib
        size = Gtk.SpinButton.new_with_range(1, max(1, max_mib), 1)
        size.set_value(current)
        grid.attach(Gtk.Label(label=tr("pm_dlg_size"), xalign=0), 0, 0, 1, 1)
        grid.attach(size, 1, 0, 1, 1)
        grid.attach(Gtk.Label(label=tr("pm_dlg_max", size=wp.human_mib(max_mib)), xalign=0),
                    1, 1, 1, 1)

        fmt = Gtk.CheckButton(label=tr("pm_dlg_format"))
        fmt.set_active(part is None or part.format)
        fmt.set_sensitive(part is not None and part.existing and not part.busy)
        grid.attach(fmt, 0, 2, 2, 1)

        fs = Gtk.ComboBoxText()
        choices = list(wp.FS_CHOICES)
        if part is not None and part.orig_fstype and part.orig_fstype not in choices:
            choices.append(part.orig_fstype)
        for choice in choices:
            fs.append(choice, fs_display(tr, choice))
        fs.set_active_id(part.fstype if part is not None and part.fstype in choices
                         else ("ext4" if part is None else "none"))
        grid.attach(Gtk.Label(label=tr("pm_dlg_fs"), xalign=0), 0, 3, 1, 1)
        grid.attach(fs, 1, 3, 1, 1)

        mount = Gtk.ComboBoxText.new_with_entry()
        mount.append("", tr("pm_mount_none"))
        for choice in wp.MOUNT_CHOICES:
            mount.append(choice, choice)
        default_mount = part.mountpoint if part is not None else (
            "/" if self.plan.root() is None else "")
        mount.get_child().set_text(default_mount)
        grid.attach(Gtk.Label(label=tr("pm_dlg_mount"), xalign=0), 0, 4, 1, 1)
        grid.attach(mount, 1, 4, 1, 1)

        label = Gtk.Entry()
        label.set_max_length(32)
        label.set_text(part.label if part is not None else "")
        grid.attach(Gtk.Label(label=tr("pm_dlg_label"), xalign=0), 0, 5, 1, 1)
        grid.attach(label, 1, 5, 1, 1)

        def sync(*_a):
            formatting = fmt.get_active()
            fs.set_sensitive(formatting or part is None)
            chosen = fs.get_active_id()
            mount.set_sensitive(chosen not in wp.NO_MOUNT_FS)
            if part is not None and part.existing and not formatting:
                ok_resize = part.fstype in wp.SHRINKABLE_FS | wp.GROWABLE_FS
                size.set_sensitive(ok_resize and not part.busy)
                fs.set_active_id(part.orig_fstype if part.orig_fstype in choices else "none")
            else:
                size.set_sensitive(part is None or not part.busy)

        def on_fs(*_a):
            chosen = fs.get_active_id()
            if chosen == "vfat" and not mount.get_child().get_text():
                mount.get_child().set_text("/boot/efi")
            if chosen == "bios_grub" and part is None:
                size.set_value(min(2, max_mib))
            if chosen in wp.NO_MOUNT_FS:
                mount.get_child().set_text("")
            sync()

        fmt.connect("toggled", sync)
        fs.connect("changed", on_fs)
        sync()
        return size, fmt, fs, mount, label

    def on_create(self, region=None):
        if region is None:
            kind, item = self.selected()
            region = item if kind == "free" else self.plan.largest_free()
        if region is None:
            return
        dlg, area, _ok = self._dialog(self.tr("pm_dlg_create"), self.tr("pm_dlg_ok"))
        size, _fmt, fs, mount, label = self._form(area, region=region)

        def apply():
            self.plan.create(region[0], int(size.get_value()), fs.get_active_id(),
                             mount.get_child().get_text().strip(), label.get_text().strip())
        if self._run(dlg, apply):
            self.refresh()

    def on_edit(self):
        kind, part = self.selected()
        if kind != "part" or part.busy:
            return
        dlg, area, _ok = self._dialog(self.tr("pm_dlg_edit", path=self.plan.path_of(part)),
                                      self.tr("pm_dlg_ok"))
        size, fmt, fs, mount, label = self._form(area, part=part)
        snapshot = (part.size, part.fstype, part.format, part.mountpoint, part.label, set(part.flags))

        def apply():
            try:
                self.plan.configure(part, format=fmt.get_active(), fstype=fs.get_active_id(),
                                    mountpoint=mount.get_child().get_text().strip(),
                                    label=label.get_text().strip())
                if int(size.get_value()) != int(round(part.size)):
                    self.plan.resize(part, int(size.get_value()))
            except wp.PlanError:
                (part.size, part.fstype, part.format, part.mountpoint, part.label,
                 part.flags) = snapshot
                raise
        if self._run(dlg, apply):
            self.refresh()

    def on_edit_or_create(self):
        kind, _item = self.selected()
        if kind == "free":
            self.on_create()
        else:
            self.on_edit()

    def on_delete(self):
        kind, part = self.selected()
        if kind != "part":
            return
        if part.existing:
            dlg, area, ok = self._dialog(self.tr("pm_delete"), self.tr("pm_delete"))
            ok.get_style_context().remove_class("btn-primary")
            ok.get_style_context().add_class("btn-danger")
            msg = Gtk.Label(label=self.tr("pm_confirm_delete", path=self.plan.path_of(part)),
                            xalign=0)
            msg.set_line_wrap(True)
            area.pack_start(msg, False, False, 0)
            if not self._run(dlg, lambda: self.plan.delete(part)):
                return
        else:
            self.plan.delete(part)
        self.refresh()

    def on_new_table(self):
        tr = self.tr
        dlg, area, ok = self._dialog(tr("pm_dlg_table"), tr("pm_dlg_ok"))
        ok.get_style_context().remove_class("btn-primary")
        ok.get_style_context().add_class("btn-danger")
        warn = Gtk.Label(label=tr("pm_dlg_table_warn", device=self.plan.device), xalign=0)
        warn.set_line_wrap(True)
        warn.get_style_context().add_class("pm-error")
        area.pack_start(warn, False, False, 0)
        gpt = Gtk.RadioButton.new_with_label(None, tr("pm_dlg_table_gpt"))
        msdos = Gtk.RadioButton.new_with_label_from_widget(gpt, tr("pm_dlg_table_msdos"))
        area.pack_start(gpt, False, False, 0)
        area.pack_start(msdos, False, False, 0)
        if self._run(dlg, lambda: self.plan.new_partition_table(
                "gpt" if gpt.get_active() else "msdos")):
            self.refresh()

    def on_auto_boot(self):
        try:
            self.plan.ensure_boot_partitions()
        except wp.PlanError as e:
            self.status.set_text(self.tr(e.key, **e.kwargs))
            return
        self.refresh()

    def on_revert(self):
        if self.disk is not None:
            self.plan = wp.read_disk(self.disk["path"], self.disk["size"])
        self.refresh()

    def operations_text(self):
        if self.plan is None:
            return []
        return [self.tr(key, **kw) for _destructive, key, kw in self.plan.describe()]


def confirm_apply(parent, plan, tr, scale=1.0):
    """Dernière confirmation avant d'écrire sur le disque. Retourne True pour continuer."""
    dlg = Gtk.Dialog(title=tr("pm_confirm_title"), transient_for=parent, modal=True)
    dlg.get_style_context().add_class("pm-dialog")
    dlg.set_position(Gtk.WindowPosition.CENTER_ON_PARENT)
    cancel = dlg.add_button(tr("pm_dlg_cancel"), Gtk.ResponseType.CANCEL)
    cancel.get_style_context().add_class("btn-secondary")
    apply_btn = dlg.add_button(tr("pm_confirm_apply"), Gtk.ResponseType.OK)
    apply_btn.get_style_context().add_class("btn-danger")
    apply_btn.set_sensitive(False)
    area = dlg.get_content_area()
    pad = int(16 * scale)
    for setter in (area.set_margin_start, area.set_margin_end, area.set_margin_top):
        setter(pad)
    area.set_spacing(int(8 * scale))
    intro = Gtk.Label(label=tr("pm_confirm_text", device=plan.device), xalign=0)
    intro.set_line_wrap(True)
    area.pack_start(intro, False, False, 0)
    ops = Gtk.Label(label="\n".join(tr(k, **kw) for _d, k, kw in plan.describe()), xalign=0)
    ops.get_style_context().add_class("pm-ops")
    ops.set_line_wrap(True)
    scroll = Gtk.ScrolledWindow()
    scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
    scroll.set_min_content_height(int(120 * scale))
    scroll.set_propagate_natural_height(True)
    scroll.set_max_content_height(int(320 * scale))
    scroll.add(ops)
    area.pack_start(scroll, True, True, 0)
    check = Gtk.CheckButton(label=tr("pm_confirm_check"))
    check.connect("toggled", lambda c: apply_btn.set_sensitive(c.get_active()))
    area.pack_start(check, False, False, 0)
    dlg.show_all()
    try:
        return dlg.run() == Gtk.ResponseType.OK and check.get_active()
    finally:
        dlg.destroy()
