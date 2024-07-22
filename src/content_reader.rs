//! This module provides a helper function to find all the content written between two time stamps.
//!
//! If nothing else it demonstrates how to loop through documents and read content from a docufort file.

use std::{io::{Read, Seek, SeekFrom, Write}, ops::RangeBounds};

use crate::{core::{BlockState, BlockInputs, Block, Content}, read::read_magic_number, recovery::{try_read_block, BlockReadSummary}, FILE_HEADER_LEN, MAGIC_NUMBER, ReadWriteError, ECC_LEN};

/// This function will read a docufort file and return all the content written between two time stamps.
///
/// # Arguments
/// * `file` - Some sort of Read+Write+Seek object that represents the docufort file.
/// * `start_hint` - Should be a BlockStart header position from which we want to start reading content.
/// * `range` - The range of time stamps we want content from.
///
/// # Returns
/// A vector of tuples containing the time stamp and content summaries that can be read using [read_content](crate::read::read_content).
///
/// # Notes
/// * The range will only return content *written* in the range of the given time stamp, not all items within a block that has a start time in this range.
/// * This function assumes all header timestamps are monotonically increasing.
/// * This does no ECC at all (you should have integrity checked already).
///
/// Recommended: Run integrity check on startup and provide a start_hint for the first block we want content from.
pub fn find_content<RW:Read+Write+Seek,B:BlockInputs,T:RangeBounds<u64>>(file: &mut RW, start_hint: Option<u64>,range:Option<T>) -> Result<Vec<(u64,Content)>, ReadWriteError> {
    let mut content = Vec::new();
    if let Some(s) = start_hint {
        file.seek(SeekFrom::Start(s))?;
    }else{
        file.seek(SeekFrom::Start(FILE_HEADER_LEN as u64 + MAGIC_NUMBER.len() as u64 + ECC_LEN as u64))?;//first block start
    }

    let range =range.map(|u|{
        match (u.start_bound(),u.end_bound()){
            (std::ops::Bound::Included(a), std::ops::Bound::Included(b)) => a.to_be_bytes()..=b.to_be_bytes(),
            (std::ops::Bound::Included(a), std::ops::Bound::Excluded(b)) => a.to_be_bytes()..=(b-1).to_be_bytes(),
            (std::ops::Bound::Included(a), std::ops::Bound::Unbounded) =>a.to_be_bytes()..=u64::MAX.to_be_bytes(),
            (std::ops::Bound::Excluded(a), std::ops::Bound::Included(b)) => (a+1).to_be_bytes()..=b.to_be_bytes(),
            (std::ops::Bound::Excluded(a), std::ops::Bound::Excluded(b)) => (a+1).to_be_bytes()..=(b-1).to_be_bytes(),
            (std::ops::Bound::Excluded(a), std::ops::Bound::Unbounded) => (a+1).to_be_bytes()..=u64::MAX.to_be_bytes(),
            (std::ops::Bound::Unbounded, std::ops::Bound::Included(b)) => 0u64.to_be_bytes()..=b.to_be_bytes(),
            (std::ops::Bound::Unbounded, std::ops::Bound::Excluded(b)) => 0u64.to_be_bytes()..=(b-1).to_be_bytes(),
            (std::ops::Bound::Unbounded, std::ops::Bound::Unbounded) => 0u64.to_be_bytes()..=u64::MAX.to_be_bytes(),
        }
    });

    //we read from where we are. if there is a range we only capture if it is in range
    //if we are less than range, we proceed
    //if we are past range, we return.
    //we also return if we can't decode a block.
    //we do no ECC

    'outer: loop {
        let bs = try_read_block::<_, B>(file, false,false)?;
        match bs {
            BlockState::Closed(BlockReadSummary { block, .. }) => {
                match block {
                    Block::A { middle,start,.. } => {
                        let start_time = start.time_stamp();
                        let ts = u64::from_be_bytes(start_time);
                        if let Some(r) = range.as_ref() {
                            if r.contains(&start_time){
                                content.push((ts,middle))
                            }else {
                                match r.end_bound(){
                                    std::ops::Bound::Included(x) if &start_time > x => break,
                                    std::ops::Bound::Excluded(x) if &start_time >= x => break,
                                    _ => (),
                                }
                            }
                        }else{
                            content.push((ts,middle))
                        }
                    },
                    Block::B { middle, .. } => {
                        for (s,m) in middle {
                            let start_time = s.time_stamp();
                            let ts = u64::from_be_bytes(start_time);
                            if let Some(r) = range.as_ref() {
                                if r.contains(&start_time){
                                    content.push((ts,m))
                                }else {
                                    match r.end_bound(){
                                        std::ops::Bound::Included(x) if &start_time > x => break 'outer,
                                        std::ops::Bound::Excluded(x) if &start_time >= x => break 'outer,
                                        _ => (),
                                    }
                                }
                            }else{
                                content.push((ts,m))
                            }
                        }
                    },
                }
            },
            BlockState::OpenBBlock { content:middle, .. } => {
                for (s,m) in middle {
                    let start_time = s.time_stamp();
                    let ts = u64::from_be_bytes(start_time);
                    if let Some(r) = range.as_ref() {
                        if r.contains(&start_time){
                            content.push((ts,m))
                        }else {
                            match r.end_bound(){
                                std::ops::Bound::Included(x) if &start_time > x => break 'outer,
                                std::ops::Bound::Excluded(x) if &start_time >= x => break 'outer,
                                _ => (),
                            }
                        }
                    }else{
                        content.push((ts,m))
                    }
                }
            }
            _ => break,
        }
        let res = read_magic_number(file, false);
        if res.is_err(){break}
    }
    Ok(content)
}
