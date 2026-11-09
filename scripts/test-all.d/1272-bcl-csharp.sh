# BCL Winlux écrite en C# (étape 7 du moteur .NET) : compilée par Roslyn
# épinglé, puis ses témoins lancés par winlux. Demande le paquet
# dotnet-runtime-10.0 (construction seulement).
if command -v dotnet >/dev/null; then
    test "$quick" = 0 && step "BCL C# : mscorlib compilée et témoins (étape 7)" bash scripts/test-bcl.sh
else
    skip "BCL C# (étape 7) : paquet dotnet-runtime-10.0 absent"
fi
