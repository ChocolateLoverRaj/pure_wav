#![no_std]
use zerocopy::{
    FromBytes, Immutable, KnownLayout,
    little_endian::{U16, U32},
    transmute_ref,
};

#[derive(Debug, Clone, Copy, FromBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct Header {
    pub chunk_id: [u8; 4],
    chunk_size: U32,
}

impl Header {
    pub fn chunk_size(&self) -> u32 {
        self.chunk_size.get()
    }
}

// #[derive(Debug, Clone, Copy, FromBytes, Immutable, KnownLayout)]
// #[repr(C)]
// pub struct RiffData {
//     pub wave_id: [u8; 4],
// }

pub type WaveId = [u8; 4];

#[derive(Debug, Clone, Copy, FromBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct FmtData {
    pub format_tag: U16,
    pub n_channels: U16,
    pub n_samples_per_sec: U32,
    pub n_avg_bytes_per_sec: U32,
    pub n_block_align: U16,
    pub w_bits_per_sample: U16,
    pub cb_size: U16,
    pub w_valid_bits_per_sample: U16,
    pub dw_channel_mask: [u8; 4],
    pub sub_format: [u8; 16],
}

#[derive(Debug)]
pub enum ParseTopHeaderError {
    /// Expected chunk id: "RIFF". Contains actual chunk id.
    UnexpectedChunkId([u8; 4]),
    /// Expected wave id: "WAVE". Contains actual wave id.
    UnexpectedWaveId(WaveId),
}

#[derive(Debug, Clone, Copy, FromBytes, Immutable, KnownLayout)]
#[repr(C)]
struct RiffHeaderWithWaveId {
    header: Header,
    wave_id: WaveId,
}

/// Read 12 bytes from the start of the file
pub fn parse_top_header(
    bytes: &[u8; size_of::<RiffHeaderWithWaveId>()],
) -> Result<WaveFile, ParseTopHeaderError> {
    let RiffHeaderWithWaveId { header, wave_id }: &RiffHeaderWithWaveId = transmute_ref!(bytes);
    if &header.chunk_id == b"RIFF" {
        if wave_id == b"WAVE" {
            Ok(WaveFile {
                chunk_size: header.chunk_size.get(),
            })
        } else {
            Err(ParseTopHeaderError::UnexpectedWaveId(*wave_id))
        }
    } else {
        Err(ParseTopHeaderError::UnexpectedChunkId(header.chunk_id))
    }
}

#[derive(Debug)]
pub struct WaveFile {
    chunk_size: u32,
}

#[derive(Debug)]
pub enum GetChunkHeaderAddressError {
    /// There should be one more chunk, but it wouldn't fit
    IncompleteChunkHeader,
}

pub struct ChunkInfo {
    /// Address within the data of the top chunk
    address: u32,
    size: u32,
}

impl WaveFile {
    /// Get the address (within the **file**) of the chunk info for the next chunk.
    /// If `None` is returned, that means there is no next chunk.
    pub fn get_chunk_header_address(
        &self,
        previous_chunk: Option<ChunkInfo>,
    ) -> Result<Option<u32>, GetChunkHeaderAddressError> {
        let next_chunk_address = if let Some(previous_chunk) = previous_chunk {
            previous_chunk.address + size_of::<Header>() as u32 + previous_chunk.size
        } else {
            0
        };
        let remaining_data = self.chunk_size - size_of::<WaveId>() as u32 - next_chunk_address;
        if remaining_data == 0 {
            Ok(None)
        } else if remaining_data > size_of::<Header>() as u32 {
            Ok(Some(
                size_of::<RiffHeaderWithWaveId>() as u32 + next_chunk_address,
            ))
        } else {
            Err(GetChunkHeaderAddressError::IncompleteChunkHeader)
        }
    }

    pub fn get_chunk_info(address: u32, header: &Header) -> ChunkInfo {
        ChunkInfo {
            address: address - size_of::<RiffHeaderWithWaveId>() as u32,
            size: header.chunk_size(),
        }
    }
}

pub trait StateMachine {
    type Input<'a>;
    type Output;

    fn output(&self) -> Self::Output;
    fn input(&mut self, input: Self::Input<'_>);
}

#[derive(Debug, Clone, Copy)]
pub struct MetaDataForI2s {
    pub fmt_data: FmtData,
    pub data_address: u32,
    pub data_size: u32,
}

#[derive(Debug)]
pub struct ReadRequest {
    pub address: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum GetMetaDataForI2sError {
    /// Expected chunk id: "RIFF". Contains actual chunk id.
    UnexpectedChunkId([u8; 4]),
    /// Expected wave id: "WAVE". Contains actual wave id.
    UnexpectedWaveId(WaveId),
    /// There should be one more chunk, but it wouldn't fit
    IncompleteChunkHeader,
    /// The file did not contain both the `fmt` and `data` chunks, which are needed
    MissingChunks,
}

#[derive(Debug)]
pub enum GetMetaDataForI2sOutput {
    Done(Result<MetaDataForI2s, GetMetaDataForI2sError>),
    Read(ReadRequest),
}

#[derive(Debug)]
enum ReadFmtState {
    ScanningForChunk,
    /// We just detected that the current chunk is a `fmt` chunk
    ReadCurrentChunk {
        current_chunk_size: u32,
    },
    ReadFmtData(FmtData),
}

enum GetMetaDataForI2sState {
    ParseTopHeader,
    GetChunks {
        top_header_size: u32,
        current_address_within_data: u32,
        fmt: ReadFmtState,
        /// address within data, size
        data: Option<(u32, u32)>,
    },
    Done(Result<MetaDataForI2s, GetMetaDataForI2sError>),
}

/// - Make sure the file is a wave file.
/// - Get the `fmt` chunk, which tells you information like the samples per second
/// - Get the address and length of the actual `data` chunk
pub struct GetMetaDataForI2s {
    state: GetMetaDataForI2sState,
}

impl GetMetaDataForI2s {
    /// Helps you pre-allocate memory for a read request
    pub const MAX_READ_LEN: usize = size_of::<FmtData>();

    pub fn new() -> Self {
        Self {
            state: GetMetaDataForI2sState::ParseTopHeader,
        }
    }
}

impl StateMachine for GetMetaDataForI2s {
    type Output = GetMetaDataForI2sOutput;
    type Input<'a> = &'a [u8];

    fn output(&self) -> Self::Output {
        match &self.state {
            GetMetaDataForI2sState::ParseTopHeader => GetMetaDataForI2sOutput::Read(ReadRequest {
                address: 0,
                size: size_of::<RiffHeaderWithWaveId>() as u32,
            }),
            GetMetaDataForI2sState::GetChunks {
                top_header_size: _,
                current_address_within_data,
                fmt,
                data: _,
            } => GetMetaDataForI2sOutput::Read(match fmt {
                ReadFmtState::ReadCurrentChunk {
                    current_chunk_size: _,
                } => ReadRequest {
                    address: size_of::<RiffHeaderWithWaveId>() as u32 + size_of::<Header>() as u32,
                    size: size_of::<FmtData>() as u32,
                },
                _ => ReadRequest {
                    address: size_of::<RiffHeaderWithWaveId>() as u32
                        + *current_address_within_data,
                    size: size_of::<Header>() as u32,
                },
            }),
            GetMetaDataForI2sState::Done(result) => GetMetaDataForI2sOutput::Done(*result),
        }
    }

    fn input(&mut self, input: Self::Input<'_>) {
        match &mut self.state {
            GetMetaDataForI2sState::ParseTopHeader => {
                let RiffHeaderWithWaveId { header, wave_id }: &RiffHeaderWithWaveId = transmute_ref!(
                    <&[u8; size_of::<RiffHeaderWithWaveId>()]>::try_from(input).unwrap()
                );
                if &header.chunk_id == b"RIFF" {
                    if wave_id == b"WAVE" {
                        self.state = GetMetaDataForI2sState::GetChunks {
                            top_header_size: header.chunk_size.get(),
                            current_address_within_data: 0,
                            fmt: ReadFmtState::ScanningForChunk,
                            data: None,
                        };
                    } else {
                        self.state = GetMetaDataForI2sState::Done(Err(
                            GetMetaDataForI2sError::UnexpectedWaveId(*wave_id),
                        ));
                    }
                } else {
                    self.state = GetMetaDataForI2sState::Done(Err(
                        GetMetaDataForI2sError::UnexpectedChunkId(header.chunk_id),
                    ));
                }
            }
            GetMetaDataForI2sState::GetChunks {
                top_header_size: top_chunk_size,
                current_address_within_data,
                fmt,
                data,
            } => {
                let advance_chunk = match fmt {
                    ReadFmtState::ReadCurrentChunk { current_chunk_size } => {
                        let current_chunk_size = *current_chunk_size;
                        *fmt = ReadFmtState::ReadFmtData(*transmute_ref!(
                            <&[u8; size_of::<FmtData>()]>::try_from(input).unwrap()
                        ));
                        Some(current_chunk_size)
                    }
                    _ => {
                        let header: &Header =
                            transmute_ref!(<&[u8; size_of::<Header>()]>::try_from(input).unwrap());
                        match &header.chunk_id {
                            b"fmt " => {
                                *fmt = ReadFmtState::ReadCurrentChunk {
                                    current_chunk_size: header.chunk_size.get(),
                                };
                                None
                            }
                            b"data" => {
                                *data =
                                    Some((*current_address_within_data, header.chunk_size.get()));
                                Some(header.chunk_size.get())
                            }
                            _ => Some(header.chunk_size.get()),
                        }
                    }
                };
                if let ReadFmtState::ReadFmtData(fmt_data) = fmt
                    && let Some((data_chunk_address, data_size)) = data
                {
                    self.state = GetMetaDataForI2sState::Done(Ok(MetaDataForI2s {
                        fmt_data: *fmt_data,
                        data_address: size_of::<RiffHeaderWithWaveId>() as u32
                            + *data_chunk_address
                            + size_of::<Header>() as u32,
                        data_size: *data_size,
                    }))
                } else if let Some(current_chunk_size) = advance_chunk {
                    let remaining_data =
                        *top_chunk_size - size_of::<WaveId>() as u32 - *current_address_within_data;
                    if remaining_data == 0 {
                        self.state = GetMetaDataForI2sState::Done(Err(
                            GetMetaDataForI2sError::MissingChunks,
                        ));
                    } else if remaining_data > size_of::<Header>() as u32 {
                        *current_address_within_data +=
                            size_of::<Header>() as u32 + current_chunk_size;
                    } else {
                        self.state = GetMetaDataForI2sState::Done(Err(
                            GetMetaDataForI2sError::IncompleteChunkHeader,
                        ));
                    }
                }
            }
            GetMetaDataForI2sState::Done(_) => unreachable!(),
        }
    }
}
