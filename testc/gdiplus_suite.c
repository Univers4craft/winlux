/* gdiplus_suite : l'API plate de GDI+ telle qu'un programme C la lie
 * (libgdiplus.a de MinGW). Code de sortie = numéro du premier échec. */
#include <windows.h>
#include <gdiplus.h>
#include <stdio.h>
#include <string.h>

#define CHECK(n, cond)                                       \
    do {                                                     \
        if (!(cond)) {                                       \
            printf("gdiplus : test %d en échec (%s)\n", n, #cond); \
            return n;                                        \
        }                                                    \
    } while (0)

static int png_clsid(CLSID *out) {
    UINT n = 0, size = 0;
    if (GdipGetImageEncodersSize(&n, &size) != Ok || size == 0) return 0;
    ImageCodecInfo *info = (ImageCodecInfo *)HeapAlloc(GetProcessHeap(), 0, size);
    int found = 0;
    if (GdipGetImageEncoders(n, size, info) == Ok)
        for (UINT i = 0; i < n; i++)
            if (wcscmp(info[i].MimeType, L"image/png") == 0) {
                *out = info[i].Clsid;
                found = 1;
            }
    HeapFree(GetProcessHeap(), 0, info);
    return found;
}

int main(void) {
    GdiplusStartupInput in = {1, NULL, FALSE, FALSE};
    ULONG_PTR token = 0;
    CHECK(1, GdiplusStartup(&token, &in, NULL) == Ok);

    GpBitmap *bmp = NULL;
    CHECK(2, GdipCreateBitmapFromScan0(64, 32, 0, PixelFormat32bppARGB, NULL, &bmp) == Ok);
    GpGraphics *g = NULL;
    CHECK(3, GdipGetImageGraphicsContext((GpImage *)bmp, &g) == Ok);
    CHECK(4, GdipGraphicsClear(g, 0xFFFFFFFF) == Ok);
    GpSolidFill *red = NULL;
    CHECK(5, GdipCreateSolidFill(0xFFFF0000, &red) == Ok);
    CHECK(6, GdipFillRectangleI(g, (GpBrush *)red, 4, 4, 8, 8) == Ok);
    ARGB c = 0;
    CHECK(7, GdipBitmapGetPixel(bmp, 6, 6, &c) == Ok && c == 0xFFFF0000);
    CHECK(8, GdipBitmapGetPixel(bmp, 20, 6, &c) == Ok && c == 0xFFFFFFFF);

    /* Texte : il doit encrer quelques pixels et se mesurer. */
    GpFontFamily *fam = NULL;
    GpFont *font = NULL;
    CHECK(9, GdipGetGenericFontFamilySansSerif(&fam) == Ok);
    CHECK(10, GdipCreateFont(fam, 14.0f, FontStyleRegular, UnitPixel, &font) == Ok);
    GpSolidFill *black = NULL;
    GdipCreateSolidFill(0xFF000000, &black);
    RectF box = {16.0f, 0.0f, 48.0f, 32.0f}, bound;
    INT fitted = 0, lines = 0;
    CHECK(11, GdipMeasureString(g, L"Ab", -1, font, &box, NULL, &bound, &fitted, &lines) == Ok);
    CHECK(12, fitted == 2 && lines == 1 && bound.Width > 5.0f && bound.Height > 10.0f);
    CHECK(13, GdipDrawString(g, L"Ab", -1, font, &box, NULL, (GpBrush *)black) == Ok);

    /* LockBits en lecture. */
    BitmapData data;
    GpRect r = {0, 0, 64, 32};
    CHECK(14, GdipBitmapLockBits(bmp, &r, ImageLockModeRead, PixelFormat32bppARGB, &data) == Ok);
    CHECK(15, data.Stride == 64 * 4 && data.Width == 64 && data.Height == 32);
    DWORD *px = (DWORD *)data.Scan0;
    CHECK(16, px[6 * 64 + 6] == 0xFFFF0000);
    int dark = 0;
    for (int i = 0; i < 64 * 32; i++)
        if ((i % 64) >= 16 && (px[i] & 0xFF) < 0x80) dark++;
    GdipBitmapUnlockBits(bmp, &data);
    CHECK(17, dark > 0);

    /* Enregistrement en PNG puis relecture. */
    CLSID png;
    CHECK(18, png_clsid(&png));
    WCHAR path[MAX_PATH];
    GetTempPathW(MAX_PATH, path);
    wcscat(path, L"winlux-gdiplus-suite.png");
    CHECK(19, GdipSaveImageToFile((GpImage *)bmp, path, &png, NULL) == Ok);
    GpImage *back = NULL;
    CHECK(20, GdipLoadImageFromFile(path, &back) == Ok);
    UINT w = 0, h = 0;
    GdipGetImageWidth(back, &w);
    GdipGetImageHeight(back, &h);
    CHECK(21, w == 64 && h == 32);
    CHECK(22, GdipBitmapGetPixel((GpBitmap *)back, 6, 6, &c) == Ok && c == 0xFFFF0000);
    GdipDisposeImage(back);
    DeleteFileW(path);

    /* HBITMAP pour GDI. */
    HBITMAP hbm = NULL;
    CHECK(23, GdipCreateHBITMAPFromBitmap(bmp, &hbm, 0) == Ok && hbm != NULL);
    DeleteObject(hbm);

    GdipDeleteBrush((GpBrush *)red);
    GdipDeleteBrush((GpBrush *)black);
    GdipDeleteFont(font);
    GdipDeleteFontFamily(fam);
    GdipDeleteGraphics(g);
    GdipDisposeImage((GpImage *)bmp);
    GdiplusShutdown(token);
    printf("gdiplus : OK\n");
    return 0;
}
