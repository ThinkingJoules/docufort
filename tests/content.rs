
mod common;

use common::*;
use docufort::*;
use docufort::content_reader::find_content;

use std::io::Cursor;


#[test]
fn test_find_content_clean() {
    let mut cursor = generate_test_file();
    let summary = find_content::<_,DummyInput,_>(&mut cursor,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    assert_eq!(summary.len(),5);
}
#[test]
fn test_find_content_trailing_truncate() {
    let cursor = generate_test_file();
    let mut file_content = cursor.into_inner();
    file_content.extend_from_slice(&MAGIC_NUMBER);
    let mut cursor = Cursor::new(file_content);
    let summary = find_content::<_,DummyInput,_>(&mut cursor,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    assert_eq!(summary.len(),5);
}
#[test]
fn test_find_content_open_a_data() {
    let cursor = generate_test_file();
    let block_start = 268;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN+ECC_LEN+4;
    file_content.truncate(new_len);//part way through the data
    let mut cursor = Cursor::new(file_content);
    let summary = find_content::<_,DummyInput,_>(&mut cursor,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    assert_eq!(summary.len(),4);
}
#[test]
fn test_find_content_open_a_header() {
    let cursor = generate_test_file();
    let block_start = 268;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN+ECC_LEN-4;
    file_content.truncate(new_len);//part way through the data
    let mut cursor = Cursor::new(file_content);
    let summary = find_content::<_,DummyInput,_>(&mut cursor,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    assert_eq!(summary.len(),4);
}
#[test]
fn test_find_content_open_b() {
    let cursor = generate_test_file();
    let block_start = 23;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN*2+ECC_LEN*2+4;
    file_content.truncate(new_len);//part way through the data
    let mut cursor = Cursor::new(file_content);
    let summary = find_content::<_,DummyInput,_>(&mut cursor,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    assert_eq!(summary.len(),0);
}
#[test]
fn test_find_content_ecc_block_3_data() {
    let cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN;
    let orig = cursor.into_inner();
    let mut file_contents = orig.clone();
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start+2] ^= file_contents[content_start+2];
    assert_ne!(orig,file_contents);
    let mut cursor = Cursor::new(file_contents);
    let summary = find_content::<_,DummyInput,_>(&mut cursor,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    assert_eq!(summary.len(),5);
}
#[test]
fn test_find_content_ecc_block_3_header() {
    let cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN - 3;
    let orig = cursor.into_inner();
    let mut file_contents = orig.clone();
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start+2] ^= file_contents[content_start+2];
    assert_ne!(orig,file_contents);
    let mut cursor = Cursor::new(file_contents);
    let summary = find_content::<_,DummyInput,_>(&mut cursor,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    assert_eq!(summary.len(),5);
}
#[test]
fn test_find_content_open_3_corrupt_2() {
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
    let mut cursor = Cursor::new(file_contents);
    let summary = find_content::<_,DummyInput,_>(&mut cursor,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    assert_eq!(summary.len(),4);
}
#[test]
fn test_find_content_open_2_corrupt_1() {
    let cursor = generate_test_file();
    let block_start = 184;
    let orig = cursor.into_inner();
    let mut file_contents = orig.clone();
    let new_len = block_start+HEADER_LEN+ECC_LEN+4;
    file_contents.truncate(new_len);//part way through the data
    let content_start1 = 40 + HEADER_LEN + ECC_LEN;
    file_contents[content_start1] ^= file_contents[content_start1];
    file_contents[content_start1+2] ^= file_contents[content_start1+2];
    assert_ne!(orig,file_contents);
    let mut cursor = Cursor::new(file_contents);
    let summary = find_content::<_,DummyInput,_>(&mut cursor,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    assert_eq!(summary.len(),3);
}