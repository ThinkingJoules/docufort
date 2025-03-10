/*!
MIGRATE TO THE io_retry module.

This is a wrapper for trying to write IO repeatedly.

I don't know how likely IO is to return error, when a second or third attempt may succeed.

This is a wrapper that will try to write distinct parts of the block multiple times in case of some weird error stuff.

This may be overkill, but logically it will only fail if there is *really* a problem.

The retry count is per Operation attempt.

The idea is that this would be put in it's own thread and other threads can send Operations to it through a channel.

Or you could wrap it in a struct that stores the return values and a file handle, and then wrap that in a mutex or something.

*/

use std::fmt::Debug;

use crate::{core::{BlockInputs, ComponentHeader}, write::{write_magic_number, write_header, write_block_hash, write_atomic_block, write_content_component}, HeaderTag};



#[derive(Debug)]
pub enum Op<T:AsRef<[u8]>> {
    CloseBlock,
    AtomicWrite(T),
    ///This timestamp is for the content header.
    ///If a BlockStart needs to be written then it's timestamp will come from the Operation
    ContentWrite(T,Option<u64>),
}
pub struct Operation<T:AsRef<[u8]>,C>{
    pub op:Op<T>,
    ///This is basically always the header for the Op.
    ///If the Op is ContentWrite, then this is 'BlockStart'
    pub timestamp:Option<u64>,
    pub calc_ecc:bool,
    pub compress:Option<C>
}

#[derive(Debug)]
enum InnerOp<T:AsRef<[u8]>,B:BlockInputs> {
    WriteMagicNumber,
    WriteABlock{time_stamp:u64,content: T, calc_ecc: bool, compress:Option<B::CompLevel> },
    WriteBBlockStart{time_stamp:[u8;8]},
    WriteContentComponent{time_stamp:u64,content: T, calc_ecc: bool, compress:Option<B::CompLevel>,hasher:Option<B>},
    WriteEndHeader{time_stamp:Option<[u8;8]>,hasher:Option<B>},
    WriteHash(Option<B>)
}

impl<T: AsRef<[u8]>, B: BlockInputs> InnerOp<T, B> {
    fn insert_hasher(&mut self,hasher:B){
        match self {
            InnerOp::WriteContentComponent{hasher:b,..} |
            InnerOp::WriteEndHeader { hasher:b, .. } |
            InnerOp::WriteHash(b) =>{let _ = b.insert(hasher);},
            _ => ()


        }
    }
}
struct InnerOperation<T:AsRef<[u8]>,B:BlockInputs>{
    inner:InnerOp<T,B>,
    start_offset:Option<u64>
}


#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TailState<B>{
    OpenBBlock{hasher:B},
    ClosedBlock,
    MagicNumber
}

impl<B> TailState<B> {
    pub fn is_magic_number(&self)->bool{
        matches!(self,TailState::MagicNumber)
    }
    pub fn is_closed(&self)->bool{
        matches!(self,TailState::ClosedBlock)
    }
    pub fn is_open(&self)->bool{
        matches!(self,TailState::OpenBBlock { .. })
    }
    pub fn insert_hasher(self,hasher:B)->Self{
        match self {
            TailState::OpenBBlock { .. } => TailState::OpenBBlock { hasher },
            a => a,
        }
    }
}

///The file Read, assumes it is positioned where this function last left it, and the tailstate is the same as what this function returns.
pub fn perform_file_op<RWS, T, B>(
    file: &mut RWS,
    tail: TailState<B>,
    oper: Operation<T,B::CompLevel>,
    mut write_attempts:usize
) -> Result<TailState<B>,Vec<std::io::Error>>//outer error is unrecoverable
where
    RWS: std::io::Read + std::io::Write + std::io::Seek,
    T: AsRef<[u8]>+Debug,
    B: BlockInputs+Debug,
{
    let Operation { op, timestamp, calc_ecc, compress } = oper;
    //let time_stamp = timestamp.map(|u|u.to_be_bytes());
    let (tail_state,inner_ops) = match (tail,op) {
        (TailState::OpenBBlock { hasher }, Op::CloseBlock) => {
            (
                TailState::ClosedBlock,
                vec![
                    InnerOp::WriteEndHeader {time_stamp:timestamp.map(|u|u.to_be_bytes()), hasher: None},
                    InnerOp::WriteHash(Some(hasher)),
                ]
            )
        },
        (TailState::OpenBBlock { hasher }, Op::AtomicWrite(t)) => {
            let time_stamp = timestamp.unwrap_or_else(B::current_timestamp);
            (
                TailState::ClosedBlock,
                vec![
                    InnerOp::WriteEndHeader { time_stamp:None ,hasher:None },
                    InnerOp::WriteHash(Some(hasher)),
                    InnerOp::WriteMagicNumber,
                    InnerOp::WriteABlock{time_stamp, content: t, calc_ecc, compress },
                ]
            )
        },
        (TailState::OpenBBlock { hasher }, Op::ContentWrite(t,_)) => {
            let time_stamp = timestamp.unwrap_or_else(B::current_timestamp);
            (
                TailState::OpenBBlock { hasher:B::new() },
                vec![
                    InnerOp::WriteContentComponent{time_stamp, content: t, calc_ecc, compress ,hasher:Some(hasher)},
                ]
            )
        },

        (clean, Op::CloseBlock) => {
            //No Op
            return Ok(clean)
        },
        (clean, Op::AtomicWrite(t)) =>{
            let time_stamp = timestamp.unwrap_or_else(B::current_timestamp);
            let ops = vec![
                if clean.is_closed() { Some(InnerOp::WriteMagicNumber) } else { None },
                Some(InnerOp::WriteABlock { time_stamp, content: t, calc_ecc, compress  }),
            ].into_iter().filter_map(|x| x).collect::<Vec<_>>();
            (TailState::ClosedBlock,ops)
        },
        (clean, Op::ContentWrite(t,content_timestamp)) =>  {
            let (s_stamp,c_stamp) = (timestamp.unwrap_or_else(B::current_timestamp).to_be_bytes(),content_timestamp.unwrap_or_else(B::current_timestamp));
            let ops = vec![
                if clean.is_closed() { Some(InnerOp::WriteMagicNumber) } else { None },
                Some(InnerOp::WriteBBlockStart { time_stamp: s_stamp }),
                Some(InnerOp::WriteContentComponent { time_stamp:c_stamp, content: t, calc_ecc, compress , hasher: Some(B::new()) }),
            ].into_iter().filter_map(|x| x).collect::<Vec<_>>();
            (TailState::OpenBBlock { hasher:B::new() },ops)
        },
    };
    let mut inner_ops:Vec<InnerOperation<_,_>> = inner_ops.into_iter().rev().map(|inner|InnerOperation { inner, start_offset: None }).collect();
    let mut errors = Vec::new();
    'outer: loop {
        write_attempts -= 1;
        loop {
            if inner_ops.is_empty(){return Ok(tail_state)}
            let inner = inner_ops.pop().unwrap();
            match perform_inner_op::<_,T,B>(file,inner){
                Ok(Some(b)) => if let Some(i) = inner_ops.last_mut() {
                    i.inner.insert_hasher(b)
                }else{
                    return Ok(tail_state.insert_hasher(b))
                },
                Ok(_) => (),
                Err((o,e)) => {
                    inner_ops.push(o);
                    errors.push(e);
                    if write_attempts == 0 {
                        return Err(errors)
                    }
                    continue 'outer;
                },
            }
        }
    }
}

fn perform_inner_op<RWS, T, B>(
    file: &mut RWS,
    oper: InnerOperation<T,B>,
) -> Result<Option<B>,(InnerOperation<T,B>,std::io::Error)>
where
    RWS: std::io::Read + std::io::Write + std::io::Seek,
    T: AsRef<[u8]>+Debug,
    B: BlockInputs+Debug,
{
    let InnerOperation { inner, start_offset } = oper;
    let start_offset = if let Some(start) = start_offset{
        match file.seek(std::io::SeekFrom::Start(start)){
            Ok(s) => s,
            Err(e) => return Err((InnerOperation{ inner, start_offset },e.into())),
        }
    }else{
        match file.seek(std::io::SeekFrom::Current(0)){
            Ok(s) => s,
            Err(e) => return Err((InnerOperation{ inner, start_offset },e.into())),
        }
    };
    match inner {
        InnerOp::WriteMagicNumber => {
            if let Err(e) = write_magic_number(file) {
                return Err((InnerOperation{ inner, start_offset:Some(start_offset) },e.into()))
            }
            Ok(None)
        },
        InnerOp::WriteABlock{ time_stamp, calc_ecc, content, compress } => {

            if let Err(e) = write_atomic_block::<_,B>(file,Some(time_stamp),content.as_ref(),calc_ecc,compress.as_ref(),None) {
                return Err((InnerOperation{ inner:InnerOp::WriteABlock{ time_stamp, calc_ecc, content, compress }, start_offset:Some(start_offset) },e))
            }
            Ok(None)
        },
        InnerOp::WriteBBlockStart { time_stamp } => {
            let tag = HeaderTag::StartBBlock as u8;
            let header = ComponentHeader::new_from_parts(tag, time_stamp, None);
            if let Err(e) = write_header(file,&header) {
                return Err((InnerOperation{ inner, start_offset:Some(start_offset) },e))
            }
            Ok(None)
        },
        InnerOp::WriteContentComponent { time_stamp, content, calc_ecc, compress, hasher } => {
            let mut b = if let Some(b) = hasher {b}else{B::new()};
            let hasher = Some(b.clone());//preserve hash state in case of failure
            if let Err(e) = write_content_component(file,calc_ecc,compress.as_ref(),Some(time_stamp),content.as_ref(),&mut b) {
                return Err((InnerOperation{ inner:InnerOp::WriteContentComponent {  time_stamp, content, calc_ecc, compress, hasher}, start_offset:Some(start_offset) },e))
            }
            Ok(Some(b))
        },
        InnerOp::WriteEndHeader { time_stamp, hasher } => {
            let tag = HeaderTag::EndBlock as u8;
            let time_stamp = time_stamp.unwrap_or_else(||B::current_timestamp().to_be_bytes());
            let header = ComponentHeader::new_from_parts(tag, time_stamp, None);
            if let Err(e) = write_header(file,&header) {
                return Err((InnerOperation{ inner:InnerOp::WriteEndHeader { time_stamp:Some(time_stamp), hasher }, start_offset:Some(start_offset) },e))
            }
            Ok(hasher)
        },
        InnerOp::WriteHash(h) => {
            let hasher = h.unwrap();
            let hash = hasher.finalize();
            if let Err(e) = write_block_hash(file,&hash) {
                return Err((InnerOperation{ inner:InnerOp::WriteHash(Some(hasher)), start_offset:Some(start_offset) },e))
            }
            Ok(Some(hasher))
        },
    }

}

#[cfg(test)]
mod test_super {
    use super::*;
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

        fn current_timestamp() -> u64{
            u64::from_be_bytes([7, 6, 5, 4, 3, 2, 1, 0])
        }

        type CompLevel= i32;

        fn compress<W:std::io::Write>(_data: &[u8], _writer: &mut W, _comp_level: &Self::CompLevel) -> std::io::Result<usize> {
            unimplemented!()
        }

        fn decompress<R:std::io::Read,W:std::io::Write>(_compressed: &mut R, _sink: &mut W,_s:u32) -> std::io::Result<usize> {
            unimplemented!()
        }

    }

    use std::io::Cursor;
    use crate::*;
    use crate::write::*;

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
        write_content_component(&mut cursor, false,None,None, B_CONTENT, &mut hasher).unwrap();

        if log_pos {println!("CONTENT COMPONENT START: {}",cursor.position())};
        write_content_component(&mut cursor, true,None,None, B_CONTENT, &mut hasher).unwrap();

        if log_pos {println!("CONTENT COMPONENT START: {}",cursor.position())};
        write_content_component(&mut cursor, false,None,None, B_CONTENT, &mut hasher).unwrap();


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
    use crate::write::init_file;

    pub fn generate_test_file_lib() -> Cursor<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::new());
        // Init the file with header
        init_file(&mut cursor).unwrap();

        let ops = [
            Operation{ op:Op::ContentWrite(B_CONTENT.to_vec(),None), timestamp: Some(DummyInput::current_timestamp()), calc_ecc: false , compress:None},
            Operation{ op:Op::ContentWrite(B_CONTENT.to_vec(),None), timestamp: Some(DummyInput::current_timestamp()), calc_ecc: true , compress:None},
            Operation{ op:Op::ContentWrite(B_CONTENT.to_vec(),None), timestamp: Some(DummyInput::current_timestamp()), calc_ecc: false, compress:None },
            Operation{ op:Op::AtomicWrite(A_CONTENT.to_vec()), timestamp: Some(DummyInput::current_timestamp()), calc_ecc: false, compress:None },
            Operation{ op:Op::AtomicWrite(A_CONTENT.to_vec()), timestamp: Some(DummyInput::current_timestamp()), calc_ecc: true , compress:None},
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
}