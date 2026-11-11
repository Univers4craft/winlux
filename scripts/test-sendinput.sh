#!/bin/sh
# Saisie simulée (#1683) : testc/msvc/sendinput_msvc.c, construit ici en x64
# et en x86 avec clang (cible MSVC), tape dans sa propre fenêtre sous Xvfb
# par SendInput et keybd_event (clic, VK_A, « é », « € », molette), une fois
# l'autorisation « input » accordée ; sans elle, SendInput rend 0 et
# ERROR_ACCESS_DENIED. Sauté (code 0) sans runtime, clang ou xvfb-run.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
src="$root/testc/msvc/sendinput_msvc.c"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for d in clang lld-link llvm-dlltool xvfb-run; do
    command -v "$d" >/dev/null 2>&1 || { echo "$d absent : test sauté"; exit 0; }
done

tmp="$out/saisie"
mkdir -p "$tmp"
kernel='ExitProcess@4 GetLastError@0 SetLastError@4 Sleep@4 GetTickCount@0 GetCommandLineW@0 GetModuleHandleW@4'
user='RegisterClassW@4 CreateWindowExW@48 DefWindowProcW@16 PeekMessageW@20 TranslateMessage@4 DispatchMessageW@4 SendInput@12 keybd_event@16 GetWindowRect@8 GetSystemMetrics@4 SetForegroundWindow@4 SetFocus@4 ShowWindow@8 UpdateWindow@4'
for arch in 64 32; do
    exe="$out/sendinput$arch.exe"
    if test -f "$exe" && ! test "$src" -nt "$exe" && ! test "$0" -nt "$exe"; then
        continue
    fi
    for dll in kernel32 user32; do
        { echo "LIBRARY $dll.dll"; echo EXPORTS
          if test "$dll" = kernel32; then list=$kernel; else list=$user; fi
          for f in $list; do
              if test "$arch" = 64; then echo "${f%@*}"; else echo "$f"; fi
          done; } >"$tmp/$dll$arch.def"
        if test "$arch" = 64; then
            llvm-dlltool -m i386:x86-64 -d "$tmp/$dll$arch.def" -l "$tmp/$dll$arch.lib"
        else
            llvm-dlltool -m i386 -k -d "$tmp/$dll$arch.def" -l "$tmp/$dll$arch.lib"
        fi
    done
    if test "$arch" = 64; then
        target=x86_64-pc-windows-msvc; extra=
    else
        target=i686-pc-windows-msvc; extra=/safeseh:no
    fi
    clang --target=$target -O1 -fms-extensions -fno-builtin -fno-stack-protector -c "$src" -o "$tmp/sendinput$arch.obj"
    lld-link /nologo /entry:entry /subsystem:console /nodefaultlib $extra "$tmp/sendinput$arch.obj" \
        "$tmp/kernel32$arch.lib" "$tmp/user32$arch.lib" /out:"$exe"
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/home"
run() {
    exe=$1; shift
    set +e
    env HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" \
        XDG_CONFIG_HOME="$work/home/.config" WINLUX_LOCK_ROOT="$work" \
        WINLUX_PERMISSIONS_FILE="$work/permissions.tsv" WINLUX_PERMISSION_HELPER="$work/absent" \
        WINLUX_REGISTRY_PATH="$work/registre.tsv" TMPDIR="$work" \
        xvfb-run -a -s '-screen 0 1024x768x24' env -u WINLUX_GUI -u WINLUX_PERMISSIONS \
        timeout 60 "$runtime" "$exe" "$@" >"$work/out.txt" 2>&1
    status=$?
    set -e
}
for arch in 64 32; do
    exe="$out/sendinput$arch.exe"
    real=$(readlink -f "$exe")
    : >"$work/permissions.tsv"
    run "$exe" refus
    test "$status" -eq 0 || {
        tail -20 "$work/out.txt"
        echo "sendinput$arch.exe sans autorisation : le test n° $status a échoué" >&2
        exit 1
    }
    printf 'input\tallow\t%s\n' "$real" >"$work/permissions.tsv"
    run "$exe"
    test "$status" -eq 0 || {
        tail -20 "$work/out.txt"
        echo "sendinput$arch.exe : le test n° $status a échoué" >&2
        exit 1
    }
done
echo "sendinput : SendInput et keybd_event tapent dans la session en x64 et en x86 (clic, VK_A, é, €, molette) une fois autorisés, refusés sinon"
