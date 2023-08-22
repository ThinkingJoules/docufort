use std::{io::{SeekFrom, Seek}, fs::OpenOptions};

use crate::{core::{BlockState, BlockInputs, Block}, ReadWriteError, ComponentTag, read::{verify_configs, read_magic_number}, recovery::{try_read_block, BlockReadSummary}, CorruptDataSegment};



#[derive(Debug)]
pub struct IntegrityCheckOk{
    pub last_block_state:Option<BlockState>,
    ///Number of errors we fixed and wrote back to the file
    ///Does not indicate number of bytes corrected
    ///To estimate: ECC_LEN/2 is number of correctable errors per 255 bytes
    ///So if we did not return Err::Corruption, there was always less than that many errors per 'ecc chunk'.
    pub errors_corrected: usize,
    ///Number of bytes of 'Content' (without ECC data counted) in the file.
    pub data_contents: u64,
    ///Number of Blocks in file
    pub num_blocks:usize,
    ///This is the index up to which we checked
    ///It may be in the middle of a block
    pub file_len_checked:u64,
    ///These are all the content data segments that are not 'as written'
    ///They can either be corrupted and have no ECC or
    ///they can be corrupted beyond what ECC can do.
    pub corrupted_segments: Vec<CorruptDataSegment>,
    ///Contains the block start position and the time stamp found there
    pub block_times: Vec<(u64,u64)>
    
}
#[derive(Debug)]
pub enum IntegrityErr{
    Other(ReadWriteError),
    ///This only returns if a Component Header (or hash) is corrupted. 
    ///We cannot process the file any farther. We only read Front to Back so the position is all the farther we checked the file.
    ///The file may still be able to succeed at tail recovery if this corruption is earlier than the second to last block.
    ///If found in the last block, then a tail recovery would truncate this block.
    ///Integrity check handles the last block, so if you have this error then somehow part of the file got corrupted, badly.
    Corruption(u64,ComponentTag), // TODO: Make a hash recovery routine in the unlikely event the hash is corrupt and nothing else is.
    ///This is really an implementation error, where we find the wrong 'pattern' of headers. This should only occur in testing ideally.
    InvalidBlockStructure{start_of_bad_component:u64},
    ///Either the MAGIC_NUMBER, the V1 tag, or the ECC_LEN don't match this compiled program.
    ///Most likely would happen if you upgraded or have multiple docufort wrappers that use a different ECC_LEN
    ///You should only open docufort files that were written with the current compiled software.
    FileConfigMisMatch
}
impl From<std::io::Error> for IntegrityErr{
    fn from(value: std::io::Error) -> Self {
        Self::Other(value.into())
    }
}
impl From<ReadWriteError> for IntegrityErr{
    fn from(value: ReadWriteError) -> Self {
        Self::Other(value)
    }
}
pub fn integrity_check_file<B: BlockInputs>(file_path: &std::path::Path) -> Result<IntegrityCheckOk, IntegrityErr> {
    let mut file = OpenOptions::new().read(true).write(true).open(file_path)?;
    let mut file_len = file.metadata()?.len();
    let mut errors_corrected = 0;
    let mut data_contents = 0;
    let mut num_blocks = 0;
    let mut corrupted_segments = Vec::new();
    let mut block_times = Vec::new();

    if !verify_configs(&mut file)?{return Err(IntegrityErr::FileConfigMisMatch)}
    let mut last_state= None;
    loop {
        let cur_pos = file.seek(SeekFrom::Current(0))?;
        let res = read_magic_number(&mut file, true);
        let after_read_pos = file.seek(SeekFrom::Current(0))?;
        if cur_pos > file_len || after_read_pos > file_len || res.is_err() {//we read too far from when the fn was originally called.
            //We set the file_len to reflect how far we have integrity checked
            file_len = if cur_pos>file_len{file_len}else{cur_pos};
            break;
        }
        errors_corrected += res?;
        let bs = try_read_block::<_, B>(&mut file, true,true)?;//if we get an error now, there is some non-integrity problem
        last_state = Some(bs);
        match last_state.as_ref().unwrap() {
            BlockState::Closed(BlockReadSummary { errors_corrected: e, block,  corrupted_content_blocks, block_start, block_start_timestamp, .. }) => {
                errors_corrected += e;
                corrupted_segments.extend_from_slice(corrupted_content_blocks.as_slice());
                match block {
                    Block::A { middle, .. } => data_contents += middle.data_len as u64,
                    Block::B { middle, .. } => middle.iter().for_each(|(_,c)|data_contents+=c.data_len as u64),
                }
                num_blocks += 1;
                block_times.push((*block_start,*block_start_timestamp))
                // let BlockEnd { hash, .. } = block.clone().take_end();
                // assert_eq!(&hash_as_read[..],hash.hash());//impl assertion since we are error correcting every block
            },
            BlockState::OpenABlock { truncate_at } | 
            BlockState::OpenBBlock { truncate_at, .. } => {
                //We set the file_len to reflect how far we have integrity checked
                file_len = *truncate_at;
                break;
            },
            BlockState::IncompleteStartHeader { truncate_at } => {
                //We set the file_len to reflect how far we have integrity checked
                file_len = *truncate_at;
                break;
            },
            BlockState::InvalidBlockStructure { end_of_last_good_component, .. } =>{
                return Err(IntegrityErr::InvalidBlockStructure { start_of_bad_component: *end_of_last_good_component})
            }
            BlockState::ProbablyNotStartHeader { start_from } => {
                return Err(IntegrityErr::Corruption(*start_from,ComponentTag::StartHeader))
            }
            BlockState::DataCorruption { component_start, component_tag,.. } => {
                return Err(IntegrityErr::Corruption(*component_start,*component_tag))
            },
        }
    }
    Ok(IntegrityCheckOk {
        last_block_state: last_state,
        errors_corrected,
        data_contents,
        num_blocks,
        file_len_checked: file_len,
        corrupted_segments,
        block_times
    })
}
