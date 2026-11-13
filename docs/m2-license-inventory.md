# M2 ingestion license inventory (local observation)

Date: 2026-11-12. **Status: PARTIAL.** This is an observed inventory of the
locally installed ingestion stack and its declared package licenses. It is not
legal advice and it does not close the `G-1` license item by itself.

## Native helper stack

Versions and license expressions were read from the host package database and
the installed binaries. No package was downloaded or changed for this inventory.

| Component | Observed version | Declared package license | Role |
| --- | --- | --- | --- |
| Poppler (`pdfinfo`, `pdftohtml`, `pdftoppm`, `pdftocairo`, `pdfimages`, `pdftotext`) | 26.08.0-1.1 | `GPL-2.0-only`, `GPL-3.0-or-later`, `LGPL-2.0-or-later`, `LGPL-2.1-or-later`, `MIT`, `HPND-sell-variant` | PDF inspection, text/image geometry, rendering, SVG extraction |
| Tesseract OCR | 5.5.3-1.1 | `APACHE` | Provisional OCR geometry only; not authoritative text recovery |
| `tesseract-data-eng` / `tesseract-data-osd` | 2:4.1.0-5 | `Apache` | English and orientation data |
| ImageMagick | 7.1.2.32-1.1 | `ImageMagick` | PNG-to-WebP crop encoding under a restricted policy |
| librsvg (`rsvg-convert`) | 2:2.62.3-1.1 | `LGPL-2.1-or-later` | Sanitized vector-mask rasterization |

The Rust workspace calls these programs as separate processes. Poppler's GPL
utilities are not linked into the Rust binaries. Distribution of a product that
bundles these helpers must still be reviewed against their own terms; this
inventory does not grant redistribution rights.

## Rust dependency expressions

`cargo metadata --locked` reports permissive expressions for all third-party
crates used by the workspace, predominantly `MIT OR Apache-2.0`, plus
`Unicode-3.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Zlib`, `0BSD`,
`CDLA-Permissive-2.0`, and `Apache-2.0 WITH LLVM-exception`. The only packages
without a declared license expression are the four private workspace crates
(`campuslingo-domain`, `campuslingo-ingestion`, `campuslingo-server`,
`campuslingo-storage`). A release review should pin the full dependency list and
retain the generated metadata with the build artifacts.

## Models

Hash-pinned public artifacts were downloaded on 2026-11-13 after explicit user
approval. No private corpus file or derivative was uploaded. The local identity,
size, digest, upstream revision and declared license are recorded in
`artifacts/private/model-audit-m2-20261113.json`.

| Role | Pinned artifact | Declared license | M2 use |
| --- | --- | --- | --- |
| OCR | `mradermacher/PaddleOCR-VL-1.6-GGUF@39d08d7` `PaddleOCR-VL-1.6.Q8_0.gguf`, SHA-256 `03604eb7…c62048` | Apache-2.0 (model card and GGUF metadata) | Managed local OCR for the synthetic probe and the OCR-enabled private run |
| OCR projector | `PaddlePaddle/PaddleOCR-VL-1.6-GGUF@511b096` `PaddleOCR-VL-1.6-GGUF-mmproj.gguf`, SHA-256 `204d757d…0a058a` | Apache-2.0 (model card and GGUF metadata) | Paired with the Q8_0 model |
| Embeddings | `Qwen/Qwen3-Embedding-0.6B-GGUF@370f27d` `Qwen3-Embedding-0.6B-Q8_0.gguf`, SHA-256 `06507c7b…9c3e439` | Apache-2.0 (model card and GGUF metadata) | Managed sequential 1024-d chunk embeddings (`S-02`–`S-04`) |
| Generator (M3 preparation) | `unsloth/Qwen3.5-4B-GGUF@e87f176` `Qwen3.5-4B-Q4_K_M.gguf`, SHA-256 `00fe7986…ef11a4`, GGUF `file_type=15` | Apache-2.0 | Downloaded, audited; no M3 generation run |
| English TTS (M3 preparation) | `shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped@02ff6fe` `onnx/model.onnx`, SHA-256 `45e6d0d8…8de9a0`; `voices/af_heart.bin`, SHA-256 `d583ccff…a8f0b` | Repository card: Apache-2.0; based on `onnx-community/Kokoro-82M-v1.0-ONNX` and `hexgrad/Kokoro-82M` | Downloaded, audited; no TTS inference run |

The frozen candidate text names a `mmproj-f16` projector. The official upstream
GGUF repository publishes this projector as GGUF `file_type=32`
(`MOSTLY_BF16`), a 16-bit projector; there is no separate f16 file in the
published repository. This is recorded as an explicit candidate deviation, not
silently relabeled. The Q8_0 model itself matches the published LFS object.

Several pre-existing local files are rejected for the frozen roles because their
identity does not match their names or the published digests: the local
`qwen3.5-4b-instruct-q4_k_m.gguf` is actually Q6_K, the 0.8B file is Q8_0, the
cached OCR pair differs from the published artifacts, and
`kokoro-v0_19.onnx` is the older v0.19 export rather than the pinned v1.0 ONNX.
They are not used for acceptance.

The model cards assert Apache-2.0 for the OCR, Qwen and Kokoro weights. That does
not by itself complete redistribution review: retain the upstream `LICENSE` /
`NOTICE` files, review the quantizer's and ONNX exporter's terms, and review
voice-data provenance separately. Kokoro's model card states that its training
audio was permissive/non-copyrighted and identifies CC BY components; the voice
files still need their own distribution notice review. No model weight or voice
sample is redistributed by this repository.

## Private UTM corpus

The three evaluation PDFs are institution-provided course material stored under
the user's local UTM eLearning directory. They are read locally, are not copied
into the repository, and are not uploaded to any OCR, vision, or annotation
service. Institutional terms for redistribution, derivative publication, or
model training have **not** been verified. The private probe evidence therefore
remains local-only and cannot be published as a corpus release.

## Gate boundary

A scoped user decision checklist is at
[m2-license-signoff-checklist.md](m2-license-signoff-checklist.md). Completing
it records a local-only or conditioned decision; it does not by itself close
`G-1` or authorize redistribution.

- **PASS (observed only):** native package names, versions, and declared license
  expressions listed above; Rust dependency expressions summarized from the
  locked metadata.
- **OPEN:** redistribution review for bundled Poppler/ImageMagick/librsvg
  binaries, upstream/quantizer/model-card and voice-data terms, and the UTM
  corpus terms. Model identity and declared licenses are now recorded, but that
  is not a redistribution approval.
- This document does not change `DEC-005`/`DEC-008`, does not authorize egress,
  and does not claim `G-1` closure.
