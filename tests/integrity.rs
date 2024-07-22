
mod common;

use common::*;
use docufort::*;
use docufort::integrity::{integrity_check_file, IntegrityCheckOk};
use docufort::core::*;

use std::io::Cursor;

#[test]
fn test_empty_file_recovery() {
    let mut cursor = Cursor::new(Vec::new());
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor);
    assert!(summary.is_err());
}
#[test]
fn test_file_with_incomplete_header() {
    let mut cursor = Cursor::new(vec![0x01, 0x02, 0x03]);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor);
    assert!(summary.is_err());
}
#[test]
fn test_integrity_recovery_clean() {
    let file_content = generate_test_file().into_inner();
    let mut cursor = Cursor::new(file_content);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor).unwrap();
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
    let mut file_content = generate_test_file().into_inner();
    file_content.extend_from_slice(&MAGIC_NUMBER);
    let mut cursor = Cursor::new(file_content);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor).unwrap();
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
    let block_start = 268;
    let mut file_content = generate_test_file().into_inner();
    let new_len = block_start + HEADER_LEN + ECC_LEN + 4;
    file_content.truncate(new_len);
    let mut cursor = Cursor::new(file_content);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor).unwrap();
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
    assert_eq!(last_block_state, Some(BlockState::OpenABlock { truncate_at: 256 }));
}

#[test]
fn test_integrity_recovery_open_a_header() {
    let block_start = 268;
    let mut file_content = generate_test_file().into_inner();
    let new_len = block_start + HEADER_LEN + ECC_LEN - 4;
    file_content.truncate(new_len);
    let mut cursor = Cursor::new(file_content);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor).unwrap();
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
    assert_eq!(last_block_state, Some(BlockState::IncompleteStartHeader { truncate_at: 256 }));
}

#[test]
fn test_integrity_recovery_open_b() {
    let block_start = 23;
    let mut file_content = generate_test_file().into_inner();
    let new_len = block_start + HEADER_LEN * 2 + ECC_LEN * 2 + 4;
    file_content.truncate(new_len);
    let mut cursor = Cursor::new(file_content);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor).unwrap();

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
    assert_eq!(last_block_state.unwrap().is_open_b(), true);
}

#[test]
fn test_integrity_test_recovery_ecc_block_3_data() {
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN;
    let mut file_contents = generate_test_file().into_inner();
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start + 2] ^= file_contents[content_start + 2];
    let mut cursor = Cursor::new(file_contents);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor).unwrap();
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
    assert_eq!(last_block_state.unwrap().is_closed(), true);
}

#[test]
fn test_integrity_test_recovery_ecc_block_3_header() {
    let block_start = 268;
    let content_start = block_start + HEADER_LEN + ECC_LEN - 3;
    let mut file_contents = generate_test_file().into_inner();
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start + 2] ^= file_contents[content_start + 2];
    let mut cursor = Cursor::new(file_contents);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor).unwrap();
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
    assert_eq!(last_block_state.unwrap().is_closed(), true);
}

#[test]
fn test_integrity_test_recovery_open_3_corrupt_2() {
    let block_start = 268;
    let mut file_contents = generate_test_file().into_inner();
    let new_len = block_start + HEADER_LEN + ECC_LEN + 4;
    file_contents.truncate(new_len);

    let block_start = 184;
    let content_start = block_start + HEADER_LEN + ECC_LEN;
    file_contents[content_start] ^= file_contents[content_start];
    file_contents[content_start + 2] ^= file_contents[content_start + 2];
    let mut cursor = Cursor::new(file_contents);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor).unwrap();
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
    assert_eq!(last_block_state.unwrap().is_open_a(), true);
    let cc = CorruptDataSegment::Corrupt { data_start: content_start as u64, data_len: A_CONTENT.len() as u32 };
    assert_eq!(corrupted_segments[0], cc);
}
#[test]
fn test_integrity_test_recovery_open_2_corrupt_1() {
    let block_start = 184;
    let mut file_contents = generate_test_file().into_inner();
    let new_len = block_start + HEADER_LEN + ECC_LEN + 4;
    file_contents.truncate(new_len);

    let content_start1 = 40 + HEADER_LEN + ECC_LEN;
    let content_start3 = 102 + HEADER_LEN + ECC_LEN;
    file_contents[content_start1] ^= file_contents[content_start1];
    file_contents[content_start1 + 2] ^= file_contents[content_start1 + 2];

    let mut cursor = Cursor::new(file_contents);
    let summary = integrity_check_file::<_, DummyInput>(&mut cursor).unwrap();
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
    assert_eq!(last_block_state.unwrap().is_open_a(), true);
    let cc1 = CorruptDataSegment::MaybeCorrupt { data_start: content_start1 as u64, data_len: B_CONTENT.len() as u32 };
    let cc2 = CorruptDataSegment::MaybeCorrupt { data_start: content_start3 as u64, data_len: B_CONTENT.len() as u32 };
    assert_eq!(corrupted_segments[0], cc1);
    assert_eq!(corrupted_segments[1], cc2);
}