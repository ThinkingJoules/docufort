/*!
# Docufort
This is an append only file format with built in error correction and recovery.

This allows for recovery and consistency of partially written data (due to power loss, improper shutdown, etc).


## Features
- **ECC**: Error Correction Codes are used to correct errors in the data.
- **Compression**: Data can be compressed before being written.
- **Recovery**: If a block is corrupt, it can be recovered if the ECC data is intact.
- **Integrity**: The file format has a hash of each block to ensure data integrity.

The error correction is used as both a checksum and self-healing corruption protection in the header portions of the file, and is optional for content stored.
The default allows for 2 errors every 251 bytes of data. Set the proper feature to change this.

This library provides a trait that handles all the hashing, compression and decompression for the implementer, making it transparent for usage.

## File Format
The file format is roughly as follows:
- **Magic Number**: 8 bytes, `docufort`
- **Version**: 2 bytes, `V1`
- **ECC Length**: 1 byte, the length of the ECC data used in the file.
- **Block**[]: A block is a set of headers and content.
    - **Header**: A header is a timestamp and a type byte.
    - **Content**: The content of a block.
    - **Hash**: A hash of the block.

## Toolbox
This library is more of a toolbox, and requires proper wrapping to be useful.
The purpose of exposing everything is to allow others to implement their own strategies per the spec.
This library is sort of a reference implementation for the spec.


*/






use reed_solomon::DecoderError;
//use write::{WriteError, FILE_HEADER_LEN};
use crate::core::BlockInputs;
pub mod core;
pub mod read;
pub mod write;
pub mod ecc;
pub mod recovery;
pub mod integrity;
pub mod retry_writer;
pub mod content_reader;
pub mod io_retry;

///Magic Number for the file format: "docufort"
pub const MAGIC_NUMBER: [u8; 8] = [0x64, 0x6F, 0x63, 0x75, 0x66, 0x6F, 0x72, 0x74]; //b"docufort"
pub const MN_ECC_LEN:usize = MAGIC_NUMBER.len() + ECC_LEN;

#[cfg(feature = "ecc_len_2")]
pub const ECC_LEN: usize = 2;
#[cfg(feature = "ecc_len_2")]
pub const MN_ECC: [u8;ECC_LEN] = [97, 115];

#[cfg(feature = "ecc_len_4")]
pub const ECC_LEN: usize = 4;
#[cfg(feature = "ecc_len_4")]
pub const MN_ECC: [u8;ECC_LEN] = [14, 182, 66, 232];

#[cfg(feature = "ecc_len_6")]
pub const ECC_LEN: usize = 6;
#[cfg(feature = "ecc_len_6")]
pub const MN_ECC: [u8;ECC_LEN] = [89, 235, 177, 40, 193, 248];

#[cfg(feature = "ecc_len_8")]
pub const ECC_LEN: usize = 8;
#[cfg(feature = "ecc_len_8")]
pub const MN_ECC: [u8;ECC_LEN] = [149, 154, 128, 141, 63, 79, 245, 149];

#[cfg(feature = "ecc_len_16")]
pub const ECC_LEN: usize = 16;
#[cfg(feature = "ecc_len_16")]
pub const MN_ECC: [u8;ECC_LEN] = [211, 210, 180, 83, 88, 174, 45, 67, 100, 212, 100, 132, 1, 168, 15, 154];

#[cfg(feature = "ecc_len_32")]
pub const ECC_LEN: usize = 32;
#[cfg(feature = "ecc_len_32")]
pub const MN_ECC: [u8;ECC_LEN] = [83, 167, 242, 14, 210, 222, 207, 128, 220, 246, 44, 99, 124, 84, 131, 64, 179, 22, 142, 190, 162, 181, 70, 110, 139, 197, 88, 22, 116, 21, 212, 200];

pub const DATA_SIZE:usize = (255 - ECC_LEN) as usize;

///MAGIC_NUMBER(8) + Ver(2) + ECC_LEN(1)
pub const FILE_HEADER_LEN:u8 = 11;

///TYPE(1) + TS(8) + DATA(4)
pub const HEADER_LEN:usize = 13;
///HASH(20)
pub const HASH_LEN:usize = 20;
///HASH(20) + ECC_LEN
pub const HASH_AND_ECC_LEN:usize = HASH_LEN+ECC_LEN;

// Type Byte for Header
///Tag for an Atomic Block (b'A') with **no** ECC on content.
pub const A_BLOCK:u8 = 0b0000_0000;
///Tag for a Best Effort Block (b'B')
pub const B_BLOCK:u8 = 0b0010_0000;
/// First byte tag for the 'Content' message with **no** ECC on content.
pub const CON_TAG:u8 = 0b0100_0000;
/// First byte tag for the 'End Block' message.
pub const END_TAG:u8 = 0b0110_0000;
/// Bit flag indicating the presence of ECC data.
pub const HAS_ECC:u8 = 0b0000_1000;
/// Bit flag indicating the content is compressed.
pub const IS_COMP:u8 = 0b0000_0100;


///Represents our different block types for matching against.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HeaderTag {
    ///Atomic Start, no ECC
    StartABlock = A_BLOCK,
    ///Atomic Start, with ECC
    StartAEBlock =  A_BLOCK | HAS_ECC,
    ///Atomic Start, with !ECC && COMP
    StartACBlock = A_BLOCK | IS_COMP,
    ///Atomic Start, with ECC && COMP
    StartAECBlock = A_BLOCK | IS_COMP | HAS_ECC,
    ///Best Effort Start
    StartBBlock = B_BLOCK,
    ///Content Start
    CComponent = CON_TAG,
    ///Content Start with ECC
    CEComponent = CON_TAG | HAS_ECC,
    ///Atomic Start, with !ECC && COMP
    CCComponent = CON_TAG | IS_COMP,
    ///Atomic Start, with ECC && COMP
    CECComponent = CON_TAG | IS_COMP | HAS_ECC,
    ///Block End
    EndBlock = END_TAG,
}

impl HeaderTag {
    fn has_ecc(&self)->bool{
        *self as u8 & HAS_ECC == HAS_ECC
    }
    fn is_comp(&self)->bool{
        *self as u8 & IS_COMP == IS_COMP
    }
}

impl From<u8> for HeaderTag {
    fn from(val: u8) -> Self {
        match val {
            B_BLOCK => HeaderTag::StartBBlock,
            END_TAG => HeaderTag::EndBlock,
            A_BLOCK => HeaderTag::StartABlock,
            a if a == A_BLOCK | HAS_ECC => HeaderTag::StartAEBlock,
            a if a == A_BLOCK | IS_COMP => HeaderTag::StartACBlock,
            a if a == A_BLOCK | HAS_ECC | IS_COMP => HeaderTag::StartAECBlock,
            CON_TAG => HeaderTag::CComponent,
            a if a == CON_TAG | HAS_ECC => HeaderTag::CEComponent,
            a if a == CON_TAG | IS_COMP => HeaderTag::CCComponent,
            a if a == CON_TAG | HAS_ECC | IS_COMP => HeaderTag::CECComponent,
            _ => panic!("Unknown block tag!"),
        }
    }
}

///Represents the different read components.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ComponentTag {
    StartHeader,
    EndHeader,
    Header,
    ContentHeader,
    ///Eventually we need to recover a block if its only failing is an unrecoverable hash
    Hash
}


///A ReadWriterError for problems occurring during operations.
#[derive(Debug)]
pub enum ReadWriteError{
    Io(std::io::Error),
    EndOfFile,
    EccTooManyErrors
}
impl From<std::io::Error> for ReadWriteError{
    fn from(value: std::io::Error) -> Self {
        match value.kind() {
            std::io::ErrorKind::UnexpectedEof => Self::EndOfFile,
            _ => Self::Io(value),
        }
    }
}
impl From<DecoderError> for ReadWriteError{
    fn from(_value: DecoderError) -> Self {
        Self::EccTooManyErrors
    }
}
impl std::fmt::Display for ReadWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ReadWriteError::Io(err) => write!(f, "I/O error: {}", err),
            ReadWriteError::EndOfFile => write!(f, "Unexpected end of file"),
            ReadWriteError::EccTooManyErrors => write!(f, "Too many ECC errors"),
        }
    }
}

impl std::error::Error for ReadWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReadWriteError::Io(err) => Some(err),
            _ => None,
        }
    }
}

pub struct HashAdapter<'a,RW,B:BlockInputs> {
    pub hasher:&'a mut B,
    pub writer:&'a mut RW,
}

impl<'a,W: std::io::Write,B:BlockInputs> HashAdapter<'a,W,B> {
    pub fn new(writer: &'a mut W,hasher:&'a mut B) -> Self {
        Self { writer,hasher }
    }
}

impl<'a,W: std::io::Write,B:BlockInputs> std::io::Write for HashAdapter<'a,W,B> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let bytes_written = self.writer.write(buf)?;
        self.hasher.update(&buf[..bytes_written]);
        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
impl<'a, R: std::io::Read, B: BlockInputs> std::io::Read for HashAdapter<'a, R, B> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_read = self.writer.read(buf)?;
        if bytes_read > 0 {
            self.hasher.update(&buf[..bytes_read]);
        }
        Ok(bytes_read)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CorruptDataSegment{
    ///This is for corruption beyond what ECC could correct within the 255 byte block.
    ///The chunk len is DATA_SIZE len, and ecc is ECC_LEN, together they equal 255.
    ///The provided [apply_ecc](crate::ecc::apply_ecc) expects a buffer with data first, followed by the ecc data.
    ///The best you can do is attempt to decode and fix the content and recalculate the ECC.
    ///Then *carefully* rewrite all the ECC data concatenated together beginning at
    EccChunk{chunk_start:u64,chunk_ecc_start:u64,ecc_start:u64,data_start:u64,data_len:u32},
    ///This is returned for a B block content component that does not have ECC calculated and stored.
    ///MaybeCorrupt is because a B Block can have more than one Content Component and if more than one does not have
    ///ECC calculated, then we only know that the block hash mismatches but we don't know where the error is.
    ///If you have structured data within the content, you should try decoding the content to see if you can find the error.
    ///If you can fix it, then you should *carefully* write the corrected bytes back at data_start..data_start+data_len.
    MaybeCorrupt{data_start:u64,data_len:u32},
    ///This is returned for an A block that does not have ECC calculated and stored.
    ///If you have structured data within the content, you should try decoding the content to see if you can find the error.
    ///If you can fix it, then you should *carefully* write the corrected bytes back at data_start..data_start+data_len.
    Corrupt{data_start:u64,data_len:u32}
}

pub trait FileLike:std::io::Read+std::io::Write+std::io::Seek {
    /// Truncates the underlying data to the given length.
    fn truncate(&mut self, len: u64)->std::io::Result<()>;
    /// Returns the length of the underlying data.
    fn len(&self)->std::io::Result<u64>;
}

impl FileLike for std::io::Cursor<Vec<u8>>{
    fn truncate(&mut self, len: u64)->std::io::Result<()>{
        let data = self.get_mut();
        data.truncate(len as usize);
        Ok(())
    }

    fn len(&self)->std::io::Result<u64> {
        Ok(self.get_ref().len() as u64)
    }
}
impl FileLike for std::fs::File{
    fn truncate(&mut self, len: u64)->std::io::Result<()>{
        self.set_len(len)
    }

    fn len(&self)->std::io::Result<u64> {
        self.metadata().map(|m|m.len())
    }
}

#[cfg(test)]
mod test_super {
    use std::io::Cursor;

    use crate::ecc::calculate_ecc_chunk;

    use super::*;
    #[test]
    fn test_calculate_magic_ecc() {
        let mut ecc = [0u8;ECC_LEN];
        let mut writer = Cursor::new(&mut ecc[..]);

        calculate_ecc_chunk(&MAGIC_NUMBER, &mut writer).unwrap();

        // Verify the writer contains the expected ECC data
        assert_eq!(writer.into_inner(), MN_ECC);
    }
}