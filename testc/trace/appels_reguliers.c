/* WINLUX_APITRACE_DELAY : un programme qui appelle Sleep toutes les 50 ms
   pendant 1,5 s. Avec un délai d'une seconde, le journal d'API ne contient
   que les appels faits après la première seconde. */
#include <windows.h>
int main(void)
{
    DWORD t0 = GetTickCount();
    while (GetTickCount() - t0 < 1500) Sleep(50);
    return 0;
}
