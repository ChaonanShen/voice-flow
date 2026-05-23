//! voice-asr-local
//!
//! 端侧 ASR 引擎实现（sherpa-onnx + Streaming Zipformer）。

use std::path::{Path, PathBuf};

use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};
use voice_core::asr::{AsrEngine, AsrError};
use voice_core::capture::AudioFormat;

/// 默认模型目录名，与 sherpa-onnx 官方 release archive 解压后目录一致。
pub const DEFAULT_STREAMING_ZIPFORMER_DIR: &str =
    "sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20";

/// 可选真实模型加载测试使用的环境变量。
pub const MODEL_DIR_ENV: &str = "XENGINEER_SHERPA_ZIPFORMER_MODEL_DIR";

const ENCODER_FILE: &str = "encoder-epoch-99-avg-1.int8.onnx";
const DECODER_FILE: &str = "decoder-epoch-99-avg-1.onnx";
const JOINER_FILE: &str = "joiner-epoch-99-avg-1.int8.onnx";
const TOKENS_FILE: &str = "tokens.txt";

/// Streaming Zipformer transducer 模型文件集合。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamingZipformerModel {
    root: PathBuf,
    encoder: PathBuf,
    decoder: PathBuf,
    joiner: PathBuf,
    tokens: PathBuf,
}

impl StreamingZipformerModel {
    /// 从 sherpa-onnx 官方 archive 解压后的模型目录构造路径集合。
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, AsrError> {
        let root = dir.as_ref().to_path_buf();
        let model = Self {
            encoder: root.join(ENCODER_FILE),
            decoder: root.join(DECODER_FILE),
            joiner: root.join(JOINER_FILE),
            tokens: root.join(TOKENS_FILE),
            root,
        };
        model.validate()?;
        Ok(model)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn encoder(&self) -> &Path {
        &self.encoder
    }

    pub fn decoder(&self) -> &Path {
        &self.decoder
    }

    pub fn joiner(&self) -> &Path {
        &self.joiner
    }

    pub fn tokens(&self) -> &Path {
        &self.tokens
    }

    /// 校验 sherpa-onnx 初始化前必须存在的模型文件。
    pub fn validate(&self) -> Result<(), AsrError> {
        for path in [&self.encoder, &self.decoder, &self.joiner, &self.tokens] {
            if !path.is_file() {
                return Err(AsrError::ModelNotFound(path.display().to_string()));
            }
        }
        Ok(())
    }
}

/// Streaming Zipformer recognizer 初始化参数。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamingZipformerOptions {
    pub sample_rate: i32,
    pub feature_dim: i32,
    pub num_threads: i32,
    pub provider: String,
    pub decoding_method: String,
    pub enable_endpoint: bool,
}

impl Default for StreamingZipformerOptions {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            feature_dim: 80,
            num_threads: 1,
            provider: "cpu".to_string(),
            decoding_method: "greedy_search".to_string(),
            enable_endpoint: true,
        }
    }
}

impl StreamingZipformerOptions {
    fn validate(&self) -> Result<(), AsrError> {
        if self.sample_rate <= 0 {
            return Err(AsrError::ModelLoad(
                "sample_rate must be positive".to_string(),
            ));
        }
        if self.feature_dim <= 0 {
            return Err(AsrError::ModelLoad(
                "feature_dim must be positive".to_string(),
            ));
        }
        if self.num_threads <= 0 {
            return Err(AsrError::ModelLoad(
                "num_threads must be positive".to_string(),
            ));
        }
        if self.provider.is_empty() {
            return Err(AsrError::ModelLoad(
                "provider must not be empty".to_string(),
            ));
        }
        if self.decoding_method.is_empty() {
            return Err(AsrError::ModelLoad(
                "decoding_method must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// 已初始化的 sherpa-onnx streaming Zipformer recognizer。
pub struct StreamingZipformer {
    model: StreamingZipformerModel,
    options: StreamingZipformerOptions,
    recognizer: OnlineRecognizer,
}

impl StreamingZipformer {
    /// 使用默认参数加载模型目录并初始化 recognizer。
    pub fn from_model_dir(dir: impl AsRef<Path>) -> Result<Self, AsrError> {
        let model = StreamingZipformerModel::from_dir(dir)?;
        Self::from_model(model, StreamingZipformerOptions::default())
    }

    /// 使用指定参数初始化 recognizer。
    pub fn from_model(
        model: StreamingZipformerModel,
        options: StreamingZipformerOptions,
    ) -> Result<Self, AsrError> {
        model.validate()?;
        options.validate()?;

        let mut config = OnlineRecognizerConfig::default();
        config.feat_config.sample_rate = options.sample_rate;
        config.feat_config.feature_dim = options.feature_dim;
        config.model_config.transducer.encoder = Some(path_to_string(model.encoder())?);
        config.model_config.transducer.decoder = Some(path_to_string(model.decoder())?);
        config.model_config.transducer.joiner = Some(path_to_string(model.joiner())?);
        config.model_config.tokens = Some(path_to_string(model.tokens())?);
        config.model_config.num_threads = options.num_threads;
        config.model_config.provider = Some(options.provider.clone());
        config.decoding_method = Some(options.decoding_method.clone());
        config.enable_endpoint = options.enable_endpoint;

        let recognizer = OnlineRecognizer::create(&config).ok_or_else(|| {
            AsrError::ModelLoad(format!(
                "sherpa-onnx failed to initialize streaming Zipformer from {}",
                model.root().display()
            ))
        })?;

        Ok(Self {
            model,
            options,
            recognizer,
        })
    }

    pub fn model(&self) -> &StreamingZipformerModel {
        &self.model
    }

    pub fn options(&self) -> &StreamingZipformerOptions {
        &self.options
    }

    /// 创建一个空的在线识别流。后续 PR 会在此基础上喂入 PCM chunk。
    pub fn create_stream(&self) -> OnlineStream {
        self.recognizer.create_stream()
    }

    fn transcribe_mono_f32(&self, samples: &[f32], sample_rate: i32) -> Result<String, AsrError> {
        let stream = self.create_stream();
        stream.accept_waveform(sample_rate, samples);
        stream.input_finished();

        let max_decode_steps = max_decode_steps(samples.len(), sample_rate);
        for _ in 0..max_decode_steps {
            if !self.recognizer.is_ready(&stream) {
                break;
            }
            self.recognizer.decode(&stream);
        }

        if self.recognizer.is_ready(&stream) {
            return Err(AsrError::Decode(format!(
                "decoder did not finish within {max_decode_steps} steps"
            )));
        }

        let result = self
            .recognizer
            .get_result(&stream)
            .ok_or_else(|| AsrError::Decode("missing recognizer result".to_string()))?;
        Ok(result.text.trim().to_string())
    }
}

impl AsrEngine for StreamingZipformer {
    fn transcribe(&self, pcm: &[i16], format: AudioFormat) -> Result<String, AsrError> {
        validate_audio(format, pcm)?;
        let samples = pcm_i16_to_mono_f32(pcm, format)?;
        self.transcribe_mono_f32(&samples, format.sample_rate as i32)
    }
}

fn validate_audio(format: AudioFormat, pcm: &[i16]) -> Result<(), AsrError> {
    if format.sample_rate == 0 {
        return Err(AsrError::UnsupportedFormat(
            "sample rate must be positive".to_string(),
        ));
    }
    if format.channels == 0 {
        return Err(AsrError::UnsupportedFormat(
            "channels must be positive".to_string(),
        ));
    }
    if pcm.len() % format.channels as usize != 0 {
        return Err(AsrError::UnsupportedFormat(format!(
            "{} samples is not divisible by {} channels",
            pcm.len(),
            format.channels
        )));
    }
    Ok(())
}

fn pcm_i16_to_mono_f32(pcm: &[i16], format: AudioFormat) -> Result<Vec<f32>, AsrError> {
    validate_audio(format, pcm)?;
    let channels = format.channels as usize;
    let mut out = Vec::with_capacity(pcm.len() / channels);
    for frame in pcm.chunks_exact(channels) {
        let sum: f32 = frame.iter().map(|&s| sample_i16_to_f32(s)).sum();
        out.push(sum / channels as f32);
    }
    Ok(out)
}

fn sample_i16_to_f32(sample: i16) -> f32 {
    sample as f32 / i16::MAX as f32
}

fn max_decode_steps(sample_count: usize, sample_rate: i32) -> usize {
    let sample_rate = sample_rate.max(1) as usize;
    let seconds = sample_count.div_ceil(sample_rate).max(1);
    seconds * 100
}

fn path_to_string(path: &Path) -> Result<String, AsrError> {
    path.to_str().map(ToOwned::to_owned).ok_or_else(|| {
        AsrError::ModelLoad(format!("model path is not valid UTF-8: {}", path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(path: &Path) {
        fs::write(path, []).unwrap();
    }

    fn write_model_files(dir: &Path) {
        touch(&dir.join(ENCODER_FILE));
        touch(&dir.join(DECODER_FILE));
        touch(&dir.join(JOINER_FILE));
        touch(&dir.join(TOKENS_FILE));
    }

    #[test]
    fn builds_expected_model_paths_from_dir() {
        let tmp = tempfile::tempdir().unwrap();
        write_model_files(tmp.path());

        let model = StreamingZipformerModel::from_dir(tmp.path()).unwrap();

        assert_eq!(model.encoder(), tmp.path().join(ENCODER_FILE));
        assert_eq!(model.decoder(), tmp.path().join(DECODER_FILE));
        assert_eq!(model.joiner(), tmp.path().join(JOINER_FILE));
        assert_eq!(model.tokens(), tmp.path().join(TOKENS_FILE));
    }

    #[test]
    fn reports_missing_required_model_file() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join(ENCODER_FILE));
        touch(&tmp.path().join(DECODER_FILE));
        touch(&tmp.path().join(TOKENS_FILE));

        let err = StreamingZipformerModel::from_dir(tmp.path()).unwrap_err();

        assert!(matches!(err, AsrError::ModelNotFound(_)));
        assert!(err.to_string().contains(JOINER_FILE));
    }

    #[test]
    fn rejects_invalid_options_before_calling_sherpa() {
        let tmp = tempfile::tempdir().unwrap();
        write_model_files(tmp.path());
        let model = StreamingZipformerModel::from_dir(tmp.path()).unwrap();
        let options = StreamingZipformerOptions {
            num_threads: 0,
            ..Default::default()
        };

        match StreamingZipformer::from_model(model, options) {
            Ok(_) => panic!("invalid options should fail before recognizer creation"),
            Err(err) => assert_eq!(
                err.to_string(),
                "model load failed: num_threads must be positive"
            ),
        }
    }

    #[test]
    fn rejects_zero_channels() {
        let err = pcm_i16_to_mono_f32(
            &[1, 2],
            AudioFormat {
                sample_rate: 16_000,
                channels: 0,
            },
        )
        .unwrap_err();

        assert_eq!(
            err.to_string(),
            "unsupported audio format: channels must be positive"
        );
    }

    #[test]
    fn rejects_incomplete_interleaved_frame() {
        let err = pcm_i16_to_mono_f32(
            &[1, 2, 3],
            AudioFormat {
                sample_rate: 16_000,
                channels: 2,
            },
        )
        .unwrap_err();

        assert_eq!(
            err.to_string(),
            "unsupported audio format: 3 samples is not divisible by 2 channels"
        );
    }

    #[test]
    fn converts_interleaved_i16_to_mono_f32() {
        let samples = pcm_i16_to_mono_f32(
            &[i16::MAX, 0, 0, i16::MAX],
            AudioFormat {
                sample_rate: 16_000,
                channels: 2,
            },
        )
        .unwrap();

        assert_eq!(samples, vec![0.5, 0.5]);
    }

    #[test]
    fn calculates_decode_step_budget_from_input_length() {
        assert_eq!(max_decode_steps(1, 16_000), 100);
        assert_eq!(max_decode_steps(16_000, 16_000), 100);
        assert_eq!(max_decode_steps(16_001, 16_000), 200);
    }

    #[test]
    fn loads_real_model_when_env_is_set() {
        let Ok(dir) = std::env::var(MODEL_DIR_ENV) else {
            return;
        };

        let recognizer = StreamingZipformer::from_model_dir(dir).unwrap();
        let _stream = recognizer.create_stream();
    }

    #[test]
    fn transcribes_real_model_test_wav_when_env_is_set() {
        let Ok(dir) = std::env::var(MODEL_DIR_ENV) else {
            return;
        };

        let wav = Path::new(&dir).join("test_wavs/0.wav");
        let wave = sherpa_onnx::Wave::read(&path_to_string(&wav).unwrap()).unwrap();
        let recognizer = StreamingZipformer::from_model_dir(dir).unwrap();

        let text = recognizer
            .transcribe_mono_f32(wave.samples(), wave.sample_rate())
            .unwrap();

        assert!(!text.is_empty());
    }
}
