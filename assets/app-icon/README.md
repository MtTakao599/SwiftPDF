# PdfCraft app icon

SwiftPDF does not use this artwork. Its window icon is `docs/images/SwiftPDFicon.png`, and
`swiftpdf.ico` (16, 24, 32, 48, 64, 128 and 256 px, resampled from that PNG) is what
`apps/pdfcraft/build.rs` embeds in `SwiftPDF.exe`. The files below stay so the PdfCraft icon's
licence and origin remain on record.

<img src="pdfcraft-small.svg" alt="PdfCraft app icon: an engraved lion's head on green" width="128">

**Creature:** a lion, in a frontal head-and-shoulders portrait, mane running off the bottom of the tile.

**Style:** an engraving (woodcut-weight line work) portrait in the Crafting Apps "owl template" framing:
a full-bleed colour field, no frame or roundel, the animal looking at the viewer and filling the tile.

**Palette:** exactly three colours.

| Colour | Hex | Used for |
|---|---|---|
| Ink | `#0b0b0c` | line work and the figure's contour |
| Paper | `#efe9dc` | the figure (the lion's silhouette) |
| PdfCraft green (app colour) | `#12a58a` | the full-bleed field |

**Tile:** `viewBox="0 0 512 512"`, a rounded square with `rx=112` that clips everything. Windows and Linux
icons use the full-bleed tile. macOS icons put it on Apple's grid (an 824 px body centred on a transparent
1024 px canvas).

**Provenance:** the project owner's original drawing, made in ArtCraft (2880 px, keyed to the palette), then
vectorised with craftrules `assets/logo-options/_tools/vectorize_tile.py` (potrace; no filtering or
warping). The source drawing is kept in craftrules at `assets/app-icons/pdfcraft/source.png`, not here.
Licence: [LICENSE.txt](LICENSE.txt) (`MIT OR Apache-2.0`, like the repo).

## Files

| File | What it is |
|---|---|
| `pdfcraft.svg` | the master vector (traced at 2048 px); every PNG, `.ico` and `.icns` is rendered from it |
| `pdfcraft-small.svg` | a lighter vector (traced at 1024 px) for places where size matters, such as this README |
| `pdfcraft-1024.png` | 1024 px on Apple's grid; the PdfCraft macOS artwork, kept here |
| `pdfcraft.icns` | macOS icon (16–1024 px) |
| `pdfcraft.ico` | PdfCraft Windows icon (16–256 px), kept here; SwiftPDF.exe embeds `swiftpdf.ico` |
| `pdfcraft-document.svg` | PdfCraft PDF file icon master: a Lucide document outline, green badge and outlined PDF lettering; no font dependency |
| `pdfcraft-document.ico` | PdfCraft PDF file icon (16–256 px), kept here; SwiftPDF uses `swiftpdf.ico` for resource 2 |
| `hicolor/<n>x<n>/apps/ai.storyteller.pdfcraft.png` | Linux hicolor theme, 16–512 px |
| `hicolor/scalable/apps/ai.storyteller.pdfcraft.svg` | Linux scalable icon (copy of the master) |

The Linux `.desktop` file still names the hicolor icon and the window class `ai.storyteller.pdfcraft`.
SwiftPDF on Windows uses `docs/images/SwiftPDFicon.png` for the window and `swiftpdf.ico` for the
executable, shortcuts and PDF association.

The PdfCraft document outline is adapted from Lucide `file-text` 1.49.0 (ISC); its badge and
outlined lettering are original (`MIT OR Apache-2.0`), covered by
[LICENSE-document.txt](LICENSE-document.txt). That file stays with this artwork. SwiftPDF does
not embed it.

## Regenerate

```sh
packaging/icons.sh        # needs resvg and python3; iconutil (macOS) for the .icns
cargo xtask assets        # then update the sha256 values in ATTRIBUTION.toml and run with --write
```
