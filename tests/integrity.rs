
mod common;

use common::*;
use docufort::*;
use docufort::integrity::{integrity_check_file, IntegrityCheckOk};
use docufort::core::*;

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
fn test_empty_file_recovery() {
    let path = setup_test_file("empty_i");
    let summary = integrity_check_file::<DummyInput>(&path);
    cleanup_test_file(path);
    assert!(summary.is_err());
}
#[test]
fn test_file_with_incomplete_header() {
    let path = setup_test_file("bad_header_i");
    // Using arbitrary bytes that could represent an incomplete header
    write_bytes_to_file(&path, &[0x01, 0x02, 0x03]);
    let summary = integrity_check_file::<DummyInput>(&path);
    cleanup_test_file(path);
    assert!(summary.is_err());
}
#[test]
fn test_integrity_recovery_clean() {
    let path = setup_test_file("clean_i");
    let cursor = generate_test_file();
    let file_content = cursor.into_inner();
    write_bytes_to_file(&path, &file_content);
    let summary = integrity_check_file::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let IntegrityCheckOk { 
        last_block_state, 
        errors_corrected, 
        data_contents, 
        num_blocks, 
        file_len_checked, 
        corrupted_segments, .. 
    } = summary;
    assert_eq!(errors_corrected, 0);
    assert_eq!(num_blocks, 3);
    assert_eq!(file_len_checked, 344);
    assert_eq!(data_contents, 64);
    assert!(corrupted_segments.is_empty());
    assert!(last_block_state.is_some());
}
#[test]
fn test_integrity_recovery_trailing_truncate() {
    let path = setup_test_file("trail_i");
    let cursor = generate_test_file();
    let mut file_content = cursor.into_inner();
    file_content.extend_from_slice(&MAGIC_NUMBER);
    write_bytes_to_file(&path, &file_content);
    let summary = integrity_check_file::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let IntegrityCheckOk { 
        last_block_state, 
        errors_corrected, 
        data_contents, 
        num_blocks, 
        file_len_checked, 
        corrupted_segments , .. 
    } = summary;
    assert_eq!(errors_corrected, 0);
    assert_eq!(num_blocks, 3);
    assert_eq!(file_len_checked, 344);
    assert_eq!(data_contents, 64);
    assert!(corrupted_segments.is_empty());
    assert!(last_block_state.is_some());
}
#[test]
fn test_integrity_recovery_open_a_data() {
    let path = setup_test_file("open_a_d_i");
    let cursor = generate_test_file();
    let block_start = 268;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN+ECC_LEN+4;
    file_content.truncate(new_len);//part way through the data
    write_bytes_to_file(&path, &file_content);
    let summary = integrity_check_file::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let IntegrityCheckOk { 
        last_block_state, 
        errors_corrected, 
        data_contents, 
        num_blocks, 
        file_len_checked, 
        corrupted_segments , .. 
    } = summary;
    assert_eq!(errors_corrected, 0);
    assert_eq!(num_blocks, 2);
    assert_eq!(file_len_checked as usize, 256);
    assert_eq!(data_contents, 50);
    assert!(corrupted_segments.is_empty());
    assert_eq!(last_block_state,Some(BlockState::OpenABlock { truncate_at: 256 }));
}
#[test]
fn test_integrity_recovery_open_a_header() {
    let path = setup_test_file("open_a_h_i");
    let cursor = generate_test_file();
    let block_start = 268;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN+ECC_LEN-4;
    file_content.truncate(new_len);//part way through the data
    write_bytes_to_file(&path, &file_content);
    let summary = integrity_check_file::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    let IntegrityCheckOk { 
        last_block_state, 
        errors_corrected, 
        data_contents, 
        num_blocks, 
        file_len_checked, 
        corrupted_segments , .. 
    } = summary;
    assert_eq!(errors_corrected, 0);
    assert_eq!(num_blocks, 2);
    assert_eq!(file_len_checked as usize, 256);
    assert_eq!(data_contents, 50);
    assert!(corrupted_segments.is_empty());
    assert_eq!(last_block_state,Some(BlockState::IncompleteStartHeader { truncate_at: 256 }));
}
#[test]
fn test_integrity_recovery_open_b() {
    let path = setup_test_file("open_b_i");
    let cursor = generate_test_file();
    let block_start = 23;
    let mut file_content = cursor.into_inner();
    let new_len = block_start+HEADER_LEN*2+ECC_LEN*2+4;
    file_content.truncate(new_len);//part way through the data
    write_bytes_to_file(&path, &file_content);
    let summary = integrity_check_file::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);

    let IntegrityCheckOk { 
        last_block_state, 
        errors_corrected, 
        data_contents, 
        num_blocks, 
        file_len_checked, 
        corrupted_segments , .. 
    } = summary;
    assert_eq!(errors_corrected, 0);
    assert_eq!(num_blocks, 0);
    assert_eq!(file_len_checked as usize, 40);
    assert_eq!(data_contents, 0);
    assert!(corrupted_segments.is_empty());
    assert_eq!(last_block_state.unwrap().is_open_b(),true);

}
#[test]
fn test_integrity_test_recovery_ecc_block_3_data() {
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
    let summary = integrity_check_file::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    dbg!(&summary);
    let IntegrityCheckOk { 
        last_block_state, 
        errors_corrected, 
        data_contents, 
        num_blocks, 
        file_len_checked, 
        corrupted_segments , .. 
    } = summary;
    assert_eq!(errors_corrected, 2);
    assert_eq!(num_blocks, 3);
    assert_eq!(file_len_checked as usize, 344);
    assert_eq!(data_contents, 64);
    assert!(corrupted_segments.is_empty());
    assert_eq!(last_block_state.unwrap().is_closed(),true);
}
#[test]
fn test_integrity_test_recovery_ecc_block_3_header() {
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
    let summary = integrity_check_file::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    dbg!(&summary);
    let IntegrityCheckOk { 
        last_block_state, 
        errors_corrected, 
        data_contents, 
        num_blocks, 
        file_len_checked, 
        corrupted_segments , .. 
    } = summary;
    assert_eq!(errors_corrected, 2);
    assert_eq!(num_blocks, 3);
    assert_eq!(file_len_checked as usize, 344);
    assert_eq!(data_contents, 64);
    assert!(corrupted_segments.is_empty());
    assert_eq!(last_block_state.unwrap().is_closed(),true);
}
#[test]
fn test_integrity_test_recovery_open_3_corrupt_2() {
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
    let summary = integrity_check_file::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    dbg!(&summary);
    let IntegrityCheckOk { 
        last_block_state, 
        errors_corrected, 
        data_contents, 
        num_blocks, 
        file_len_checked, 
        corrupted_segments , .. 
    } = summary;
    assert_eq!(errors_corrected, 0);
    assert_eq!(num_blocks, 2);
    assert_eq!(file_len_checked as usize, 256);
    assert_eq!(data_contents, 50);
    assert_eq!(last_block_state.unwrap().is_open_a(),true);
    let cc = CorruptDataSegment::Corrupt { data_start: content_start as u64 , data_len: A_CONTENT.len() as u32 };
    assert_eq!(corrupted_segments[0],cc);
    
}
#[test]
fn test_integrity_test_recovery_open_2_corrupt_1() {
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
    let summary = integrity_check_file::<DummyInput>(&path).unwrap();
    cleanup_test_file(path);
    dbg!(&summary);
    let IntegrityCheckOk { 
        last_block_state, 
        errors_corrected, 
        data_contents, 
        num_blocks, 
        file_len_checked, 
        corrupted_segments , .. 
    } = summary;
    assert_eq!(errors_corrected, 0);
    assert_eq!(num_blocks, 1);
    assert_eq!(file_len_checked as usize, 172);
    assert_eq!(data_contents, 36);
    assert_eq!(last_block_state.unwrap().is_open_a(),true);
    let cc1 = CorruptDataSegment::MaybeCorrupt { data_start: content_start1 as u64 , data_len: B_CONTENT.len() as u32 };
    let cc2 = CorruptDataSegment::MaybeCorrupt { data_start: content_start3 as u64 , data_len: B_CONTENT.len() as u32 };
    assert_eq!(corrupted_segments[0],cc1);
    assert_eq!(corrupted_segments[1],cc2);

}