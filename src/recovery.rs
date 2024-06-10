/*! This module contains functions for recovering the end of a docufort file.

This is used at startup to determine a new end of the file after a crash or power loss.
*/

use std::fs::OpenOptions;
use std::io::{SeekFrom, Seek};

use crate::core::HeaderAsContent;
use crate::read::{read_header, check_read_content, read_hash, read_block_middle, BlockMiddleState};
use crate::write::write_block_end;
//use write::{WriteError, FILE_HEADER_LEN};

use crate::*;

use crate::{core::{ComponentHeader,Block,BlockInputs,BlockState, BlockEnd}, ecc::apply_ecc};


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockReadSummary{
    pub errors_corrected:usize,
    pub block:Block,
    pub block_start:u64,
    pub block_start_timestamp:u64,
    pub hash_as_read:[u8;HASH_LEN],
    pub corrupted_content_blocks:Vec<CorruptDataSegment>
}

/// Attempts to find a MAGIC_NUMBER, starting from the given position of the reader.
pub fn find_block_start<RW: std::io::Read + std::io::Write + std::io::Seek>(file: &mut RW)-> std::io::Result<u64> {
    const MN_SIZE:usize = MAGIC_NUMBER.len();

    // Ensure the file is large enough to contain the magic number
    let start_pos = file.seek(SeekFrom::Current(0))?;
    let min_size = FILE_HEADER_LEN as usize + MN_ECC_LEN;
    if start_pos == FILE_HEADER_LEN as u64 {return Ok(FILE_HEADER_LEN as u64)}
    if start_pos > FILE_HEADER_LEN as u64 && start_pos < min_size as u64 {return Ok(FILE_HEADER_LEN as u64)}
    if start_pos < min_size as u64 {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "File is too small"));
    }
    let mut buff = [0u8;MN_ECC_LEN];
    let end_index = start_pos - MN_ECC_LEN as u64;
    // Iterate over the file in reverse, one byte at a time
    for start_index in (FILE_HEADER_LEN as u64..=end_index).rev() {
        file.seek(SeekFrom::Start(start_index))?;

        file.read_exact(&mut buff)?;
        match apply_ecc(&mut buff) {
            Ok(_errors) if &buff[..MN_SIZE] == &MAGIC_NUMBER => {
                return Ok((start_index + MN_ECC_LEN as u64) as u64)
            },
            _ => {
                // Move back last read an additional byte for the next iteration
                file.seek(SeekFrom::Current(-(1+MN_ECC_LEN as i64)))?;
                continue
            },
        }
    }
    Ok(0)
}


/// Reader should be positioned at the start of a header (after the magic number).
/// This function will hash, and optionally it will ecc the headers and or the content.
/// This function will intercept any relevant IO or decode Errors and return them as part of the Ok(BlockState)
pub fn try_read_block<RW:std::io::Write + std::io::Read + std::io::Seek,B:BlockInputs>(reader_writer:&mut RW,error_correct_header:bool,error_correct_content:bool)->Result<BlockState,ReadWriteError>{
    let block_start = reader_writer.seek(std::io::SeekFrom::Current(0))?;
    let mut hasher = B::new();
    let (mut errors_corrected,start) = match read_header(reader_writer,error_correct_header){
        Ok(a) => a,
        Err(ReadWriteError::EndOfFile) => return  Ok(BlockState::IncompleteStartHeader { truncate_at: block_start - MN_ECC_LEN as u64 }),
        Err(ReadWriteError::EccTooManyErrors) => return Ok(BlockState::ProbablyNotStartHeader{start_from:block_start}) ,//return Ok(BlockState::DataCorruption { component_start:block_start, is_b_block: false, component_tag: ComponentTag::StartHeader }),
        Err(e) => return Err(e)
    };
    match start.tag() {
        HeaderTag::StartACBlock |
        HeaderTag::StartAECBlock |
        HeaderTag::StartABlock |
        HeaderTag::StartAEBlock => {
            let h_content = start.as_content();
            let (mut corrupted_content_blocks, content) = match check_read_content(reader_writer, &h_content, error_correct_content,&mut hasher) {
                Ok((errs,cc,content)) => {
                    errors_corrected+=errs;
                    (cc,content)
                },
                Err(ReadWriteError::EndOfFile) => return Ok(BlockState::OpenABlock { truncate_at: block_start-(MN_ECC_LEN) as u64 }),
                Err(e)=>return Err(e)
            };
            let position = reader_writer.seek(std::io::SeekFrom::Current(0))?;
            let (e1,header) = match read_header(reader_writer, error_correct_header){
                Ok(a) => a,
                Err(ReadWriteError::EndOfFile) => return Ok(BlockState::OpenABlock { truncate_at: block_start-(MN_ECC_LEN) as u64 }),
                Err(ReadWriteError::EccTooManyErrors) => return Ok(BlockState::DataCorruption { component_start:position, is_b_block: false, component_tag: ComponentTag::EndHeader }),
                Err(e)=>return Err(e)
            };
            let position = reader_writer.seek(std::io::SeekFrom::Current(0))?;
            if let HeaderTag::EndBlock = header.tag() {
                let (e2,hash) = match read_hash(reader_writer, error_correct_header){
                    Ok(a) => a,
                    Err(ReadWriteError::EndOfFile) => return Ok(BlockState::OpenABlock { truncate_at: block_start-(MN_ECC_LEN) as u64 }),
                    Err(ReadWriteError::EccTooManyErrors) => return Ok(BlockState::DataCorruption { component_start:position, is_b_block: false, component_tag: ComponentTag::Hash }),
                    Err(e)=>return Err(e)
                };
                errors_corrected += e1+e2;
                let hash_as_read = hasher.finalize();

                if !content.ecc && hash_as_read != hash.hash() && error_correct_content{
                    assert!(corrupted_content_blocks.is_empty());
                    let HeaderAsContent { data_len, data_start, .. } = start.as_content();
                    corrupted_content_blocks.push(CorruptDataSegment::Corrupt{ data_start, data_len });
                }
                let end = BlockEnd{ header, hash };
                let brs = BlockReadSummary { hash_as_read,errors_corrected, block_start,block_start_timestamp:u64::from_be_bytes(start.time_stamp()),corrupted_content_blocks, block: Block::A { start, middle: content, end }};
                Ok(BlockState::Closed(brs))
            }else{
                Ok(BlockState::InvalidBlockStructure {end_of_last_good_component:block_start, info: "Did not find BlockEnd at correct position".to_string() })
            }
        }
        HeaderTag::StartBBlock => {
            match read_block_middle::<_,B>(reader_writer,error_correct_header,error_correct_content){
                Ok(BlockMiddleState::BBlock { middle, end, errors_corrected:ec, hash, corrupted_content_blocks }) => {
                    errors_corrected += ec;
                    let brs = BlockReadSummary { hash_as_read:hash,errors_corrected, block_start, block_start_timestamp:u64::from_be_bytes(start.time_stamp()), block: Block::B { start, middle, end }, corrupted_content_blocks };
                    Ok(BlockState::Closed(brs))
                },
                Ok(BlockMiddleState::InvalidBlockStructure { last_good_component_end }) => {
                    Ok(BlockState::InvalidBlockStructure {end_of_last_good_component:last_good_component_end, info: "Found a BlockStart variant in a B Block".to_string() })

                },
                Ok(BlockMiddleState::UnexpectedEof { last_good_component_end, hash_at_last_good_component, content }) => {
                    Ok(BlockState::OpenBBlock { truncate_at: last_good_component_end, errors: errors_corrected, hash_for_end:hash_at_last_good_component,content })
                },
                Ok(BlockMiddleState::DataCorruption { component_start, component_tag  }) => {
                    Ok(BlockState::DataCorruption { component_start, is_b_block: true,component_tag})
                }
                Err(e) => return Err(e),
            }
        },

        HeaderTag::CCComponent |
        HeaderTag::CECComponent |
        HeaderTag::CComponent |
        HeaderTag::CEComponent => return Ok(BlockState::InvalidBlockStructure {end_of_last_good_component:block_start, info: "Found a Content Component, Expected BlockStart".to_string()}),
        HeaderTag::EndBlock => return Ok(BlockState::InvalidBlockStructure {end_of_last_good_component:block_start, info: "Found a BlockEnd, expected BlockStart".to_string() }),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TailRecoverySummary{
    pub original_file_len:u64,
    pub recovered_file_len:u64,
    ///This is a list of tail block states it got from successive calls try_read_block after file manipulations.
    pub file_ops:Vec<(u64,BlockState)>,
    pub has_blocks:bool,
    pub tot_errors_corrected:usize,
    ///Corruption exceeds ECC for content in the following file offsets that are DATA_SIZE len
    pub corrupted_content_blocks:Vec<CorruptDataSegment>
}
///Recovers the end of the DocuFort file.
///As long as the headers have corruption below the error correction ability, this will at most truncate the last block, if it is incomplete.
///If headers are corrupted, then it will keep truncating the end of the file until it can read a complete block.
///This does *not* truncate a block whose *contents* are corrupted beyond repair.
pub fn recover_tail<B:BlockInputs>(file_path: &std::path::Path) -> Result<TailRecoverySummary, ReadWriteError> {
    let mut file = OpenOptions::new().read(true).write(true).open(file_path)?;
    let original_file_len = file.metadata()?.len();
    file.seek(SeekFrom::End(0))?;
    let mut file_ops = Vec::new();
    let mut tot_errors_corrected = 0;
    let mut error_correct_content = false;
    let mut other_start = None;
    loop {
        let current_file_len = file.metadata()?.len();
        if let Some(offset) = other_start.take() {
            file.seek(SeekFrom::Start(offset))?;
        }
        let block_start_offset = match find_block_start(&mut file) {
            Ok(offset) if offset <= FILE_HEADER_LEN as u64 => return Ok(TailRecoverySummary { original_file_len, recovered_file_len: current_file_len, file_ops, has_blocks: false, tot_errors_corrected,corrupted_content_blocks:vec![] }),
            Err(e) => return Err(e.into()),
            Ok(offset) => offset,
        };
        file.seek(SeekFrom::Start(block_start_offset))?;
        let bs = try_read_block::<_,B>(&mut file, true,error_correct_content)?;
        let crsr_pos = file.seek(SeekFrom::Current(0)).unwrap();
        file_ops.push((block_start_offset,bs));
        let (_,bs) = file_ops.last().unwrap();
        match bs {
            BlockState::ProbablyNotStartHeader{ start_from } => {other_start = Some(*start_from)}
            BlockState::Closed (BlockReadSummary { errors_corrected, block,  hash_as_read, corrupted_content_blocks, .. }) => {
                tot_errors_corrected += errors_corrected;
                let BlockEnd { hash,.. } = block.clone().take_end();
                if !error_correct_content && &hash_as_read[..] != hash.hash() {
                    error_correct_content = true;
                    continue;//read the same block over, but correct the errors
                }else{//hash is perfect, skip ecc, clean recovery
                    if crsr_pos < current_file_len{
                        //we must truncate, as their is an incomplete MN+ECC chunk of bytes after
                        assert!(crsr_pos + MN_ECC_LEN as u64 > current_file_len,"{} !> {}",crsr_pos+MN_ECC_LEN as u64,current_file_len);
                        file.set_len(crsr_pos)?;
                    }else{
                        assert_eq!(crsr_pos,current_file_len);
                    }

                    //let content_has_uncorrectable_errors = error_correct_content && &hash_as_read[..] != hash.hash();
                    //we could try to recover a b block that has one or more Content components that do not have ecc but has errors
                    //we know there is at least one error since the hash doesn't match.
                    //should we worry about this? an integrity check will identify this, for recovery, we have a complete tail.
                    //for now, we will consider this 'recovered'
                    //the application using this should also not be able to decode the data properly.
                    let corrupted_content_blocks = corrupted_content_blocks.clone();

                    return Ok(TailRecoverySummary { original_file_len, recovered_file_len:crsr_pos, file_ops, has_blocks: true, tot_errors_corrected,corrupted_content_blocks })
                }
            },
            BlockState::OpenBBlock { truncate_at: truncate_at_then_close_block, errors, hash_for_end, .. } => {
                tot_errors_corrected += errors;
                //let truncation_amt = file.metadata()?.len() - truncate_at_then_close_block;
                //how do we avoid allocating a really big vec? we would need to know when to start hashing, up to the truncate
                //then we could just buffer update to get the hash to avoid a large allocation.
                file.set_len(*truncate_at_then_close_block)?;
                file.seek(SeekFrom::End(0))?;
                let time_stamp = B::current_timestamp();
                let header = ComponentHeader::new_from_parts(HeaderTag::EndBlock as u8, time_stamp.to_be_bytes(), None);
                write_block_end(&mut file, &header, &hash_for_end)?;
                continue; //should end in a closed block
            },
            BlockState::OpenABlock { truncate_at } => {
                file.set_len(*truncate_at)?;
                file.seek(SeekFrom::End(0))?;
                error_correct_content = false;
                continue; //should try the next block back
            },
            BlockState::InvalidBlockStructure { end_of_last_good_component, .. } => {
                file.set_len(*end_of_last_good_component)?;
                file.seek(SeekFrom::End(0))?;
                error_correct_content = false;
                continue; //If this is an A block, it will be OpenA next, if B Block, will try to close it next.
            },
            BlockState::DataCorruption { component_start,.. } => {
                //This should really only occur on headers.
                file.set_len(*component_start)?;
                file.seek(SeekFrom::End(0))?;
                error_correct_content = false;
                continue; //If this is an A block, it will be OpenA next, if B Block, will try to close it next.
            },
            BlockState::IncompleteStartHeader { truncate_at } => {
                file.set_len(*truncate_at)?;
                file.seek(SeekFrom::End(0))?;
                error_correct_content = false;
                continue; //We don't know what we are, but we just try again after truncation.
            },
        }
    }
}
