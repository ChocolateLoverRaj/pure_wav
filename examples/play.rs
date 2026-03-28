use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    time::Duration,
};

use pure_wav::{Parser, ProcessDataOutput, ReadInstruction, WavMetaData};
use rodio::{Player, Sample, Source};

fn main() {
    let mut file =
        File::open("Warriyo, Laura Brehm - Mortals (feat. Laura Brehm) [NCS Release].wav").unwrap();
    let meta_data = {
        let mut parser = Parser::default();
        let mut buffer = [Default::default(); Parser::MAX_BUFFER_LEN];
        loop {
            let ReadInstruction { position, len } = parser.read_instruction();
            file.seek(SeekFrom::Start(position.into())).unwrap();
            let buffer = &mut buffer[..len.try_into().unwrap()];
            file.read_exact(buffer).unwrap();
            match parser.process_data(buffer).unwrap() {
                ProcessDataOutput::InProgress(next_parser) => {
                    parser = next_parser;
                }
                ProcessDataOutput::Done(meta_data) => {
                    break meta_data;
                }
            }
        }
    };
    println!("{meta_data:#?}");
    let source = WavSource::new(file, meta_data);
    let sink = rodio::DeviceSinkBuilder::open_default_sink().unwrap();
    let player = Player::connect_new(sink.mixer());
    player.append(source);
    player.sleep_until_end();
    std::thread::sleep(Duration::from_secs(5));
}

struct WavSource {
    file: File,
    meta_data: WavMetaData,
    buffer: [u8; 2],
    position: u32,
}

impl WavSource {
    pub fn new(mut file: File, meta_data: WavMetaData) -> Self {
        file.seek(SeekFrom::Start(meta_data.data_position.into()))
            .unwrap();

        Self {
            file,
            meta_data,
            buffer: Default::default(),
            position: 0,
        }
    }
}

impl Source for WavSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.meta_data.fmt.n_channels.get().try_into().unwrap()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.meta_data
            .fmt
            .n_samples_per_sec
            .get()
            .try_into()
            .unwrap()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        let n_samples =
            self.meta_data.data_len as f64 / self.meta_data.fmt.w_bits_per_sample.get() as f64;
        let secs_per_sample = 1.0 / self.meta_data.fmt.n_samples_per_sec.get() as f64;
        Some(Duration::from_secs_f64(n_samples * secs_per_sample))
    }
}

impl Iterator for WavSource {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position == self.meta_data.data_len {
            return None;
        }
        self.file.read_exact(&mut self.buffer).unwrap();
        let sample = match self.meta_data.fmt.w_bits_per_sample.get() {
            16 => i16::from_le_bytes(self.buffer) as f64 / i16::MAX as f64,
            _ => todo!(),
        };
        self.position += 2;
        Some(sample)
    }
}
