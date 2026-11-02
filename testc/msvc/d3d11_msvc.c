// Rendu Direct3D 11 réel (#137) : shaders DXBC (assemblés ici, jeton par
// jeton), tampon de sommets et d'index, mise en page des entrées, tampon de
// constantes, texture échantillonnée par un échantillonneur ponctuel, état
// de rastérisation par défaut (faces arrière éliminées), puis relecture des
// pixels par CopyResource et Map d'une texture de transfert.
// Format MSVC (clang en mode x86_64-pc-windows-msvc, sans CRT).
// Code de sortie : 0 si tout passe, 77 si aucun affichage OpenGL, sinon le
// numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) int __stdcall D3D11CreateDevice(void *, u32, void *, u32, const u32 *, u32, u32, void **, u32 *,
                                                      void **);

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}
void *memcpy(void *d, const void *s, u64 n) {
    u8 *o = d;
    const u8 *i = s;
    while (n--) *o++ = *i++;
    return d;
}
int _fltused = 0;

// ── Appels COM par numéro de méthode ──
#define VT(obj, n) ((void **)*(void **)(obj))[n]
#define CALL(obj, n, type, ...) ((type)VT(obj, n))(obj, __VA_ARGS__)

typedef long(__stdcall *CreateBuffer)(void *, const void *, const void *, void **);
typedef long(__stdcall *CreateTex)(void *, const void *, const void *, void **);
typedef long(__stdcall *CreateView)(void *, void *, const void *, void **);
typedef long(__stdcall *CreateIL)(void *, const void *, u32, const void *, u64, void **);
typedef long(__stdcall *CreateShader)(void *, const void *, u64, void *, void **);
typedef long(__stdcall *CreateState)(void *, const void *, void **);
typedef void(__stdcall *SetArray)(void *, u32, u32, void *const *);
typedef void(__stdcall *SetShader)(void *, void *, void *const *, u32);
typedef void(__stdcall *DrawIndexed)(void *, u32, u32, int);
typedef long(__stdcall *Map)(void *, void *, u32, u32, u32, void *);
typedef void(__stdcall *Unmap)(void *, void *, u32);
typedef void(__stdcall *SetOne)(void *, void *);
typedef void(__stdcall *SetVB)(void *, u32, u32, void *const *, const u32 *, const u32 *);
typedef void(__stdcall *SetIB)(void *, void *, u32, u32);
typedef void(__stdcall *SetU32)(void *, u32);
typedef void(__stdcall *SetRT)(void *, u32, void *const *, void *);
typedef void(__stdcall *SetVP)(void *, u32, const void *);
typedef void(__stdcall *Copy)(void *, void *, void *);
typedef void(__stdcall *ClearRT)(void *, void *, const float *);

// ── Petit assembleur DXBC (SM 4.0) ──
static u32 code[256];
static u32 ncode;
static void tok(u32 t) { code[ncode++] = t; }
// Instruction : opcode puis opérandes déjà rangés dans ops.
static void ins(u32 op, const u32 *ops, u32 n) {
    tok(op | ((1 + n) << 24));
    for (u32 i = 0; i < n; i++) tok(ops[i]);
}
#define OP_TEMP 0
#define OP_INPUT 1
#define OP_OUTPUT 2
#define OP_IMM32 4
#define OP_SAMPLER 6
#define OP_RESOURCE 7
#define OP_CB 8
#define DST(kind, mask) (2 | ((mask) << 4) | ((kind) << 12) | (1 << 20))
#define SRC(kind, sw) (2 | (1 << 2) | ((sw) << 4) | ((kind) << 12) | (1 << 20))
#define XYZW 0xE4 /* swizzle identité */
#define XYYY 0x54

static u32 blob[512];
static u32 nblob;
static u8 *bytes(void) { return (u8 *)blob; }
// Signature ISGN/OSGN : (nom, valeur système, registre, masque), flottants.
typedef struct { const char *name; u32 sv, reg, mask; } Elem;
static u32 put_signature(u8 *out, const Elem *e, u32 n) {
    u32 *w = (u32 *)out;
    w[0] = n;
    w[1] = 8;
    u32 names = 8 + 24 * n, at = names;
    for (u32 i = 0; i < n; i++) {
        u32 *r = w + 2 + 6 * i;
        r[0] = at;
        r[1] = 0;
        r[2] = e[i].sv;
        r[3] = 3;
        r[4] = e[i].reg;
        r[5] = e[i].mask | (e[i].mask << 8);
        const char *s = e[i].name;
        while (*s) out[at++] = (u8)*s++;
        out[at++] = 0;
    }
    while (at & 3) out[at++] = 0;
    return at;
}
// Conteneur DXBC : ISGN, OSGN, SHDR.
static u64 container(const Elem *in, u32 nin, const Elem *outs, u32 nout) {
    u8 *b = bytes();
    memset(b, 0, sizeof blob);
    memcpy(b, "DXBC", 4);
    u32 off = 32 + 3 * 4;
    const char *tags[3] = {"ISGN", "OSGN", "SHDR"};
    for (int c = 0; c < 3; c++) {
        ((u32 *)(b + 32))[c] = off;
        memcpy(b + off, tags[c], 4);
        u32 len;
        if (c == 0) len = put_signature(b + off + 8, in, nin);
        else if (c == 1) len = put_signature(b + off + 8, outs, nout);
        else {
            code[1] = ncode;
            memcpy(b + off + 8, code, ncode * 4);
            len = ncode * 4;
        }
        *(u32 *)(b + off + 4) = len;
        off += 8 + len;
    }
    *(u32 *)(b + 20) = 1;
    *(u32 *)(b + 24) = off;
    *(u32 *)(b + 28) = 3;
    return off;
}

static u8 vs_blob[2048], ps_blob[2048];
static u64 vs_len, ps_len;

static void build_vs(void) {
    ncode = 0;
    tok((1 << 16) | 0x40); // vs_4_0
    tok(0);
    u32 a[] = {DST(OP_INPUT, 0x3), 0};
    ins(95, a, 2); // dcl_input v0.xy
    u32 b[] = {DST(OP_INPUT, 0x3), 1};
    ins(95, b, 2); // dcl_input v1.xy
    u32 c[] = {DST(OP_OUTPUT, 0xF), 0, 1};
    ins(103, c, 3); // dcl_output_siv o0.xyzw, position
    u32 d[] = {DST(OP_OUTPUT, 0x3), 1};
    ins(101, d, 2); // dcl_output o1.xy
    u32 e[] = {DST(OP_OUTPUT, 0x3), 0, SRC(OP_INPUT, XYYY), 0};
    ins(54, e, 4); // mov o0.xy, v0.xyyy
    u32 f[] = {DST(OP_OUTPUT, 0xC), 0, 2 | (OP_IMM32 << 12), 0, 0, 0, 0x3F800000};
    ins(54, f, 7); // mov o0.zw, l(0,0,0,1)
    u32 g[] = {DST(OP_OUTPUT, 0x3), 1, SRC(OP_INPUT, XYYY), 1};
    ins(54, g, 4); // mov o1.xy, v1.xyyy
    tok(62 | (1 << 24)); // ret
    Elem in[] = {{"POSITION", 0, 0, 0x3}, {"TEXCOORD", 0, 1, 0x3}};
    Elem out[] = {{"SV_Position", 1, 0, 0xF}, {"TEXCOORD", 0, 1, 0x3}};
    vs_len = container(in, 2, out, 2);
    memcpy(vs_blob, bytes(), vs_len);
}

static void build_ps(void) {
    ncode = 0;
    tok((0 << 16) | 0x40); // ps_4_0
    tok(0);
    u32 a[] = {2 | (1 << 2) | (XYZW << 4) | (OP_CB << 12) | (2 << 20), 0, 1};
    ins(89, a, 3); // dcl_constantbuffer cb0[1]
    u32 b[] = {(OP_SAMPLER << 12) | (1 << 20), 0};
    ins(90, b, 2); // dcl_sampler s0
    u32 c[] = {(OP_RESOURCE << 12) | (1 << 20), 0, 0x5555};
    ins(88 | (3 << 11), c, 3); // dcl_resource_texture2d (float…) t0
    u32 d[] = {DST(OP_INPUT, 0x3), 1};
    ins(98 | (2 << 11), d, 2); // dcl_input_ps linear v1.xy
    u32 e[] = {DST(OP_OUTPUT, 0xF), 0};
    ins(101, e, 2); // dcl_output o0
    tok(104 | (2 << 24)); // dcl_temps 1
    tok(1);
    u32 f[] = {DST(OP_TEMP, 0xF), 0, SRC(OP_INPUT, XYYY), 1, SRC(OP_RESOURCE, XYZW), 0, (OP_SAMPLER << 12) | (1 << 20), 0};
    ins(69, f, 8); // sample r0, v1.xy, t0, s0
    u32 g[] = {DST(OP_OUTPUT, 0xF), 0, SRC(OP_TEMP, XYZW), 0, 2 | (1 << 2) | (XYZW << 4) | (OP_CB << 12) | (2 << 20), 0, 0};
    ins(56, g, 7); // mul o0, r0, cb0[0]
    tok(62 | (1 << 24)); // ret
    Elem in[] = {{"SV_Position", 1, 0, 0xF}, {"TEXCOORD", 0, 1, 0x3}};
    Elem out[] = {{"SV_Target", 64, 0, 0xF}};
    ps_len = container(in, 2, out, 1);
    memcpy(ps_blob, bytes(), ps_len);
}

typedef struct { u32 width, usage, bind, cpu, misc, stride; } BufferDesc;
typedef struct { const void *mem; u32 pitch, slice; } Init;
typedef struct { u32 w, h, mips, array, format, count, quality, usage, bind, cpu, misc; } TexDesc;
typedef struct { const char *name; u32 index, format, slot, offset, cls, step; } InputElem;
typedef struct { float x, y, w, h, zmin, zmax; } Viewport;
typedef struct { void *data; u32 row, depth; } Mapped;

#define SIZE 64

void entry(void) {
    void *dev = 0, *ctx = 0;
    u32 fl = 0;
    long hr = D3D11CreateDevice(0, 1, 0, 0, 0, 0, 7, &dev, &fl, &ctx);
    if (hr < 0 || !dev || !ctx) ExitProcess(1);

    // Cible de rendu et texture de transfert.
    TexDesc td = {SIZE, SIZE, 1, 1, 28 /* R8G8B8A8_UNORM */, 1, 0, 0, 0x20 | 0x8, 0, 0};
    void *rt = 0, *staging = 0, *rtv = 0;
    if (CALL(dev, 5, CreateTex, &td, 0, &rt) < 0 || !rt) ExitProcess(77);
    if (CALL(dev, 9, CreateView, rt, 0, &rtv) < 0 || !rtv) ExitProcess(2);
    TexDesc sd = td;
    sd.usage = 3; // STAGING
    sd.bind = 0;
    sd.cpu = 0x20000; // CPU_ACCESS_READ
    if (CALL(dev, 5, CreateTex, &sd, 0, &staging) < 0) ExitProcess(3);

    // Texture 2×2 : rangée du haut rouge, du bas verte.
    static const u32 texels[4] = {0xFF0000FF, 0xFF0000FF, 0xFF00FF00, 0xFF00FF00};
    TexDesc xd = {2, 2, 1, 1, 28, 1, 0, 0, 0x8, 0, 0};
    Init xi = {texels, 8, 0};
    void *tex = 0, *srv = 0, *smp = 0;
    if (CALL(dev, 5, CreateTex, &xd, &xi, &tex) < 0) ExitProcess(4);
    if (CALL(dev, 7, CreateView, tex, 0, &srv) < 0) ExitProcess(5);
    u32 sdesc[13] = {0 /* MIN_MAG_MIP_POINT */, 3, 3, 3, 0, 1, 1, 0, 0, 0, 0, 0, 0x7F7FFFFF};
    if (CALL(dev, 23, CreateState, sdesc, &smp) < 0 || !smp) ExitProcess(6);

    // Quart gauche de l'écran, sens horaire (face avant D3D).
    static const float verts[] = {-1, 1, 0, 0, 0, 1, 1, 0, 0, -1, 1, 1, -1, -1, 0, 1};
    static const u16 index[] = {0, 1, 2, 0, 2, 3};
    BufferDesc vb = {sizeof verts, 0, 1, 0, 0, 0}, ib = {sizeof index, 0, 2, 0, 0, 0};
    Init vi = {verts, 0, 0}, ii = {index, 0, 0};
    void *vbuf = 0, *ibuf = 0, *cbuf = 0;
    if (CALL(dev, 3, CreateBuffer, &vb, &vi, &vbuf) < 0) ExitProcess(7);
    if (CALL(dev, 3, CreateBuffer, &ib, &ii, &ibuf) < 0) ExitProcess(8);
    BufferDesc cb = {16, 2 /* DYNAMIC */, 4, 0x10000, 0, 0};
    if (CALL(dev, 3, CreateBuffer, &cb, 0, &cbuf) < 0) ExitProcess(9);

    build_vs();
    build_ps();
    void *vs = 0, *ps = 0, *il = 0;
    if (CALL(dev, 12, CreateShader, vs_blob, vs_len, 0, &vs) < 0) ExitProcess(10);
    if (CALL(dev, 15, CreateShader, ps_blob, ps_len, 0, &ps) < 0) ExitProcess(11);
    InputElem le[] = {{"POSITION", 0, 16 /* R32G32_FLOAT */, 0, 0, 0, 0}, {"TEXCOORD", 0, 16, 0, 0xFFFFFFFF, 0, 0}};
    if (CALL(dev, 11, CreateIL, le, 2, vs_blob, vs_len, &il) < 0) ExitProcess(12);

    // Constantes : blanc, alpha 0.5 (écrit par Map WRITE_DISCARD).
    Mapped m = {0};
    if (CALL(ctx, 14, Map, cbuf, 0, 4, 0, &m) < 0 || !m.data) ExitProcess(13);
    float *c = m.data;
    c[0] = c[1] = c[2] = 1.0f;
    c[3] = 0.5f;
    CALL(ctx, 15, Unmap, cbuf, 0);

    static const float blue[4] = {0, 0, 1, 1};
    CALL(ctx, 33, SetRT, 1, &rtv, 0);
    CALL(ctx, 50, ClearRT, rtv, blue);
    Viewport vp = {0, 0, SIZE, SIZE, 0, 1};
    CALL(ctx, 44, SetVP, 1, &vp);
    u32 stride = 16, offset = 0;
    CALL(ctx, 17, SetOne, il);
    CALL(ctx, 18, SetVB, 0, 1, &vbuf, &stride, &offset);
    CALL(ctx, 19, SetIB, ibuf, 57 /* R16_UINT */, 0);
    CALL(ctx, 24, SetU32, 4 /* TRIANGLELIST */);
    CALL(ctx, 11, SetShader, vs, 0, 0);
    CALL(ctx, 9, SetShader, ps, 0, 0);
    CALL(ctx, 16, SetArray, 0, 1, &cbuf);
    CALL(ctx, 8, SetArray, 0, 1, &srv);
    CALL(ctx, 10, SetArray, 0, 1, &smp);
    CALL(ctx, 12, DrawIndexed, 6, 0, 0);

    CALL(ctx, 47, Copy, staging, rt);
    Mapped r = {0};
    if (CALL(ctx, 14, Map, staging, 0, 1 /* READ */, 0, &r) < 0 || !r.data) ExitProcess(14);
    const u8 *px = r.data;
#define AT(x, y) (px + (y) * r.row + (x) * 4)
    // En haut à gauche : rouge, alpha 0.5 ; en bas à gauche : vert ;
    // à droite : le bleu de l'effacement.
    const u8 *tl = AT(4, 4), *bl = AT(4, SIZE - 5), *right = AT(SIZE - 5, SIZE / 2);
    if (tl[0] < 250 || tl[1] > 5 || tl[2] > 5 || tl[3] < 120 || tl[3] > 136) ExitProcess(20 + (tl[1] > 200));
    if (bl[0] > 5 || bl[1] < 250 || bl[2] > 5) ExitProcess(22 + (bl[0] > 200));
    if (right[0] > 5 || right[1] > 5 || right[2] < 250 || right[3] < 250) ExitProcess(24);
    // Bord du quart : la colonne 31 est dessinée, la 33 non.
    if (AT(31, 10)[2] > 5 || AT(33, 10)[2] < 250) ExitProcess(25);
    CALL(ctx, 15, Unmap, staging, 0);
    ExitProcess(0);
}
