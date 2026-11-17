#!/bin/sh
# Diagnostic CLR (#2780) : une méthode que le moteur ne sert pas est
# journalisée avec son appelant (méthode et offset IL) et le type réel de
# `this`, même quand l'appli rattrape la MissingMethodException.
# Programme d'essai compilé par le Roslyn épinglé de build-bcl.sh, contre une
# petite assembly « System.Windows.Forms » dont le moteur ne servira jamais ces types.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
csc=$root/build/roslyn/5.0.0/tasks/netcore/bincore/csc.dll
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
command -v dotnet >/dev/null || { echo "dotnet absent : test sauté"; exit 0; }
test -f "$root/build/bcl/mscorlib.dll" && test -f "$csc" || { echo "lancer scripts/build-bcl.sh d'abord : test sauté"; exit 0; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat >"$work/F.cs" <<'CS'
[assembly: System.Reflection.AssemblyVersion("4.0.0.0")]
namespace System.Windows.Forms {
    public class EssaiAbsentStatique { public static void SetText(string s) { } }
    public class EssaiAbsent { public EssaiAbsent() { } public void Start() { } }
}
CS
cat >"$work/Essai.cs" <<'CS'
extern alias Facade;
using System;
static class Essai
{
    static int Main()
    {
        try { Facade::System.Windows.Forms.EssaiAbsentStatique.SetText("x"); } catch (MissingMethodException) { }
        try { new Facade::System.Windows.Forms.EssaiAbsent().Start(); } catch (MissingMethodException) { }
        return 200;
    }
}
CS
cc() { DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 dotnet "$csc" -nologo -noconfig -nostdlib -r:"$root/build/bcl/mscorlib.dll" "$@"; }
cc -target:library -out:"$work/System.Windows.Forms.dll" "$work/F.cs" >/dev/null
mkdir "$work/run"
cc -r:Facade="$work/System.Windows.Forms.dll" -out:"$work/run/Essai.exe" "$work/Essai.cs" >/dev/null
code=0
(cd "$work/run" && WINLUX_BCL_DIR="$root/build/bcl" "$winlux" Essai.exe) >"$work/out.txt" 2>&1 || code=$?
fail() { echo "ÉCHEC : $1" >&2; cat "$work/out.txt" >&2; exit 1; }
test "$code" = 200 || fail "code $code au lieu de 200"
grep -q "méthode introuvable System.Windows.Forms.EssaiAbsentStatique::SetText appelée par Essai::Main IL_[0-9a-f]*, this : (statique)" "$work/out.txt" \
    || fail "appel statique sans appelant"
grep -q "méthode introuvable System.Windows.Forms.EssaiAbsent::Start appelée par Essai::Main IL_[0-9a-f]*, this : System.Windows.Forms.EssaiAbsent" "$work/out.txt" \
    || fail "appel d'instance sans appelant ni type de this"
echo "CLR : méthode introuvable journalisée avec appelant et type de this (#2780)"
