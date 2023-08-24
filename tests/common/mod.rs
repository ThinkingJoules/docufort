#![allow(dead_code)]

#[derive(Clone, Debug)]
pub struct DummyInput {
    hasher: blake3::Hasher,
}
impl BlockInputs for DummyInput {
    fn new() -> Self {
        DummyInput {
            hasher: blake3::Hasher::new(),
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    fn finalize(&self) -> [u8; HASH_LEN] {
        let hash = self.hasher.finalize();
        let mut result = [0u8; HASH_LEN];
        result.copy_from_slice(&hash.as_bytes()[..HASH_LEN]);
        result
    }

    fn current_timestamp() -> u64 {
        u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])
    }
}

use std::io::Cursor;
use docufort::*;
use docufort::{write::*, core::*};

pub const B_CONTENT:&[u8;12] = b"Some content";
pub const A_CONTENT:&[u8;14] = b"Atomic content";

pub fn generate_test_file() -> Cursor<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    let mut hasher = DummyInput::new();
    let log_pos = true;
    // Init the file with header
    init_file(&mut cursor).unwrap();
    if log_pos {println!("MN START: {}",cursor.position())};
    write_magic_number(&mut cursor).unwrap();

    // Write BlockStart for Best Effort Block
    if log_pos {println!("BLOCK START: {}",cursor.position())};
    let b_block_header = ComponentHeader::new_from_parts(HeaderTag::StartBBlock as u8, DummyInput::current_timestamp().to_be_bytes(), None);
    write_header(&mut cursor, &b_block_header).unwrap();

    // Write 3 Content Components
    if log_pos {println!("CONTENT COMPONENT START: {}",cursor.position())};
    write_content_component(&mut cursor, false,None, None,B_CONTENT, &mut hasher).unwrap();
    
    if log_pos {println!("CONTENT COMPONENT START: {}",cursor.position())};
    write_content_component(&mut cursor, true,None, None,B_CONTENT, &mut hasher).unwrap();
    
    if log_pos {println!("CONTENT COMPONENT START: {}",cursor.position())};
    write_content_component(&mut cursor, false,None, None,B_CONTENT, &mut hasher).unwrap();


    let b_block_hash = hasher.finalize();
    let block_end_header = ComponentHeader::new_from_parts(HeaderTag::EndBlock as u8, DummyInput::current_timestamp().to_be_bytes(), None);
    write_block_end(&mut cursor, &block_end_header, &b_block_hash).unwrap();
    
    if log_pos {println!("MN START: {}",cursor.position())};
    write_magic_number(&mut cursor).unwrap();
    if log_pos {println!("BLOCK START: {}",cursor.position())};
    write_atomic_block::<_,DummyInput>(&mut cursor, None, A_CONTENT, false, None,None).unwrap();
    
    if log_pos {println!("MN START: {}",cursor.position())};
    write_magic_number(&mut cursor).unwrap();
    if log_pos {println!("BLOCK START: {}",cursor.position())};
    write_atomic_block::<_,DummyInput>(&mut cursor, None, A_CONTENT, true, None,None).unwrap();


    cursor
}
pub const NULL_HASH:[u8;HASH_LEN] = [175, 19, 73, 185, 245, 249, 161, 166, 160, 64, 77, 234, 54, 220, 201, 73, 155, 203, 37, 201];
pub const BLOCK_1_HASH:[u8;HASH_LEN] = [33, 215, 215, 192, 27, 1, 94, 58, 192, 97, 207, 38, 108, 77, 159, 4, 65, 107, 184, 244];
pub const BLOCK_3_HASH:[u8;HASH_LEN] = [59, 64, 117, 102, 139, 248, 203, 101, 132, 81, 227, 62, 79, 23, 156, 103, 106, 46, 127, 152];