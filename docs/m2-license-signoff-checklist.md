# M2 license sign-off checklist

Date prepared: 2026-11-13. **Status: AWAITING USER SIGN-OFF.** This checklist is
evidence for a scoped decision, not legal advice. It does not modify the frozen
master plan or `implementation_decisions.md`, does not change `DEC-005` /
`DEC-008`, and does not authorize private-corpus egress.

The observed inventory remains [PARTIAL](m2-license-inventory.md) until the
decision block at the end is completed. Model identity and digests are recorded
in `artifacts/private/model-audit-m2-20261113.json`.

## 1. Scope of the decision

Choose the scope that matches what you are approving:

- **M2 local-only evaluation:** run the pinned OCR model/projector locally on
  the private UTM corpus, keep every source/derivative under `artifacts/private`,
  and publish no corpus content, crops, OCR text, or model outputs.
- **Repository/code redistribution:** ship the Rust workspace and/or helper
  binaries outside this host.
- **Model-weight redistribution:** ship the OCR, projector, Qwen, or Kokoro
  files outside this host.
- **Corpus/derivative publication:** publish the UTM PDFs, crops, overlays, OCR
  text, annotations, or embeddings.

The current M2 evidence is **local-only**. No corpus file or derivative has been
uploaded, and no model weight is redistributed by this repository.

## 2. Native helper stack

| Component | Observed license expression | M2 role | Sign-off question |
| --- | --- | --- | --- |
| Poppler utilities | `GPL-2.0-only`, `GPL-3.0-or-later`, `LGPL-2.0-or-later`, `LGPL-2.1-or-later`, `MIT`, `HPND-sell-variant` | PDF inspection, text/image geometry, rendering, SVG extraction | If binaries are bundled, retain notices and satisfy the applicable GPL/LGPL source/relink obligations. Local-only use needs no distribution step. |
| Tesseract + English/OSD data | `Apache` | Provisional OCR geometry only | Retain Apache notices if redistributed. |
| ImageMagick | ImageMagick License | PNG-to-WebP crop encoding under a restricted policy | Review the ImageMagick License before bundling. |
| librsvg (`rsvg-convert`) | `LGPL-2.1-or-later` | Sanitized vector-mask rasterization | If bundled, provide the LGPL notice/source offer and relink path. |

These programs are invoked as separate processes; they are not linked into the
Rust binaries. That reduces, but does not eliminate, distribution obligations if
a product bundles them.

## 3. Rust dependencies

`cargo metadata --locked` reports permissive expressions for all third-party
crates, predominantly `MIT OR Apache-2.0`, plus `Unicode-3.0`, `BSD-2-Clause`,
`BSD-3-Clause`, `ISC`, `Zlib`, `0BSD`, `CDLA-Permissive-2.0`, and
`Apache-2.0 WITH LLVM-exception`. The four workspace crates have no declared
license expression yet.

Sign-off question: retain the locked metadata and a generated third-party
notice file with any code distribution. For local-only M2 evidence, no action is
required beyond keeping the metadata reproducible.

## 4. Pinned models and voice data

| Role | Pinned identity | Declared license | Used in M2? | Open item |
| --- | --- | --- | --- | --- |
| OCR model | `mradermacher/PaddleOCR-VL-1.6-GGUF@39d08d7cb9b08a36ae88735b9a8801d3d18a76fd`, `PaddleOCR-VL-1.6.Q8_0.gguf`, SHA-256 `03604eb7e10e2608a17640c62378aa54335b03eb823f67c066c4d49d8bc62048` | Apache-2.0 claim | Yes | Retain upstream `LICENSE`/`NOTICE`; review the quantizer repository's terms and any bundled notices. |
| OCR projector | `PaddlePaddle/PaddleOCR-VL-1.6-GGUF@511b09642bb324401f15f97cc23bc67e8f0a291d`, `PaddleOCR-VL-1.6-GGUF-mmproj.gguf`, SHA-256 `204d757d7610d9b3faab10d506d69e5b244e32bf765e2bab2d0167e65e0a058a` | Apache-2.0 claim | Yes | BF16 (`file_type=32`) deviation from the frozen `mmproj-f16` text is explicit; retain notices. |
| Embeddings | `Qwen/Qwen3-Embedding-0.6B-GGUF@370f27d7550e0def9b39c1f16d3fbaa13aa67728`, `Qwen3-Embedding-0.6B-Q8_0.gguf`, SHA-256 `06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439` | Apache-2.0 claim | Yes | Retain upstream `LICENSE`/`NOTICE`; sequential under single-model `ModelLease`. |
| Generator | `unsloth/Qwen3.5-4B-GGUF@e87f176479d0855a907a41277aca2f8ee7a09523`, `Qwen3.5-4B-Q4_K_M.gguf`, SHA-256 `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4` | Apache-2.0 claim | No (M3 preparation) | Do not treat M3 preparation as M3 acceptance; retain notices before any generation/distribution. |
| TTS model | `shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped@02ff6fe940737afa4ecac05273a1633ba0bfda33`, `model.onnx`, SHA-256 `45e6d0d8fd257386edf18b02c64934777ec2017ebb35d7d7e29ea138cb8de9a0` | Repository card: Apache-2.0 | No (M3 preparation) | Review exporter terms and the base-model notices. |
| TTS voice | Same repository revision, `af_heart.bin`, SHA-256 `d583ccff3cdca2f7fae535cb998ac07e9fcb90f09737b9a41fa2734ec44a8f0b` | Repository claim: Apache-2.0; voice-data provenance separate | No | Voice-data distribution notice/provenance is not verified. Defer to M3. |

The local look-alike files rejected in the audit (mislabeled Qwen Q6_K, older
Kokoro v0.19, cached OCR pair with a different digest) remain unused.

## 5. Private UTM corpus

The three evaluation PDFs are institution-provided course material. Their
institutional terms for redistribution, derivative publication, and model
training have **not** been verified. All probes, crops, overlays, OCR text,
review packets, and annotations remain local under `artifacts/private`.

Sign-off question: confirm that no UTM corpus content or derivative will be
published, uploaded, or used for model training under the current M2 scope. If
institutional permission exists, record its scope and evidence separately.

## 6. Decision block (complete locally)

Mark one option per row. Suggested baseline for the current evidence is shown
first; it does not decide the later distribution milestone.

| Item | Decision |
| --- | --- |
| M2 OCR local evaluation | `[ ] APPROVE LOCAL-ONLY` / `[ ] REJECT` / `[ ] DEFER` |
| Repository/code redistribution | `[ ] NO REDISTRIBUTION (local-only)` / `[ ] APPROVE WITH CONDITIONS` / `[ ] DEFER` |
| Model-weight redistribution | `[ ] NO REDISTRIBUTION (local-only)` / `[ ] APPROVE WITH CONDITIONS` / `[ ] DEFER` |
| UTM corpus publication/training | `[ ] NO PUBLICATION OR TRAINING (local-only)` / `[ ] APPROVED WITH RECORDED TERMS` / `[ ] DEFER` |
| Kokoro voice-data review | `[ ] DEFER TO M3` / `[ ] REVIEW NOW` |

Conditions or recorded permissions:

```text
______________________________________________________________________________
______________________________________________________________________________
______________________________________________________________________________
```

Reviewer name or local identifier:

```text
______________________________________________________________________________
```

Date and scope of attestation:

```text
______________________________________________________________________________
```

## 7. Effect of sign-off

- A **local-only** decision resolves the M2 license question for this host and
  keeps every corpus/model output private. It does not approve redistribution,
  publication, training, or egress.
- An **approve-with-conditions** decision must list the notices, source offers,
  attribution, or institutional permissions to retain. Those conditions become
  part of the M2 evidence record.
- A **defer/reject** decision leaves the license item `OPEN`; G-1 cannot pass on
  license grounds.
- Sign-off does not by itself close G-1. Independent annotation, IoU/coverage,
  OCR-fidelity review, and the whole-stack `O-01` measurement remain separate
  requirements.
