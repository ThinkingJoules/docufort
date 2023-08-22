/*!
This module should follow the inverse of the [write module](crate::write).
We always write to the file if we find errors reading system messages.

Content error correction happens at a higher level.
*/


use crate::{FILE_HEADER_LEN, MAGIC_NUMBER, ECC_LEN, core::{ComponentHeader, Content, BlockHash, BlockInputs, BlockEnd}, ReadWriteError, HEADER_LEN, ecc::{apply_ecc, calc_ecc_data_len}, HASH_AND_ECC_LEN, DATA_SIZE, BlockTag, HASH_LEN, ComponentTag, CorruptDataSegment, MN_ECC_LEN, MN_ECC};




/// Verifies a DocuFort file at the specified path by comparing its header data with the compiled system constants.
///
/// # Errors
///
/// Returns an `std::io::Error` if:
///
/// - Read Error from Reader
/// 
/// Return Ok(true) if everything matches, and Ok(false) if something mis-matches
pub fn verify_configs<R:std::io::Read>(file: &mut R) -> std::io::Result<bool> {
    // Create a buffer large enough for all data
    let mut buffer = [0; FILE_HEADER_LEN as usize];
    file.read_exact(&mut buffer)?;

    // Split the buffer into the magic number and the constants
    let (magic_number, constants) = buffer.split_at(MAGIC_NUMBER.len());
    // Convert the magic number slice to an array
    let magic_number_arr: [u8; 8] = magic_number.try_into().expect("Wrong size for magic number");

    if magic_number_arr != MAGIC_NUMBER {
        return Ok(false);
    }
    if &constants[0..2] != &[b'V',b'1'] {
        return Ok(false);
    }
    if constants[2] != ECC_LEN as u8 {
        return Ok(false);
    }

    Ok(true)
}

//read opt ecc mn
pub fn read_magic_number<RW:std::io::Write + std::io::Read + std::io::Seek>(reader_writer:&mut RW,error_correct:bool)->Result<usize,ReadWriteError>{
    let mut buf = [0u8;MN_ECC_LEN];
    let start = reader_writer.seek(std::io::SeekFrom::Current(0))?;
    reader_writer.read_exact(&mut buf)?;
    let errors = if error_correct && (&buf[..MAGIC_NUMBER.len()] != &MAGIC_NUMBER || &buf[MAGIC_NUMBER.len()..] != MN_ECC) {
        let errors = apply_ecc(&mut buf)?;
        assert!(errors > 0);
        reader_writer.seek(std::io::SeekFrom::Start(start))?;
        reader_writer.write_all(&buf)?;
        errors
    }else{0};
    Ok(errors)
}

/// Reader should be positioned at the start of a header.
/// Returns Ok(errors_corrected, ComponentHeader)
pub fn read_header<RW:std::io::Write + std::io::Read + std::io::Seek>(reader_writer:&mut RW,error_correct:bool)->Result<(usize,ComponentHeader),ReadWriteError>{
    let mut header = [0u8;HEADER_LEN+ECC_LEN];
    let start = reader_writer.seek(std::io::SeekFrom::Current(0))?;
    reader_writer.read_exact(&mut header[..])?;
    let errors = if error_correct {
        let errors = apply_ecc(&mut header)?;
        if errors > 0 {
            reader_writer.seek(std::io::SeekFrom::Start(start))?;
            reader_writer.write_all(&header)?;
        }
        errors
    }else{0};
    Ok((errors,ComponentHeader::new(&header[0..HEADER_LEN],start)))
}
/// Reader should be positioned at the start of a header.
/// Returns Ok(errors_corrected, ComponentHeader)
pub fn read_content_header<RW:std::io::Write + std::io::Read + std::io::Seek, B:BlockInputs>(reader_writer:&mut RW,error_correct:bool,hasher:&mut B)->Result<(usize,ComponentHeader),ReadWriteError>{
    let mut header = [0u8;HEADER_LEN+ECC_LEN];
    let start = reader_writer.seek(std::io::SeekFrom::Current(0))?;
    reader_writer.read_exact(&mut header[..])?;
    let errors = if error_correct {
        let errors = apply_ecc(&mut header)?;
        if errors > 0 {
            reader_writer.seek(std::io::SeekFrom::Start(start))?;
            reader_writer.write_all(&header)?;
        }
        errors
    }else{0};
    hasher.update(&header);
    Ok((errors,ComponentHeader::new(&header[0..HEADER_LEN],start)))
}

///Reader should be positioned at the start of the hash (after the read of the end header).
/// Returns Ok(errors_corrected, BlockHash)
pub fn read_hash<RW:  std::io::Write + std::io::Read + std::io::Seek>(reader_writer:&mut RW,error_correct:bool)->Result<(usize,BlockHash),ReadWriteError>{
    let mut hash = [0u8;HASH_AND_ECC_LEN];
    let start = reader_writer.seek(std::io::SeekFrom::Current(0))?;
    reader_writer.read_exact(&mut hash[..])?;
    let errors = if error_correct {
        let errors = apply_ecc(&mut hash)?;
        if errors > 0 {
            reader_writer.seek(std::io::SeekFrom::Start(start))?;
            reader_writer.write_all(&hash)?;
        }
        errors
    }else{0};
    Ok((errors,BlockHash::new(hash)))
}

///This will read the data from the file and into the given destination writer.
pub fn load_content<RW:std::io::Write + std::io::Read + std::io::Seek,W:std::io::Write>(reader_writer:&mut RW,dest:&mut W,content_info:&Content)->Result<(),ReadWriteError>{
    let Content { data_len, data_start,  ..} = *content_info;
    reader_writer.seek(std::io::SeekFrom::Start(data_start))?;
    copy_n(reader_writer, dest, data_len as usize)?;
    Ok(())
}
/// This is used to during block verification. It does not error correct, since on the first read through we rather just hash it, since ecc is expensive.
/// Reader should be position at the start of the content portion (ecc bytes if present, else the data bytes).
pub fn read_content<RW:std::io::Write + std::io::Read + std::io::Seek, B:BlockInputs>(reader_writer:&mut RW,content_info:&Content,error_correct:bool,hasher:&mut B)->Result<(usize,Vec<CorruptDataSegment>),ReadWriteError>{
    let Content { data_len, data_start, ecc , ..} = *content_info;
    let ecc_len = if ecc{calc_ecc_data_len(data_len as usize)}else{0};
    let to_read = data_len as usize + ecc_len;
    let cursor_start = data_start - ecc_len as u64;
    let mut corruption = Vec::new();
    reader_writer.seek(std::io::SeekFrom::Start(cursor_start))?;//should already be positioned here
    if !ecc || (ecc && !error_correct) {
        buffer_hash(reader_writer, to_read as usize, hasher)?;
        return Ok((0,corruption))
    }
    let num_chunks = ecc_len/ECC_LEN;
    let mut ecc_data = vec![0u8;ecc_len];
    reader_writer.read_exact(&mut ecc_data[..])?;
    let mut data = [0u8;DATA_SIZE+ECC_LEN];
    let mut tot_errors = 0;

    for i in 0..num_chunks {
        let data_chunk_end = if i+1 < num_chunks{DATA_SIZE}else{data_len as usize%DATA_SIZE};
        let chunk_end = data_chunk_end + ECC_LEN;
        let (e_s,e_e) = (i*ECC_LEN,(i*ECC_LEN)+ECC_LEN);
        {
            let (d,e) = data.split_at_mut(data_chunk_end);
            reader_writer.read_exact(d)?;
            e[..ECC_LEN].copy_from_slice(&ecc_data[e_s..e_e])
        }
        let (crsr_e,crsr_d) = (cursor_start + (i*ECC_LEN) as u64, cursor_start + (ecc_len + (i*DATA_SIZE)) as u64);
        match apply_ecc(&mut data[..chunk_end]) {
            Ok(errors) => {
                if errors == 0 {continue;}
                //seek to ecc slot, write
                reader_writer.seek(std::io::SeekFrom::Start(crsr_e))?;
                reader_writer.write_all(&data[data_chunk_end..chunk_end])?;
                //seek to data chunk, write
                reader_writer.seek(std::io::SeekFrom::Start(crsr_d))?;
                reader_writer.write_all(&data[..data_chunk_end])?;
                tot_errors += errors;
            },
            Err(_) => {
                corruption.push(CorruptDataSegment::EccChunk{ chunk_start: crsr_d, chunk_ecc_start: crsr_e, ecc_start: cursor_start, data_start, data_len })
            },
        }
    }
    reader_writer.seek(std::io::SeekFrom::Start(cursor_start))?;
    buffer_hash(reader_writer, to_read, hasher)?;
    Ok((tot_errors, corruption))
}

pub fn buffer_hash<R:std::io::Read, B:BlockInputs>(reader:&mut R,mut num_bytes:usize,hasher:&mut B)->std::io::Result<()>{
    const BUF_LEN:usize = 4096;
    let mut buf = [0u8;BUF_LEN];
    while num_bytes > 0 {
        let bytes_read = reader.read(&mut buf[..num_bytes.min(BUF_LEN)])?;
        if bytes_read > 0 {
            hasher.update(&buf[..bytes_read]);
        }else{// 0 == EOF
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Unexpected end of file").into());       
        }
        num_bytes -= bytes_read;
    }
    Ok(())
}

#[derive(Debug)]
pub enum BlockMiddleState{
    InvalidBlockStructure{last_good_component_end:u64},
    UnexpectedEof{last_good_component_end:u64,hash_at_last_good_component:[u8;HASH_LEN],content:Vec<(ComponentHeader,Content)>},
    DataCorruption{component_start:u64,component_tag:ComponentTag},
    BBlock { middle: Vec<(ComponentHeader,Content)>, end: BlockEnd, errors_corrected: usize , hash:[u8;HASH_LEN],corrupted_content_blocks:Vec<CorruptDataSegment>}
}

/// This is a wrapper to just keep reading all the content.
/// If hasher is Some, this will hash && !ecc, if none it will !hash && ecc. 
/// The reader should be positioned after reading a BBlockStart header
pub fn read_block_middle<RW:std::io::Write + std::io::Read + std::io::Seek, B:BlockInputs>(reader_writer:&mut RW,error_correct_header:bool,error_correct_content:bool)->Result<BlockMiddleState,ReadWriteError>{
    let mut middle = Vec::new();
    let mut errors_corrected = 0;
    let mut hasher = B::new();
    let mut corrupted_content_blocks = Vec::new();
    loop{
        let last_good_component_end = reader_writer.seek(std::io::SeekFrom::Current(0))?;
        let hash_at_last_good_component = hasher.finalize();
        let (errs,header) = match read_content_header(reader_writer,error_correct_header,&mut hasher){
            Ok(a) => a,
            Err(ReadWriteError::EndOfFile) => {
                return Ok(BlockMiddleState::UnexpectedEof { last_good_component_end,hash_at_last_good_component,content:middle })
            },
            Err(ReadWriteError::EccTooManyErrors) => {
                return Ok(BlockMiddleState::DataCorruption { component_start: last_good_component_end,component_tag:ComponentTag::Header})
            },
            Err(e)=>return Err(e)
        };
        errors_corrected += errs;
        match header.tag() {
            BlockTag::StartABlock |
            BlockTag::StartAEBlock |
            BlockTag::StartBBlock => {
                return Ok(BlockMiddleState::InvalidBlockStructure { last_good_component_end })
            },
            BlockTag::CComponent |
            BlockTag::CEComponent => {
                let content = header.as_content();
                match read_content(reader_writer, &content, error_correct_content,&mut hasher) {
                    Ok((errs,cc)) => {
                        let Content { data_len, data_start, ecc } = content;
                        errors_corrected += errs;
                        if !ecc && error_correct_content {
                            corrupted_content_blocks.push(CorruptDataSegment::MaybeCorrupt { data_start, data_len })
                        }else{
                            corrupted_content_blocks.extend_from_slice(cc.as_slice());
                        }
                    },
                    Err(ReadWriteError::EndOfFile) => {
                        return Ok(BlockMiddleState::UnexpectedEof { last_good_component_end,hash_at_last_good_component,content:middle})
                    },
                    Err(e)=>return Err(e)
                }
                middle.push((header,content));
            },
            BlockTag::EndBlock => {
                let (errs,hash) = match read_hash(reader_writer,false) {
                    Ok(a) => a,
                    Err(ReadWriteError::EndOfFile) => {
                        return Ok(BlockMiddleState::UnexpectedEof { last_good_component_end,hash_at_last_good_component,content:middle })
                    },
                    Err(ReadWriteError::EccTooManyErrors) => {
                        return Ok(BlockMiddleState::DataCorruption { component_start: last_good_component_end,component_tag:ComponentTag::Hash})
                    },
                    Err(e)=>return Err(e)
                };
                errors_corrected += errs;
                if hash.hash() == hash_at_last_good_component && error_correct_content{
                    corrupted_content_blocks.clear();//we loaded up all the non ecc Contents to this vec in case hash didn't check out
                }
                let end = BlockEnd{ header, hash };
                return Ok(BlockMiddleState::BBlock { middle, end, errors_corrected,hash:hash_at_last_good_component,corrupted_content_blocks })
            },
        }
    }

} 

fn copy_n<R: std::io::Read, W: std::io::Write>(reader: &mut R, writer: &mut W, n: usize) -> std::io::Result<()> {
    const BUFFER_SIZE: usize = 4096;
    let mut buffer = [0; BUFFER_SIZE];
    let mut to_read = n;

    while to_read > 0 {
        let read = reader.read(&mut buffer[..BUFFER_SIZE.min(to_read)])?;
        if read == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Didn't reach expected number of bytes"));
        }
        writer.write_all(&buffer[..read])?;
        to_read -= read;
    }

    Ok(())
}