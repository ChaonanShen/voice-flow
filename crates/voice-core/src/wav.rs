//! WAV 文件写入。
//!
//! 仅支持 PCM 16-bit 格式（与 `capture::PcmSample` 对齐）。
//! 自手写 RIFF/WAVE 头，不引入 `hound`/`wav` 等额外依赖。

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::capture::{AudioFormat, PcmSample};

#[derive(Debug, thiserror::Error)]
pub enum WavError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sample count {samples} exceeds 32-bit RIFF limit")]
    TooLarge { samples: u64 },
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
}
