/* Winsock x86 (#1408) : fonctions d'extension rendues par WSAIoctl
 * (AcceptEx, GetAcceptExSockaddrs, ConnectEx) avec OVERLAPPED x86 et port
 * de complétion, comme le client Steam. */
#include <winsock2.h>
#include <mswsock.h>
#include <stdio.h>
#include <string.h>
#include <wchar.h>

static int fails;
#define CHECK(n, c) do { if (!(c)) { printf("echec %d : %s (WSA %d)\n", n, #c, WSAGetLastError()); fails = n; } } while (0)

static void *ext(SOCKET s, GUID g) {
    void *f = NULL;
    DWORD got = 0;
    if (WSAIoctl(s, SIO_GET_EXTENSION_FUNCTION_POINTER, &g, sizeof g, &f, sizeof f, &got, NULL, NULL) != 0)
        return NULL;
    return got == 4 ? f : NULL;
}

int main(void) {
    WSADATA w;
    CHECK(1, WSAStartup(0x0202, &w) == 0);
    SOCKET l = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    struct sockaddr_in a;
    memset(&a, 0, sizeof a);
    a.sin_family = AF_INET;
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    int alen = sizeof a;
    CHECK(2, bind(l, (struct sockaddr *)&a, sizeof a) == 0 && listen(l, 4) == 0
              && getsockname(l, (struct sockaddr *)&a, &alen) == 0);
    LPFN_ACCEPTEX accept_ex = (LPFN_ACCEPTEX)ext(l, (GUID)WSAID_ACCEPTEX);
    LPFN_GETACCEPTEXSOCKADDRS addrs = (LPFN_GETACCEPTEXSOCKADDRS)ext(l, (GUID)WSAID_GETACCEPTEXSOCKADDRS);
    LPFN_CONNECTEX connect_ex = (LPFN_CONNECTEX)ext(l, (GUID)WSAID_CONNECTEX);
    CHECK(3, accept_ex && addrs && connect_ex);
    if (fails) return fails;

    HANDLE port = CreateIoCompletionPort(INVALID_HANDLE_VALUE, NULL, 0, 0);
    SOCKET in = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    SOCKET c = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    struct sockaddr_in any;
    memset(&any, 0, sizeof any);
    any.sin_family = AF_INET;
    CHECK(4, port && bind(c, (struct sockaddr *)&any, sizeof any) == 0
              && CreateIoCompletionPort((HANDLE)l, port, 1, 0)
              && CreateIoCompletionPort((HANDLE)c, port, 2, 0));

    char buf[2 * (sizeof(struct sockaddr_in) + 16)];
    DWORD n = 0;
    OVERLAPPED oa, oc;
    memset(&oa, 0, sizeof oa);
    memset(&oc, 0, sizeof oc);
    BOOL ra = accept_ex(l, in, buf, 0, sizeof(struct sockaddr_in) + 16, sizeof(struct sockaddr_in) + 16, &n, &oa);
    CHECK(5, ra || WSAGetLastError() == ERROR_IO_PENDING);
    BOOL rc = connect_ex(c, (struct sockaddr *)&a, sizeof a, NULL, 0, NULL, &oc);
    CHECK(6, rc || WSAGetLastError() == ERROR_IO_PENDING);

    int seen = 0;
    for (int i = 0; i < 2; i++) {
        DWORD bytes;
        ULONG_PTR key = 0;
        OVERLAPPED *o = NULL;
        if (GetQueuedCompletionStatus(port, &bytes, &key, &o, 5000))
            seen |= (o == &oa && key == 1) ? 1 : (o == &oc && key == 2) ? 2 : 4;
    }
    CHECK(7, seen == 3);
    CHECK(8, oa.Internal == 0 && oc.Internal == 0);

    struct sockaddr *loc = NULL, *rem = NULL;
    int ll = 0, rl = 0;
    addrs(buf, 0, sizeof(struct sockaddr_in) + 16, sizeof(struct sockaddr_in) + 16, &loc, &ll, &rem, &rl);
    struct sockaddr_in me;
    int mel = sizeof me;
    getsockname(c, (struct sockaddr *)&me, &mel);
    CHECK(9, loc && rem && ll == 16 && rl == 16 && rem->sa_family == AF_INET
              && ((struct sockaddr_in *)rem)->sin_port == me.sin_port
              && ((struct sockaddr_in *)loc)->sin_port == a.sin_port);

    char got[4] = {0};
    CHECK(10, send(c, "ok", 2, 0) == 2 && recv(in, got, sizeof got, 0) == 2 && !strcmp(got, "ok"));
    closesocket(c);
    closesocket(in);
    closesocket(l);
    /* #1438 : programme « large address aware » ; comme sous WOW64, rien
     * n'est placé dans les 64 derniers Ko sous 4 Go. */
    CHECK(11, VirtualAlloc((void *)0xffff0000u, 0x1000, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE) == NULL);
    wchar_t *env = GetEnvironmentStringsW();
    CHECK(12, env && (ULONG_PTR)env < 0xffff0000u && wcschr(env, L'=') != NULL);
    FreeEnvironmentStringsW(env);
    if (!fails) printf("Réseau 32 bits : tout est correct\n");
    return fails;
}
