# Independent recorded shape inputs

`shapes.json` was derived from the Lead-selected host recordings listed below,
before projecting any replay output. It contains only field names, primitive
kinds, references and recording numbers. No captured string value, dynamic map
key, identifier, timestamp or body is present. The source recordings stay on the
host; tests need only this committed immutable shape table. Hashes identify the
input bytes for Lead review; they do not claim separate reviewer acceptance.

Each root selects one whole response variant by recording, including status.
Objects require exactly their recorded keys. Arrays admit whole recorded item
variants, never a union of optional fields. Empty recorded arrays admit no item.
Only recorded `chunks` maps admit dynamic keys. SSE events and JSON strings in
RPC content are decoded and checked recursively; session headers are required.

The replay state has no Serialize implementation. Only `project` builds nominal
wire values. `apply` records a retain or performs synchronous deletion; completion
requires an explicit recorded TraceStep. Facts and opaque child identities are
invented controller inputs, never extracted text or a sequential identifier.
Same-operation dedup ignores changed bytes before/after completion (010/012/054).
An explicit payload-bearing child lookup (116) alone exposes accepted contents;
ordinary lists and pending-parent lookup (434/446) never echo the request.

The scope is the selected measured variants, not a complete replacement server.
Uncaptured routes, completed-parent lookups, metadata-bearing document summaries,
and nonempty full-detail model lists stop explicitly. No default simulates lag,
pruning, transport failure, finality or extraction. R1b and R1c-R1g are absent here;
the old listener remains independent until its separately assigned replacement.
Tools/list advertises the two captured read-tool shapes only, with invented
help/default values and protocol schema vocabulary; it does not enable writes.
All bodies, ids, names, dates and help strings in projection/tests are invented.

## Source recording byte pins (host custody only)

| Recording | SHA256 |
| --- | --- |
| 006 | 26f37becadc3b26bec2e73be29bf98722dbe5c7234f351f39bb876dc53f421bf |
| 007 | 5d57e4ca7890affeb7290269364e72a7af4c67195de95db5ed702b4634ba0274 |
| 010 | 22ddcfc534c8af9d7ec3bc2f4ff4036f7e16977155a5bec4871e17b3acbed05d |
| 012 | 53b73fc993b06d470a63107f2df96080dac6fa5e1c3799a6357240fb5d0aa6e5 |
| 016 | ed4ad8da057d8ee6bc7753af86cebff076fc04b23adefaaea34327335a8db8aa |
| 018 | 91d95aa4e8f6d3855ce9b094c3a55341236ce17933555adb1e8b8486613ba4fa |
| 021 | 84d75e35aa18dfc673bf99d009ccde44b17f9bfbc9887d55b290a13c1ea412cb |
| 024 | 8a3855212da77da72573fbb5e4d25248cea486848e3eae55cccd8f591ca16b14 |
| 025 | 1ecab8710b302e3dc1bf57847a07aba7ef89176bd095f1a95209fac5cadc8319 |
| 030 | 0cd9b5aac41b5bc95d1a655ed1f491fc6d3bc3d5c8430c7a8f6fe714becf0c73 |
| 031 | 5235d6f9d8834fb3c76ea95b3e9cfcc21302bdcdf615934cd0192afeb1a8e337 |
| 032 | 275787910be0968c7901e83a35995ddbf4835af4c7ff1938df87c12590fd920c |
| 033 | 23ed6ce464a5a8fde7ea64aafccead3d28c65310f570d0648b4549990e7ddda6 |
| 036 | bad94219a961e892ab3f1d4bf3f9cd1a7f3c89f20b0a8bed27a19985c754a382 |
| 054 | 53dfbbe9c8062cc72880c6e51d2fb5379d1f5e115a7d5eb78d3b7f2706970e96 |
| 060 | 6c119c8198f581dc1eb6ac76b3218bcc516ec4c7737d89e2b00589eec2b06b97 |
| 061 | d2cc61637c1200da2198344225b3bbc32c61a59fe7cda36ab9ac09937f22c292 |
| 062 | 223528b787091cd2e20af9ebe7e7cfa6429e6d0965799ec4910756d7a7fb7871 |
| 072 | 4eff7643e37d79a11cb9a2eae3a152f6003d1bde30e3fe1700a573c39b93f8d8 |
| 073 | 20636560d364d4bcdfe1c0529ed7d1b4c274de8d22bf2476c017ff06d4e62dda |
| 075 | f380c398321a6648b93224fe406577c44daf1956ccd95b11201f666247edd494 |
| 079 | cd25262784b5410ad580244e2e2e37796a48061340a8614254ad18ad1b33ad19 |
| 109 | 7778731e2faecd61a485f861d688057808cde60f23a922f9f80bcb9415295bcb |
| 110 | 27d973b1fd04f990430fa94df29dbdf6fd88c4ac7acfaa9f47534a6d663e0ed6 |
| 116 | 413df7241143f92cba8c0833124fccc0de38ee4b0c1cb3a131a810e0d5385951 |
| 124 | dd8ac3ae1f9d63742502ed8d930897bb7357a21ed6e31fb2bd91ae829fdede5d |
| 125 | 78a9d409e6b99937f16f68ef4846b775bacdc609501ebfec43127d3d71603a5f |
| 126 | c2e049649eb330bc62ce389c88377b9326464b7860f9e8292793b56b8c170d78 |
| 127 | d333001375a7571bdcc8bf13bd6f1845e092dbe086204ce9aecb610664be6abf |
| 128 | eb0fd65cb8897e9271f0b2aa73205f6f6a79df2a3befd943d9df8ed1c56bf439 |
| 161 | 530f2be732df5da1f3e1dbb861b5aa3b5278f1e5103c1149e904d7e46a7133d9 |
| 181 | c861a6fda87978faefd0aee91541bf7b26c38520f4026656c1ac1d8fe08e2c8c |
| 190 | 84bbd848ebaf8a93240ce2f16e487254e6ab2e69939095ed06756f9f6c514ec1 |
| 434 | 339d7f7387ee45b149d24309fa420e1266451dbf73301b2ceabacce591f75495 |
| 446 | af4c2ad77194eb91437b347d41d312a8d987efa51b31a3d2ebe9261f05d20b5d |
| 465 | f9aa5c9ef9a99f43056ca3a0409c34bd8ea1c99e01f4fcd63bf69640f5eb194c |
| 470 | 1b5d26adf71e13cfeb3cc996fd415f0393104adfb57da884a781802274c23e58 |
| 472 | 43307042dedccae7271e97f03811ecaa04fc53b63cf2b7f5856bd3fb23be489f |
| 473 | c4e2a451294c946bae07a9ad69574f6ee92b404763cc9d74ef5f15f60aceb5a9 |
| 475 | 831c2803d135a45f0b6842dc322a31730a6ca9a3cce6e67639843414cd4066e3 |
