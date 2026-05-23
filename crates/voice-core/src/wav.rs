//! WAV 文件读写。
//!
//! 仅支持 PCM 16-bit 格式（与 `capture::PcmSample` 对齐）。
//! 自手写 RIFF/WAVE 头，不引入 `hound`/`wav` 等额外依赖。

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use crate::capture::{AudioFormat, PcmSample};

#[derive(Debug, thiserror::Error)]
pub enum WavError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sample count {samples} exceeds 32-bit RIFF limit")]
    TooLarge { samples: u64 },
    #[error("not a RIFF/WAVE file")]
    NotRiffWave,
    #[error("unsupported format: {0}")]
    Unsupported(String),
    #[error("missing chunk: {0}")]
    MissingChunk(&'static str),
}

/// 把交错存储的 PCM 16-bit 样本写为 WAV 文件。
///
/// `samples` 长度需为 `format.channels` 的整数倍。
pub fn write_pcm16_wav(
    path: impl AsRef<Path>,
    format: AudioFormat,
    samples: &[PcmSample],
) -> Result<(), WavError> {
    let file = File::create(path)?;
    let mut w = BufWriter::new(file);
    write_pcm16_wav_to(&mut w, format, samples)?;
    w.flush()?;
    Ok(())
}

/// 写入到任意 `Write`，便于单元测试和未来流式写入。
pub fn write_pcm16_wav_to<W: Write>(
    w: &mut W,
    format: AudioFormat,
    samples: &[PcmSample],
) -> Result<(), WavError> {
    const BITS_PER_SAMPLE: u16 = 16;
    const BYTES_PER_SAMPLE: u32 = 2;
    const FMT_CHUNK_SIZE: u32 = 16;

    let n_samples = samples.len() as u64;
    let data_bytes_u64 = n_samples
        .checked_mul(BYTES_PER_SAMPLE as u64)
        .ok_or(WavError::TooLarge { samples: n_samples })?;
    if data_bytes_u64 > u32::MAX as u64 - 36 {
        return Err(WavError::TooLarge { samples: n_samples });
    }
    let data_bytes = data_bytes_u64 as u32;

    let byte_rate = format.sample_rate * format.channels as u32 * BYTES_PER_SAMPLE;
    let block_align = format.channels * BITS_PER_SAMPLE / 8;
    let riff_size = 36 + data_bytes;

    // RIFF header
    w.write_all(b"RIFF")?;
    w.write_all(&riff_size.to_le_bytes())?;
    w.write_all(b"WAVE")?;

    // fmt chunk
    w.write_all(b"fmt ")?;
    w.write_all(&FMT_CHUNK_SIZE.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&format.channels.to_le_bytes())?;
    w.write_all(&format.sample_rate.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;

    // data chunk
    w.write_all(b"data")?;
    w.write_all(&data_bytes.to_le_bytes())?;
    for &s in samples {
        w.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

/// 读取整个 PCM 16-bit WAV 文件。
///
/// 返回交错存储的 i16 样本与音频参数。仅接受 PCM (format tag = 1)，
/// 16 bit/sample；其他格式返回 `WavError::Unsupported`。
pub fn read_pcm16_wav(path: impl AsRef<Path>) -> Result<(AudioFormat, Vec<PcmSample>), WavError> {
    let file = File::open(path)?;
    let mut r = BufReader::new(file);
    read_pcm16_wav_from(&mut r)
}

/// 从任意 `Read` 读取 PCM 16-bit WAV，便于单测。
pub fn read_pcm16_wav_from<R: Read>(r: &mut R) -> Result<(AudioFormat, Vec<PcmSample>), WavError> {
    let mut riff = [0u8; 12];
    r.read_exact(&mut riff)?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err(WavError::NotRiffWave);
    }

    let mut fmt: Option<(AudioFormat, u16, u16)> = None; // (format, fmt_tag, bits)
    let mut data: Option<Vec<u8>> = None;

    loop {
        let mut header = [0u8; 8];
        match r.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        let id = [header[0], header[1], header[2], header[3]];
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;

        match &id {
            b"fmt " => {
                if size < 16 {
                    return Err(WavError::Unsupported(format!("fmt chunk size {size}")));
                }
                let mut buf = vec![0u8; size];
                r.read_exact(&mut buf)?;
                let fmt_tag = u16::from_le_bytes([buf[0], buf[1]]);
                let channels = u16::from_le_bytes([buf[2], buf[3]]);
                let sample_rate = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let bits = u16::from_le_bytes([buf[14], buf[15]]);
                fmt = Some((
                    AudioFormat {
                        sample_rate,
                        channels,
                    },
                    fmt_tag,
                    bits,
                ));
                if size % 2 == 1 {
                    let mut pad = [0u8; 1];
                    r.read_exact(&mut pad)?;
                }
            }
            b"data" => {
                let mut buf = vec![0u8; size];
                r.read_exact(&mut buf)?;
                data = Some(buf);
                if size % 2 == 1 {
                    let mut pad = [0u8; 1];
                    let _ = r.read_exact(&mut pad);
                }
                break;
            }
            _ => {
                let mut skip = vec![0u8; size + (size % 2)];
                r.read_exact(&mut skip)?;
            }
        }
    }

    let (format, fmt_tag, bits) = fmt.ok_or(WavError::MissingChunk("fmt "))?;
    let bytes = data.ok_or(WavError::MissingChunk("data"))?;
    if fmt_tag != 1 {
        return Err(WavError::Unsupported(format!("format tag {fmt_tag}")));
    }
    if bits != 16 {
        return Err(WavError::Unsupported(format!("{bits} bits per sample")));
    }
    if bytes.len() % 2 != 0 {
        return Err(WavError::Unsupported("odd data byte count".into()));
    }

    let samples: Vec<i16> = bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    Ok((format, samples))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_layout_for_mono_16k_is_44_bytes_plus_data() {
        let fmt = AudioFormat {
            sample_rate: 16_000,
            channels: 1,
        };
        let samples: Vec<i16> = vec![0, 1, -1, i16::MAX, i16::MIN];
        let mut buf = Vec::new();
        write_pcm16_wav_to(&mut buf, fmt, &samples).unwrap();

        assert_eq!(&buf[0..4], b"RIFF");
        assert_eq!(&buf[8..12], b"WAVE");
        assert_eq!(&buf[12..16], b"fmt ");
        assert_eq!(&buf[36..40], b"data");

        // riff_size = 36 + data_bytes; data_bytes = 5 samples * 2 bytes = 10
        let riff_size = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        assert_eq!(riff_size, 36 + 10);

        let data_bytes = u32::from_le_bytes(buf[40..44].try_into().unwrap());
        assert_eq!(data_bytes, 10);

        // sample rate
        let sr = u32::from_le_bytes(buf[24..28].try_into().unwrap());
        assert_eq!(sr, 16_000);

        // total length
        assert_eq!(buf.len(), 44 + 10);
    }

    #[test]
    fn stereo_byte_rate_and_block_align_are_correct() {
        let fmt = AudioFormat {
            sample_rate: 48_000,
            channels: 2,
        };
        let mut buf = Vec::new();
        write_pcm16_wav_to(&mut buf, fmt, &[]).unwrap();

        let byte_rate = u32::from_le_bytes(buf[28..32].try_into().unwrap());
        assert_eq!(byte_rate, 48_000 * 2 * 2);

        let block_align = u16::from_le_bytes(buf[32..34].try_into().unwrap());
        assert_eq!(block_align, 4);
    }

    #[test]
    fn round_trip_preserves_format_and_samples() {
        let fmt = AudioFormat {
            sample_rate: 16_000,
            channels: 1,
        };
        let samples: Vec<i16> = (0..1024)
            .map(|i| ((i * 37) as i16).wrapping_mul(11))
            .collect();
        let mut buf = Vec::new();
        write_pcm16_wav_to(&mut buf, fmt, &samples).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let (fmt2, samples2) = read_pcm16_wav_from(&mut cursor).unwrap();
        assert_eq!(fmt2, fmt);
        assert_eq!(samples2, samples);
    }

    #[test]
    fn skips_unknown_chunks_before_data() {
        let fmt = AudioFormat {
            sample_rate: 8_000,
            channels: 1,
        };
        let samples: Vec<i16> = vec![1, 2, 3, 4];

        // 手动构造：RIFF + WAVE + fmt + LIST(unknown) + data
        let mut wav = Vec::new();
        let data_bytes = (samples.len() * 2) as u32;
        let list_payload: &[u8] = b"INFOICMT\x06\x00\x00\x00hello\x00";
        let list_size = list_payload.len() as u32;
        let riff_size = 4 + (8 + 16) + (8 + list_size) + (8 + data_bytes);

        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&riff_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&fmt.channels.to_le_bytes());
        wav.extend_from_slice(&fmt.sample_rate.to_le_bytes());
        wav.extend_from_slice(&(fmt.sample_rate * fmt.channels as u32 * 2).to_le_bytes());
        wav.extend_from_slice(&(fmt.channels * 2).to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"LIST");
        wav.extend_from_slice(&list_size.to_le_bytes());
        wav.extend_from_slice(list_payload);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_bytes.to_le_bytes());
        for s in &samples {
            wav.extend_from_slice(&s.to_le_bytes());
        }

        let mut cursor = std::io::Cursor::new(wav);
        let (fmt2, samples2) = read_pcm16_wav_from(&mut cursor).unwrap();
        assert_eq!(fmt2, fmt);
        assert_eq!(samples2, samples);
    }

    #[test]
    fn rejects_non_pcm_format() {
        // fmt_tag = 3 (IEEE float)
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&36u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&3u16.to_le_bytes()); // tag
        wav.extend_from_slice(&1u16.to_le_bytes()); // channels
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&64_000u32.to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&32u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&0u32.to_le_bytes());

        let mut cursor = std::io::Cursor::new(wav);
        assert!(matches!(
            read_pcm16_wav_from(&mut cursor),
            Err(WavError::Unsupported(_))
        ));
    }

    #[test]
    fn rejects_non_riff_file() {
        let mut cursor = std::io::Cursor::new(b"NOPE_NOT_A_WAV_FILE_____".to_vec());
        assert!(matches!(
            read_pcm16_wav_from(&mut cursor),
            Err(WavError::NotRiffWave)
        ));
    }
}
