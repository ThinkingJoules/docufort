

use docufort::*;
use docufort_macros::{generate_stub_structs,make_system,make_msg_decoder,MsgCoder, MsgReadWrite};

generate_stub_structs!();

make_system!({
    data_comp_flag:0b01000000,
    ecc_flag:0b00100000,
    msg_data_flag:0b01000000,
    msg_and_data_ecc_len:5,
    min_len_try_comp: 35,
    write_serializer: WriterStruct,
    read_deserializer: ReaderStruct,
    compressor: CompressorStruct,
    eccer: EccerStruct,
    writer_error:AllError,
    reader_error:AllError
});

make_msg_decoder!(
    TestMessage,
    TestMessage1,
    AllError
);

impl DocuFortMsg for TestMessage{
    const MSG_TAG:u8 = 2;
    const FIXED_INTS:bool = false;
    fn take_data(self)->Option<Vec<u8>>{
        Some(self.data)
    }
    fn has_data(&self)->Option<usize>{
        Some(self.data.len())
    }

    fn set_data(&mut self, data:Vec<u8>) {
        self.data = data;
    }
}

#[derive(Debug,MsgCoder,MsgReadWrite)]
#[write_error(AllError)]
#[read_error(AllError)]
pub struct TestMessage{
    field1:u8,
    field2:u32,
    field3:bool,
    data:Vec<u8>,
}

#[derive(Debug,MsgCoder,MsgReadWrite)]
#[write_error(AllError)]
#[read_error(AllError)]
pub struct TestMessage1{
    field1:u8,
    field2:u32,
    field3:bool,
}

impl DocuFortMsg for TestMessage1{
    const MSG_TAG:u8 = 3;
    const FIXED_INTS:bool = false;
    fn take_data(self)->Option<Vec<u8>>{
        None
    }
    fn has_data(&self)->Option<usize>{
        None
    }

    fn set_data(&mut self, _data:Vec<u8>) {
        panic!("No Data")
    }
}


#[test]
fn test_() {
    
}