
mod common;

use common::*;
use docufort::*;
use docufort::content_reader::find_content;

use std::io::Write;
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
fn test_find_content_clean() {
    let path = setup_test_file("content");
    let cursor = generate_test_file();
    let file_content = cursor.into_inner();
    write_bytes_to_file(&path, &file_content);
    let summary = find_content::<DummyInput,_>(&path,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    cleanup_test_file(path);
    assert_eq!(summary.len(),5);
}
#[test]
fn test_find_content_trailing_truncate() {
    let path = setup_test_file("trail_i");
    let cursor = generate_test_file();
    let mut file_content = cursor.into_inner();
    file_content.extend_from_slice(&MAGIC_NUMBER);
    write_bytes_to_file(&path, &file_content);
    let summary = find_content::<DummyInput,_>(&path,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    cleanup_test_file(path);
    assert_eq!(summary.len(),5);
}
#[test]
fn test_find_content_open_a_data() {
    let path = setup_test_file("open_a_d_i");
    let cursor = generate_test_file();
    let block_start = 268;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN+ECC_LEN+4;
    file_content.truncate(new_len);//part way through the data
    write_bytes_to_file(&path, &file_content);
    let summary = find_content::<DummyInput,_>(&path,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    cleanup_test_file(path);
    assert_eq!(summary.len(),4);
}
#[test]
fn test_find_content_open_a_header() {
    let path = setup_test_file("open_a_h_i");
    let cursor = generate_test_file();
    let block_start = 268;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN+ECC_LEN-4;
    file_content.truncate(new_len);//part way through the data
    write_bytes_to_file(&path, &file_content);
    let summary = find_content::<DummyInput,_>(&path,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    cleanup_test_file(path);
    assert_eq!(summary.len(),4);
}
#[test]
fn test_find_content_open_b() {
    let path = setup_test_file("open_b_i");
    let cursor = generate_test_file();
    let block_start = 23;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN*2+ECC_LEN*2+4;
    file_content.truncate(new_len);//part way through the data
    write_bytes_to_file(&path, &file_content);
    let summary = find_content::<DummyInput,_>(&path,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    cleanup_test_file(path);
    assert_eq!(summary.len(),0);
}
#[test]
fn test_find_content_ecc_block_3_data() {
    let path = setup_test_file("ecc_3_data_i");
    let cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN;
    let orig = cursor.into_inner();
    let mut file_contents = orig.clone();
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start+2] ^= file_contents[content_start+2]; 
    assert_ne!(orig,file_contents);
    write_bytes_to_file(&path, &file_contents);
    let summary = find_content::<DummyInput,_>(&path,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    cleanup_test_file(path);
    assert_eq!(summary.len(),5);
}
#[test]
fn test_find_content_ecc_block_3_header() {
    let path = setup_test_file("ecc_3_head_i");
    let cursor = generate_test_file();
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN - 3;
    let orig = cursor.into_inner();
    let mut file_contents = orig.clone();
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start+2] ^= file_contents[content_start+2]; 
    assert_ne!(orig,file_contents);
    write_bytes_to_file(&path, &file_contents);
    let summary = find_content::<DummyInput,_>(&path,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    cleanup_test_file(path);
    assert_eq!(summary.len(),5);
}
#[test]
fn test_find_content_open_3_corrupt_2() {
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
    let summary = find_content::<DummyInput,_>(&path,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    cleanup_test_file(path);
    assert_eq!(summary.len(),4);
}
#[test]
fn test_find_content_open_2_corrupt_1() {
    let path = setup_test_file("open2_corrupt_1");
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
    write_bytes_to_file(&path, &file_contents);
    let summary = find_content::<DummyInput,_>(&path,None,Some(u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])..)).unwrap();
    cleanup_test_file(path);
    assert_eq!(summary.len(),3);
}