// Rendu Direct3D 11 à partir de HLSL source (#463) : D3DCompile compile un
// vertex et un pixel shader (tampon de constantes avec matrice column_major,
// structure de sortie, fonction auxiliaire, texture et échantillonneur), puis
// le quad texturé est dessiné et les pixels relus comme dans d3d11_msvc.c.
// Format MSVC (clang en mode x86_64-pc-windows-msvc, sans CRT).
// Code de sortie : 0 si tout passe, 77 si aucun affichage OpenGL, sinon le
// numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) long __stdcall D3DCompile(const void *, u64, const char *, const void *, void *, const char *,
                                                const char *, u32, u32, void **, void **);
__declspec(dllimport) int __stdcall D3D11CreateDevice(void *, u32, void *, u32, const u32 *, u32, u32, void **, u32 *,
                                                      void **);
__declspec(dllimport) long __stdcall CreateDXGIFactory1(const void *, void **);

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
typedef void(__stdcall *Dispatch)(void *, u32, u32, u32);
typedef void(__stdcall *SetUAV)(void *, u32, u32, void *const *, const u32 *);
typedef void(__stdcall *ClearDS)(void *, void *, u32, float, u8);

typedef void(__stdcall *UpdateSub)(void *, void *, u32, const void *, const void *, u32, u32);
typedef void(__stdcall *CopySub)(void *, void *, u32, u32, u32, u32, void *, u32, const void *);
typedef long(__stdcall *CreateSC)(void *, void *, const void *, void **);
typedef long(__stdcall *GetBuffer)(void *, u32, const void *, void **);
typedef void(__stdcall *GetDesc)(void *, void *);
typedef long(__stdcall *Present)(void *, u32, u32);

typedef void *(__stdcall *BlobPtr)(void *);
typedef u64(__stdcall *BlobSize)(void *);

static const char hlsl[] =
    "cbuffer Transform : register(b0) { float4x4 world; };\n"
    "cbuffer Material : register(b1) { float4 tint; };\n"
    "struct VSOut { float4 pos : SV_Position; float2 uv : TEXCOORD0; };\n"
    "VSOut vs_main(float2 p : POSITION, float2 uv : TEXCOORD0) {\n"
    "    VSOut o;\n"
    "    o.pos = mul(float4(p, 0.0, 1.0), world);\n"
    "    o.uv = uv;\n"
    "    return o;\n"
    "}\n"
    "Texture2D tex : register(t0);\n"
    "SamplerState smp : register(s0);\n"
    "float4 teinte(float4 c) { return c * tint; }\n"
    "[maxvertexcount(3)]\n"
    "void gs_main(triangle VSOut v[3], inout TriangleStream<VSOut> st) {\n"
    "    for (int k = 0; k < 3; k++) st.Append(v[k]);\n"
    "    st.RestartStrip();\n"
    "}\n"
    "Texture2DArray texa : register(t1);\n"
    "float4 ps_main(VSOut i) : SV_Target {\n"
    "    return teinte(tex.Sample(smp, i.uv)) * texa.Sample(smp, float3(i.uv, 1));\n"
    "}\n";

// Compute shader (#580) : image RWTexture2D, tampons structurés en
// lecture et en écriture, mémoire partagée de groupe, barrière et atomique.
static const char hlsl_cs[] =
    "RWTexture2D<unorm float4> img : register(u0);\n"
    "RWStructuredBuffer<uint> sortie : register(u1);\n"
    "RWByteAddressBuffer compteur : register(u2);\n"
    "StructuredBuffer<uint> entree : register(t0);\n"
    "cbuffer Params : register(b0) { uint mult; uint3 pad; };\n"
    "groupshared uint partage[64];\n"
    "[numthreads(8, 8, 1)]\n"
    "void cs_main(uint3 id : SV_DispatchThreadID, uint gi : SV_GroupIndex) {\n"
    "    partage[gi] = gi;\n"
    "    GroupMemoryBarrierWithGroupSync();\n"
    "    img[id.xy] = float4(id.x < 32 ? 1.0 : 0.0, 0.0, id.y < 32 ? 1.0 : 0.0, 1.0);\n"
    "    if (id.y == 0 && id.x < 16) sortie[id.x] = entree[id.x] * mult + partage[63 - gi];\n"
    "    uint ancien;\n"
    "    compteur.InterlockedAdd(0, 1, ancien);\n"
    "}\n";

static void *compile_src(const char *src, u64 len, const char *entry, const char *target, int fail) {
    void *code = 0, *errors = 0;
    if (D3DCompile(src, len, "cs.hlsl", 0, 0, entry, target, 0, 0, &code, &errors) < 0 || !code) ExitProcess(fail);
    return code;
}

static void *compile(const char *entry, const char *target, int fail) {
    void *code = 0, *errors = 0;
    if (D3DCompile(hlsl, sizeof hlsl - 1, "quad.hlsl", 0, 0, entry, target, 0, 0, &code, &errors) < 0 || !code)
        ExitProcess(fail);
    return code;
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

    // Chaîne d'échange créée APRÈS la vue de rendu, comme ANGLE (#509) : son
    // contexte GL devient courant, et la vue créée avant doit toujours viser
    // sa texture, pas le tampon arrière de la chaîne.
    static const u8 factory_iid[16] = {0x78, 0xae, 0x0a, 0x77, 0x6f, 0xf2, 0xba, 0x4d,
                                       0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87};
    void *factory = 0, *swap = 0;
    if (CreateDXGIFactory1(factory_iid, &factory) < 0 || !factory) ExitProcess(40);
    u32 scd[18] = {0};
    scd[0] = SIZE;     // BufferDesc.Width
    scd[1] = SIZE;     // BufferDesc.Height
    scd[4] = 28;       // BufferDesc.Format
    scd[7] = 1;        // SampleDesc.Count
    scd[9] = 0x20;     // BufferUsage RENDER_TARGET_OUTPUT
    scd[10] = 1;       // BufferCount ; OutputWindow (scd[12..14]) = 0
    scd[14] = 1;       // Windowed
    if (CALL(factory, 10, CreateSC, dev, scd, &swap) < 0 || !swap) ExitProcess(41);
    // Le tampon arrière a la taille demandée (il était toujours en 1280x720).
    void *back = 0;
    static const u8 tex2d_iid[16] = {0xf2, 0xaa, 0x15, 0x6f, 0x08, 0xd2, 0x89, 0x4e,
                                     0x9a, 0xb4, 0x48, 0x95, 0x35, 0xd3, 0x4f, 0x9c};
    if (CALL(swap, 9, GetBuffer, 0, tex2d_iid, &back) < 0 || !back) ExitProcess(42);
    TexDesc bd = {0};
    CALL(back, 10, GetDesc, &bd);
    if (bd.w != SIZE || bd.h != SIZE) ExitProcess(43);

    // Texture 2×2 : rangée du haut rouge, du bas verte. Remplie comme le
    // fait ANGLE (#514) : UpdateSubresource dans une texture intermédiaire
    // (pas de ligne de 16 octets, puis une boîte pour la 2e rangée), puis
    // CopySubresourceRegion vers la texture échantillonnée à 2 niveaux.
    static const u32 padded[8] = {0xFF0000FF, 0xFF0000FF, 0, 0, 0xFFFF0000, 0xFFFF0000, 0, 0};
    static const u32 green[2] = {0xFF00FF00, 0xFF00FF00};
    TexDesc xd = {2, 2, 2, 1, 28, 1, 0, 0, 0x8, 0, 0};
    TexDesc ud = {2, 2, 1, 1, 28, 1, 0, 0, 0, 0, 0};
    void *tex = 0, *up = 0, *srv = 0, *smp = 0;
    if (CALL(dev, 5, CreateTex, &xd, 0, &tex) < 0) ExitProcess(4);
    if (CALL(dev, 5, CreateTex, &ud, 0, &up) < 0) ExitProcess(4);
    CALL(ctx, 48, UpdateSub, up, 0, 0, padded, 16, 0);
    u32 row1[6] = {0, 1, 0, 2, 2, 1};
    CALL(ctx, 48, UpdateSub, up, 0, row1, green, 8, 0);
    CALL(ctx, 46, CopySub, tex, 0, 0, 0, 0, up, 0, 0);
    // Vue limitée au niveau 0 comme celles d'ANGLE (#516) ; MinLOD = 1
    // force la réduction : sans la limite, le niveau 1 (vide) serait lu.
    u32 srvd[4] = {28, 4 /* TEXTURE2D */, 0 /* MostDetailedMip */, 1 /* MipLevels */};
    if (CALL(dev, 7, CreateView, tex, srvd, &srv) < 0) ExitProcess(5);
    // Tableau de 2 tranches en BC1 (#519) : tranche 0 noire, tranche 1
    // blanche ; le pixel shader lit la tranche 1. Sans tableau ni format
    // compressé, la texture est incomplète et tout devient noir.
    static const u16 bc_black[4] = {0x0000, 0x0000, 0, 0}, bc_white[4] = {0xFFFF, 0xFFFF, 0, 0};
    TexDesc ad = {4, 4, 1, 2, 71 /* BC1_UNORM */, 1, 0, 0, 0x8, 0, 0};
    Init ai[2] = {{bc_black, 8, 0}, {bc_white, 8, 0}};
    void *texa = 0, *srva = 0;
    if (CALL(dev, 5, CreateTex, &ad, ai, &texa) < 0 || !texa) ExitProcess(45);
    if (CALL(dev, 7, CreateView, texa, 0, &srva) < 0 || !srva) ExitProcess(46);
    u32 sdesc[13] = {0 /* MIN_MAG_MIP_POINT */, 3, 3, 3, 0, 1, 1, 0, 0, 0, 0, 0x3F800000 /* MinLOD 1 */, 0x7F7FFFFF};
    if (CALL(dev, 23, CreateState, sdesc, &smp) < 0 || !smp) ExitProcess(6);

    // Moitié gauche de l'écran (sens horaire, face avant D3D), que la matrice
    // du vertex shader déplace à droite.
    static const float verts[] = {-1, 1, 0, 0, 0, 1, 1, 0, 0, -1, 1, 1, -1, -1, 0, 1};
    static const u16 index[] = {0, 1, 2, 0, 2, 3};
    BufferDesc vb = {sizeof verts, 0, 1, 0, 0, 0}, ib = {sizeof index, 0, 2, 0, 0, 0};
    Init vi = {verts, 0, 0}, ii = {index, 0, 0};
    void *vbuf = 0, *ibuf = 0, *cbuf = 0;
    if (CALL(dev, 3, CreateBuffer, &vb, &vi, &vbuf) < 0) ExitProcess(7);
    if (CALL(dev, 3, CreateBuffer, &ib, &ii, &ibuf) < 0) ExitProcess(8);
    BufferDesc cb = {16, 2 /* DYNAMIC */, 4, 0x10000, 0, 0};
    if (CALL(dev, 3, CreateBuffer, &cb, 0, &cbuf) < 0) ExitProcess(9);
    // Matrice column_major (registre j = colonne j) : translation x de +1,
    // le quad passe de la moitié gauche à la moitié droite.
    static const float world[16] = {1, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1};
    BufferDesc mb = {sizeof world, 0, 4, 0, 0, 0};
    Init mi = {world, 0, 0};
    void *mbuf = 0;
    if (CALL(dev, 3, CreateBuffer, &mb, &mi, &mbuf) < 0) ExitProcess(26);

    void *vsb = compile("vs_main", "vs_5_0", 30), *psb = compile("ps_main", "ps_5_0", 31);
    void *vs_blob = ((BlobPtr)VT(vsb, 3))(vsb), *ps_blob = ((BlobPtr)VT(psb, 3))(psb);
    u64 vs_len = ((BlobSize)VT(vsb, 4))(vsb), ps_len = ((BlobSize)VT(psb, 4))(psb);
    void *vs = 0, *ps = 0, *il = 0;
    if (CALL(dev, 12, CreateShader, vs_blob, vs_len, 0, &vs) < 0) ExitProcess(10);
    if (CALL(dev, 15, CreateShader, ps_blob, ps_len, 0, &ps) < 0) ExitProcess(11);
    // Geometry shader HLSL de passage (#520) : le quad doit être identique.
    void *gsb = compile("gs_main", "gs_5_0", 32), *gs = 0;
    void *gs_blob = ((BlobPtr)VT(gsb, 3))(gsb);
    u64 gs_len = ((BlobSize)VT(gsb, 4))(gsb);
    if (CALL(dev, 13, CreateShader, gs_blob, gs_len, 0, &gs) < 0 || !gs) ExitProcess(33);
    InputElem le[] = {{"POSITION", 0, 16 /* R32G32_FLOAT */, 0, 0, 0, 0}, {"TEXCOORD", 0, 16, 0, 0xFFFFFFFF, 0, 0}};
    if (CALL(dev, 11, CreateIL, le, 2, vs_blob, vs_len, &il) < 0) ExitProcess(12);

    // Constantes : blanc, alpha 0.5 (écrit par Map WRITE_DISCARD).
    Mapped m = {0};
    if (CALL(ctx, 14, Map, cbuf, 0, 4, 0, &m) < 0 || !m.data) ExitProcess(13);
    float *c = m.data;
    c[0] = c[1] = c[2] = 1.0f;
    c[3] = 0.5f;
    CALL(ctx, 15, Unmap, cbuf, 0);

    // Profondeur comme la crée ANGLE (#506) : R24G8_TYPELESS liée en
    // profondeur et en lecture, vue D24_UNORM_S8_UINT.
    TexDesc dd = {SIZE, SIZE, 1, 1, 44 /* R24G8_TYPELESS */, 1, 0, 0, 0x40 | 0x8, 0, 0};
    void *depth = 0, *dsv = 0;
    if (CALL(dev, 5, CreateTex, &dd, 0, &depth) < 0 || !depth) ExitProcess(27);
    u32 dsvd[4] = {45 /* D24_UNORM_S8_UINT */, 3 /* TEXTURE2D */, 0, 0};
    if (CALL(dev, 10, CreateView, depth, dsvd, &dsv) < 0 || !dsv) ExitProcess(28);
    static const float blue[4] = {0, 0, 1, 1};
    CALL(ctx, 33, SetRT, 1, &rtv, dsv);
    CALL(ctx, 53, ClearDS, dsv, 1 | 2, 1.0f, 0);
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
    CALL(ctx, 23, SetShader, gs, 0, 0);
    CALL(ctx, 7, SetArray, 0, 1, &mbuf);  // VSSetConstantBuffers b0
    CALL(ctx, 16, SetArray, 1, 1, &cbuf); // PSSetConstantBuffers b1
    void *srvs[2] = {srv, srva};
    CALL(ctx, 8, SetArray, 0, 2, srvs);
    CALL(ctx, 10, SetArray, 0, 1, &smp);
    CALL(ctx, 12, DrawIndexed, 6, 0, 0);

    CALL(ctx, 47, Copy, staging, rt);
    Mapped r = {0};
    if (CALL(ctx, 14, Map, staging, 0, 1 /* READ */, 0, &r) < 0 || !r.data) ExitProcess(14);
    const u8 *px = r.data;
#define AT(x, y) (px + (y) * r.row + (x) * 4)
    // Quad déplacé à droite : en haut à droite rouge, alpha 0.5 ; en bas à
    // droite vert ; à gauche, le bleu de l'effacement.
    const u8 *tl = AT(SIZE - 5, 4), *bl = AT(SIZE - 5, SIZE - 5), *right = AT(4, SIZE / 2);
    if (tl[0] < 250 || tl[1] > 5 || tl[2] > 5 || tl[3] < 120 || tl[3] > 136) ExitProcess(20 + (tl[1] > 200));
    if (bl[0] > 5 || bl[1] < 250 || bl[2] > 5) ExitProcess(22 + (bl[0] > 200));
    if (right[0] > 5 || right[1] > 5 || right[2] < 250 || right[3] < 250) ExitProcess(24);
    // Bord du quad : la colonne 33 est dessinée, la 30 non.
    if (AT(33, 10)[2] > 5 || AT(30, 10)[2] < 250) ExitProcess(25);
    CALL(ctx, 15, Unmap, staging, 0);

    // Passe « profondeur seule » comme les cascades d'ombre d'ANGLE (#530) :
    // aucune vue couleur, une vue de profondeur sur la tranche 1 d'un
    // tableau D32_FLOAT. Le quad (profondeur 0) doit y être écrit ; il
    // partait dans la fenêtre et la tranche restait à 1.0.
    TexDesc zd = {SIZE, SIZE, 1, 2, 40 /* D32_FLOAT */, 1, 0, 0, 0x40, 0, 0};
    void *zarr = 0, *zdsv = 0, *zstaging = 0;
    if (CALL(dev, 5, CreateTex, &zd, 0, &zarr) < 0 || !zarr) ExitProcess(50);
    u32 zdsvd[6] = {40, 4 /* TEXTURE2DARRAY */, 0 /* Flags */, 0 /* MipSlice */, 1 /* First */, 1 /* Size */};
    if (CALL(dev, 10, CreateView, zarr, zdsvd, &zdsv) < 0 || !zdsv) ExitProcess(51);
    TexDesc zs = zd;
    zs.usage = 3;
    zs.bind = 0;
    zs.cpu = 0x20000;
    if (CALL(dev, 5, CreateTex, &zs, 0, &zstaging) < 0 || !zstaging) ExitProcess(52);
    // L'effacement a lieu pendant qu'une petite cible 16x16 est liée, comme
    // chez ANGLE (ClearDSV avant OMSetRenderTargets) : il doit couvrir toute
    // la vue de profondeur, pas l'intersection avec la cible (#537).
    TexDesc pd = td;
    pd.w = pd.h = 16;
    void *small = 0, *small_rtv = 0;
    if (CALL(dev, 5, CreateTex, &pd, 0, &small) < 0 || !small) ExitProcess(56);
    if (CALL(dev, 9, CreateView, small, 0, &small_rtv) < 0 || !small_rtv) ExitProcess(57);
    CALL(ctx, 33, SetRT, 1, &small_rtv, 0);
    CALL(ctx, 53, ClearDS, zdsv, 1, 1.0f, 0);
    CALL(ctx, 33, SetRT, 0, 0, zdsv);
    CALL(ctx, 12, DrawIndexed, 6, 0, 0);
    CALL(ctx, 47, Copy, zstaging, zarr);
    Mapped z = {0};
    if (CALL(ctx, 14, Map, zstaging, 1 /* tranche 1 */, 1, 0, &z) < 0 || !z.data) ExitProcess(53);
    const float *zr = (const float *)((const u8 *)z.data + (SIZE / 2) * z.row);
    // Moitié droite : le quad (0.0) ; moitié gauche : l'effacement (1.0).
    if (zr[SIZE - 5] > 0.01f) ExitProcess(54);
    if (zr[4] < 0.99f || zr[20] < 0.99f) ExitProcess(55);
    CALL(ctx, 15, Unmap, zstaging, 1);
    CALL(ctx, 33, SetRT, 1, &rtv, dsv);

    // Calcul (#580) : grille 64x64 en groupes 8x8. Image : rouge à
    // gauche, bleu en haut ; sortie[i] = entree[i] * 3 + (63 - i % 8) ;
    // compteur = 4096 invocations.
    {
        TexDesc ud = td;
        ud.bind = 0x80 | 0x8; // UNORDERED_ACCESS | SHADER_RESOURCE
        void *utex = 0, *uview = 0, *ustaging = 0;
        if (CALL(dev, 5, CreateTex, &ud, 0, &utex) < 0 || !utex) ExitProcess(60);
        if (CALL(dev, 8, CreateView, utex, 0, &uview) < 0 || !uview) ExitProcess(61);
        if (CALL(dev, 5, CreateTex, &sd, 0, &ustaging) < 0 || !ustaging) ExitProcess(62);
        u32 init[16];
        for (u32 k = 0; k < 16; k++) init[k] = k + 1;
        BufferDesc inb = {sizeof init, 0, 0x8, 0, 0x40 /* STRUCTURED */, 4};
        BufferDesc outb = {sizeof init, 0, 0x80, 0, 0x40, 4};
        BufferDesc rawb = {16, 0, 0x80, 0, 0x20 /* ALLOW_RAW_VIEWS */, 0};
        BufferDesc readb = {sizeof init, 3, 0, 0x20000, 0, 0};
        Init ini = {init, 0, 0};
        void *inbuf = 0, *outbuf = 0, *rawbuf = 0, *readbuf = 0, *readraw = 0;
        if (CALL(dev, 3, CreateBuffer, &inb, &ini, &inbuf) < 0) ExitProcess(63);
        if (CALL(dev, 3, CreateBuffer, &outb, 0, &outbuf) < 0) ExitProcess(64);
        if (CALL(dev, 3, CreateBuffer, &rawb, 0, &rawbuf) < 0) ExitProcess(65);
        if (CALL(dev, 3, CreateBuffer, &readb, 0, &readbuf) < 0) ExitProcess(66);
        readb.width = 16;
        if (CALL(dev, 3, CreateBuffer, &readb, 0, &readraw) < 0) ExitProcess(66);
        u32 srvd[4] = {0, 1 /* BUFFER */, 0, 16}, uavd[5] = {0, 1, 0, 16, 0};
        u32 rawd[5] = {39 /* R32_TYPELESS */, 1, 0, 4, 1 /* RAW */};
        void *insrv = 0, *outuav = 0, *rawuav = 0;
        if (CALL(dev, 7, CreateView, inbuf, srvd, &insrv) < 0 || !insrv) ExitProcess(67);
        if (CALL(dev, 8, CreateView, outbuf, uavd, &outuav) < 0 || !outuav) ExitProcess(68);
        if (CALL(dev, 8, CreateView, rawbuf, rawd, &rawuav) < 0 || !rawuav) ExitProcess(69);
        static const u32 params[4] = {3, 0, 0, 0};
        BufferDesc pb = {16, 0, 4, 0, 0, 0};
        Init pi = {params, 0, 0};
        void *pbuf = 0;
        if (CALL(dev, 3, CreateBuffer, &pb, &pi, &pbuf) < 0) ExitProcess(70);
        void *csb = compile_src(hlsl_cs, sizeof hlsl_cs - 1, "cs_main", "cs_5_0", 71), *cs = 0;
        void *cs_blob = ((BlobPtr)VT(csb, 3))(csb);
        u64 cs_len = ((BlobSize)VT(csb, 4))(csb);
        if (CALL(dev, 18, CreateShader, cs_blob, cs_len, 0, &cs) < 0 || !cs) ExitProcess(72);
        static const u32 zero[4] = {0, 0, 0, 0};
        typedef void(__stdcall * ClearU)(void *, void *, const u32 *);
        CALL(ctx, 51, ClearU, rawuav, zero);
        void *uavs[3] = {uview, outuav, rawuav};
        CALL(ctx, 69, SetShader, cs, 0, 0);
        CALL(ctx, 68, SetUAV, 0, 3, uavs, 0);
        CALL(ctx, 67, SetArray, 0, 1, &insrv);
        CALL(ctx, 71, SetArray, 0, 1, &pbuf);
        CALL(ctx, 41, Dispatch, SIZE / 8, SIZE / 8, 1);
        CALL(ctx, 47, Copy, ustaging, utex);
        CALL(ctx, 47, Copy, readbuf, outbuf);
        CALL(ctx, 47, Copy, readraw, rawbuf);
        Mapped u = {0};
        if (CALL(ctx, 14, Map, ustaging, 0, 1, 0, &u) < 0 || !u.data) ExitProcess(73);
        const u8 *a = (const u8 *)u.data + 4 * u.row + 4 * 4, *b = (const u8 *)u.data + 60 * u.row + 60 * 4;
        if (a[0] < 250 || a[1] > 5 || a[2] < 250 || a[3] < 250) ExitProcess(74);
        if (b[0] > 5 || b[2] > 5 || b[3] < 250) ExitProcess(75);
        CALL(ctx, 15, Unmap, ustaging, 0);
        Mapped o = {0};
        if (CALL(ctx, 14, Map, readbuf, 0, 1, 0, &o) < 0 || !o.data) ExitProcess(76);
        const u32 *ov = o.data;
        for (u32 k = 0; k < 16; k++)
            if (ov[k] != (k + 1) * 3 + (63 - k % 8)) ExitProcess(78);
        CALL(ctx, 15, Unmap, readbuf, 0);
        if (CALL(ctx, 14, Map, readraw, 0, 1, 0, &o) < 0 || !o.data) ExitProcess(79);
        if (*(const u32 *)o.data != SIZE * SIZE) ExitProcess(80);
        CALL(ctx, 15, Unmap, readraw, 0);
        CALL(ctx, 69, SetShader, 0, 0, 0);
    }

    // Image copiée dans le tampon arrière puis présentée.
    CALL(ctx, 47, Copy, back, rt);
    if (CALL(swap, 8, Present, 0, 0) < 0) ExitProcess(44);
    ExitProcess(0);
}
