use anyhow::{Context, Result, bail, ensure};
use cpal::{
    Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
};

const WAV_HEADER_BYTES: usize = 68;
const MAX_WAV_BYTES: usize = 25 << 20;

#[derive(Default)]
struct Buffer {
    samples: Vec<i16>,
    error: Option<String>,
}

pub(super) struct Microphone {
    stream: Stream,
    buffer: Arc<Mutex<Buffer>>,
    config: StreamConfig,
    limit: usize,
}

impl Microphone {
    pub(super) fn start() -> Result<Self> {
        let device = cpal::default_host()
            .default_input_device()
            .context("Nessun microfono disponibile")?;
        let supported = device
            .default_input_config()
            .context("Impossibile configurare il microfono")?;
        let config = supported.config();
        let channels = usize::from(config.channels);
        ensure!(
            channels > 0 && config.sample_rate.0 > 0,
            "Configurazione audio invalida"
        );
        let limit = ((MAX_WAV_BYTES - WAV_HEADER_BYTES) / size_of::<i16>() / channels) * channels;
        let buffer = Arc::new(Mutex::new(Buffer::default()));
        let stream = match supported.sample_format() {
            SampleFormat::I8 => input_stream::<i8>(&device, &config, &buffer, limit),
            SampleFormat::I16 => input_stream::<i16>(&device, &config, &buffer, limit),
            SampleFormat::I24 => input_stream::<cpal::I24>(&device, &config, &buffer, limit),
            SampleFormat::I32 => input_stream::<i32>(&device, &config, &buffer, limit),
            SampleFormat::I64 => input_stream::<i64>(&device, &config, &buffer, limit),
            SampleFormat::U8 => input_stream::<u8>(&device, &config, &buffer, limit),
            SampleFormat::U16 => input_stream::<u16>(&device, &config, &buffer, limit),
            SampleFormat::U32 => input_stream::<u32>(&device, &config, &buffer, limit),
            SampleFormat::U64 => input_stream::<u64>(&device, &config, &buffer, limit),
            SampleFormat::F32 => input_stream::<f32>(&device, &config, &buffer, limit),
            SampleFormat::F64 => input_stream::<f64>(&device, &config, &buffer, limit),
            format => bail!("Formato del microfono non supportato: {format:?}"),
        }
        .context("Impossibile aprire il microfono. Verifica i permessi di registrazione")?;
        stream.play().context("Impossibile avviare il microfono")?;
        Ok(Self {
            stream,
            buffer,
            config,
            limit,
        })
    }

    pub(super) fn is_full(&self) -> Result<bool> {
        let buffer = self
            .buffer
            .lock()
            .map_err(|_| anyhow::anyhow!("Errore nel buffer audio"))?;
        if let Some(error) = &buffer.error {
            bail!("Errore del microfono: {error}");
        }
        Ok(buffer.samples.len() == self.limit)
    }

    pub(super) fn finish(self) -> Result<(Vec<u8>, i64)> {
        drop(self.stream);
        let mut buffer = self
            .buffer
            .lock()
            .map_err(|_| anyhow::anyhow!("Errore nel buffer audio"))?;
        if let Some(error) = buffer.error.take() {
            bail!("Errore del microfono: {error}");
        }
        let samples = std::mem::take(&mut buffer.samples);
        encode_wav(&samples, self.config.channels, self.config.sample_rate.0)
    }
}

fn input_stream<T>(
    device: &Device,
    config: &StreamConfig,
    buffer: &Arc<Mutex<Buffer>>,
    limit: usize,
) -> std::result::Result<Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    i16: FromSample<T>,
{
    let samples = Arc::clone(buffer);
    let errors = Arc::clone(buffer);
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            if let Ok(mut buffer) = samples.lock() {
                let remaining = limit.saturating_sub(buffer.samples.len());
                buffer
                    .samples
                    .extend(data.iter().take(remaining).copied().map(i16::from_sample_));
            }
        },
        move |error| {
            if let Ok(mut buffer) = errors.lock() {
                buffer.error = Some(error.to_string());
            }
        },
        None,
    )
}

fn encode_wav(samples: &[i16], channels: u16, rate: u32) -> Result<(Vec<u8>, i64)> {
    let frames = samples.len() / usize::from(channels);
    ensure!(frames > 0, "Nessun audio ricevuto dal microfono");
    let mut output = Cursor::new(Vec::with_capacity(
        WAV_HEADER_BYTES + std::mem::size_of_val(samples),
    ));
    let mut wav = hound::WavWriter::new(
        &mut output,
        hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    for &sample in &samples[..frames * usize::from(channels)] {
        wav.write_sample(sample)?;
    }
    wav.finalize()?;
    let duration_ms = frames as i64 * 1000 / i64::from(rate);
    Ok((output.into_inner(), duration_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_preserves_pcm_and_reports_duration_by_frame() -> Result<()> {
        let samples = [100, -100, 200, -200, 300, -300, 400];
        let (bytes, duration) = encode_wav(&samples, 2, 1000)?;
        let mut wav = hound::WavReader::new(Cursor::new(bytes))?;
        assert_eq!(wav.spec().channels, 2);
        assert_eq!(wav.spec().sample_rate, 1000);
        assert_eq!(duration, 3);
        assert_eq!(
            wav.samples::<i16>().collect::<hound::Result<Vec<_>>>()?,
            samples[..6]
        );
        assert!(encode_wav(&[], 1, 1000).is_err());
        Ok(())
    }
}
