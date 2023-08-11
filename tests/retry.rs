use std::io::Cursor;
mod common;
use common::*;
use docufort::{write::init_file, retry_writer::*, core::BlockInputs};

pub fn generate_test_file_lib() -> Cursor<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    // Init the file with header
    init_file(&mut cursor).unwrap();

    let ops = [
        Operation{ op:Op::ContentWrite(B_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp(0)), calc_ecc: false },
        Operation{ op:Op::ContentWrite(B_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp(0)), calc_ecc: true },
        Operation{ op:Op::ContentWrite(B_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp(0)), calc_ecc: false },
        Operation{ op:Op::AtomicWrite(A_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp(0)), calc_ecc: false },
        Operation{ op:Op::AtomicWrite(A_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp(0)), calc_ecc: true },
    ];
    let mut tail_state: TailState<DummyInput> = TailState::ClosedBlock;
    for oper in ops {
        tail_state = perform_file_op(&mut cursor, tail_state, oper, 1).unwrap();
    }

    cursor
}



#[test]
fn compare_test_files() {
    let orig = generate_test_file().into_inner();
    let mut hasher = DummyInput::new();
    hasher.update(&orig);
    let orig_hash = hasher.finalize();
    let lib = generate_test_file_lib().into_inner();
    assert_eq!(orig.len(),lib.len());
    hasher = DummyInput::new();
    hasher.update(&lib);
    let range = 69..102;
    assert_eq!(&orig[range.clone()],&lib[range]);
    let lib_hash = hasher.finalize();
    assert_eq!(orig_hash,lib_hash)
}