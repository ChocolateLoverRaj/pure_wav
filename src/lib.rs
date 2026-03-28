//! # Usage
//! Create a [`Parser`] with [`Parser::default`].
//! Call [`Parser::read_instruction`] to get where in the file you need to read.
//! Read the file.
//! Call [`Parser::process_data`] with the data you read.
//! In the end you will get meta data about the format of the samples and the location of teh
//! samples themselves.
//!
//! # Handling untrusted data
//! This library may ask you to read data outside of the range of the file / whatever is storing
//! the WAVE. The meta data might point to a range outside of the file. It's your responsibility
//! to validate those ranges if you want to.
//!
//! # The WAVE format
//! See
//! - <https://en.wikipedia.org/wiki/WAV#File_specifications>
//! - <https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/WAVE.html>
//!
//! Basically, a WAVE file is formatted with RIFF. There is a `fmt ` chunk that contains
//! information such as the number of channels and bits per sample. There is a `data` chunk
//! that contains the actual audio samples. There can also be other chunks to describe things like
//! the artist and title of the song, but they are not needed for simply playing audio.
#![no_std]
pub use pure_riff;
use pure_riff::{
    BUFFER_LEN, Id, ParseChunkOutput, RiffChunkHeader, SUB_CHUNKS_OFFSET, parse_chunk,
};
pub use zerocopy;
use zerocopy::{
    FromBytes, Immutable, KnownLayout,
    little_endian::{U16, U32},
    transmute_ref,
};

#[derive(Debug, Clone, Copy, FromBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct FmtData {
    pub format_tag: U16,
    pub n_channels: U16,
    pub n_samples_per_sec: U32,
    pub n_avg_bytes_per_sec: U32,
    pub n_block_align: U16,
    pub w_bits_per_sample: U16,
    // The optional 20-byte version of this contains these fields
    // pub cb_size: U16,
    // pub w_valid_bits_per_sample: U16,
    // The optional 40-byte version of this has these fields in addition to the 20-byte fields
    // pub dw_channel_mask: [u8; 4],
    // pub sub_format: [u8; 16],
}

enum ParseStage {
    Riff,
    Fmt {
        sub_chunks_len: u32,
        position_in_sub_chunks: u32,
    },
    Data {
        sub_chunks_len: u32,
        position_in_sub_chunks: u32,
        fmt_data: FmtData,
    },
}

pub struct Parser {
    stage: ParseStage,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            stage: ParseStage::Riff,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// Expected chunk id: "RIFF". Contains actual chunk id.
    UnexpectedChunkId(Id),
    /// Invalid RIFF format
    InvalidRiff,
    /// Expected container id: "WAVE". Contains actual container id.
    UnexpectedContainerId(Id),
    FmtDataTooSmall(u32),
}

#[derive(Debug)]
pub struct ReadInstruction {
    pub position: u32,
    pub len: u32,
}

#[derive(Debug)]
pub struct WavMetaData {
    pub fmt: FmtData,
    pub data_position: u32,
    pub data_len: u32,
}

pub enum ProcessDataOutput {
    Done(WavMetaData),
    InProgress(Parser),
}

impl Parser {
    pub const MAX_BUFFER_LEN: usize = size_of::<RiffChunkHeader>() + size_of::<FmtData>();

    pub fn read_instruction(&self) -> ReadInstruction {
        match &self.stage {
            ParseStage::Riff => ReadInstruction {
                position: 0,
                len: size_of::<RiffChunkHeader>().try_into().unwrap(),
            },
            ParseStage::Fmt {
                sub_chunks_len: _sub_chunks_len,
                position_in_sub_chunks,
            } => ReadInstruction {
                position: SUB_CHUNKS_OFFSET + position_in_sub_chunks,
                len: (size_of::<RiffChunkHeader>() + size_of::<FmtData>())
                    .try_into()
                    .unwrap(),
            },
            ParseStage::Data {
                sub_chunks_len: _sub_chunks_len,
                position_in_sub_chunks,
                fmt_data: _fmt_data,
            } => ReadInstruction {
                position: SUB_CHUNKS_OFFSET + position_in_sub_chunks,
                len: size_of::<RiffChunkHeader>().try_into().unwrap(),
            },
        }
    }

    pub fn process_data(self, data: &[u8]) -> Result<ProcessDataOutput, Error> {
        match self.stage {
            ParseStage::Riff => {
                let data = <&[u8; size_of::<RiffChunkHeader>()]>::try_from(data).unwrap();
                let riff_chunk: &RiffChunkHeader = transmute_ref!(data);
                if &riff_chunk.chunk_id != b"RIFF" {
                    return Err(Error::UnexpectedChunkId(riff_chunk.chunk_id));
                }
                let sub_chunks_len = riff_chunk
                    .container_info()
                    .unwrap()
                    .map_err(|_| Error::InvalidRiff)?
                    .sub_chunks_len;
                Ok(ProcessDataOutput::InProgress(Self {
                    stage: ParseStage::Fmt {
                        sub_chunks_len,
                        position_in_sub_chunks: 0,
                    },
                }))
            }
            ParseStage::Fmt {
                sub_chunks_len,
                position_in_sub_chunks,
            } => {
                let ParseChunkOutput {
                    parsed_chunk,
                    next_chunk_relative_position,
                } = parse_chunk(data[..BUFFER_LEN].try_into().unwrap());
                if &parsed_chunk.chunk_id == b"fmt " {
                    let fmt_data_len = parsed_chunk.chunk_len.get();
                    if fmt_data_len < size_of::<FmtData>().try_into().unwrap() {
                        return Err(Error::FmtDataTooSmall(fmt_data_len));
                    }
                    let data = <&[u8; size_of::<FmtData>()]>::try_from(
                        &data[size_of::<RiffChunkHeader>()..],
                    )
                    .unwrap();
                    Ok(ProcessDataOutput::InProgress(Self {
                        stage: ParseStage::Data {
                            sub_chunks_len,
                            position_in_sub_chunks: position_in_sub_chunks
                                + next_chunk_relative_position,
                            fmt_data: *transmute_ref!(data),
                        },
                    }))
                } else {
                    Ok(ProcessDataOutput::InProgress(Self {
                        stage: ParseStage::Fmt {
                            sub_chunks_len,
                            position_in_sub_chunks: position_in_sub_chunks
                                + next_chunk_relative_position,
                        },
                    }))
                }
            }
            ParseStage::Data {
                sub_chunks_len,
                position_in_sub_chunks,
                fmt_data,
            } => {
                let ParseChunkOutput {
                    parsed_chunk,
                    next_chunk_relative_position,
                } = parse_chunk(data.try_into().unwrap());
                if &parsed_chunk.chunk_id == b"data" {
                    Ok(ProcessDataOutput::Done(WavMetaData {
                        fmt: fmt_data,
                        data_position: SUB_CHUNKS_OFFSET
                            + position_in_sub_chunks
                            + u32::try_from(size_of::<RiffChunkHeader>()).unwrap(),
                        data_len: parsed_chunk.chunk_len.get(),
                    }))
                } else {
                    Ok(ProcessDataOutput::InProgress(Self {
                        stage: ParseStage::Data {
                            sub_chunks_len,
                            position_in_sub_chunks: position_in_sub_chunks
                                + next_chunk_relative_position,
                            fmt_data,
                        },
                    }))
                }
            }
        }
    }
}
