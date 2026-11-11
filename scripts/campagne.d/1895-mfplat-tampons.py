# #1895 mfplat : MFCreateMediaBufferWrapper, MFCreateMediaBufferFromMediaType et MFCopyImage (Discord).
s = (runtime_dlls / "mfplat.rs").read_text(encoding="utf-8")
ok('"MFCreateMediaBufferWrapper"' in s and '"MFCreateMediaBufferFromMediaType"' in s and '"MFCopyImage"' in s,
   "mfplat sert la famille des tampons Media Foundation (#1895)")
