# M2 region extraction — implementation and fidelity probes

Date: 2026-11-13. **M2 IN PROGRESS; G-1 OPEN.** The frozen master is unchanged.
Implementation choices are logged as `DEC-011..DEC-017` in the authoritative
`/home/mahdi/Downloads/plan/implementation_decisions.md` and its publication
snapshot. This report supersedes only the earlier M2 region-work checklist.

## Implemented coverage

- Explicit 72-DPI Poppler XML includes full font names, verbatim text and image
  geometry. Embedded image bytes are reduced to identity hashes in layout
  metadata. Internal-entity DTDs and non-finite coordinates are rejected;
  external DTDs are never fetched, while normal XML character entities decode.
- Per-page MediaBox, CropBox and rotation normalize to rotated, top-left CropBox
  coordinates. Rotated text uses supplemental contained `pdftotext` word boxes;
  unmatched geometry is recorded, never guessed. This adds a seventh contained
  helper to the previous slice's six.
- Independent `PageActions` can simultaneously extract clean digital text,
  protect/OCR math-font or garbled blocks, crop vector/raster figures and request
  full-page scan OCR. `PageRoute` is an audit summary, not the execution switch.
- Sanitized Poppler SVG supplies non-template vector geometry. Glyph references,
  text-covered outlined glyphs and raster XObjects do not enter `M_vec`.
  Background and full-deck template exclusion precede 150-DPI rasterization,
  Euclidean closing (radius 1.5% of page width) and 8-connected components.
  Eligibility retains `segments >=25`, `0.04 <= area/page_area <0.90`,
  `non_text_ink_ratio >=0.35`, and `path_ink_ratio >=0.05`.
- Presentation consolidation uses prose barriers, anisotropic merging, two-hop
  label absorption, overlap merging and bounded padding. Enlarged boxes recheck
  their frozen vector/raster predicates, including raster variance. Labels stay
  in the digital-text stream; digital-text extraction creates no text-line WebPs.
  Adjacent math blocks merge without crossing digital prose.
- Raster figure bounds are **4% inclusive to 85% exclusive**. A non-uniform single
  raster covering **85% or more** requests scan OCR, never a giant raw figure crop.
  Uniform images are excluded even on one-page documents. Full-deck exact-image
  identity + normalized-placement recurrence uses both **>=60%** and **>=3 pages**
  when the deck has **>=5 pages**; sampled probes do not infer recurrence.
- Font-first math checks include the approved font families, plus Unicode
  operator density and the frozen `garbled_glyph_ratio > 0.08` check. Original text
  remains separate from OCR-derived text. Missing OCR preserves the source crop
  and records an explicit failure instead of silently accepting the block.
- Analysis rasters and colored verification overlays use **150 DPI**. Selected
  rectangles are rendered again **from the PDF at 250 DPI**, then encoded to WebP
  without resizing. Identity binds input SHA, run/page/index and source rectangle.
  Crop-count overflow errors instead of silently truncating selected regions.
- Source-crop limits: <=8192px per axis, <=24M decoded pixels, <=8 MiB WebP.
  These are worker limits, not the later GLB texture/triangle contract. All crops,
  source-region PNGs, overlays and JSON evidence stay in private run directories.
- Verbatim source and chunks retain code indentation/tabs; leading indentation
  participates in the critical fingerprint to prevent unsafe normalized merges.
- OCR clients accept only literal loopback HTTP(S) addresses and disable
  redirects/proxies. The contained full-deck OCR run uses the leased local
  PaddleOCR-VL-1.6 Q8_0 + official BF16 projector via `llama-server` on
  `Vulkan0` (`--ocr-device auto`, falling back to `--ocr-device none` CPU
  mode if unavailable). Tesseract performs layout/scan geometry checks alongside
  authoritative PaddleOCR-VL text recovery.

Pipeline version: **`m2-pipeline-v4-vector-consolidation`**. It invalidates previous
page artifacts. The vector-dependent 60% raster / <80 digital-character fallback
requires complete vector evidence and no eligible vector candidate. Scan crops
are contrast-isolated, bounded non-text regions, not Tesseract text-line boxes.
- Tesseract geometry and scan-region isolation now pair with leased local
  PaddleOCR-VL extraction under bounded `pipeline.rs` concurrency and
  `--parallel 2` inference slots.

## Verification commands and results

Run Cargo checks sequentially when they share this workspace's target directory.

```sh
bash scripts/verify_m2_containment.sh
cargo test --workspace --locked
bash scripts/run_ingestion_contained.sh node scripts/run_m1_db_tests.mjs
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
node --test tools/eval/*.test.mjs

cargo build --release -p campuslingo-ingestion --locked
bash scripts/run_ingestion_contained.sh node tools/eval/full_deck_probe.mjs --managed-ocr --model artifacts/private/models/paddleocr-vl-1.6/PaddleOCR-VL-1.6.Q8_0.gguf --mmproj artifacts/private/models/paddleocr-vl-1.6/PaddleOCR-VL-1.6-mmproj.gguf --threads 8
# Reinspect existing artifacts without rerunning extraction; preserves index.json:
node tools/eval/full_deck_probe.mjs --recheck PRIVATE_PROBE_DIRECTORY/index.json
node tools/eval/check_region_probes.mjs PRIVATE_PROBE_DIRECTORY/index-rechecked.json
node tools/eval/prepare_region_review.mjs PRIVATE_PROBE_DIRECTORY/index-rechecked.json
# Expected exit 2 until complete reviewed references are provided:
node tools/eval/score_region_review.mjs PRIVATE_REVIEW_DIRECTORY/annotations.json
```

| Check | Result |
| --- | --- |
| Default ingestion library suite | PASS: 52 tests (+ 2 bin tests = 54); 11 environment-dependent tests run separately |
| Real contained helper + synthetic PDF + Vulkan0 OCR suite | PASS: 11 tests, 0 ignored (`52` filtered out in second pass); includes four rotations, non-zero CropBox offsets, math-font metadata, actual source-crop pixel alignment, simultaneous actions, kernel OOM kill, and explicit partial status |
| Workspace native suite, sequential run | PASS: 80 tests; 21 ignored here (11 contained tests and 10 PostgreSQL tests run separately) |
| Disposable PostgreSQL + pgvector suite | PASS: 10 tests, including 4 M2 checkpoint/fencing/authorization/real-worker tests (`Memory peak: 582.7M`, `swap: 0B`) |
| WebP header verifier regressions | PASS: 3 tests (lossy high-bit signature, extended/lossless dimensions, malformed/truncated input) |
| Artifact verifier regressions | PASS: 6 tests (including no-OCR vs managed-OCR probe assertions, synthetic geometry, hashes/dimensions, explicit partial versus hard failures) |
| Annotation diagnostic regressions | PASS: 11 tests; exhaustive small-set assignment oracle and artifact-bound local CLI checks |
| Clippy, Rust formatting, diff whitespace, `--release` binary | PASS |
| Private OCR-enabled artifact integrity / geometry / releases | PASS: 3 PDFs, 200 pages, 511 crops, 200 overlays, 200 vector masks; no text-line crops; 0 missing-OCR/no-chunks failures; 22 clean model releases (`0` OOM kills); 1 explicit `unsupported_vector_group` |

The frozen master SHA-256 remains
`404caf2db246269b854ae573a02b3e2adfd5fa49d7c38788c6ba3397c21cc588`.
Earlier pre-OCR sequential validation logs are also retained privately at
`artifacts/private/m2-validation-20261112T130723Z/index.json`.

Original synthetic fixture source: `scripts/make_m2_region_fixture.mjs`; generated
PDF: `tests/fixtures/m2/region-cases.pdf`. It contains no private lecture material.
The four rotated blue source rectangles are checked at the source PNG's center
pixel after real PDF rendering, alongside encoded WebP dimension checks. This
does not measure lossy WebP visual fidelity or substitute for source annotation.

### Full private corpus run (not annotated acceptance)

The three PDFs are the existing `artifacts/private/corpus.json` selections.
All **101 + 64 + 35 = 200 pages** were processed. Source names, text, formula crops,
image bytes and source hashes are not reproduced in this report.

The earlier private v1 report's `inference_performed: false` means no leased
GGUF/ONNX inference, not the absence of Tesseract geometry. New summaries use the
explicit `leased_model_inference_performed` / `tesseract_geometry_attempted` fields.

| Private category | Pages | Crops (image / vector / math) | Components / eligible | Restarts / Releases | Duration | Result |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Diagram/math | 101 | 454 (182 / 25 / 247) | 173 / 25 | 16 / 17 | 142,460 ms | SUCCEEDED: 0 failures, max cgroup 241,311,744 B, max RSS 564,285,440 B |
| Digital text | 64 | 56 (41 / 15 / 0) | 99 / 15 | 1 / 2 | 66,317 ms | PARTIAL: 1 unsupported_vector_group, 1 ocr_page, max cgroup 265,953,280 B, max RSS 564,137,984 B |
| Low-text/image | 35 | 1 (1 / 0 / 0) | 0 / 0 | 2 / 3 | 145,630 ms | SUCCEEDED: 0 failures, 35 ocr_pages, max cgroup 311,222,272 B, max RSS 564,310,016 B |

Every page has an overlay, vector evidence and a mask. Template counting covers
all pages: statuses are `complete` on 200 pages, but this means deck enumeration,
not complete SVG/content support. The digital-text cover explicitly records
`unsupported_vector_group`; unsupported paint/group semantics are not stripped
to fabricate a successful diagram. Overall exclusions: 239 backgrounds,
230 text-covered glyph primitives and 208 recurring vector templates.

Action totals: 3,327 digital-text extractions, 192 raster crops, 40 vector crops,
247 math/garbled OCR blocks and 76 full-page scan actions. Scan isolation can
produce image crops in addition to the raster actions; these counts are not
expected to match. All 324 previously missing OCR blocks and all 36 previously
no-chunks pages resolved with **0 missing-OCR failures** and **0 no-chunks
failures** under leased `Vulkan0` PaddleOCR-VL inference.

The diagram/math deck has at most 20 crops on one page (17 math regions).
Counts such as the suggested ~10–18 are presentation guidance, not `G-1` gates.
The single scan-deck crop does not prove high segmentation recall. Large
composites, math fragments and scan omissions still require local visual review.

Latest private evidence:
`artifacts/private/m2-full-deck-1794541547719/index-rechecked.json` and its run folders
(run with `--release` binary under contained `Vulkan0`, preserving earlier
no-OCR `m2-full-deck-1794477843920/`). The checker verifies source rectangles,
integer pixel bounds, 250-DPI metadata, actual file lengths, SHA-256 integrity,
PNG overlay dimensions, private modes (directories 0700, crops/overlays 0600),
leased model inference (`leased_model_inference_performed: true`), zero release
failures (`22` clean releases, `0` OOM kills), and zero OCR failures. The
overall contained run took **5m 55.692s** (`12m 26.926s` CPU), with a **1.4G
service cgroup peak** (`0B` swap), min host `MemAvailable` **8,577,298,432 bytes**,
and max model RSS **564,310,016 bytes** (whole-stack `O-01` DRM VRAM/host telemetry
recorded separately in `artifacts/private/o01-1794542035999/o01-measurement.json`).

### Bugs found and corrected

1. `pdftohtml` produces zero-width boxes at 90/270 degrees and negative widths
   at 180 degrees; rotated word-box supplementation fixes the synthetic cases.
2. `960 * (150 / 72)` can round just above 2000 and incorrectly request 2001
   pixels. Multiplying before dividing fixes the failed digital-slide probes;
   the 960x540pt / 2000x1125px case is now a regression test.
3. The previous internal 4096px crop-width cap rejected a legitimate wide scan
   text line at mandatory 250 DPI. The bounded 8192px source-crop budget fixes
   that case without lowering DPI or clipping content (`DEC-013`).
4. A concurrent Cargo/rustdoc run reported a missing storage crate after another
   Cargo invocation rebuilt shared artifacts. The final sequential workspace
   run passed. No source/dependency reset was used; keep these checks sequential.
5. The full-deck summary initially marked all crops invalid because ASCII
   decoding clears the high bit of WebP's `0x9d` keyframe marker. Byte comparison
   fixes that false failure. The original `index.json` is preserved, not edited;
   `index-rechecked.json` records the corrected verifier and original timestamp.
   All 511 saved files passed header dimensions, geometry, SHA-256 and modes.

## Remaining acceptance work

**G-1 OPEN:** authoritative `Vulkan0` PaddleOCR-VL extraction is now recorded on
all 200 corpus pages, leaving independent human annotation of diagrams,
formulas, numbers, units, negation, and code indentation, OCR fidelity review,
measured IoU/coverage, unsafe-deduplication checks, and user assessment of the
remaining `unsupported_vector_group` on the digital-text cover as open blockers.
Font-first behavior is tested synthetically; this is not a real-font annotation
set. Exact image-byte identity plus normalized position is not a pHash matcher.

The current OCR-enabled host-only review packet is prepared at
`artifacts/private/m2-full-deck-1794541547719/review-1794542002239/`
(preserving the earlier pre-OCR packet `review-1794488740362/`). Its static
`review.html` links local rasters/overlays/crops with a network-blocking CSP;
`annotations.json` contains **14 metadata-selected samples, NOT_REVIEWED**.
Selection is agent-assisted, not independent visual annotation. Reference regions
are empty/unannotated, `iou` is null, and no visual-fidelity claim is made.
Schema-v2 packets bind each page's immutable artifact SHA-256. The local
scorer (`scores-1794542002363.json`) correctly withholds all 14 unreviewed
samples (exit 2, `BLOCKED_NO_REVIEWED_REFERENCES`), with no IoU or acceptance
claim. See [local review instructions](m2-local-review.md).

The SVG gap is a gradient rectangle with a raster soft mask, not an ordinary
unmasked diagram path. Synthetic regressions preserve its explicit partial
status rather than manufacture vector ink by stripping the mask. No extraction
behavior changed, so the v4 corpus evidence remains applicable.

Do not send private overlays/crops to remote vision or reviewers: manual visual
annotation must remain host-local unless the frozen derivative-egress gate is
explicitly satisfied. Numeric checks cannot substitute for that annotation.

**Other M2 gates:** durable fenced DB checkpoints, private crop delivery,
contained managed model releases, and scoped whole-stack `O-01` measurement
(`artifacts/private/o01-1794542035999/o01-measurement.json`) are recorded;
learned embeddings/recall (`S-02`/`S-03`/`S-04`/`P-04`) and user sign-off on
the [license checklist](m2-license-signoff-checklist.md) remain open. See
[M2 status](m2-status.md) and the [partial license inventory](m2-license-inventory.md).
M3 has not started; no milestone closure, deployment or change to `DEC-005 OPEN`
/ `DEC-008 ESCALATE` is claimed.
