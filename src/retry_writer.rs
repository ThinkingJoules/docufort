/*!
This is a wrapper for trying to write IO repeatedly.

I don't know how likely IO is to return error, when a second or third attempt may succeed.

This is a wrapper that will try to write distinct parts of the block multiple times in case of some weird error stuff.

This may be overkill, but logically it will only fail if there is *really* a problem.

The retry count is per Operation attempt.

The idea is that this would be put in it's own thread and other threads can send Operations to it through a channel.

Or you could wrap it in a struct that stores the return values and a file handle, and then wrap that in a mutex or something.

*/

use std::fmt::Debug;

use crate::{core::{BlockInputs, ComponentHeader}, write::{write_magic_number, write_header, write_content_header, write_content, write_block_hash}, BlockTag, ReadWriteError};



#[derive(Debug)]
pub enum Op<T:AsRef<[u8]>> {
    CloseBlock,
    AtomicWrite(T),
    ContentWrite(T),
}
pub struct Operation<T:AsRef<[u8]>>{
    pub op:Op<T>,
    pub time_stamp:Option<[u8;8]>,
    pub calc_ecc:bool
}

#[derive(Debug)]
enum InnerOp<T:AsRef<[u8]>,B:BlockInputs> {
    WriteMagicNumber,
    WriteABlockStart{data_len:u32,time_stamp:[u8;8],calc_ecc:bool},
    WriteBBlockStart{time_stamp:[u8;8]},
    WriteContentHeader{data_len:u32,time_stamp:[u8;8],calc_ecc:bool,hasher:Option<B>},
    WriteContent(T,Option<B>,bool),
    WriteEndHeader{time_stamp:Option<[u8;8]>,hasher:Option<B>},
    WriteHash(Option<B>)
}

impl<T: AsRef<[u8]>, B: BlockInputs> InnerOp<T, B> {
    fn insert_hasher(&mut self,hasher:B){
        match self {
            InnerOp::WriteContentHeader{hasher:b,..} |
            InnerOp::WriteContent(_, b,_) |
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
    oper: Operation<T>,
    mut write_attempts:usize
) -> Result<TailState<B>,Vec<ReadWriteError>>//outer error is unrecoverable
where
    RWS: std::io::Read + std::io::Write + std::io::Seek,
    T: AsRef<[u8]>+Debug,
    B: BlockInputs+Debug,
{    
    let Operation { op, time_stamp, calc_ecc } = oper;
    let (tail_state,inner_ops) = match (tail,op) {
        (TailState::OpenBBlock { hasher }, Op::CloseBlock) => {
            (
                TailState::ClosedBlock,
                vec![
                    InnerOp::WriteEndHeader {time_stamp, hasher: None},
                    InnerOp::WriteHash(Some(hasher)),
                ]
            )
        },
        (TailState::OpenBBlock { hasher }, Op::AtomicWrite(t)) => {
            let time_stamp = time_stamp.unwrap_or_else(||B::current_timestamp());
            let data_len = t.as_ref().len() as u32;
            (
                TailState::ClosedBlock,
                vec![
                    InnerOp::WriteEndHeader { time_stamp:None ,hasher:None },
                    InnerOp::WriteHash(Some(hasher)),
                    InnerOp::WriteMagicNumber,
                    InnerOp::WriteABlockStart{time_stamp, data_len, calc_ecc },
                    InnerOp::WriteContent(t,None,calc_ecc),
                    InnerOp::WriteEndHeader {time_stamp:None, hasher:None },
                    InnerOp::WriteHash (None)

                ]
            )
        },
        (TailState::OpenBBlock { hasher }, Op::ContentWrite(t)) => {
            let time_stamp = time_stamp.unwrap_or_else(||B::current_timestamp());
            let data_len = t.as_ref().len() as u32;
            (
                TailState::OpenBBlock { hasher:B::new() },
                vec![
                    InnerOp::WriteContentHeader{time_stamp, data_len, calc_ecc ,hasher:Some(hasher)},
                    InnerOp::WriteContent(t,None,calc_ecc),
                ]
            )
        },
        
        (clean, Op::CloseBlock) => {
            //No Op
            return Ok(clean)
        },
        (clean, Op::AtomicWrite(t)) =>{
            let time_stamp = time_stamp.unwrap_or_else(||B::current_timestamp());
            let data_len = t.as_ref().len() as u32;
            let ops = vec![
                if clean.is_closed() { Some(InnerOp::WriteMagicNumber) } else { None },
                Some(InnerOp::WriteABlockStart { time_stamp, data_len, calc_ecc }),
                Some(InnerOp::WriteContent(t, None, calc_ecc)),
                Some(InnerOp::WriteEndHeader { time_stamp: None, hasher: None }),
                Some(InnerOp::WriteHash(None)),
            ].into_iter().filter_map(|x| x).collect::<Vec<_>>();
            (TailState::ClosedBlock,ops)
        },
        (clean, Op::ContentWrite(t)) =>  {
            let (s_stamp,c_stamp) = if let Some(ts) = time_stamp {
                (B::current_timestamp(),ts)   
            }else{
                (B::current_timestamp(),B::current_timestamp())
            };
            let data_len = t.as_ref().len() as u32;
            let ops = vec![
                if clean.is_closed() { Some(InnerOp::WriteMagicNumber) } else { None },
                Some(InnerOp::WriteBBlockStart { time_stamp: s_stamp }),
                Some(InnerOp::WriteContentHeader { time_stamp: c_stamp, data_len, calc_ecc, hasher: Some(B::new()) }),
                Some(InnerOp::WriteContent(t, None, calc_ecc)),
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
) -> Result<Option<B>,(InnerOperation<T,B>,ReadWriteError)>
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
        InnerOp::WriteABlockStart{ data_len, time_stamp, calc_ecc } => {
            let tag = if calc_ecc {BlockTag::StartAEBlock as u8}else{BlockTag::StartABlock as u8};
            let header = ComponentHeader::new_from_parts(tag, time_stamp, Some(data_len));
            if let Err(e) = write_header(file,&header) {
                return Err((InnerOperation{ inner, start_offset:Some(start_offset) },e))
            }
            Ok(None)
        },
        InnerOp::WriteBBlockStart { time_stamp } => {
            let tag = BlockTag::StartBBlock as u8;
            let header = ComponentHeader::new_from_parts(tag, time_stamp, None);
            if let Err(e) = write_header(file,&header) {
                return Err((InnerOperation{ inner, start_offset:Some(start_offset) },e))
            }
            Ok(None)
        },
        InnerOp::WriteContentHeader { data_len, time_stamp, calc_ecc, hasher } => {
            let mut b = if let Some(b) = hasher {b}else{B::new()};
            let hasher = Some(b.clone());//preserve hash state in case of failure
            if let Err(e) = write_content_header(file,data_len,calc_ecc,Some(time_stamp),&mut b) {
                return Err((InnerOperation{ inner:InnerOp::WriteContentHeader { data_len, time_stamp, calc_ecc, hasher}, start_offset:Some(start_offset) },e))
            }
            Ok(Some(b))
        },
        InnerOp::WriteContent(data, hasher,calc_ecc) => {
            let mut b = if let Some(b) = hasher {b}else{B::new()};
            let hasher = Some(b.clone());//preserve hash state in case of failure
            if let Err(e) = write_content(file,data.as_ref(),calc_ecc,&mut b) {
                return Err((InnerOperation{ inner:InnerOp::WriteContent(data, hasher,calc_ecc), start_offset:Some(start_offset) },e))
            }
            Ok(Some(b))
        },
        InnerOp::WriteEndHeader { time_stamp, hasher } => {
            let tag = BlockTag::EndBlock as u8;
            let time_stamp = time_stamp.unwrap_or_else(||B::current_timestamp());
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
    
        fn current_timestamp() -> [u8; 8] {
            [7, 6, 5, 4, 3, 2, 1, 0]
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
        let b_block_header = ComponentHeader::new_from_parts(BlockTag::StartBBlock as u8, DummyInput::current_timestamp(), None);
        write_header(&mut cursor, &b_block_header).unwrap();
    
        // Write 3 Content Components
        if log_pos {println!("CONTENT COMPONENT START: {}",cursor.position())};
        write_content_component(&mut cursor, false,None, B_CONTENT, &mut hasher).unwrap();
        
        if log_pos {println!("CONTENT COMPONENT START: {}",cursor.position())};
        write_content_component(&mut cursor, true,None, B_CONTENT, &mut hasher).unwrap();
        
        if log_pos {println!("CONTENT COMPONENT START: {}",cursor.position())};
        write_content_component(&mut cursor, false,None, B_CONTENT, &mut hasher).unwrap();
    
    
        let b_block_hash = hasher.finalize();
        let block_end_header = ComponentHeader::new_from_parts(BlockTag::EndBlock as u8, DummyInput::current_timestamp(), None);
        write_block_end(&mut cursor, &block_end_header, &b_block_hash).unwrap();
        
        if log_pos {println!("MN START: {}",cursor.position())};
        write_magic_number(&mut cursor).unwrap();
        if log_pos {println!("BLOCK START: {}",cursor.position())};
        write_atomic_block::<_,DummyInput>(&mut cursor, None, A_CONTENT, false, None).unwrap();
        
        if log_pos {println!("MN START: {}",cursor.position())};
        write_magic_number(&mut cursor).unwrap();
        if log_pos {println!("BLOCK START: {}",cursor.position())};
        write_atomic_block::<_,DummyInput>(&mut cursor, None, A_CONTENT, true, None).unwrap();
    
    
        cursor
    }
    use crate::write::init_file;

    pub fn generate_test_file_lib() -> Cursor<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::new());
        // Init the file with header
        init_file(&mut cursor).unwrap();

        let ops = [
            Operation{ op:Op::ContentWrite(B_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp()), calc_ecc: false },
            Operation{ op:Op::ContentWrite(B_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp()), calc_ecc: true },
            Operation{ op:Op::ContentWrite(B_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp()), calc_ecc: false },
            Operation{ op:Op::AtomicWrite(A_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp()), calc_ecc: false },
            Operation{ op:Op::AtomicWrite(A_CONTENT.to_vec()), time_stamp: Some(DummyInput::current_timestamp()), calc_ecc: true },
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