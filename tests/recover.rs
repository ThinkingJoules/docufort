
mod common;

use common::*;
use docufort::*;
use docufort::{core::*,recovery::*,write::*};

#[test]
fn test_block_1_hash() {
    let mut hasher = DummyInput::new();
    let mut cursor = Cursor::new(Vec::new());

    println!("CONTENT COMPONENT START: {}",cursor.position());
    write_content_component(&mut cursor, false,None, B_CONTENT, &mut hasher).unwrap();
    
    println!("CONTENT COMPONENT START: {}",cursor.position());
    write_content_component(&mut cursor, true,None, B_CONTENT, &mut hasher).unwrap();
    
    println!("CONTENT COMPONENT START: {}",cursor.position());
    write_content_component(&mut cursor, false,None, B_CONTENT, &mut hasher).unwrap();
    assert_eq!(hasher.finalize(),BLOCK_1_HASH);
}
#[test]
fn test_find_block_start_after_truncation() {
    let cursor = generate_test_file();
    let mut vec = cursor.into_inner();
    for (trunc,ans) in [(290,268),(200,184),(100,23)]{
        vec.truncate(trunc);
        let mut crsr = Cursor::new(vec);
        crsr.seek(std::io::SeekFrom::End(0)).unwrap();
        let position = find_block_start(&mut crsr).unwrap();
        assert_eq!(position,ans);
        vec = crsr.into_inner();

    }
}
#[test]
fn test_try_read_block_3_clean() {
    let mut cursor = generate_test_file();
    cursor.set_position(268);
    let res = try_read_block::<_,DummyInput>(&mut cursor, false,false);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::Closed(BlockReadSummary { errors_corrected, block, hash_as_read, .. }) => {
            assert_eq!(&hash_as_read[..],block.take_end().hash.hash());
            assert_eq!(&hash_as_read[..],BLOCK_3_HASH);
            assert_eq!(errors_corrected,0);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_3_one_err_data() {
    let mut cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HASH_AND_ECC_LEN;
    let mut v = cursor.into_inner();
    v[content_start] |= 128; // should set a bit high on the utf 8 str, making an illegal char.
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, false,false);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::Closed(BlockReadSummary { errors_corrected, block, hash_as_read, .. }) => {
            assert_ne!(&hash_as_read[..],block.take_end().hash.hash());
            assert_eq!(errors_corrected,0);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_3_data_recovery() {
    let mut cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN;
    let mut v = cursor.into_inner();
    v[content_start] ^= v[content_start];
    v[content_start+2] ^= v[content_start+2]; 
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, true,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::Closed(BlockReadSummary { errors_corrected, block, hash_as_read, .. }) => {
            assert_eq!(&hash_as_read[..],block.clone().take_end().hash.hash());
            assert_eq!(errors_corrected,2);
            if let Block::A { middle, .. } = block {
                let Content{ data_len, data_start, ecc } = middle;
                assert!(ecc);
                cursor.set_position(data_start);
                let mut data = vec![0u8;data_len as usize];
                cursor.read_exact(&mut data).unwrap();
                assert_eq!(data.as_slice(),A_CONTENT.as_slice());
            }
            
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_3_one_err_data_corrected() {
    let mut cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN;
    let mut v = cursor.into_inner();
    v[content_start] |= 128; // should set a bit high on the utf 8 str, making an illegal char.
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, true,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::Closed(BlockReadSummary { errors_corrected, block, hash_as_read, .. }) => {
            assert_eq!(&hash_as_read[..],block.take_end().hash.hash());
            assert_eq!(errors_corrected,1);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_3_one_err_corrected() {
    let mut cursor = generate_test_file();
    let block_start = 268;
    let mut v = cursor.into_inner();
    v[block_start] ^= v[block_start]; // invert all the bits on the tag for the block
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, true,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::Closed(BlockReadSummary { errors_corrected, block, hash_as_read, .. }) => {
            assert_eq!(&hash_as_read[..],block.take_end().hash.hash());
            assert_eq!(errors_corrected,1);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_3_header_corruption() {
    let mut cursor = generate_test_file();
    let block_start = 268;
    let mut v = cursor.into_inner();
    v[block_start] ^= v[block_start]; // invert all the bits
    v[block_start+1] ^= v[block_start+1]; 
    v[block_start+2] ^= v[block_start+2]; 
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, true,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::ProbablyNotStartHeader { start_from } => {
            assert_eq!(start_from,block_start as u64);
            
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_3_data_corruption() {
    let mut cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN;
    let mut v = cursor.into_inner();
    v[content_start] ^= v[content_start]; // invert all the bits
    v[content_start+2] ^= v[content_start+2]; 
    v[content_start+3] ^= v[content_start+3]; 
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, true,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::Closed(BlockReadSummary { corrupted_content_blocks,.. }) => {
            assert_eq!(corrupted_content_blocks.len(),1);
            let cc = CorruptDataSegment::EccChunk { chunk_start: 289, chunk_ecc_start: 285, ecc_start: 285, data_start: (content_start + ECC_LEN)as u64 , data_len: A_CONTENT.len() as u32 };
            assert_eq!(corrupted_content_blocks[0],cc);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_3_truncate_in_data() {
    let mut cursor = generate_test_file();
    let block_start = 268;
    let mut v = cursor.into_inner();
    v.truncate(block_start+HEADER_LEN+ECC_LEN+4);//part way through the data
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, true,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::OpenABlock { truncate_at } => {
            assert_eq!(truncate_at,(block_start-MN_ECC_LEN) as u64);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_3_truncate_in_header() {
    let mut cursor = generate_test_file();
    let block_start = 268;
    let mut v = cursor.into_inner();
    v.truncate(block_start+HEADER_LEN+ECC_LEN-4);//part way through the data
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, true,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::IncompleteStartHeader { truncate_at } => {
            assert_eq!(truncate_at,(block_start-MN_ECC_LEN) as u64);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_1_clean() {
    let mut cursor = generate_test_file();
    let block_start = 23;
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, false,false);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::Closed(BlockReadSummary { errors_corrected, block, hash_as_read, corrupted_content_blocks,.. }) => {
            assert!(corrupted_content_blocks.is_empty());
            assert_eq!(errors_corrected,0);
            assert_eq!(&hash_as_read[..],block.take_end().hash.hash());
            assert_eq!(&hash_as_read[..],BLOCK_1_HASH);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_1_corrupt() {
    let mut cursor = generate_test_file();
    let block_start = 23;
    let mut file_contents = cursor.into_inner();
    let content_start1 = 40 + HEADER_LEN + ECC_LEN;
    file_contents[content_start1] ^= file_contents[content_start1];
    file_contents[content_start1+2] ^= file_contents[content_start1+2]; 
    cursor = Cursor::new(file_contents);
    cursor.set_position(block_start);
    dbg!(&B_CONTENT);
    let res = try_read_block::<_,DummyInput>(&mut cursor, false,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::Closed(BlockReadSummary { errors_corrected, block, hash_as_read, corrupted_content_blocks,.. }) => {
            assert!(corrupted_content_blocks.len() == 2);
            assert_eq!(errors_corrected,0);
            assert_ne!(&hash_as_read[..],block.take_end().hash.hash());
            assert_ne!(&hash_as_read[..],BLOCK_1_HASH);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_1_truncate_in_data() {
    let mut cursor = generate_test_file();
    let block_start = 23;
    let mut v = cursor.into_inner();
    v.truncate(block_start+HEADER_LEN*2+ECC_LEN*2+4);//part way through the data
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, true,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::OpenBBlock { truncate_at, hash_for_end, errors ,..} => {
            //This leaves no content, but that is technically a valid B Block.
            assert_eq!(truncate_at,(block_start+HEADER_LEN+ECC_LEN) as u64);
            assert_eq!(hash_for_end,NULL_HASH);
            assert_eq!(errors,0);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}
#[test]
fn test_try_read_block_1_truncate_in_header() {
    let mut cursor = generate_test_file();
    let block_start = 184;
    let mut v = cursor.into_inner();
    v.truncate(block_start+HEADER_LEN+ECC_LEN-4);//part way through the data
    cursor = Cursor::new(v);
    cursor.set_position(block_start as u64);
    let res = try_read_block::<_,DummyInput>(&mut cursor, true,true);
    assert!(res.is_ok());
    match res.unwrap() {
        BlockState::IncompleteStartHeader { truncate_at } => {
            assert_eq!(truncate_at,(block_start-MN_ECC_LEN) as u64);
        },
        a => panic!("Invalid Read: {:?}",a),
    }
}

use std::io::{Write, Cursor, Read, Seek};
use std::fs::OpenOptions;

fn write_bytes_to_file(file_path: &std::path::Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(file_path)
        .unwrap();
    file.write_all(bytes).unwrap();
}
use std::fs;
fn setup_test_file(suffix:&str) -> std::path::PathBuf {
    let s = format!("./target/tmp/test_file_{}.bin",suffix);
    let path = std::path::Path::new(s.as_str());
    if path.exists() {
        fs::remove_file(path).unwrap();
    }
    path.to_path_buf()
}
fn cleanup_test_file(path: std::path::PathBuf) {
    if path.exists() {
        fs::remove_file(&path).unwrap_or_else(|err| {
            eprintln!("Failed to delete the file: {:?}", err);
        });
    }
}
#[test]
fn test_empty_file_recovery() {
    let path = setup_test_file("empty");
    let summary = recover_tail::<DummyInput>(&path);
    cleanup_test_file(path);
    assert!(summary.is_err());
}
#[test]
fn test_file_with_incomplete_header() {
    let path = setup_test_file("bad_header");
    // Using arbitrary bytes that could represent an incomplete header
    write_bytes_to_file(&path, &[0x01, 0x02, 0x03]);
    let summary = recover_tail::<DummyInput>(&path);
    cleanup_test_file(path);
    assert!(summary.is_err());
}
#[test]
fn test_tail_recovery_clean() {
    let path = setup_test_file("clean");
    let cursor = generate_test_file();
    let file_content = cursor.into_inner();
    write_bytes_to_file(&path, &file_content);
    let summary = recover_tail::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let TailRecoverySummary {
        original_file_len, 
        recovered_file_len,  
        has_blocks, 
        tot_errors_corrected,.. 
    } = summary;
    assert_eq!(tot_errors_corrected, 0);
    assert_eq!(original_file_len, recovered_file_len);
    assert!(has_blocks);
}
#[test]
fn test_tail_recovery_trailing_truncate() {
    let path = setup_test_file("trail");
    let cursor = generate_test_file();
    let mut file_content = cursor.into_inner();
    file_content.extend_from_slice(&MAGIC_NUMBER);
    write_bytes_to_file(&path, &file_content);
    let summary = recover_tail::<DummyInput>(&path.as_path()).unwrap();
    cleanup_test_file(path);
    let TailRecoverySummary {
        original_file_len, 
        recovered_file_len,  
        has_blocks, 
        tot_errors_corrected,
        file_ops,
        corrupted_content_blocks, 
    } = summary;
    assert_eq!(tot_errors_corrected, 0);
    assert_eq!(file_ops.len(), 1);
    assert_eq!(original_file_len-MAGIC_NUMBER.len() as u64, recovered_file_len);
    assert!(has_blocks);
    assert!(corrupted_content_blocks.is_empty());
}
#[test]
fn test_tail_recovery_open_a_data() {
    let path = setup_test_file("open_a_d");
    let cursor = generate_test_file();
    let block_start = 268;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN+ECC_LEN+4;
    file_content.truncate(new_len);//part way through the data
    write_bytes_to_file(&path, &file_content);
    let summary = recover_tail::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let TailRecoverySummary {
        original_file_len, 
        recovered_file_len,  
        has_blocks, 
        tot_errors_corrected,
        file_ops,
        corrupted_content_blocks,
    } = summary;
    assert_eq!(tot_errors_corrected, 0);
    assert_eq!(original_file_len as usize, new_len);
    assert_eq!(recovered_file_len as usize,block_start-MN_ECC_LEN);
    assert!(has_blocks);
    assert_eq!(file_ops.len(), 2);
    assert!(corrupted_content_blocks.is_empty());

}
#[test]
fn test_tail_recovery_open_a_header() {
    let path = setup_test_file("open_a_h");
    let cursor = generate_test_file();
    let block_start = 268;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN+ECC_LEN-4;
    file_content.truncate(new_len);//part way through the data
    write_bytes_to_file(&path, &file_content);
    let summary = recover_tail::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let TailRecoverySummary {
        original_file_len, 
        recovered_file_len,  
        has_blocks, 
        tot_errors_corrected,
        file_ops,
        corrupted_content_blocks 
    } = summary;
    assert_eq!(tot_errors_corrected, 0);
    assert_eq!(original_file_len as usize, new_len);
    assert_eq!(recovered_file_len as usize,block_start-MN_ECC_LEN);
    assert!(has_blocks);
    assert_eq!(file_ops.len(), 2);
    assert!(corrupted_content_blocks.is_empty());

}
#[test]
fn test_tail_recovery_open_b() {
    let path = setup_test_file("open_b");
    let cursor = generate_test_file();
    let block_start = 23;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN*2+ECC_LEN*2+4;
    file_content.truncate(new_len);//part way through the data
    write_bytes_to_file(&path, &file_content);
    let summary = recover_tail::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let TailRecoverySummary {
        original_file_len, 
        recovered_file_len,  
        has_blocks, 
        tot_errors_corrected,
        file_ops,
        corrupted_content_blocks,
    } = summary;
    assert_eq!(tot_errors_corrected, 0);
    assert_eq!(original_file_len as usize, new_len);
    assert_eq!(recovered_file_len as usize,81);
    assert!(has_blocks);
    assert_eq!(file_ops.len(), 2);
    assert!(corrupted_content_blocks.is_empty());

}
#[test]
fn test_tail_test_recovery_ecc_block_3_data() {
    let path = setup_test_file("ecc_3_data");
    let cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN;
    let orig = cursor.into_inner();
    let mut file_contents = orig.clone();
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start+2] ^= file_contents[content_start+2]; 
    assert_ne!(orig,file_contents);
    write_bytes_to_file(&path, &file_contents);
    let summary = recover_tail::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let TailRecoverySummary {
        original_file_len, 
        recovered_file_len,  
        has_blocks, 
        tot_errors_corrected,
        file_ops,
        corrupted_content_blocks,
    } = summary;
    assert_eq!(tot_errors_corrected, 2);
    assert_eq!(original_file_len,recovered_file_len);
    assert!(has_blocks);
    assert_eq!(file_ops.len(), 2);//one for a closed read, then another for ecc application on content
    assert!(corrupted_content_blocks.is_empty());
}
#[test]
fn test_tail_test_recovery_ecc_block_3_header() {
    let path = setup_test_file("ecc_3_head");
    let cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN - 3;
    let orig = cursor.into_inner();
    let mut file_contents = orig.clone();
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start+2] ^= file_contents[content_start+2]; 
    assert_ne!(orig,file_contents);
    write_bytes_to_file(&path, &file_contents);
    let summary = recover_tail::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let TailRecoverySummary {
        original_file_len, 
        recovered_file_len,  
        has_blocks, 
        tot_errors_corrected,
        file_ops,
        corrupted_content_blocks,
    } = summary;
    assert_eq!(tot_errors_corrected, 2);
    assert_eq!(original_file_len,recovered_file_len);
    assert!(has_blocks);
    assert_eq!(file_ops.len(), 1);//one for a closed read
    assert!(corrupted_content_blocks.is_empty());
}
#[test]
fn test_tail_test_recovery_open_3_corrupt_2() {
    let path = setup_test_file("open3_corrupt_2");
    let cursor = generate_test_file();
    let block_start = 268;
    let orig = cursor.into_inner();
    let mut file_contents = orig.clone();
    let new_len = block_start+HEADER_LEN+ECC_LEN+4;
    file_contents.truncate(new_len);//part way through the data

    let block_start = 184;
    let content_start = block_start + HEADER_LEN + ECC_LEN;
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start+2] ^= file_contents[content_start+2]; 
    assert_ne!(orig,file_contents);
    write_bytes_to_file(&path, &file_contents);
    let summary = recover_tail::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let TailRecoverySummary {
        original_file_len, 
        recovered_file_len,  
        has_blocks, 
        tot_errors_corrected,
        file_ops,
        corrupted_content_blocks,
    } = summary;
    //file_ops.iter().for_each(|(i,o)|{dbg!(i,o);});
    assert_eq!(tot_errors_corrected, 0);
    assert_eq!(recovered_file_len as usize,256);
    assert_eq!(original_file_len,new_len as u64);
    assert!(has_blocks);
    assert_eq!(file_ops.len(), 3);//OpenA, Closed mismatch, closed corrupted content block
    let cc = CorruptDataSegment::Corrupt { data_start: content_start as u64 , data_len: A_CONTENT.len() as u32 };
    assert_eq!(corrupted_content_blocks[0],cc);
    
    for (i,(l,o)) in file_ops.into_iter().enumerate(){
        //dbg!(&i,&o);
        match i {
            0 =>{
                assert_eq!(l,268);
                let expects = BlockState::OpenABlock { truncate_at: 256 };
                assert_eq!(&o,&expects);
            }
            1 =>{
                assert_eq!(l,184);
                if let BlockState::Closed(BlockReadSummary {block, hash_as_read,.. }) = o {
                    assert_ne!(block.take_end().hash.hash(),&hash_as_read[..]);
                }else{
                    panic!("Expected a closed block state!")
                }
            }
            2 =>{
                assert_eq!(l,184);
                if let BlockState::Closed(BlockReadSummary {block, hash_as_read,.. }) = o {
                    assert_ne!(block.take_end().hash.hash(),&hash_as_read[..]);
                }else{
                    panic!("Expected a closed block state!")
                }
            }
            _ => panic!("Too many ops!")
        }
    }
}
#[test]
fn test_tail_test_recovery_open_2_corrupt_1() {
    let path = setup_test_file("open2_corrupt_1");
    let cursor = generate_test_file();
    let block_start = 184;
    let orig = cursor.into_inner();
    let mut file_contents = orig.clone();
    let new_len = block_start+HEADER_LEN+ECC_LEN+4;
    file_contents.truncate(new_len);//part way through the data

    let content_start1 = 40 + HEADER_LEN + ECC_LEN;
    let content_start3 = 102 + HEADER_LEN + ECC_LEN;
    file_contents[content_start1] ^= file_contents[content_start1];
    file_contents[content_start1+2] ^= file_contents[content_start1+2]; 
    assert_ne!(orig,file_contents);
    write_bytes_to_file(&path, &file_contents);
    let summary = recover_tail::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let TailRecoverySummary {
        original_file_len, 
        recovered_file_len,  
        has_blocks, 
        tot_errors_corrected,
        file_ops,
        corrupted_content_blocks,
    } = summary;
    //file_ops.iter().for_each(|(i,o)|{dbg!(i,o);});
    // dbg!(&corrupted_content_blocks);
    assert_eq!(tot_errors_corrected, 0);
    assert_eq!(recovered_file_len as usize,172);
    assert_eq!(original_file_len,new_len as u64);
    assert!(has_blocks);
    assert_eq!(file_ops.len(), 3);//one for a closed read, then another for ecc application on content
    let cc1 = CorruptDataSegment::MaybeCorrupt { data_start: content_start1 as u64 , data_len: B_CONTENT.len() as u32 };
    let cc2 = CorruptDataSegment::MaybeCorrupt { data_start: content_start3 as u64 , data_len: B_CONTENT.len() as u32 };
    assert_eq!(corrupted_content_blocks[0],cc1);
    assert_eq!(corrupted_content_blocks[1],cc2);

    for (i,(l,o)) in file_ops.into_iter().enumerate(){
        //dbg!(&i,&o);
        match i {
            0 =>{
                assert_eq!(l,184);
                let expects = BlockState::OpenABlock { truncate_at: 172 };
                assert_eq!(&o,&expects);
            }
            1 =>{
                assert_eq!(l,23);
                if let BlockState::Closed(BlockReadSummary {block, hash_as_read,.. }) = o {
                    assert_ne!(block.take_end().hash.hash(),&hash_as_read[..]);
                }else{
                    panic!("Expected a closed block state!")
                }
            }
            2 =>{
                assert_eq!(l,23);
                if let BlockState::Closed(BlockReadSummary {block, hash_as_read,.. }) = o {
                    assert_ne!(block.take_end().hash.hash(),&hash_as_read[..]);
                }else{
                    panic!("Expected a closed block state!")
                }
            }
            _ => panic!("Too many ops!")
        }
    }
}