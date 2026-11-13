//! English `Kokoro-82M` ONNX TTS pipeline using `P-07` `CLAUSE_CONCAT`.
//!
//! Splits English narration into clean speakable clauses, phonemizes via
//! `libespeak-ng` (`#[cfg(feature = "onnx")]`), synthesizes each clause in
//! single-resident `ort` ONNX memory under `ModelLease`, trims leading/trailing
//! silence, concatenates PCM with exact sample-derived clause timestamps
//! (`AlignmentMethod::ClauseConcat`), synchronizes `DirectorTrack` cues, and
//! transcodes to `.opus` inside a delegated cgroup-v2 helper.

use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    time::Duration,
};

use campuslingo_domain::contracts::{
    AlignmentMethod, AudioManifest, AudioWord, Codec, DirectorTrack, FocusRole, Language, ShotType,
};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use tokio::process::Command;
use uuid::Uuid;

use crate::process::{HelperError, HelperLimits, run_helper};

pub const KOKORO_SAMPLE_RATE_HZ: u32 = 24_000;
pub const PIPER_AR_SAMPLE_RATE_HZ: u32 = 22_050;
pub const KOKORO_MAX_TOKENS: usize = 512;
pub const KOKORO_MAX_PHONEME_LEN: usize = 510;
pub const CLAUSE_GAP_SAMPLES: usize = 1_440;
pub const PIPER_AR_GAP_SAMPLES: usize = 1_323;
pub const MAX_TTS_RSS_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_KOKORO_MODEL_ID: &str = "shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped";
pub const DEFAULT_KOKORO_REVISION: &str = "02ff6fe";
pub const DEFAULT_PIPER_AR_MODEL_ID: &str = "rhasspy/piper-voices";
pub const DEFAULT_PIPER_AR_REVISION: &str = "cf910fb59f1d79ab3b625a63993a108751c6c25d";
pub const DEFAULT_PIPER_AR_SHA256: &str =
    "9e95cab07b679da603bba17c4dec7ab3111320571964ee95c0379603c086491e";
pub const P01_MAX_NORMALIZED_WER: f64 = 0.12;
pub const DEFAULT_SILENCE_THRESHOLD: f32 = 0.002;
pub const DEFAULT_EDGE_FADE_SAMPLES: usize = 240;

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("invalid TTS input: {0}")]
    InvalidInput(String),
    #[error("tokenizer error: {0}")]
    Tokenizer(String),
    #[error("voice style bank error: {0}")]
    VoiceBank(String),
    #[error("ONNX TTS model load failed: {0}")]
    ModelLoad(String),
    #[error("ONNX TTS inference failed: {0}")]
    Inference(String),
    #[error("phonemizer failed: {0}")]
    Phonemizer(String),
    #[error("Opus encoding or probing failed: {0}")]
    Encoding(String),
    #[error("TTS worker exceeded memory or headroom budget")]
    Memory,
    #[error("TTS model lease failed: {0}")]
    Lease(String),
    #[error("helper containment failed: {0}")]
    Helper(#[from] HelperError),
}

#[derive(Debug, Deserialize)]
struct TokenizerFile {
    model: TokenizerModel,
}

#[derive(Debug, Deserialize)]
struct TokenizerModel {
    vocab: HashMap<String, i64>,
}

/// Character-to-token encoder matching `kokoro-82m/tokenizer.json`.
#[derive(Debug, Clone)]
pub struct KokoroTokenizer {
    vocab: HashMap<char, i64>,
    pad_token_id: i64,
}

impl KokoroTokenizer {
    /// Load tokenizer vocabulary from a `tokenizer.json` file.
    pub fn from_file(path: &Path) -> Result<Self, TtsError> {
        let raw = fs::read_to_string(path)
            .map_err(|err| TtsError::Tokenizer(format!("read failed: {err}")))?;
        Self::from_json_str(&raw)
    }

    /// Parse tokenizer vocabulary directly from JSON text.
    pub fn from_json_str(raw: &str) -> Result<Self, TtsError> {
        let parsed: TokenizerFile = serde_json::from_str(raw)
            .map_err(|err| TtsError::Tokenizer(format!("malformed json: {err}")))?;
        let pad_token_id = *parsed
            .model
            .vocab
            .get("$")
            .ok_or_else(|| TtsError::Tokenizer("missing '$' pad token".into()))?;
        let mut vocab = HashMap::new();
        for (sym, id) in parsed.model.vocab {
            if sym == "$" {
                continue;
            }
            let mut chars = sym.chars();
            if let Some(ch) = chars.next()
                && chars.next().is_none()
            {
                vocab.insert(ch, id);
            }
        }
        if vocab.is_empty() {
            return Err(TtsError::Tokenizer("empty vocabulary".into()));
        }
        Ok(Self {
            vocab,
            pad_token_id,
        })
    }

    /// Map IPA phoneme characters into `[pad, ...tokens..., pad]`.
    pub fn encode_phonemes(&self, ipa: &str) -> Result<Vec<i64>, TtsError> {
        let mut tokens = Vec::with_capacity(ipa.chars().count() + 2);
        tokens.push(self.pad_token_id);
        for ch in ipa.chars() {
            if let Some(&id) = self.vocab.get(&ch) {
                tokens.push(id);
            } else if ch.is_whitespace() {
                if let Some(&space_id) = self.vocab.get(&' ') {
                    tokens.push(space_id);
                }
            } else {
                return Err(TtsError::Tokenizer(format!(
                    "phoneme character U+{:04X} absent from tokenizer vocab",
                    ch as u32
                )));
            }
        }
        if tokens.len() <= 1 {
            return Err(TtsError::Tokenizer("no encodable phoneme tokens".into()));
        }
        tokens.push(self.pad_token_id);
        if tokens.len() > KOKORO_MAX_TOKENS {
            return Err(TtsError::Tokenizer(format!(
                "clause token length {} exceeds {}",
                tokens.len(),
                KOKORO_MAX_TOKENS
            )));
        }
        Ok(tokens)
    }
}

/// Packed little-endian `f32` `[N, 256]` Kokoro voice style bank (`af_heart.bin`).
#[derive(Debug, Clone)]
pub struct VoiceStyleBank {
    rows: Vec<[f32; 256]>,
}

impl VoiceStyleBank {
    /// Load a binary Kokoro voice pack (`N x 256 x f32`).
    pub fn from_file(path: &Path) -> Result<Self, TtsError> {
        let bytes =
            fs::read(path).map_err(|err| TtsError::VoiceBank(format!("read failed: {err}")))?;
        Self::from_bytes(&bytes)
    }

    /// Decode little-endian `[N, 256]` `f32` style vectors.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TtsError> {
        let stride = 256 * 4;
        if bytes.is_empty() || !bytes.len().is_multiple_of(stride) {
            return Err(TtsError::VoiceBank(format!(
                "invalid voice buffer size {} (expected multiple of {stride})",
                bytes.len()
            )));
        }
        let mut rows = Vec::with_capacity(bytes.len() / stride);
        for chunk in bytes.chunks_exact(stride) {
            let mut row = [0.0_f32; 256];
            for (slot, word) in row.iter_mut().zip(chunk.chunks_exact(4)) {
                let val = f32::from_le_bytes([word[0], word[1], word[2], word[3]]);
                if !val.is_finite() {
                    return Err(TtsError::VoiceBank(
                        "non-finite weight in voice pack".into(),
                    ));
                }
                *slot = val;
            }
            rows.push(row);
        }
        Ok(Self { rows })
    }

    /// Select the style vector keyed by phoneme character length (`0..rows.len()`).
    pub fn style_for_phoneme_len(&self, phoneme_len: usize) -> Vec<f32> {
        let idx = phoneme_len.min(self.rows.len() - 1);
        self.rows[idx].to_vec()
    }
}

/// Convert a PCM sample count at `sample_rate_hz` into integer milliseconds.
pub fn samples_to_ms(samples: usize, sample_rate_hz: u32) -> u32 {
    if sample_rate_hz == 0 {
        return 0;
    }
    ((samples as u64).saturating_mul(1000) / u64::from(sample_rate_hz)) as u32
}

/// Deterministic English clause splitter preserving decimals and keeping clauses
/// short enough to stay under Kokoro's 510-phoneme limit.
pub fn split_clauses_for_p07(text: &str) -> Result<Vec<String>, TtsError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(TtsError::InvalidInput("narration text is empty".into()));
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let mut raw_clauses = Vec::new();
    let mut start = 0usize;

    for (idx, &ch) in chars.iter().enumerate() {
        let is_sentence_end = matches!(ch, '!' | '?' | '؟' | '\n')
            || (ch == '.'
                && !(idx > 0
                    && idx + 1 < chars.len()
                    && chars[idx - 1].is_ascii_digit()
                    && chars[idx + 1].is_ascii_digit()));
        if is_sentence_end {
            let segment: String = chars[start..=idx].iter().collect();
            push_wrapped_clauses(&segment, &mut raw_clauses);
            start = idx + 1;
        }
    }
    if start < chars.len() {
        let tail: String = chars[start..].iter().collect();
        push_wrapped_clauses(&tail, &mut raw_clauses);
    }

    if raw_clauses.is_empty() {
        return Err(TtsError::InvalidInput(
            "no speakable clauses found in narration text".into(),
        ));
    }
    Ok(raw_clauses)
}

fn push_wrapped_clauses(segment: &str, out: &mut Vec<String>) {
    let clean = segment.trim();
    if !clean.chars().any(char::is_alphanumeric) {
        return;
    }
    if clean.chars().count() <= 110 {
        out.push(clean.to_owned());
        return;
    }
    let mut start = 0usize;
    let bytes: Vec<(usize, char)> = clean.char_indices().collect();
    for (pos, &(byte_idx, ch)) in bytes.iter().enumerate() {
        let span_chars = clean[start..=byte_idx].chars().count();
        if matches!(ch, ';' | ':' | ',' | '؛' | '،' | '\u{2014}') && span_chars >= 36 {
            let part = clean[start..=byte_idx].trim();
            if part.chars().any(char::is_alphanumeric) {
                out.push(part.to_owned());
            }
            start = if pos + 1 < bytes.len() {
                bytes[pos + 1].0
            } else {
                clean.len()
            };
        }
    }
    if start < clean.len() {
        let rem = clean[start..].trim();
        if rem.chars().any(char::is_alphanumeric) {
            out.push(rem.to_owned());
        }
    }
}

/// Trim leading/trailing near-silent padding from synthesized clause audio while
/// keeping a short cosine/linear fade pad so edges remain click-free.
pub fn trim_clause_silence(samples: &[f32], threshold: f32, pad_samples: usize) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    let Some(first_active) = samples.iter().position(|&s| s.abs() > threshold) else {
        return Vec::new();
    };
    let last_active = samples
        .iter()
        .rposition(|&s| s.abs() > threshold)
        .unwrap_or(first_active);
    let start = first_active.saturating_sub(pad_samples);
    let end = (last_active + 1 + pad_samples).min(samples.len());
    let mut out = samples[start..end].to_vec();
    let fade = pad_samples.min(out.len() / 2);
    if fade > 0 {
        for i in 0..fade {
            let gain = (i as f32 + 1.0) / (fade as f32 + 1.0);
            out[i] *= gain;
            let tail = out.len() - 1 - i;
            out[tail] *= gain;
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct ClauseSynthesisInput {
    pub text: String,
    pub pcm_f32: Vec<f32>,
    pub focus_role: Option<FocusRole>,
    pub shot_type: Option<ShotType>,
    pub explode_factor: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ClauseConcatResult {
    pub pcm_f32: Vec<f32>,
    pub duration_ms: u32,
    pub manifest: AudioManifest,
    pub director_tracks: Vec<DirectorTrack>,
    pub clause_sample_spans: Vec<(usize, usize)>,
}

/// Assemble synthesized clauses in a single `CLAUSE_CONCAT` pass (`P-07`),
/// deriving exact `AudioWord` and `DirectorTrack` timings from trimmed sample spans.
/// Optional `P-02` Tashkeel pre-pass for Arabic technical terms during TTS pronunciation.
/// Returns a pronunciation string only; caller retains original unmutated card/caption text.
pub fn diacritize_arabic_for_tts_p02(clause: &str) -> String {
    const TECH_LEXICON: &[(&str, &str)] = &[
        ("الشبكة", "الشَّبَكَة"),
        ("المحرك", "المُحَرِّك"),
        ("التدفق", "التَّدَفُّق"),
        ("المصفوفة", "المَصْفُوفَة"),
        ("الحلقة", "الحَلْقَة"),
        ("البيانات", "البَيَانَات"),
        ("التحكم", "التَّحَكُّم"),
        ("الطاقة", "الطَّاقَة"),
        ("المعيار", "المِعْيَار"),
        ("التوازن", "التَّوَازُن"),
        ("الخوارزمية", "الخُوَارِزْمِيَّة"),
        ("الكمون", "الكُمُون"),
        ("المقاومة", "المُقَاوَمَة"),
        ("المكثف", "المُكَثِّف"),
    ];
    let mut out = clause.to_owned();
    for &(plain, vocalized) in TECH_LEXICON {
        if out.contains(plain) {
            out = out.replace(plain, vocalized);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArabicP01Report {
    pub normalized_wer: f64,
    pub missing_technical_tokens: Vec<String>,
    pub passed: bool,
    pub fallback_used: bool,
    pub quality_note: Option<String>,
}

fn is_arabic_diacritic(ch: char) -> bool {
    matches!(ch, '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{0640}')
}

fn normalize_p01_tokens(input: &str) -> Vec<String> {
    let normalized: String = input
        .chars()
        .filter(|&c| !is_arabic_diacritic(c))
        .map(|c| match c {
            'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
            'ة' => 'ه',
            'ى' => 'ي',
            _ if c.is_alphanumeric() => c.to_ascii_lowercase(),
            _ => ' ',
        })
        .collect();
    normalized.split_whitespace().map(str::to_owned).collect()
}

fn levenshtein_tokens(a: &[String], b: &[String]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, tok_a) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, tok_b) in b.iter().enumerate() {
            let cost = usize::from(tok_a != tok_b);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Evaluate `P-01` Arabic TTS quality gate: normalized WER (`<= 0.12`) plus 100%
/// preservation of required technical/unit tokens after Tashkeel normalization.
pub fn evaluate_arabic_p01_gate(
    expected_clauses: &[String],
    recognized_clauses: &[String],
    required_technical_tokens: &[String],
) -> ArabicP01Report {
    let expected_tokens = normalize_p01_tokens(&expected_clauses.join(" "));
    let recognized_tokens = normalize_p01_tokens(&recognized_clauses.join(" "));
    let wer = if expected_tokens.is_empty() {
        if recognized_tokens.is_empty() {
            0.0
        } else {
            1.0
        }
    } else {
        levenshtein_tokens(&expected_tokens, &recognized_tokens) as f64
            / expected_tokens.len() as f64
    };
    let mut missing = Vec::new();
    for tech in required_technical_tokens {
        let req_norm = normalize_p01_tokens(tech);
        if req_norm
            .iter()
            .any(|needle| !recognized_tokens.iter().any(|tok| tok == needle))
        {
            missing.push(tech.clone());
        }
    }
    let passed = wer <= P01_MAX_NORMALIZED_WER && missing.is_empty();
    let quality_note = if passed {
        None
    } else {
        Some(format!(
            "P-01 gate failed: normalized_wer={wer:.3} missing_technical_tokens={}",
            missing.len()
        ))
    };
    ArabicP01Report {
        normalized_wer: wer,
        missing_technical_tokens: missing,
        passed,
        fallback_used: false,
        quality_note,
    }
}

/// Apply a `P-01` gate evaluation (or degraded fallback flag) onto an `AudioManifest`.
pub fn apply_p01_gate_to_manifest(manifest: &mut AudioManifest, report: &ArabicP01Report) {
    let ok = report.passed && !report.fallback_used;
    manifest.quality_gate_passed = Some(ok);
    manifest.normalized_wer = Some(report.normalized_wer);
    manifest.degraded = if ok { None } else { Some(true) };
    manifest.quality_note = report.quality_note.clone();
}

/// Trim leading/trailing silence, concatenate clauses with exact sample-counted gaps,
/// and align `AudioWord` and `DirectorTrack` cues for English (`24 kHz`) narration.
pub fn assemble_clause_concat(
    asset_id: &str,
    full_text: &str,
    clauses: &[ClauseSynthesisInput],
    base_tracks: &[DirectorTrack],
) -> Result<ClauseConcatResult, TtsError> {
    assemble_clause_concat_with_language(
        asset_id,
        full_text,
        Language::English,
        KOKORO_SAMPLE_RATE_HZ,
        CLAUSE_GAP_SAMPLES,
        clauses,
        base_tracks,
    )
}

/// Shared clause concatenation engine for `Kokoro-82M` (`Language::English`, `24 kHz`)
/// and `Piper` (`Language::Arabic`, `22.05 kHz`) voiceovers.
pub fn assemble_clause_concat_with_language(
    asset_id: &str,
    full_text: &str,
    language: Language,
    sample_rate_hz: u32,
    gap_samples: usize,
    clauses: &[ClauseSynthesisInput],
    base_tracks: &[DirectorTrack],
) -> Result<ClauseConcatResult, TtsError> {
    if asset_id.trim().is_empty() {
        return Err(TtsError::InvalidInput("asset_id must not be empty".into()));
    }
    if sample_rate_hz == 0 {
        return Err(TtsError::InvalidInput(
            "sample_rate_hz must be non-zero".into(),
        ));
    }
    if clauses.is_empty() {
        return Err(TtsError::InvalidInput(
            "at least one synthesized clause is required".into(),
        ));
    }

    let mut pcm_f32 = Vec::new();
    let mut words = Vec::with_capacity(clauses.len());
    let mut spans = Vec::with_capacity(clauses.len());
    let mut director_tracks = Vec::with_capacity(clauses.len());

    for (idx, clause) in clauses.iter().enumerate() {
        let clause_text = clause.text.trim();
        if clause_text.is_empty() {
            return Err(TtsError::InvalidInput(
                "clause text must not be empty".into(),
            ));
        }
        let trimmed = trim_clause_silence(
            &clause.pcm_f32,
            DEFAULT_SILENCE_THRESHOLD,
            DEFAULT_EDGE_FADE_SAMPLES,
        );
        if trimmed.is_empty() {
            return Err(TtsError::Inference(format!(
                "clause {idx} produced silent audio after trimming"
            )));
        }
        let start_sample = pcm_f32.len();
        pcm_f32.extend_from_slice(&trimmed);
        let end_sample = pcm_f32.len();
        spans.push((start_sample, end_sample));

        let start_ms = samples_to_ms(start_sample, sample_rate_hz);
        let end_ms = samples_to_ms(end_sample, sample_rate_hz).max(start_ms + 1);
        words.push(AudioWord {
            text: clause_text.to_owned(),
            start_ms,
            end_ms,
        });

        let base = base_tracks
            .get(idx)
            .or_else(|| (!base_tracks.is_empty()).then(|| &base_tracks[idx % base_tracks.len()]));
        let (default_role, default_shot, default_explode) = match idx % 3 {
            0 => (FocusRole::Concept, ShotType::Overview, 0.0),
            1 => (FocusRole::Input, ShotType::CloseUp, 0.18),
            _ => (FocusRole::Output, ShotType::Compare, 0.25),
        };
        director_tracks.push(DirectorTrack {
            start_ms,
            end_ms,
            focus_role: clause
                .focus_role
                .clone()
                .or_else(|| base.map(|t| t.focus_role.clone()))
                .unwrap_or(default_role),
            shot_type: clause
                .shot_type
                .clone()
                .or_else(|| base.map(|t| t.shot_type.clone()))
                .unwrap_or(default_shot),
            explode_factor: clause
                .explode_factor
                .or_else(|| base.map(|t| t.explode_factor))
                .unwrap_or(default_explode),
        });

        if idx + 1 < clauses.len() {
            pcm_f32.resize(pcm_f32.len() + gap_samples, 0.0);
        }
    }

    let duration_ms = samples_to_ms(pcm_f32.len(), sample_rate_hz).max(1);
    let manifest = AudioManifest {
        asset_id: asset_id.to_owned(),
        language,
        codec: Codec::Opus,
        text: full_text.trim().to_owned(),
        duration_ms,
        alignment_method: AlignmentMethod::ClauseConcat,
        words,
        degraded: None,
        quality_gate_passed: None,
        normalized_wer: None,
        quality_note: None,
    };

    Ok(ClauseConcatResult {
        pcm_f32,
        duration_ms,
        manifest,
        director_tracks,
        clause_sample_spans: spans,
    })
}

#[derive(Debug, Clone)]
pub struct EncodedOpusMetadata {
    pub bytes: u64,
    pub codec_name: String,
    pub sample_rate_hz: u32,
    pub duration_ms: u32,
}

/// Transcode mono `24 kHz` `f32le` PCM into `.opus` and verify with `ffprobe`,
/// running both tools inside delegated cgroup-v2 helpers.
pub async fn encode_pcm_to_opus_contained(
    pcm_f32: &[f32],
    work_dir: &Path,
    output_opus: &Path,
    ffmpeg_bin: &Path,
    ffprobe_bin: &Path,
) -> Result<EncodedOpusMetadata, TtsError> {
    encode_pcm_to_opus_with_rate_contained(
        pcm_f32,
        KOKORO_SAMPLE_RATE_HZ,
        work_dir,
        output_opus,
        ffmpeg_bin,
        ffprobe_bin,
    )
    .await
}

/// Transcode mono `sample_rate_hz` `f32le` PCM into `.opus` and verify with `ffprobe`
/// inside delegated cgroup-v2 helpers (shared by English `24 kHz` and Arabic `22.05 kHz`).
pub async fn encode_pcm_to_opus_with_rate_contained(
    pcm_f32: &[f32],
    sample_rate_hz: u32,
    work_dir: &Path,
    output_opus: &Path,
    ffmpeg_bin: &Path,
    ffprobe_bin: &Path,
) -> Result<EncodedOpusMetadata, TtsError> {
    if sample_rate_hz == 0 {
        return Err(TtsError::InvalidInput(
            "sample_rate_hz must be non-zero".into(),
        ));
    }
    if pcm_f32.is_empty() {
        return Err(TtsError::InvalidInput(
            "cannot encode empty PCM buffer".into(),
        ));
    }
    fs::create_dir_all(work_dir)
        .map_err(|err| TtsError::Encoding(format!("work_dir create failed: {err}")))?;
    if let Some(parent) = output_opus.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| TtsError::Encoding(format!("output dir create failed: {err}")))?;
    }

    let raw_pcm = work_dir.join(format!("tts-{}.f32", Uuid::new_v4()));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&raw_pcm)
            .map_err(|err| TtsError::Encoding(format!("pcm file open failed: {err}")))?;
        let mut bytes = Vec::with_capacity(pcm_f32.len() * 4);
        for &sample in pcm_f32 {
            let clamped = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            bytes.extend_from_slice(&clamped.to_le_bytes());
        }
        file.write_all(&bytes)
            .map_err(|err| TtsError::Encoding(format!("pcm write failed: {err}")))?;
        file.sync_all()
            .map_err(|err| TtsError::Encoding(format!("pcm sync failed: {err}")))?;
    }

    let mut enc_cmd = Command::new(ffmpeg_bin);
    enc_cmd
        .args(["-v", "error", "-y", "-f", "f32le", "-ar"])
        .arg(sample_rate_hz.to_string())
        .args(["-ac", "1", "-i"])
        .arg(&raw_pcm)
        .args([
            "-c:a",
            "libopus",
            "-b:a",
            "32k",
            "-vbr",
            "on",
            "-application",
            "voip",
        ])
        .arg(output_opus);
    let enc_limits = HelperLimits {
        wall_time: Duration::from_secs(30),
        memory_bytes: 256 * 1024 * 1024,
        ..HelperLimits::pdf(Duration::from_secs(30), 1024 * 1024)
    };
    let enc_res = run_helper(&mut enc_cmd, enc_limits).await;
    let _ = fs::remove_file(&raw_pcm);
    let enc_out = enc_res?;
    if !enc_out.status.success() {
        return Err(TtsError::Encoding(format!(
            "ffmpeg libopus exited {}",
            enc_out.status
        )));
    }

    let mut probe_cmd = Command::new(ffprobe_bin);
    probe_cmd
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(output_opus);
    let probe_limits = HelperLimits {
        wall_time: Duration::from_secs(15),
        memory_bytes: 128 * 1024 * 1024,
        ..HelperLimits::pdf(Duration::from_secs(15), 1024 * 1024)
    };
    let probe_out = run_helper(&mut probe_cmd, probe_limits).await?;
    if !probe_out.status.success() {
        return Err(TtsError::Encoding(format!(
            "ffprobe exited {}",
            probe_out.status
        )));
    }
    let probe_json: Value = serde_json::from_slice(&probe_out.stdout)
        .map_err(|err| TtsError::Encoding(format!("ffprobe json parse error: {err}")))?;
    let stream = probe_json["streams"]
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| TtsError::Encoding("missing ffprobe audio stream".into()))?;
    let codec_name = stream["codec_name"]
        .as_str()
        .ok_or_else(|| TtsError::Encoding("missing stream codec_name".into()))?
        .to_owned();
    if codec_name != "opus" {
        return Err(TtsError::Encoding(format!(
            "expected opus codec, got {codec_name}"
        )));
    }
    let sample_rate_hz: u32 = stream["sample_rate"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(48_000);
    let duration_secs: f64 = probe_json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| stream["duration"].as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| TtsError::Encoding("missing ffprobe duration".into()))?;
    let bytes = fs::metadata(output_opus)
        .map_err(|err| TtsError::Encoding(format!("missing output opus: {err}")))?
        .len();
    if bytes == 0 || !duration_secs.is_finite() || duration_secs <= 0.0 {
        return Err(TtsError::Encoding("invalid empty opus artifact".into()));
    }
    File::open(output_opus)
        .and_then(|f| f.sync_all())
        .map_err(|err| TtsError::Encoding(format!("sync opus failed: {err}")))?;

    Ok(EncodedOpusMetadata {
        bytes,
        codec_name,
        sample_rate_hz,
        duration_ms: (duration_secs * 1000.0).round() as u32,
    })
}

#[cfg(feature = "onnx")]
pub mod onnx_runtime {
    use super::*;
    use std::{
        ffi::{CStr, CString, c_char, c_int, c_void},
        path::PathBuf,
        sync::{Mutex, OnceLock},
    };

    use ort::{
        environment::Environment,
        session::{Session, builder::GraphOptimizationLevel},
        value::Tensor,
    };

    use crate::{ModelLease, RESERVED_HOST_BYTES, host_memory, llama, process_rss};

    static ORT_ENV: OnceLock<Result<Environment, String>> = OnceLock::new();
    static ESPEAK_MUTEX: Mutex<()> = Mutex::new(());

    /// Acquire or reuse the process-wide `ort::Environment` loaded dynamically
    /// from `onnx_library`.
    pub fn ensure_onnx_environment(onnx_library: &Path) -> Result<Environment, TtsError> {
        let res = ORT_ENV.get_or_init(|| {
            ort::init_from(onnx_library)
                .map_err(|e| e.to_string())?
                .build()
                .map_err(|e| e.to_string())
        });
        res.clone().map_err(TtsError::ModelLoad)
    }

    type FnInitialize = unsafe extern "C" fn(c_int, c_int, *const c_char, c_int) -> c_int;
    type FnSetVoiceByName = unsafe extern "C" fn(*const c_char) -> c_int;
    type FnTextToPhonemes = unsafe extern "C" fn(*mut *const c_char, c_int, c_int) -> *const c_char;
    type FnTerminate = unsafe extern "C" fn() -> c_int;

    struct LoadedEspeak {
        handle: *mut c_void,
        set_voice: FnSetVoiceByName,
        text_to_phonemes: FnTextToPhonemes,
        terminate: FnTerminate,
    }

    // SAFETY: Protected by `ESPEAK_MUTEX` whenever invoking C symbols.
    unsafe impl Send for LoadedEspeak {}

    impl Drop for LoadedEspeak {
        fn drop(&mut self) {
            unsafe {
                (self.terminate)();
                libc::dlclose(self.handle);
            }
        }
    }

    /// Dynamic `libespeak-ng` phonemizer using `libc::dlopen`/`dlsym`.
    pub struct EspeakNgPhonemizer {
        lib: LoadedEspeak,
    }

    impl EspeakNgPhonemizer {
        /// Load `libespeak-ng` and initialize English (`en-us`) phonemization.
        pub fn open(library_path: &Path, data_dir: &Path) -> Result<Self, TtsError> {
            let _guard = ESPEAK_MUTEX
                .lock()
                .map_err(|_| TtsError::Phonemizer("phonemizer mutex poisoned".into()))?;
            let lib_cstr = CString::new(library_path.to_string_lossy().as_bytes())
                .map_err(|_| TtsError::Phonemizer("interior nul in library path".into()))?;
            // If caller points directly at `.../espeak-ng-data`, espeak expects
            // its parent container directory unless already containing `espeak-ng-data`.
            let effective_data_dir = if data_dir
                .file_name()
                .is_some_and(|name| name == "espeak-ng-data")
            {
                data_dir.parent().unwrap_or(data_dir)
            } else {
                data_dir
            };
            let data_cstr = CString::new(effective_data_dir.to_string_lossy().as_bytes())
                .map_err(|_| TtsError::Phonemizer("interior nul in data path".into()))?;

            unsafe {
                let handle = libc::dlopen(lib_cstr.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                if handle.is_null() {
                    return Err(TtsError::Phonemizer(format!(
                        "dlopen failed for {}",
                        library_path.display()
                    )));
                }
                let sym = |name: &[u8]| -> Result<*mut c_void, TtsError> {
                    let ptr = libc::dlsym(handle, name.as_ptr().cast());
                    if ptr.is_null() {
                        Err(TtsError::Phonemizer(format!(
                            "missing symbol {}",
                            String::from_utf8_lossy(name)
                        )))
                    } else {
                        Ok(ptr)
                    }
                };
                let init_res: Result<LoadedEspeak, TtsError> = (|| {
                    let initialize: FnInitialize =
                        std::mem::transmute(sym(b"espeak_Initialize\0")?);
                    let set_voice: FnSetVoiceByName =
                        std::mem::transmute(sym(b"espeak_SetVoiceByName\0")?);
                    let text_to_phonemes: FnTextToPhonemes =
                        std::mem::transmute(sym(b"espeak_TextToPhonemes\0")?);
                    let terminate: FnTerminate = std::mem::transmute(sym(b"espeak_Terminate\0")?);
                    let sr = initialize(0, 0, data_cstr.as_ptr(), 0);
                    if sr <= 0 {
                        return Err(TtsError::Phonemizer(format!(
                            "espeak_Initialize failed (rc={sr})"
                        )));
                    }
                    let rc = set_voice(c"en-us".as_ptr());
                    if rc != 0 {
                        let _ = terminate();
                        return Err(TtsError::Phonemizer(format!(
                            "espeak_SetVoiceByName(en-us) failed (rc={rc})"
                        )));
                    }
                    Ok(LoadedEspeak {
                        handle,
                        set_voice,
                        text_to_phonemes,
                        terminate,
                    })
                })();
                match init_res {
                    Ok(lib) => Ok(Self { lib }),
                    Err(err) => {
                        libc::dlclose(handle);
                        Err(err)
                    }
                }
            }
        }

        /// Convert English clause text into IPA phoneme text compatible with Kokoro.
        pub fn phonemize_clause(&self, clause: &str) -> Result<String, TtsError> {
            self.phonemize_clause_for_voice(c"en-us", clause)
        }

        /// Convert clause text into IPA phoneme text under an explicit espeak-ng voice
        /// (e.g. `c"en-us"` for Kokoro-82M or `c"ar"` for Piper Arabic).
        pub fn phonemize_clause_for_voice(
            &self,
            voice: &CStr,
            clause: &str,
        ) -> Result<String, TtsError> {
            let trimmed = clause.trim();
            if trimmed.is_empty() {
                return Err(TtsError::Phonemizer("empty clause".into()));
            }
            let _guard = ESPEAK_MUTEX
                .lock()
                .map_err(|_| TtsError::Phonemizer("phonemizer mutex poisoned".into()))?;
            let c_input = CString::new(trimmed.as_bytes())
                .map_err(|_| TtsError::Phonemizer("interior nul in clause text".into()))?;
            let mut cursor = c_input.as_ptr();
            let mut parts = Vec::new();
            unsafe {
                let rc = (self.lib.set_voice)(voice.as_ptr());
                if rc != 0 {
                    return Err(TtsError::Phonemizer(format!(
                        "set_voice({}) rc={rc}",
                        voice.to_string_lossy()
                    )));
                }
                for _ in 0..1024 {
                    if cursor.is_null() || *cursor == 0 {
                        break;
                    }
                    let out_ptr = (self.lib.text_to_phonemes)(&mut cursor, 1, 0x02);
                    if !out_ptr.is_null() {
                        let piece = CStr::from_ptr(out_ptr).to_string_lossy().trim().to_owned();
                        if !piece.is_empty() {
                            parts.push(piece);
                        }
                    }
                }
            }
            let mut ipa = parts.join(" ");
            if let Some(last) = trimmed.chars().last()
                && matches!(last, '.' | '?' | '!' | ',' | ';' | ':' | '؟' | '؛' | '،')
            {
                let normalized_punct = match last {
                    '؟' => '?',
                    '؛' => ';',
                    '،' => ',',
                    other => other,
                };
                if !ipa.ends_with(normalized_punct) {
                    ipa.push(normalized_punct);
                }
            }
            if ipa.trim().is_empty() {
                return Err(TtsError::Phonemizer("empty IPA output".into()));
            }
            Ok(ipa)
        }
    }

    #[derive(Debug, Clone)]
    pub struct KokoroConfig {
        pub onnx_library: PathBuf,
        pub model_path: PathBuf,
        pub tokenizer_path: PathBuf,
        pub voice_path: PathBuf,
        pub espeak_library: PathBuf,
        pub espeak_data_dir: PathBuf,
        pub ffmpeg_bin: PathBuf,
        pub ffprobe_bin: PathBuf,
        pub lease_path: PathBuf,
        pub threads: u32,
        pub budget_bytes: u64,
    }

    impl KokoroConfig {
        /// Build config from an asset directory containing `model.onnx`,
        /// `tokenizer.json`, and `af_heart.bin`.
        pub fn with_kokoro_dir(
            kokoro_dir: &Path,
            onnx_library: PathBuf,
            espeak_library: PathBuf,
            espeak_data_dir: PathBuf,
            lease_path: PathBuf,
        ) -> Self {
            Self {
                onnx_library,
                model_path: kokoro_dir.join("model.onnx"),
                tokenizer_path: kokoro_dir.join("tokenizer.json"),
                voice_path: kokoro_dir.join("af_heart.bin"),
                espeak_library,
                espeak_data_dir,
                ffmpeg_bin: PathBuf::from("/usr/bin/ffmpeg"),
                ffprobe_bin: PathBuf::from("/usr/bin/ffprobe"),
                lease_path,
                threads: llama::default_compute_threads(),
                budget_bytes: MAX_TTS_RSS_BYTES,
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct SynthesizedOpusBundle {
        pub manifest: AudioManifest,
        pub director_tracks: Vec<DirectorTrack>,
        pub opus_path: PathBuf,
        pub encoded: EncodedOpusMetadata,
        pub total_samples: usize,
        pub peak_rss_bytes: u64,
    }

    /// Single-resident English `Kokoro-82M` synthesizer holding an exclusive `ModelLease`.
    pub struct KokoroOnnxSynthesizer {
        config: KokoroConfig,
        session: Session,
        tokenizer: KokoroTokenizer,
        voices: VoiceStyleBank,
        phonemizer: EspeakNgPhonemizer,
        _lease: ModelLease,
    }

    impl KokoroOnnxSynthesizer {
        /// Acquire `ModelLease`, load ONNX Runtime + `Kokoro-82M` + `espeak-ng`,
        /// and enforce the TTS memory ceiling.
        pub fn start(config: KokoroConfig) -> Result<Self, TtsError> {
            let model_bytes = fs::metadata(&config.model_path)
                .map_err(|err| TtsError::ModelLoad(format!("model metadata failed: {err}")))?
                .len();
            let reserve = RESERVED_HOST_BYTES
                .saturating_add(model_bytes)
                .saturating_add(128 * 1024 * 1024);
            if host_memory().map_err(|_| TtsError::Memory)?.available_bytes < reserve {
                return Err(TtsError::Memory);
            }
            let lease = ModelLease::acquire(&config.lease_path, reserve)
                .map_err(|err| TtsError::Lease(err.to_string()))?;
            let env = ensure_onnx_environment(&config.onnx_library)?;
            let threads = config.threads.clamp(1, 16) as usize;
            let session = Session::builder(&env)
                .map_err(|err| TtsError::ModelLoad(err.to_string()))?
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|err| TtsError::ModelLoad(err.to_string()))?
                .with_intra_threads(threads)
                .map_err(|err| TtsError::ModelLoad(err.to_string()))?
                .commit_from_file(&config.model_path)
                .map_err(|err| TtsError::ModelLoad(err.to_string()))?;
            let tokenizer = KokoroTokenizer::from_file(&config.tokenizer_path)?;
            let voices = VoiceStyleBank::from_file(&config.voice_path)?;
            let phonemizer =
                EspeakNgPhonemizer::open(&config.espeak_library, &config.espeak_data_dir)?;
            let rss = process_rss(std::process::id()).map_err(|_| TtsError::Memory)?;
            if rss > config.budget_bytes {
                return Err(TtsError::Memory);
            }
            Ok(Self {
                config,
                session,
                tokenizer,
                voices,
                phonemizer,
                _lease: lease,
            })
        }

        /// Synthesize narration clause-by-clause, concatenate trimmed PCM, encode
        /// `.opus` inside a delegated helper, and emit synchronized manifests.
        pub async fn synthesize_clause_concat(
            &mut self,
            asset_id: &str,
            narration_text: &str,
            speed: f32,
            base_tracks: &[DirectorTrack],
            work_dir: &Path,
            output_opus: &Path,
        ) -> Result<SynthesizedOpusBundle, TtsError> {
            if !speed.is_finite() || !(0.5..=2.0).contains(&speed) {
                return Err(TtsError::InvalidInput(format!(
                    "invalid Kokoro speed {speed}"
                )));
            }
            let clauses = split_clauses_for_p07(narration_text)?;
            let mut clause_inputs = Vec::with_capacity(clauses.len());
            let mut peak_rss = process_rss(std::process::id()).unwrap_or(0);

            for clause in &clauses {
                let ipa = self.phonemizer.phonemize_clause(clause)?;
                let phoneme_len = ipa.chars().count();
                let ids = self.tokenizer.encode_phonemes(&ipa)?;
                let style = self.voices.style_for_phoneme_len(phoneme_len);
                let seq_len = ids.len();

                let ids_tensor = Tensor::from_array(([1_usize, seq_len], ids.into_boxed_slice()))
                    .map_err(|err| TtsError::Inference(err.to_string()))?;
                let style_tensor = Tensor::from_array(([1_usize, 256], style.into_boxed_slice()))
                    .map_err(|err| TtsError::Inference(err.to_string()))?;
                let speed_tensor = Tensor::from_array(([1_usize], vec![speed].into_boxed_slice()))
                    .map_err(|err| TtsError::Inference(err.to_string()))?;

                let outputs = self
                    .session
                    .run(ort::inputs![
                        "input_ids" => ids_tensor,
                        "style" => style_tensor,
                        "speed" => speed_tensor,
                    ])
                    .map_err(|err| TtsError::Inference(err.to_string()))?;
                let waveform = outputs
                    .get("waveform")
                    .ok_or_else(|| TtsError::Inference("missing waveform output".into()))?;
                let (_, samples) = waveform
                    .try_extract_tensor::<f32>()
                    .map_err(|err| TtsError::Inference(err.to_string()))?;
                clause_inputs.push(ClauseSynthesisInput {
                    text: clause.clone(),
                    pcm_f32: samples.to_vec(),
                    focus_role: None,
                    shot_type: None,
                    explode_factor: None,
                });

                let rss = process_rss(std::process::id()).unwrap_or(0);
                peak_rss = peak_rss.max(rss);
                if rss > self.config.budget_bytes {
                    return Err(TtsError::Memory);
                }
            }

            let concat =
                assemble_clause_concat(asset_id, narration_text, &clause_inputs, base_tracks)?;
            let total_samples = concat.pcm_f32.len();
            let encoded = encode_pcm_to_opus_contained(
                &concat.pcm_f32,
                work_dir,
                output_opus,
                &self.config.ffmpeg_bin,
                &self.config.ffprobe_bin,
            )
            .await?;

            Ok(SynthesizedOpusBundle {
                manifest: concat.manifest,
                director_tracks: concat.director_tracks,
                opus_path: output_opus.to_path_buf(),
                encoded,
                total_samples,
                peak_rss_bytes: peak_rss,
            })
        }
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    struct PiperAudioCfg {
        sample_rate: u32,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    struct PiperEspeakCfg {
        voice: String,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    struct PiperInferenceCfg {
        noise_scale: f32,
        length_scale: f32,
        noise_w: f32,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    pub struct PiperVoiceConfig {
        audio: PiperAudioCfg,
        espeak: PiperEspeakCfg,
        inference: PiperInferenceCfg,
        phoneme_id_map: HashMap<String, Vec<i64>>,
    }

    impl PiperVoiceConfig {
        pub fn from_file(path: &Path) -> Result<Self, TtsError> {
            let raw = fs::read_to_string(path).map_err(|err| {
                TtsError::Tokenizer(format!(
                    "failed to read Piper config {}: {err}",
                    path.display()
                ))
            })?;
            serde_json::from_str(&raw)
                .map_err(|err| TtsError::Tokenizer(format!("invalid Piper config JSON: {err}")))
        }

        pub fn sample_rate_hz(&self) -> u32 {
            self.audio.sample_rate
        }

        pub fn espeak_voice(&self) -> &str {
            &self.espeak.voice
        }

        /// Encode IPA phoneme string using Piper VITS BOS/pad/EOS conventions.
        pub fn encode_phonemes(&self, ipa: &str) -> Result<Vec<i64>, TtsError> {
            let bos = self
                .phoneme_id_map
                .get("^")
                .and_then(|v| v.first())
                .copied()
                .unwrap_or(1);
            let pad = self
                .phoneme_id_map
                .get("_")
                .and_then(|v| v.first())
                .copied()
                .unwrap_or(0);
            let eos = self
                .phoneme_id_map
                .get("$")
                .and_then(|v| v.first())
                .copied()
                .unwrap_or(2);
            let mut ids = Vec::with_capacity(ipa.chars().count() * 2 + 4);
            ids.push(bos);
            ids.push(pad);
            let mut mapped_phonemes = 0usize;
            for ch in ipa.chars() {
                let mut buf = [0u8; 4];
                let key = ch.encode_utf8(&mut buf);
                if let Some(seq) = self.phoneme_id_map.get(key) {
                    for &id in seq {
                        ids.push(id);
                        ids.push(pad);
                    }
                    mapped_phonemes += 1;
                }
            }
            if mapped_phonemes == 0 {
                return Err(TtsError::Tokenizer(
                    "Piper Arabic phoneme encoding produced zero mapped symbols".into(),
                ));
            }
            ids.push(eos);
            Ok(ids)
        }
    }

    #[derive(Debug, Clone)]
    pub struct PiperArConfig {
        pub onnx_library: PathBuf,
        pub model_path: PathBuf,
        pub config_path: PathBuf,
        pub espeak_library: PathBuf,
        pub espeak_data_dir: PathBuf,
        pub ffmpeg_bin: PathBuf,
        pub ffprobe_bin: PathBuf,
        pub lease_path: PathBuf,
        pub threads: u32,
        pub budget_bytes: u64,
    }

    impl PiperArConfig {
        pub fn with_piper_ar_dir(
            piper_dir: &Path,
            onnx_library: PathBuf,
            espeak_library: PathBuf,
            espeak_data_dir: PathBuf,
            lease_path: PathBuf,
        ) -> Self {
            Self {
                onnx_library,
                model_path: piper_dir.join("ar_JO-kareem-medium.onnx"),
                config_path: piper_dir.join("ar_JO-kareem-medium.onnx.json"),
                espeak_library,
                espeak_data_dir,
                ffmpeg_bin: PathBuf::from("/usr/bin/ffmpeg"),
                ffprobe_bin: PathBuf::from("/usr/bin/ffprobe"),
                lease_path,
                threads: llama::default_compute_threads(),
                budget_bytes: super::MAX_TTS_RSS_BYTES,
            }
        }
    }

    /// Single-resident Arabic `Piper ar_JO-kareem-medium` synthesizer holding `ModelLease`.
    pub struct PiperOnnxSynthesizer {
        config: PiperArConfig,
        voice_cfg: PiperVoiceConfig,
        session: Session,
        phonemizer: EspeakNgPhonemizer,
        _lease: ModelLease,
    }

    impl PiperOnnxSynthesizer {
        pub fn start(config: PiperArConfig) -> Result<Self, TtsError> {
            let model_bytes = fs::metadata(&config.model_path)
                .map_err(|err| TtsError::ModelLoad(format!("Piper model metadata failed: {err}")))?
                .len();
            let reserve = RESERVED_HOST_BYTES
                .saturating_add(model_bytes)
                .saturating_add(128 * 1024 * 1024);
            if host_memory().map_err(|_| TtsError::Memory)?.available_bytes < reserve {
                return Err(TtsError::Memory);
            }
            let lease = ModelLease::acquire(&config.lease_path, reserve)
                .map_err(|err| TtsError::Lease(err.to_string()))?;
            let voice_cfg = PiperVoiceConfig::from_file(&config.config_path)?;
            let env = ensure_onnx_environment(&config.onnx_library)?;
            let threads = config.threads.clamp(1, 16) as usize;
            let session = Session::builder(&env)
                .map_err(|err| TtsError::ModelLoad(err.to_string()))?
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|err| TtsError::ModelLoad(err.to_string()))?
                .with_intra_threads(threads)
                .map_err(|err| TtsError::ModelLoad(err.to_string()))?
                .commit_from_file(&config.model_path)
                .map_err(|err| TtsError::ModelLoad(err.to_string()))?;
            let phonemizer =
                EspeakNgPhonemizer::open(&config.espeak_library, &config.espeak_data_dir)?;
            let rss = process_rss(std::process::id()).map_err(|_| TtsError::Memory)?;
            if rss > config.budget_bytes {
                return Err(TtsError::Memory);
            }
            Ok(Self {
                config,
                voice_cfg,
                session,
                phonemizer,
                _lease: lease,
            })
        }

        /// Synthesize Arabic narration clause-by-clause with non-mutating `P-02` Tashkeel,
        /// concatenate trimmed `22.05 kHz` PCM, and emit `CLAUSE_CONCAT` manifest + `.opus`.
        #[allow(clippy::too_many_arguments)]
        pub async fn synthesize_clause_concat(
            &mut self,
            asset_id: &str,
            narration_text: &str,
            speed: f32,
            use_p02_tashkeel: bool,
            base_tracks: &[DirectorTrack],
            work_dir: &Path,
            output_opus: &Path,
        ) -> Result<SynthesizedOpusBundle, TtsError> {
            if !speed.is_finite() || !(0.5..=2.0).contains(&speed) {
                return Err(TtsError::InvalidInput(format!(
                    "invalid Piper speed {speed}"
                )));
            }
            let clauses = super::split_clauses_for_p07(narration_text)?;
            let voice_cstr = CString::new(self.voice_cfg.espeak_voice().as_bytes())
                .map_err(|_| TtsError::Phonemizer("interior nul in espeak voice".into()))?;
            let mut clause_inputs = Vec::with_capacity(clauses.len());
            let mut peak_rss = process_rss(std::process::id()).unwrap_or(0);

            for clause in &clauses {
                let tts_clause = if use_p02_tashkeel {
                    super::diacritize_arabic_for_tts_p02(clause)
                } else {
                    clause.clone()
                };
                let ipa = self
                    .phonemizer
                    .phonemize_clause_for_voice(&voice_cstr, &tts_clause)?;
                let ids = self.voice_cfg.encode_phonemes(&ipa)?;
                let seq_len = ids.len() as i64;
                let input_tensor =
                    Tensor::from_array(([1_usize, ids.len()], ids.into_boxed_slice()))
                        .map_err(|err| TtsError::Inference(err.to_string()))?;
                let lengths_tensor =
                    Tensor::from_array(([1_usize], vec![seq_len].into_boxed_slice()))
                        .map_err(|err| TtsError::Inference(err.to_string()))?;
                let length_scale = self.voice_cfg.inference.length_scale / speed;
                let scales_tensor = Tensor::from_array((
                    [3_usize],
                    vec![
                        self.voice_cfg.inference.noise_scale,
                        length_scale,
                        self.voice_cfg.inference.noise_w,
                    ]
                    .into_boxed_slice(),
                ))
                .map_err(|err| TtsError::Inference(err.to_string()))?;

                let outputs = self
                    .session
                    .run(ort::inputs![
                        "input" => input_tensor,
                        "input_lengths" => lengths_tensor,
                        "scales" => scales_tensor,
                    ])
                    .map_err(|err| TtsError::Inference(err.to_string()))?;
                let audio_out = outputs
                    .get("output")
                    .ok_or_else(|| TtsError::Inference("missing Piper output tensor".into()))?;
                let (_, samples) = audio_out
                    .try_extract_tensor::<f32>()
                    .map_err(|err| TtsError::Inference(err.to_string()))?;

                // Keep original unmutated `clause` text in the AudioManifest / caption word
                clause_inputs.push(ClauseSynthesisInput {
                    text: clause.clone(),
                    pcm_f32: samples.to_vec(),
                    focus_role: None,
                    shot_type: None,
                    explode_factor: None,
                });

                let rss = process_rss(std::process::id()).unwrap_or(0);
                peak_rss = peak_rss.max(rss);
                if rss > self.config.budget_bytes {
                    return Err(TtsError::Memory);
                }
            }

            let sample_rate_hz = self
                .voice_cfg
                .sample_rate_hz()
                .max(super::PIPER_AR_SAMPLE_RATE_HZ);
            let gap_samples = ((sample_rate_hz as usize) * 60) / 1000;
            let concat = super::assemble_clause_concat_with_language(
                asset_id,
                narration_text,
                Language::Arabic,
                sample_rate_hz,
                gap_samples,
                &clause_inputs,
                base_tracks,
            )?;
            let total_samples = concat.pcm_f32.len();
            let encoded = super::encode_pcm_to_opus_with_rate_contained(
                &concat.pcm_f32,
                sample_rate_hz,
                work_dir,
                output_opus,
                &self.config.ffmpeg_bin,
                &self.config.ffprobe_bin,
            )
            .await?;

            Ok(SynthesizedOpusBundle {
                manifest: concat.manifest,
                director_tracks: concat.director_tracks,
                opus_path: output_opus.to_path_buf(),
                encoded,
                total_samples,
                peak_rss_bytes: peak_rss,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_clauses_respects_decimals_and_splits_long_sentences() {
        let clauses = split_clauses_for_p07(
            "The threshold is 12.5 volts. Keep power bounded; however, when load rises rapidly during surge testing, reduce step size immediately!",
        )
        .unwrap();
        assert_eq!(clauses[0], "The threshold is 12.5 volts.");
        assert!(clauses.len() >= 2);
        assert!(clauses.iter().all(|c| !c.trim().is_empty()));
        assert!(split_clauses_for_p07("   ...  ").is_err());
    }

    #[test]
    fn trim_silence_and_assemble_clause_concat_emit_exact_sample_timings() {
        let mut c1 = vec![0.0_f32; 4800];
        c1.extend(vec![0.35_f32; 12_000]); // 500 ms active
        c1.extend(vec![0.0_f32; 4800]);

        let mut c2 = vec![0.0_f32; 2400];
        c2.extend(vec![-0.30_f32; 24_000]); // 1000 ms active
        c2.extend(vec![0.0_f32; 2400]);

        let result = assemble_clause_concat(
            "asset-1",
            "First clause. Second clause.",
            &[
                ClauseSynthesisInput {
                    text: "First clause.".into(),
                    pcm_f32: c1,
                    focus_role: None,
                    shot_type: None,
                    explode_factor: None,
                },
                ClauseSynthesisInput {
                    text: "Second clause.".into(),
                    pcm_f32: c2,
                    focus_role: Some(FocusRole::Output),
                    shot_type: Some(ShotType::CloseUp),
                    explode_factor: Some(0.4),
                },
            ],
            &[],
        )
        .unwrap();

        assert!(matches!(
            result.manifest.alignment_method,
            AlignmentMethod::ClauseConcat
        ));
        assert_eq!(result.manifest.words.len(), 2);
        assert_eq!(result.director_tracks.len(), 2);

        let w0 = &result.manifest.words[0];
        let w1 = &result.manifest.words[1];
        assert_eq!(w0.start_ms, 0);
        assert_eq!(
            w0.end_ms,
            samples_to_ms(result.clause_sample_spans[0].1, KOKORO_SAMPLE_RATE_HZ)
        );
        // Active 12000 + 2*240 pad = 12480 samples => 520 ms (significantly trimmed from 21600 = 900 ms)
        assert_eq!(w0.end_ms, 520);
        // Second clause starts after w0 + 1440-sample (60 ms) gap => 580 ms
        assert_eq!(w1.start_ms, 580);
        assert_eq!(w1.end_ms, result.duration_ms);
        assert_eq!(result.director_tracks[0].start_ms, w0.start_ms);
        assert_eq!(result.director_tracks[1].end_ms, w1.end_ms);
    }

    #[test]
    fn kokoro_tokenizer_and_voice_style_bank_parse_and_reject_invalid() {
        let tok_json = r#"{"model":{"vocab":{"$":0," ":16,".":4,"h":50,"ə":83,"l":54,"o":57}}}"#;
        let tok = KokoroTokenizer::from_json_str(tok_json).unwrap();
        assert_eq!(
            tok.encode_phonemes("həlo.").unwrap(),
            vec![0, 50, 83, 54, 57, 4, 0]
        );
        assert!(tok.encode_phonemes("⌘").is_err());

        let mut bytes = Vec::with_capacity(2 * 256 * 4);
        for i in 0..512 {
            bytes.extend_from_slice(&(i as f32 * 0.01).to_le_bytes());
        }
        let bank = VoiceStyleBank::from_bytes(&bytes).unwrap();
        assert_eq!(bank.style_for_phoneme_len(0).len(), 256);
        assert!((bank.style_for_phoneme_len(99)[0] - 2.56).abs() < 1e-5);
        assert!(VoiceStyleBank::from_bytes(&[0u8; 100]).is_err());
    }

    #[tokio::test]
    #[ignore = "requires ffmpeg/ffprobe and delegated cgroup containment"]
    async fn contained_ffmpeg_encodes_and_probes_valid_opus() {
        let dir = tempfile::tempdir().unwrap();
        let opus = dir.path().join("briefing.opus");
        let mut tone = Vec::with_capacity(24_000);
        for i in 0..24_000 {
            tone.push(0.2 * ((i as f32) * 440.0 * 2.0 * std::f32::consts::PI / 24_000.0).sin());
        }
        let meta = encode_pcm_to_opus_contained(
            &tone,
            dir.path(),
            &opus,
            Path::new("/usr/bin/ffmpeg"),
            Path::new("/usr/bin/ffprobe"),
        )
        .await
        .unwrap();
        assert_eq!(meta.codec_name, "opus");
        assert!(meta.bytes > 0);
        assert!((meta.duration_ms as i64 - 1000).abs() <= 150);
    }

    #[cfg(feature = "onnx")]
    #[tokio::test]
    #[ignore = "requires local Kokoro-82M ONNX, libonnxruntime, libespeak-ng, and delegated cgroup"]
    async fn contained_kokoro_82m_english_clause_concat_and_exclusive_model_lease() {
        use super::onnx_runtime::{KokoroConfig, KokoroOnnxSynthesizer};
        use std::path::PathBuf;
        let root = crate::project_root();
        let kokoro_dir = root.join("artifacts/private/models/kokoro-82m");
        let onnx_lib = PathBuf::from("/usr/lib/libonnxruntime.so.1.29.0");
        let espeak_lib = PathBuf::from(
            "/home/mahdi/.cache/uv/archive-v0/6HBBtZn0XiXc2C0i/espeakng_loader/libespeak-ng.so.1.52.0",
        );
        let espeak_data =
            PathBuf::from("/home/mahdi/.cache/uv/archive-v0/6HBBtZn0XiXc2C0i/espeakng_loader");
        assert!(kokoro_dir.join("model.onnx").exists());
        assert!(onnx_lib.exists());
        assert!(espeak_lib.exists());

        let work_dir = tempfile::tempdir().unwrap();
        let lease_path = work_dir.path().join("tts.lease.json");
        let output_opus = work_dir.path().join("m3-briefing.opus");
        let cfg = KokoroConfig::with_kokoro_dir(
            &kokoro_dir,
            onnx_lib,
            espeak_lib,
            espeak_data,
            lease_path.clone(),
        );

        let mut synth = KokoroOnnxSynthesizer::start(cfg).unwrap();
        // Single-model residency: another model cannot acquire ModelLease while Kokoro is resident.
        assert!(crate::ModelLease::acquire(&lease_path, 0).is_err());

        let bundle = synth
            .synthesize_clause_concat(
                "m3-audio-en-1",
                "Step one: increase parameter power from 0 to 6. Output scales by 2.0 to reach 12 volts within target bounds!",
                1.0,
                &[],
                work_dir.path(),
                &output_opus,
            )
            .await
            .unwrap();

        assert_eq!(bundle.encoded.codec_name, "opus");
        assert!(bundle.encoded.bytes > 0);
        assert!(matches!(
            bundle.manifest.alignment_method,
            AlignmentMethod::ClauseConcat
        ));
        assert!(bundle.manifest.words.len() >= 2);
        assert_eq!(bundle.manifest.words.len(), bundle.director_tracks.len());
        for (w, t) in bundle
            .manifest
            .words
            .iter()
            .zip(bundle.director_tracks.iter())
        {
            assert!(w.start_ms < w.end_ms);
            assert!(w.end_ms <= bundle.manifest.duration_ms);
            assert_eq!(w.start_ms, t.start_ms);
            assert_eq!(w.end_ms, t.end_ms);
        }
        assert!(
            (i64::from(bundle.encoded.duration_ms) - i64::from(bundle.manifest.duration_ms)).abs()
                <= 150,
            "opus container duration ({}) must track exact sample duration ({})",
            bundle.encoded.duration_ms,
            bundle.manifest.duration_ms
        );
        eprintln!(
            "kokoro-82m EN CLAUSE_CONCAT: clauses={}, duration_ms={}, opus_ms={}, opus_bytes={}, peak_rss_mb={}",
            bundle.manifest.words.len(),
            bundle.manifest.duration_ms,
            bundle.encoded.duration_ms,
            bundle.encoded.bytes,
            bundle.peak_rss_bytes / (1024 * 1024)
        );

        drop(synth);
        assert!(
            crate::ModelLease::acquire(&lease_path, 0).is_ok(),
            "model lease must release when Kokoro synthesizer drops"
        );
    }

    #[test]
    fn arabic_p02_tashkeel_and_p01_quality_gate_preserve_source_and_flag_degraded() {
        let source = "تحافظ الشبكة على التوازن عند 12.5 فولت! هل تحققت من الخوارزمية؟ راقب الكمون.";
        let clauses = split_clauses_for_p07(source).unwrap();
        assert_eq!(clauses.len(), 3);
        let vocalized = diacritize_arabic_for_tts_p02(&clauses[0]);
        assert_ne!(vocalized, clauses[0]);
        // Source card/overlay text stays untouched
        assert_eq!(clauses[0], "تحافظ الشبكة على التوازن عند 12.5 فولت!");

        let pass_report = evaluate_arabic_p01_gate(
            &clauses,
            &clauses,
            &["12.5".to_string(), "الشبكة".to_string()],
        );
        assert!(pass_report.passed);
        assert!(pass_report.normalized_wer <= P01_MAX_NORMALIZED_WER);

        let fail_report = evaluate_arabic_p01_gate(
            &clauses,
            &["تحافظ الشبكة على شيء اخر تماما".to_string()],
            &["12.5".to_string()],
        );
        assert!(!fail_report.passed);
        assert_eq!(fail_report.missing_technical_tokens, vec!["12.5"]);

        let mut manifest = AudioManifest {
            asset_id: "m7-ar-gate-1".into(),
            language: Language::Arabic,
            codec: Codec::Opus,
            text: source.into(),
            duration_ms: 1500,
            alignment_method: AlignmentMethod::ClauseConcat,
            words: vec![AudioWord {
                text: clauses[0].clone(),
                start_ms: 0,
                end_ms: 1500,
            }],
            degraded: None,
            quality_gate_passed: None,
            normalized_wer: None,
            quality_note: None,
        };
        apply_p01_gate_to_manifest(&mut manifest, &fail_report);
        assert_eq!(manifest.degraded, Some(true));
        assert_eq!(manifest.quality_gate_passed, Some(false));
    }

    #[test]
    fn arabic_clause_concat_22050hz_emits_exact_sample_timings() {
        let mut c1 = vec![0.0_f32; 2205];
        c1.extend(vec![0.3_f32; 11_025]); // 500 ms active at 22.05 kHz
        c1.extend(vec![0.0_f32; 2205]);

        let c2 = vec![-0.25_f32; 22_050]; // 1000 ms active

        let result = assemble_clause_concat_with_language(
            "asset-ar-1",
            "الشبكة مستقرة. التدفق متوازن.",
            Language::Arabic,
            PIPER_AR_SAMPLE_RATE_HZ,
            PIPER_AR_GAP_SAMPLES,
            &[
                ClauseSynthesisInput {
                    text: "الشبكة مستقرة.".into(),
                    pcm_f32: c1,
                    focus_role: None,
                    shot_type: None,
                    explode_factor: None,
                },
                ClauseSynthesisInput {
                    text: "التدفق متوازن.".into(),
                    pcm_f32: c2,
                    focus_role: Some(FocusRole::Output),
                    shot_type: Some(ShotType::Compare),
                    explode_factor: Some(0.25),
                },
            ],
            &[],
        )
        .unwrap();

        assert!(matches!(result.manifest.language, Language::Arabic));
        assert!(matches!(
            result.manifest.alignment_method,
            AlignmentMethod::ClauseConcat
        ));
        assert_eq!(result.manifest.words.len(), 2);
        let w0 = &result.manifest.words[0];
        let w1 = &result.manifest.words[1];
        assert_eq!(w0.start_ms, 0);
        // 11025 + 2*240 = 11505 samples @ 22050 Hz => 521 ms
        assert_eq!(
            w0.end_ms,
            samples_to_ms(result.clause_sample_spans[0].1, PIPER_AR_SAMPLE_RATE_HZ)
        );
        assert_eq!(
            w1.start_ms,
            samples_to_ms(
                result.clause_sample_spans[0].1 + PIPER_AR_GAP_SAMPLES,
                PIPER_AR_SAMPLE_RATE_HZ
            )
        );
        assert_eq!(w1.end_ms, result.duration_ms);
    }

    #[cfg(feature = "onnx")]
    #[tokio::test]
    #[ignore = "requires local Piper ar_JO-kareem-medium ONNX, libonnxruntime, libespeak-ng, and delegated cgroup"]
    async fn contained_piper_ar_clause_concat_and_exclusive_model_lease() {
        use super::onnx_runtime::{PiperArConfig, PiperOnnxSynthesizer};
        use std::path::PathBuf;
        let root = crate::project_root();
        let piper_dir = root.join("artifacts/private/models/piper-ar");
        let onnx_lib = PathBuf::from("/usr/lib/libonnxruntime.so.1.29.0");
        let espeak_lib = PathBuf::from(
            "/home/mahdi/.cache/uv/archive-v0/6HBBtZn0XiXc2C0i/espeakng_loader/libespeak-ng.so.1.52.0",
        );
        let espeak_data =
            PathBuf::from("/home/mahdi/.cache/uv/archive-v0/6HBBtZn0XiXc2C0i/espeakng_loader");
        assert!(piper_dir.join("ar_JO-kareem-medium.onnx").exists());
        assert!(piper_dir.join("ar_JO-kareem-medium.onnx.json").exists());
        assert!(onnx_lib.exists());
        assert!(espeak_lib.exists());

        let work_dir = tempfile::tempdir().unwrap();
        let lease_path = work_dir.path().join("piper-tts.lease.json");
        let output_opus = work_dir.path().join("m7-briefing-ar.opus");
        let cfg = PiperArConfig::with_piper_ar_dir(
            &piper_dir,
            onnx_lib,
            espeak_lib,
            espeak_data,
            lease_path.clone(),
        );

        let mut synth = PiperOnnxSynthesizer::start(cfg).unwrap();
        assert!(crate::ModelLease::acquire(&lease_path, 0).is_err());

        let raw_ar = "تحافظ الشبكة على التوازن عند 12 فولت! راقب التدفق داخل المحرك.";
        let bundle = synth
            .synthesize_clause_concat(
                "m7-audio-ar-1",
                raw_ar,
                1.0,
                true,
                &[],
                work_dir.path(),
                &output_opus,
            )
            .await
            .unwrap();

        assert!(matches!(bundle.manifest.language, Language::Arabic));
        assert!(matches!(
            bundle.manifest.alignment_method,
            AlignmentMethod::ClauseConcat
        ));
        assert_eq!(bundle.manifest.text, raw_ar);
        assert_eq!(bundle.encoded.codec_name, "opus");
        assert!(bundle.encoded.bytes > 0);
        assert_eq!(bundle.manifest.words.len(), 2);
        // Source card/caption text is NOT mutated by P-02 vocalization
        assert!(!bundle.manifest.words[0].text.contains('\u{0651}'));
        assert!(
            (i64::from(bundle.encoded.duration_ms) - i64::from(bundle.manifest.duration_ms)).abs()
                <= 150,
            "opus container duration ({}) must track exact sample duration ({})",
            bundle.encoded.duration_ms,
            bundle.manifest.duration_ms
        );
        eprintln!(
            "piper ar_JO-kareem-medium AR CLAUSE_CONCAT: clauses={}, duration_ms={}, opus_ms={}, opus_bytes={}, peak_rss_mb={}",
            bundle.manifest.words.len(),
            bundle.manifest.duration_ms,
            bundle.encoded.duration_ms,
            bundle.encoded.bytes,
            bundle.peak_rss_bytes / (1024 * 1024)
        );

        drop(synth);
        assert!(crate::ModelLease::acquire(&lease_path, 0).is_ok());
    }
}
