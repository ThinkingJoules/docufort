use std::fmt::Debug;

use crate::{*, ecc::calc_ecc_data_len, recovery::BlockReadSummary};



#[derive(Copy,Debug,Clone,PartialEq,Eq,PartialOrd,Ord)]
pub struct ComponentHeader([u8;HEADER_LEN],u64);

impl ComponentHeader {
    pub fn new(slice:&[u8],start_offset:u64) -> Self {
        assert_eq!(slice.len(),HEADER_LEN);
        Self(slice.try_into().unwrap(),start_offset)
    }
    pub fn new_from_parts(tag:u8,time_stamp:[u8;8],content_len:Option<u32>) -> Self {
        let mut arr = [0u8;HEADER_LEN];
        arr[0] = tag;
        arr[1..9].copy_from_slice(&time_stamp);
        if let Some(data) = content_len {
            arr[9..13].copy_from_slice(&data.to_le_bytes());
        }
        Self(arr,0)
    }
    ///This is a bit like a transmute
    ///We interpret the header regardless of tag type as carrying content info
    ///The header doesn't carry the uncompressed info, so that must be added later. Some/None used as boolean
    pub fn as_content(&self)->HeaderAsContent{
        let data_len = u32::from_le_bytes(self.data());
        let tag = self.tag();
        let has_ecc = tag.has_ecc();
        let compressed = tag.is_comp();
        let end_pos = self.1 + (HEADER_LEN + ECC_LEN) as u64;
        let data_start = if has_ecc {calc_ecc_data_len(data_len as usize) as u64+end_pos}else{end_pos};
        HeaderAsContent{ data_len, data_start, ecc:has_ecc, compressed}
    }
    pub fn tag(&self)->HeaderTag{
        self.0[0].into()
    }
    pub fn start_pos(&self)->u64{
        self.1
    }
    pub fn time_stamp(&self)->[u8;8]{
        self.0[1..9].try_into().unwrap()
    }
    pub fn data(&self)->[u8;4]{
        self.0[9..13].try_into().unwrap()
    }
    pub fn as_slice(&self)->&[u8]{
        &self.0[..]
    }
    pub fn as_mut_slice(&mut self)->&mut [u8]{
        &mut self.0[..]
    }
}


#[derive(Debug,Clone,PartialEq,Eq,PartialOrd,Ord)]
pub enum Block{
    A{start:ComponentHeader,middle:Content,end:BlockEnd},
    B{start:ComponentHeader,middle:Vec<(ComponentHeader,Content)>,end:BlockEnd}
}

impl Block {
    pub fn is_atomic(&self)->bool{
        match self {
            Block::A {.. } => true,
            Block::B {.. } => false,
        }
    }
    pub fn take_start(self)->ComponentHeader{
        match self {
            Block::A { start,.. } => start,
            Block::B { start,.. } => start,
        }
    }
    pub fn take_end(self)->BlockEnd{
        match self {
            Block::A { end,.. } => end,
            Block::B { end,.. } => end,
        }
    }
}

#[derive(Copy,Debug,Clone,PartialEq,Eq,PartialOrd,Ord)]
pub struct HeaderAsContent {
    pub data_len: u32,
    pub data_start:u64,
    pub ecc: bool,
    pub compressed: bool
}
#[derive(Copy,Debug,Clone,PartialEq,Eq,PartialOrd,Ord)]
pub struct Content {
    pub data_len: u32,
    pub data_start:u64,
    pub ecc: bool,
    pub compressed: Option<u32>
}
/// A structure representing the end of a block in the data storage.
#[derive(Copy,Debug,Clone,PartialEq,Eq,PartialOrd,Ord)]
pub struct BlockEnd{
    pub header:ComponentHeader,
    pub hash: BlockHash
}
#[derive(Copy,Debug,Clone,PartialEq,Eq,PartialOrd,Ord)]
pub struct BlockHash([u8;HASH_AND_ECC_LEN]);

impl BlockHash {
    pub fn new(hash_and_ecc:[u8;HASH_AND_ECC_LEN]) -> Self {
        Self(hash_and_ecc)
    }
    pub fn new_from_parts(hash:[u8;HASH_LEN]) -> Self {
        let mut arr = [0u8;HASH_AND_ECC_LEN];
        arr[0..HASH_LEN].copy_from_slice(&hash);
        Self(arr)
    }
    pub fn hash(&self)->&[u8]{
        &self.0[0..HASH_LEN]
    }
    pub fn as_slice(&self)->&[u8]{
        &self.0[..]
    }
    pub fn as_mut_slice(&mut self)->&mut [u8]{
        &mut self.0[..]
    }
}


#[derive(Clone, Debug,  PartialEq, Eq)]
pub enum BlockState{
    ///Block has Start..End components, but may have errors within.
    Closed(BlockReadSummary),
    /// Something does not follow, somewhere. Truncate at give value and try again.
    InvalidBlockStructure { end_of_last_good_component:u64, info: String },
    ///If this is returned, truncate file at block_start_offset and try finding another block before this
    OpenABlock{truncate_at:u64},
    ///If this is returned, truncate file at given index and write a BlockEnd.
    OpenBBlock{hash_for_end:[u8;HASH_LEN],truncate_at:u64,errors:usize,content:Vec<(ComponentHeader,Content)>},
    ///Incomplete Start header.
    IncompleteStartHeader{truncate_at:u64},
    ///This is only returned when we try to read a block start header that doesn't pass ECC
    ///There is enough bytes so it is not incomplete, but it doesn't decode properly
    ///During Recovery we should simply ignore this and consider it a 'false match'
    ///This could happen if someone wrote the MAGIC_NUMBER and the correct ECC values as content somewhere.
    ///So for adversarial reasons we should assume we accidentally matched 'noise' in the 'content'.
    ///It *could* be a severely corrupted start header, but the idea is that ECC on our really short messages like headers should never be corrupted.
    ProbablyNotStartHeader{start_from:u64},
    ///Data Corruption, ECC cannot recover original data.
    ///This hopefully never happens, as 'recovery' from here is complicated and not dealt with in this lib.
    DataCorruption{component_start:u64,is_b_block:bool,component_tag:ComponentTag}
}

impl BlockState {
    pub fn is_closed(&self) -> bool {
        matches!(self, BlockState::Closed(_))
    }

    pub fn is_open_a(&self) -> bool {
        matches!(self, BlockState::OpenABlock { .. })
    }

    pub fn is_open_b(&self) -> bool {
        matches!(self, BlockState::OpenBBlock { .. })
    }
}

/// A trait for hashing the block data.
pub trait BlockInputs:Clone {
    fn new() -> Self;
    ///Add state to the hasher
    fn update(&mut self, data: &[u8]);
    ///Return hash from hasher
    fn finalize(&self) -> [u8; HASH_LEN];
    ///Used to return the timestamp that all headers carry.
    ///This is stored as big endian in headers so direct byte comparison works.
    fn current_timestamp() -> u64;
}

