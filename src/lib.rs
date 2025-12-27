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
