# Host-local M2 annotation diagnostics

**G-1 OPEN.** These tools prepare and score annotations supplied by a reviewer;
they do not view private lecture images, create ground truth, verify reviewer
independence or authorize a gate transition. Keep all inputs and outputs under
`artifacts/private`. Do not open the packet in a remote browser or upload it.

## Prepare and annotate

```sh
node tools/eval/prepare_region_review.mjs artifacts/private/m2-full-deck-1794541547719/index-rechecked.json
```

The command creates a new private directory with `review_annotator.html`,
`review.html` and `annotations.json` (schema 2). Older packets are preserved.
Open the HTML in a local browser only. The current OCR-enabled packet is:

`artifacts/private/m2-full-deck-1794541547719/review-1794542002239/`

(Earlier pre-OCR packet `artifacts/private/m2-full-deck-1794477843920/review-1794488740362/`
is preserved for provenance.)

All **14 samples are NOT_REVIEWED**. The selection covers fixed pages, largest
composites, highest crop counts, a scan/crop case and the explicit SVG gap. This
is metadata-assisted sampling, not a representative full-corpus annotation set.

`review_annotator.html` is a host-local drawing UI. It shows the source raster,
stored predictions (solid) and your reference boxes (dashed), uses the same
top-left rotated CropBox **points** as the stored predictions, and exports
`annotations.reviewed.json` through a local download or the textarea. It makes
no network request and does not upload anything. Put the exported file in the
same private packet directory (or copy its text over `annotations.json`) and
score it. The static `review.html` remains available for looking at the linked
rasters, overlays and crops without annotation controls.

For each reviewed sample, edit only its annotation/provenance fields:

- Add `reference_regions`, each with a unique `id`, a `kind` (`image`, `vector`
  or `math`) and `bbox: {x0, y0, x1, y1}` in top-left rotated CropBox **points**.
- Set `reference_regions_complete: true` only after annotating all target crop
  types on that page; set `visual_review_status: "REVIEWED"` only after review.
- Record the formula/number/unit/negation, indentation, diagram-label and prose
  checks as `true` or `false`; leave unassessed checks `null`.
- Supply top-level `reviewer` and ISO `reviewed_at`. Declare
  `independent_review` truthfully. Agent-assisted or author self-review is not
  independent review. Leave `annotated_corpus_acceptance: false`.

The gate owner accepted a **host-local reviewer attestation** on 2026-11-13:
the exported `reviewer` identifier plus ISO `reviewed_at` is the independence
record for this review. No separate signed attestation is required. This does
not make the automated `independent_review_verified` flag true; the scorer still
keeps it `false` because it cannot verify identity. Agent-assisted or author
self-review must still not be declared independent.

Do not edit stored predictions, run/page identities, artifact hashes or pipeline
versions. The scorer reopens the authoritative page JSON and rejects changed
predictions, stale hashes, concealed incomplete evidence and non-private inputs.

An empty reference list is **not** evidence of an empty page. Only an explicitly
reviewed, complete reference list can be scored. Even then, a page without any
reference regions receives a null reference-IoU mean, never an invented 100%.

## Score without uploading

```sh
node tools/eval/score_region_review.mjs PRIVATE_REVIEW_DIRECTORY/annotations.json
```

Each invocation writes a new private `scores-*.json`; input packets are not
overwritten. Exit codes:

- `0`: diagnostic scores were produced for at least one complete reviewed sample.
- `2`: valid packet, but no complete reviewed references; all scores withheld.
- `1`: invalid/stale/unbound/non-private input or another execution error.

The current OCR-enabled packet correctly returns **2**, with **0 scored / 14
unscored** (`samples_total: 14`, `samples_scored: 0`, `samples_unscored: 14`) and
null IoU (`status: "BLOCKED_NO_REVIEWED_REFERENCES"`, recorded in
`scores-1794542002363.json`).

Matching maximizes summed IoU using one-to-one assignment **within the exact
region kind**. Duplicate predictions cannot claim the same reference twice.
Reports include unmatched IDs and mean IoU per reference/prediction, with
unmatched regions contributing zero. Positive overlap is a diagnostic, not a
passing threshold. No numeric `G-1` acceptance threshold is introduced.

Scores cover only reviewed samples. Unreviewed/incomplete samples stay listed
and are excluded from scoring, not treated as negatives. Content checks remain
separate reviewer observations; IoU does not prove OCR, formulas or negation.
The scorer records a reviewer's independence declaration but always leaves
`independent_review_verified: false`, `annotated_corpus_acceptance: false` and
`g1_status: "OPEN"`. The gate owner's accepted local attestation can be cited
alongside the report, but the report itself does not claim verified independence
or acceptance. External gate review is still required.

The diagnostic evaluator fails rather than truncating inputs over its internal
8 MiB file / 512-region-list / 200-sample budgets. These are evaluator resource
bounds, not extraction predicates, crop targets or acceptance rules.

## Unsupported SVG group investigation

The digital-text cover's remaining `unsupported_vector_group` is a gradient
rectangle painted through a **raster soft mask**. Removing the mask would invent
unmasked vector ink; feeding its raster alpha into `M_vec` would mix raster
evidence into the non-raster mask. The warning therefore remains explicit.
Sibling vector geometry and independent raster/full-page scan actions survive.

Two original synthetic Rust regressions now verify that masked gradient
rectangles and masked vector references are not silently expanded. This changes
tests only, not extraction behavior or `m2-pipeline-v4-vector-consolidation`.
No private image was viewed by a remote model during this investigation.

## Tests

```sh
node --test tools/eval/region_review_metrics.test.mjs tools/eval/score_region_review.test.mjs
cargo test -p campuslingo-ingestion --locked --lib vectors::
```

The assignment algorithm is checked against an exhaustive small-set oracle.
Synthetic CLI fixtures exercise unreviewed packets, diagnostic-only reviewed
scores, missing reviewer provenance, stale artifacts, edited predictions,
concealed coverage gaps, duplicate samples and non-private paths.
