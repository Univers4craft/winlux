/* MessageBoxW : le bouton MB_DEFBUTTONn a le focus à l'ouverture et Entrée
 * le choisit (#2754). Deux boîtes : MB_YESNOCANCEL | MB_DEFBUTTON2 (attendu
 * IDNO = 7), puis MB_ABORTRETRYIGNORE | MB_DEFBUTTON3 (attendu IDIGNORE = 5).
 * Code de sortie : première réponse * 10 + seconde, donc 75 attendu.
 * Licence GPL 3. Compilé par MinGW-w64 ; API publique documentée sur Learn. */
#include <windows.h>

int main(void)
{
    int a = MessageBoxW(NULL, L"Enregistrer ?", L"Defaut2", MB_YESNOCANCEL | MB_ICONQUESTION | MB_DEFBUTTON2);
    int b = MessageBoxW(NULL, L"Erreur", L"Defaut3", MB_ABORTRETRYIGNORE | MB_ICONERROR | MB_DEFBUTTON3);
    return a * 10 + b;
}
