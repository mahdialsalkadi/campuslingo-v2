import json

import pytest
from campuslingo_contracts.models import AudioManifest, ClientCommand, PublicQuestion, SourceRegion
from pydantic import ValidationError


def test_command_rejects_trusted_client_fields():
    command = {"type": "deploy", "protocol_version": "1.0.0", "match_id": "00000000-0000-4000-8000-000000000001", "sequence": 0, "card_id": "00000000-0000-4000-8000-000000000002", "lane": 0}
    ClientCommand.model_validate_json(json.dumps(command))
    for key in ("player_id", "damage", "rating", "rewards"):
        with pytest.raises(ValidationError):
            ClientCommand.model_validate_json(json.dumps({**command, key: 100}))


def test_public_question_rejects_grading_secrets():
    payload = {"id": "00000000-0000-4000-8000-000000000001", "prompt": "Which?", "options": ["A", "B"]}
    PublicQuestion.model_validate_json(json.dumps(payload))
    with pytest.raises(ValidationError):
        PublicQuestion.model_validate_json(json.dumps({**payload, "correct_index": 0}))


def test_source_regions_cannot_point_outside_page():
    region = {"source_sha256": "0" * 64, "page": 1, "bbox": [0.0, 0.0, 10.0, 10.0], "page_width": 10.0, "page_height": 10.0, "text_quote": "source"}
    SourceRegion.model_validate(region)
    region["bbox"][2] = 11.0
    with pytest.raises(ValidationError):
        SourceRegion.model_validate(region)


def test_audio_alignment_provenance_and_bounds():
    manifest = {"asset_id": "00000000-0000-4000-8000-000000000001", "language": "en", "codec": "opus", "text": "hello", "duration_ms": 1000, "alignment_method": "asr_estimate", "words": [{"text": "hello", "start_ms": 0, "end_ms": 900}]}
    AudioManifest.model_validate_json(json.dumps(manifest))
    manifest["words"][0]["end_ms"] = 1100
    with pytest.raises(ValidationError):
        AudioManifest.model_validate_json(json.dumps(manifest))


def test_reference_schemas_are_strict_but_not_production_authority():
    from campuslingo_contracts.models import EXPORTED_MODELS

    for name, model in EXPORTED_MODELS.items():
        schema = model.model_json_schema()
        assert schema["title"] == name
