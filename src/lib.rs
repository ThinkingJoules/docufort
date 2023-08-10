






use reed_solomon::DecoderError;
//use write::{WriteError, FILE_HEADER_LEN};

use crate::core::BlockInputs;
pub mod core;
pub mod read;
pub mod write;
pub mod ecc;
pub mod recovery;
pub mod integrity;

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

///Tag for an Atomic Block (b'A') with **no** ECC on content.
pub const A_BLOCK:u8 = b'A';
///Tag for an Atomic Block **with** ECC on content.
pub const AE_BLOCK:u8 = b'Q';
///Tag for a Best Effort Block (b'B')
pub const B_BLOCK:u8 = b'B';
/// First byte tag for the 'Content' message with **no** ECC on content.
pub const C_TAG:u8 = b'@';
/// First byte tag for the 'Content' message with **no** ECC on content.
pub const CE_TAG:u8 = b'P';
/// First byte tag for the 'End Block' message.
pub const END_BLOCK:u8 = b'D';

///Represents are different block types for matching against.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BlockTag {
    ///Atomic Start, no ECC
    StartABlock = A_BLOCK,
    ///Atomic Start, with ECC
    StartAEBlock = AE_BLOCK,
    ///Best Effort Start
    StartBBlock = B_BLOCK,
    ///Content Start
    CComponent = C_TAG,
    ///Content Start with ECC
    CEComponent = CE_TAG,
    ///Block End
    EndBlock = END_BLOCK,
}

impl BlockTag {
    fn has_ecc(&self)->bool{
        match self {
            BlockTag::StartAEBlock |
            BlockTag::CEComponent => true,
            _ => false,
        }
    }
}

impl From<u8> for BlockTag {
    fn from(val: u8) -> Self {
        match val {
            A_BLOCK => BlockTag::StartABlock,
            B_BLOCK => BlockTag::StartBBlock,
            END_BLOCK => BlockTag::EndBlock,
            C_TAG => BlockTag::CComponent,
            CE_TAG => BlockTag::CEComponent,
            AE_BLOCK => BlockTag::StartAEBlock,
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