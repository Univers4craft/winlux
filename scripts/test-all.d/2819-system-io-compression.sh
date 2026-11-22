# #2819 : System.IO.Compression (ZipArchive en lecture, DeflateStream),
# témoin Compression contre une grosse archive écrite par Python.
if [ "$quick" = 0 ]; then
    step "System.IO.Compression : ZipArchive et DeflateStream sur une vraie archive (#2819)" sh scripts/test-zip.sh
else
    skip "System.IO.Compression : ZipArchive et DeflateStream sur une vraie archive (#2819)" "mode rapide"
fi
