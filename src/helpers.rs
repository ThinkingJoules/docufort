use std::{path::Path, io::{Error, Read, Seek, SeekFrom}, collections::HashMap, sync::Arc};
use bincode::Options;

use blake3::hash;
use memmap2::Mmap;
use tokio::{time, fs::File, io::{BufReader, AsyncReadExt, AsyncSeekExt}};

use crate::{messages::{Message, MAGIC_NUMBER, Create, Update, Data, Delete, BLOCK_START_PREFIX, BlockStart, BlockEnd, Archive}, util::{DocID, apply_ecc, calc_ecc_data_len}, Config, coder::{DocuFortMsg, disk_read_msg, SYSTEM_ECC_LEN}};


// Define an enum to hold the two variants
pub enum Reader<'a> {
    BufReader(BufReader<File>),
    Slice { data: &'a [u8], position: usize },
}

impl<'a> Reader<'a> {
    // Define read_exact function for Reader
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Reader::BufReader(reader) => reader.read_exact(buf).await,
            Reader::Slice { data, position } => {
                let end = *position + buf.len();
                if end >= *position {
                    buf.copy_from_slice(&data[*position..end]);
                    *position = end;
                    Ok(buf.len())
                } else {
                    Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "end of file"))
                }
            }
        }
    }

    // Define seek function for Reader
    pub async fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            Reader::BufReader(reader) => reader.seek(pos).await,
            Reader::Slice { data, position } => {
                let new_offset = match pos {
                    SeekFrom::Start(offset) => offset as usize,
                    SeekFrom::End(offset) => (data.len() as i64 + offset) as usize,
                    SeekFrom::Current(offset) => (*position as i64 + offset) as usize,
                };
                if new_offset <= data.len() {
                    *position = new_offset;
                    Ok(new_offset as u64)
                } else {
                    Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid seek to a position past the end of file"))
                }
            }
        }
    }
}

impl<'a>  From<BufReader<File>> for Reader<'a> {
    fn from(reader: BufReader<File>) -> Self {
        Reader::BufReader(reader)
    }
}
impl<'a>  From<(&'a [u8],usize)> for Reader<'a> {
    fn from((data,offset):( &'a [u8], usize)) -> Self {
        Reader::Slice { data, position: offset }
    }
}
impl<'a>  From<&'a [u8]> for Reader<'a> {
    fn from(data: &'a [u8]) -> Self {
        Reader::Slice { data, position: 0 }
    }
}

enum BlockRecoveryError{
    EccNeeded{corrected_segments:Vec<(usize,Vec<u8>)>},//we assume errors are bit flips since the length must be right, offset and proper bits.
    TooManyErrors(String),
    IncompleteBlock(usize), //should truncate the file and wipe out uncertain data.
    ClosableBlock(usize), //should truncate the file and then add a Block End
    NonMonotonicEvents(usize,DocID), //non-monotonic events
    IoError(std::io::Error)
}
impl From<std::io::Error> for BlockRecoveryError {
    fn from(error: std::io::Error) -> Self {
        BlockRecoveryError::IoError(error)
    }
}

///data buffer should start at the first byte after the BlockStart message
async fn process_block<'a>(bs:BlockStart,mut data: Reader<'a>,block_len:usize,ecc_len:u8)-> Result<(usize,HashMap<DocID,DocEvent>),BlockRecoveryError> {
    //let mut events = HashMap::new();
    let mut updates: HashMap<u64, bool> = HashMap::new();
    //let mut data_modal = HashMap::new();

    let block_end_ecc_len = calc_ecc_data_len(BlockEnd::FIXED_LEN, ecc_len);
    
    let stream_pos = data.seek(SeekFrom::Current(0)).await?;
    let beginning_of_block = stream_pos - (BlockStart::FIXED_LEN + calc_ecc_data_len(BlockStart::FIXED_LEN, SYSTEM_ECC_LEN)) as u64;
    //Locate and attempt to decode BlockEnd msg
    let be_start = block_len - (BlockEnd::FIXED_LEN + block_end_ecc_len + 1);
    data.seek(SeekFrom::Current((block_len - ((BlockEnd::FIXED_LEN + ecc_len as usize + 1)))as i64)).await?;
    let mut tag = [0;1];
    data.read_exact(&mut tag).await?;
    let be_res =  disk_read_msg::<BlockEnd>(&mut data, tag[0], ecc_len, true).await;
    let mut apply_ecc = be_res.is_err();
    if be_res.is_err() && bs.atomic {
        //atomic block must be closed correctly
        //means we need to subtract the BlockStart distance before truncating
        //also must be the last block in file, so we can't really know that here.
        return Err(BlockRecoveryError::IncompleteBlock(beginning_of_block as usize)) 
    }
    let be = match be_res {
        Ok((errors,be)) => {
            //seek to beginning of block
            data.seek(SeekFrom::Start(stream_pos)).await?;
            //calculate the bytes that we need to hash
            let hash_byte_len = block_len - block_end_ecc_len - 32; //|Hash Bytes |Hash in BlockEnd | Blockend ECC data|
            let mut hash_bytes = vec![0;hash_byte_len];
            data.read_exact(&mut hash_bytes).await?;
            let b3_hash = hash(&hash_bytes); //we hash everything but the hash and the ecc data
            if b3_hash != be.hash{
                apply_ecc = true
            }
            Some(be)
        },
        _ => {
            //Didn't find a BlockEnd to pair with our BlockStart.atomic != true
            apply_ecc = true;
            None
        }
    };
    //make sure we are at the beginning of the data.
    data.seek(SeekFrom::Start(stream_pos)).await?;

    let end = beginning_of_block + block_len as u64;
    let err = loop {
        
        let current_pos = data.seek(SeekFrom::Current(0)).await?;
        
        let mut tag = [0;1];
        data.read_exact(&mut tag).await?;
        let flags = tag[0];
        let tag = flags & 0b00011111;
        match tag {
            Create::MSG_TAG => {

            },
            Update::MSG_TAG => {

            },
            Data::MSG_TAG => {

            },
            Delete::MSG_TAG => {

            },
            Archive::MSG_TAG => {

            },
            _ => {
                break Some(BlockRecoveryError::ClosableBlock(current_pos as usize))
            }
        }

        // match buf_read(&data[crsr..], ecc_len, apply_ecc).await {
        //     Ok((read,msg)) => {
        //         match msg {
        //             Message::Create(Create{ doc_id })=>{
        //                 match events.remove(&doc_id) {
        //                     None => {
        //                         events.insert(doc_id, DocEvent::Create(true));
        //                     }
        //                     _ => return Err(BlockRecoveryError::NonMonotonicEvents(crsr, doc_id)),
        //                 }
                        
        //             }
        //             Message::Update(Update{ doc_id, time_stamp, .. }) =>{
        //                 match (updates.remove(&time_stamp), events.remove(&doc_id) ){
        //                     (None, None) | //created in another block
        //                     (None,Some(DocEvent::Create(_)))|
        //                     (None,Some(DocEvent::Update(_))) => {
        //                         updates.insert(time_stamp, false);
        //                         events.insert(doc_id, DocEvent::Update(false));
                                
        //                     }
        //                     _ => return Err(BlockRecoveryError::NonMonotonicEvents(crsr, doc_id))
        //                 }
        //             }
        //             Message::Data(Data{ doc_id,op_doc_id, last,.. }) => {
        //                 data_modal.insert(op_doc_id, last);
        //                 //how do we know if this is for a create or an update or an archive?
        //                 //do we care? I don't think so. That is for higher level. We just need to find Data.last == true
        //                 //so we need to not care about the bool in Create/Update... Yet.
        //             }
        //             Message::Delete(Delete{ doc_id, time_stamp }) => {
        //                 match events.remove(&doc_id) {
        //                     Some(DocEvent::Create(_)) |
        //                     Some(DocEvent::Update(_)) |
        //                     None => {
        //                         events.insert(doc_id, DocEvent::Delete);
        //                     }
        //                     _ => return Err(BlockRecoveryError::NonMonotonicEvents(crsr, doc_id)),
        //                 }
                        
        //             }
        //             Message::Archive(_) => todo!(),
        //             Message::Start(_) => todo!(),
        //             Message::End(_) => todo!(),
        //         }
        //     },
        //     Err(e) => {

        //     }
            
        // }

    };
    //next must be a block end. 
    //  If not we need to truncate the file, unless there is ECC, then we can check everything there and then close it post-facto
    //  no ECC then we could find all the messages that decoded and then truncate the last message and then close the block

    //try to get first and last blocks decoded
    //then check hash, if hash passed we can skip ecc
    //if not we need to apply ecc to try and find the bad bits
    //while processing we take notes of creates... and updates?... deletes??
    // we don't care about archives, that is for compaction system, we don't know if the old data has been transferred.
    // how are we going to compare values for atomics? Rescan the whole file? Creates will at least give us a starting point...
    // if we had the most recent update *location* we could go and read it later.
    // since the blocks are in parallel, we need to get both and then process them later, find the most recent update...

    todo!()
    
    //Ok((file_offset,events))
}

async fn recover(file_path: &Path, config:Config) -> Result<(), Error> {
    let file = File::open(file_path).await?;
    let mmap = unsafe { Mmap::map(&file)? };
    let ecc_len = config.ecc_len;
    //read the config and check ecc matches
    // MAGIC_NUMBER | ecc_len(u8) | BlockStart...
    assert!(&mmap[0..MAGIC_NUMBER.len()] == MAGIC_NUMBER);
    assert!(&mmap[MAGIC_NUMBER.len()..MAGIC_NUMBER.len()+1] == &[ecc_len]);

    let mut last_index = MAGIC_NUMBER.len()+1;
    let file_header_offset = last_index;
    let mut skip_til = last_index;

    let mut tasks = vec![];
    for (file_index, window) in mmap[..].windows(BLOCK_START_PREFIX.len()).enumerate() {
        if file_index < skip_til {continue;}
        if window == BLOCK_START_PREFIX {
            //check to make sure we aren't at the end of the file
            if mmap.len() < BlockStart::FIXED_LEN + file_index{
                //cannot be valid, was not properly closed.
                break
            }
            let mut reader = Reader::Slice { data: &mmap[file_index+1..file_index+1+BlockStart::FIXED_LEN], position: 0 };
            match disk_read_msg::<BlockStart>(&mut reader, *mmap.get(file_index).unwrap(), ecc_len, true).await {
                Ok((errors,msg)) => {
                    //must be the first block in the file
                    if file_header_offset == file_index{continue;} 

                    //skip the main loop a min amount forward, while the task works in parallel
                    skip_til = file_index + BlockStart::FIXED_LEN + BlockEnd::FIXED_LEN;
                    
                    //process previous block
                    if errors > 0 {
                        println!("Found BlockStart msg starting at ${}. Had ${} errors!",file_index,errors);
                    }else{
                        println!("Found BlockStart msg starting at ${}",file_index);
                    }

                    //create parallel file handle
                    let thread_file = File::open(file_path).await?;
                    let mut thread_reader:Reader = BufReader::new(thread_file).into();
                    thread_reader.seek(SeekFrom::Start(last_index as u64)).await?;
                    let task = tokio::task::spawn(async move {
                        process_block(msg,thread_reader,file_index-last_index,ecc_len).await
                    });
                    tasks.push(task);
                    last_index = file_index;

                }
                Err(decode_error) => {
                    //Must not be real block
                    println!("False BLOCK_START_PREFIX starting at ${}",file_index);
                }
            }            
        }
    }

    // process the remaining data after the last magic number
    let data = mmap[last_index..].to_vec();
    let last_block = tokio::task::spawn(async move {
        //process_block(&data,last_index,ecc_len).await
    });
    // wait for all tasks to complete
    for task in tasks {
        match task.await?{
            Ok((file_offset,creates)) => {
                todo!()
            },
            Err(e) =>{
                todo!()
            }
        };
    }
    
    //deal with last block
    // match last_block.await?{
    //     Ok((file_offset,creates)) => {
    //         todo!()
    //     },
    //     Err(e) =>{
    //         todo!()
    //     }
    // };

    Ok(())
}



#[derive(Copy, Clone, Debug, PartialEq, Eq,PartialOrd, Ord)]
enum DocEvent{//we have to assume one will fully write to disk before the next is processed so Create(true) -> Update(true) -> Delete
    Create(DataComplete),
    Update(DataComplete),
    Delete
}
type DataComplete = bool;



#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs::{File, remove_file};
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_reader_from_slice() {
        let data: &[u8] = &[1, 2, 3, 4, 5];
        let mut reader = Reader::from(data);
        let mut buf = vec![0; 3];
        reader.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, &[1, 2, 3]);
        reader.seek(SeekFrom::End(-3)).await.unwrap();
        reader.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, &[3, 4, 5]);
    }

    #[tokio::test]
    async fn test_reader_from_file() {
        
        let mut file = File::create("test.txt").await.unwrap();
        file.write_all(&[1, 2, 3, 4, 5]).await.unwrap();
        let buf = BufReader::new(File::open("test.txt").await.unwrap());
        let mut reader = Reader::from(buf);
        let mut buf = vec![0; 3];
        reader.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, &[1, 2, 3]);
        reader.seek(SeekFrom::End(-3)).await.unwrap();
        reader.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, &[3, 4, 5]);
        remove_file("test.txt").await.unwrap()
    }
}




// async fn recover(file_path: &Path, config:Config) -> Result<(), Error> {
//     let ecc_len = config.ecc_len;

//     let file = std::fs::File::open(file_path)?;
//     let metadata = file.metadata()?;
    
//     let mut tasks = Vec::new();
//     let mut buf = BufReader::new(file);
    
//     let mut magic_number = [0;MAGIC_NUMBER.len()];
//     buf.read_exact(&mut magic_number)?;
//     assert!(magic_number == MAGIC_NUMBER);

//     let mut ecc_len_in_file_header = [0;1];
//     buf.read_exact(&mut ecc_len_in_file_header)?;
//     let ecc_len_in_file_header = ecc_len_in_file_header[0];
//     assert!(ecc_len_in_file_header == config.ecc_len);

//     let mut window = [0u8;BLOCK_START_PREFIX.len()];
    
//     loop {
//         buf.re
//         if window == BLOCK_START_PREFIX {
//             //check to make sure we aren't at the end of the file
//             if mmap.len() < BlockStart::FIXED_LEN + index{
//                 //cannot be valid, was not properly closed.
//                 break
//             }
          

//             // //verify checksum to ensure this is not random bytes
//             // let bytes = mmap[index..index+total_block_start_len].to_vec();
//             // let corrected = apply_ecc(bytes,ecc_len as usize).await;
//             // if corrected.is_err(){
//             //     println!("ECC/Checksum Fail. Assuming this is 1:2^10 random chance...");
//             //     continue;
//             // }
//             // let (corrected_errors,corrected_bytes) = corrected.unwrap();
//             // let my_options = bincode::DefaultOptions::new();  
//             // let msg = my_options.deserialize(&corrected_bytes);
//             // if msg.is_err() && corrected_errors > 0{
//             //     println!("Warning: Error Correction corrected ${} errors in a BlockStart msg but doesn't decode. Assuming random bytes made a valid ECC...",corrected_errors);
//             //     continue;
//             // }else if msg.is_err(){
//             //     println!("Matching Block Prefix did not decode. ECC corrected no errors. Maybe it is chance? Assuming random bytes...");
//             //     continue;
//             // }
//             if let Message::Start(_) =  msg.unwrap(){
//                 //success, process the prev block
//                 let data = mmap[last_index..index].to_vec();
//                 let task = tokio::task::spawn(async move {
//                     process_block(&data,last_index,ecc_len).await
//                 });
//                 tasks.push(task);
//                 last_index = index + offset;
//             }else{
//                 panic!("I give up. MAGIC_NUMBER and ECC pass and it decodes to a valid message... but not a BlockStart message")
//             };
            
//         }
//     }

   

//     Ok(())

// }
