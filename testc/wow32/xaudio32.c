/* XAudio2 2.9 depuis un programme 32 bits (#2428), construit deux fois par
 * clang sans CRT : en x86 (xaudio32.exe) et en x64 (xaudio64.exe).
 * Couvre XAudio2Create, les voix de mastering, de sous-mixage et source
 * (objets sans IUnknown, détruits par DestroyVoice), une chaîne d'effets
 * (vumètre CreateAudioVolumeMeter), une liste d'envois, un tampon
 * XAUDIO2_BUFFER avec pContext, les rappels IXAudio2VoiceCallback écrits
 * dans le programme (stdcall en x86), les flottants passés par valeur
 * (MaxFrequencyRatio, SetVolume) et XAUDIO2_VOICE_STATE.
 * Les structures suivent xaudio2.h : #pragma pack(1) (#2442).
 * Rend 0 et écrit « XAudio2 : tout est correct » si tout va bien, sinon le
 * numéro de la vérification qui a échoué. */
typedef unsigned int DWORD, UINT, UINT32;
typedef int BOOL, HRESULT;
typedef unsigned short WORD;
typedef unsigned char BYTE;
typedef unsigned long long UINT64;
typedef void *HANDLE;
#ifdef _WIN64
typedef unsigned long long UINT_PTR;
#else
typedef unsigned int UINT_PTR;
#endif
#define WINAPI __stdcall
#define IMP __declspec(dllimport)

IMP HANDLE WINAPI GetStdHandle(DWORD);
IMP BOOL WINAPI WriteFile(HANDLE, const void *, DWORD, DWORD *, void *);
IMP void WINAPI ExitProcess(DWORD);
IMP void WINAPI Sleep(DWORD);
IMP DWORD WINAPI GetTickCount(void);
IMP HRESULT WINAPI XAudio2Create(void **, UINT32, UINT32);
IMP HRESULT WINAPI CreateAudioVolumeMeter(void **);

/* mmreg.h et xaudio2.h, xaudio2fx.h (déclarations publiques). */
#pragma pack(push, 1)
typedef struct {
    WORD wFormatTag, nChannels;
    DWORD nSamplesPerSec, nAvgBytesPerSec;
    WORD nBlockAlign, wBitsPerSample, cbSize;
} WAVEFORMATEX;
typedef struct {
    UINT32 Flags, AudioBytes;
    const BYTE *pAudioData;
    UINT32 PlayBegin, PlayLength, LoopBegin, LoopLength, LoopCount;
    void *pContext;
} XAUDIO2_BUFFER;
typedef struct { void *pCurrentBufferContext; UINT32 BuffersQueued; UINT64 SamplesPlayed; } XAUDIO2_VOICE_STATE;
typedef struct { UINT32 Flags; void *pOutputVoice; } XAUDIO2_SEND_DESCRIPTOR;
typedef struct { UINT32 SendCount; XAUDIO2_SEND_DESCRIPTOR *pSends; } XAUDIO2_VOICE_SENDS;
typedef struct { void *pEffect; BOOL InitialState; UINT32 OutputChannels; } XAUDIO2_EFFECT_DESCRIPTOR;
typedef struct { UINT32 EffectCount; XAUDIO2_EFFECT_DESCRIPTOR *pEffectDescriptors; } XAUDIO2_EFFECT_CHAIN;
typedef struct { UINT32 CreationFlags, ActiveFlags, InputChannels, InputSampleRate; } XAUDIO2_VOICE_DETAILS;
typedef struct { float *pPeakLevels, *pRMSLevels; UINT32 ChannelCount; } XAUDIO2FX_VOLUMEMETER_LEVELS;
#pragma pack(pop)

typedef struct Obj { void **vt; } Obj;
typedef HRESULT(WINAPI *M0)(void *);
typedef HRESULT(WINAPI *CreateMaster)(void *, void **, UINT32, UINT32, UINT32, const void *,
                                      const XAUDIO2_EFFECT_CHAIN *, DWORD);
typedef HRESULT(WINAPI *CreateSubmix)(void *, void **, UINT32, UINT32, UINT32, UINT32,
                                      const XAUDIO2_VOICE_SENDS *, const XAUDIO2_EFFECT_CHAIN *);
typedef HRESULT(WINAPI *CreateSource)(void *, void **, const WAVEFORMATEX *, UINT32, float, void *,
                                      const XAUDIO2_VOICE_SENDS *, const XAUDIO2_EFFECT_CHAIN *);
typedef HRESULT(WINAPI *Submit)(void *, const XAUDIO2_BUFFER *, const void *);
typedef HRESULT(WINAPI *SetVol)(void *, float, UINT32);
typedef void(WINAPI *GetVol)(void *, float *);
typedef HRESULT(WINAPI *Start)(void *, UINT32, UINT32);
typedef void(WINAPI *GetState)(void *, XAUDIO2_VOICE_STATE *, UINT32);
typedef HRESULT(WINAPI *GetParams)(void *, UINT32, void *, UINT32);
typedef void(WINAPI *Details)(void *, XAUDIO2_VOICE_DETAILS *);
typedef void(WINAPI *Destroy)(void *);
#define VT(o) (((Obj *)(o))->vt)

enum {
    XA_SOURCE = 5, XA_SUBMIX = 6, XA_MASTER = 7, XA_START = 8,
    V_DETAILS = 0, V_GETPARAMS = 7, V_SETVOL = 12, V_GETVOL = 13, V_DESTROY = 18,
    S_START = 19, S_SUBMIT = 21, S_GETSTATE = 25, S_SETRATIO = 26
};

int _fltused = 0x9875;

static void out(const char *s) {
    DWORD n = 0, len = 0;
    while (s[len]) len++;
    WriteFile(GetStdHandle((DWORD)-11), s, len, &n, 0);
}

static void fail(int n) {
    char msg[] = "echec : verification n. 000\n";
    msg[24] = (char)('0' + n / 100);
    msg[25] = (char)('0' + n / 10 % 10);
    msg[26] = (char)('0' + n % 10);
    out(msg);
    ExitProcess((DWORD)n);
}
#define CHECK(n, c) do { if (!(c)) fail(n); } while (0)

/* IXAudio2VoiceCallback écrit dans le programme (sans IUnknown). */
static volatile UINT passes, stream_ends, buffer_ends;
static void *volatile last_ctx;
static void WINAPI cb_pass_start(void *t, UINT32 bytes) { (void)t; (void)bytes; passes++; }
static void WINAPI cb_nop0(void *t) { (void)t; }
static void WINAPI cb_stream_end(void *t) { (void)t; stream_ends++; }
static void WINAPI cb_nop1(void *t, void *c) { (void)t; (void)c; }
static void WINAPI cb_buffer_end(void *t, void *c) { (void)t; last_ctx = c; buffer_ends++; }
static void WINAPI cb_error(void *t, void *c, HRESULT h) { (void)t; (void)c; (void)h; }
static void *cb_vt[7] = {(void *)cb_pass_start, (void *)cb_nop0, (void *)cb_stream_end,
                         (void *)cb_nop1, (void *)cb_buffer_end, (void *)cb_nop1,
                         (void *)cb_error};
static Obj callback = {cb_vt};

static float pcm[2400];

void entry(void) {
    void *xa = 0, *master = 0, *sub = 0, *src = 0, *meter = 0;
    WAVEFORMATEX wf = {3, 1, 24000, 96000, 4, 32, 0}; /* WAVE_FORMAT_IEEE_FLOAT */
    XAUDIO2_EFFECT_DESCRIPTOR fx;
    XAUDIO2_EFFECT_CHAIN chain;
    XAUDIO2_SEND_DESCRIPTOR send;
    XAUDIO2_VOICE_SENDS sends;
    XAUDIO2_BUFFER buf;
    struct { XAUDIO2_VOICE_STATE s; DWORD guard; } st;
    XAUDIO2_VOICE_DETAILS det;
    XAUDIO2FX_VOLUMEMETER_LEVELS lv;
    float peak[2] = {0, 0}, rms[2] = {0, 0}, vol = 0;
    DWORD t0;
    int i;

    CHECK(1, sizeof(XAUDIO2_BUFFER) == (sizeof(void *) == 4 ? 36 : 44));
    CHECK(2, XAudio2Create(&xa, 0, 1) == 0 && xa);
    CHECK(3, ((CreateMaster)VT(xa)[XA_MASTER])(xa, &master, 0, 0, 0, 0, 0, 0) == 0 && master);
    CHECK(4, CreateAudioVolumeMeter(&meter) == 0 && meter);
    fx.pEffect = meter;
    fx.InitialState = 1;
    fx.OutputChannels = 2;
    chain.EffectCount = 1;
    chain.pEffectDescriptors = &fx;
    CHECK(5, ((CreateSubmix)VT(xa)[XA_SUBMIX])(xa, &sub, 2, 48000, 0, 0, 0, &chain) == 0 && sub);
    ((M0)VT(meter)[2])(meter); /* la voix garde sa référence */
    send.Flags = 0;
    send.pOutputVoice = sub;
    sends.SendCount = 1;
    sends.pSends = &send;
    CHECK(6, ((CreateSource)VT(xa)[XA_SOURCE])(xa, &src, &wf, 0, 2.0f, &callback, &sends, 0) == 0 && src);
    ((Details)VT(src)[V_DETAILS])(src, &det);
    CHECK(7, det.InputChannels == 1 && det.InputSampleRate == 24000);
    for (i = 0; i < 2400; i++) pcm[i] = 0.5f;
    buf.Flags = 0x40; /* XAUDIO2_END_OF_STREAM */
    buf.AudioBytes = sizeof(pcm);
    buf.pAudioData = (const BYTE *)pcm;
    buf.PlayBegin = buf.PlayLength = buf.LoopBegin = buf.LoopLength = buf.LoopCount = 0;
    buf.pContext = (void *)(UINT_PTR)0x5151;
    CHECK(8, ((Submit)VT(src)[S_SUBMIT])(src, &buf, 0) == 0);
    CHECK(9, ((SetVol)VT(src)[V_SETVOL])(src, 0.5f, 0) == 0);
    ((GetVol)VT(src)[V_GETVOL])(src, &vol);
    CHECK(10, vol == 0.5f);
    CHECK(11, ((SetVol)VT(src)[S_SETRATIO])(src, 1.0f, 0) == 0);
    CHECK(12, ((M0)VT(xa)[XA_START])(xa) == 0);
    CHECK(13, ((Start)VT(src)[S_START])(src, 0, 0) == 0);
    t0 = GetTickCount();
    while (!stream_ends) {
        CHECK(14, GetTickCount() - t0 < 5000); /* OnStreamEnd jamais reçu */
        Sleep(5);
    }
    CHECK(15, buffer_ends == 1 && last_ctx == (void *)(UINT_PTR)0x5151);
    CHECK(16, passes > 5);
    st.guard = 0x5a5a5a5au;
    ((GetState)VT(src)[S_GETSTATE])(src, &st.s, 0);
    CHECK(17, st.s.SamplesPlayed == 2400 && st.s.BuffersQueued == 0);
    CHECK(18, st.guard == 0x5a5a5a5au);
    lv.pPeakLevels = peak;
    lv.pRMSLevels = rms;
    lv.ChannelCount = 2;
    CHECK(19, ((GetParams)VT(sub)[V_GETPARAMS])(sub, 0, &lv, sizeof(lv)) == 0);
    ((Destroy)VT(src)[V_DESTROY])(src);
    ((Destroy)VT(sub)[V_DESTROY])(sub);
    ((Destroy)VT(master)[V_DESTROY])(master);
    ((M0)VT(xa)[2])(xa);
    out("XAudio2 : tout est correct\n");
    ExitProcess(0);
}
