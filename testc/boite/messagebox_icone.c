/* MessageBoxW : l'icône du type MB_ICON* est affichée à gauche du texte
 * (#2854). Une boîte MB_OK | MB_ICONERROR ; le script de test vérifie sur une
 * capture que le disque rouge de l'icône d'erreur est peint, puis ferme la
 * boîte par Entrée (code de sortie IDOK = 1).
 * Licence GPL 3. Compilé par MinGW-w64 ; API publique documentée sur Learn. */
#include <windows.h>

int main(void)
{
    return MessageBoxW(NULL, L"Une erreur est survenue.", L"IconeErreur", MB_OK | MB_ICONERROR);
}
