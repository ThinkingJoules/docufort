




///Lower means less compression, higher means more
pub struct CompressionLevel(u8);

pub trait Compressor {
    type Error;
    ///Tries to compress data into writer if try_compress.is_some(). **If it is the same length or longer, then uncompressed data must be written**
    ///-> Ok(Some(data_was_compressed_to_this_length)) || Ok(None)(data was not compressed, but written as given)
    /// Implementer should watch for EoF error in case compression goes longer and the given writer was allocated for uncompressed at worst size
    /// EoF error should be returned if it occured from writing the uncompressed data.
    fn compress_into<W: std::io::Write+std::io::Seek>(writer: &mut W, data: &[u8], try_compress: Option<CompressionLevel>) -> Result<(), Self::Error>;
    ///Should only be called if the slice is known to be compressed. Writes uncompressed data to writer.
    fn decompress_into<W: std::io::Write>(writer: &mut W, data: &[u8]) -> Result<(), Self::Error>;
}
pub trait Eccer {
    type Error;
    fn calc_ecc_into<W: std::io::Write>(writer: &mut W, raw_data: &[u8]) -> Result<(), Self::Error>;
    ///Attempts to correct any errors. -> Ok((num_errors_corrected, original_raw_data_with_no_errors))
    fn apply_ecc(raw_data: &mut[u8]) -> Result<usize, Self::Error>;
    fn calc_ecc_data_len(raw_data_len:usize)->usize;
}


pub fn correct_errors<W: std::io::Write + std::io::Seek>(writer: &mut W,summary:MessageReadSummary)->Result<usize,std::io::Error>{
    let MessageReadSummary { errors, message_start, data } = summary;
    if errors.is_none() {return Ok(0)}
    let (num_errors,fixed) = errors.unwrap();
    writer.seek(std::io::SeekFrom::Start(message_start))?;
    writer.write_all(&fixed)?;
    Ok(num_errors)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageReadSummary{
    pub errors:Option<(usize,Vec<u8>)>,
    pub message_start: u64, //if errors is_some() write the whole vec starting at message_start
    ///(Start,Len,FlagByte)
    pub data: Option<(u64,u32,u8)>,
}

pub trait WriteSerializer {
    type Error;
    fn serialize_into<W: std::io::Write, T: serde::Serialize + DocuFortMsg>(writer: &mut W, message: &T) -> Result<(), Self::Error>;
    fn serialized_size<T: serde::Serialize + DocuFortMsg>(message: &T) -> Result<usize, Self::Error>;
}
pub trait ReadDeserializer {
    type Error;
    fn read_from<'de,T: serde::Deserialize<'de> + DocuFortMsg>(bytes: &[u8]) -> Result<T, Self::Error>;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq,PartialOrd, Ord)]
pub struct MsgTag(u8);
impl std::ops::Deref for MsgTag {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl MsgTag {
    pub const fn new(tag:u8) -> Self {
        assert!(tag != 0);//Sys_BlockStart
        assert!(tag != 1);//Sys_BlockEnd

        //Make test to verify message tags are unique and don't conflict
        //assert!(tag & ECC_FLAG == 0);
        //assert!(tag & MSG_DATA_FLAG == 0);
        Self(tag)
    }
}

pub trait DocuFortMsg {
    const MSG_TAG: MsgTag;
    const FIXED_INTS: bool;
    fn take_data(self)->Option<Vec<u8>>;
    fn has_data(&self)->Option<usize>;
    fn set_data(&mut self, data:Vec<u8>);
}

///u32_le + 1 tag byte
pub const DATA_META_LEN: u8 = 5;

pub trait SystemConsts{
    ///This only exists on the sys_data_tag
    const DATA_COMP_FLAG: u8;
    ///This is used in both the MSG_TAG and the sys_data_tag
    const ECC_FLAG: u8;
    ///This is only used in the MSG_TAG
    const MSG_DATA_FLAG: u8;
    const CLEAR_MSG_FLAGS: u8;
    const ECC_LEN: u8;
    const MAGIC_NUMBER: [u8; 8];
    ///Depends on how structured the data is in the messages.
    ///Pure Random breaks even around 45 (using best)
    ///u64 micro_unix only need 20 bytes to break even (using best)
    const MIN_LEN_TRY_COMP:usize;
}

pub trait ConcreteTypeProvider {
    
    type WriterType:WriteSerializer;
    type ReaderType:ReadDeserializer;
    type CompressorType:Compressor;
    type EccType:Eccer;
}

pub trait DocuFortMsgCoding<X:ConcreteTypeProvider+SystemConsts>: DocuFortMsg + serde::Serialize + for<'de>serde::Deserialize<'de> {
    fn write_to<W>(self,writer: &mut W,try_compress: Option<CompressionLevel>,calc_ecc:bool)->Result<(),<X::WriterType as WriteSerializer>::Error>
    where
        W: std::io::Write + std::io::Seek,
    ;
    fn read_from<R>(reader:&mut R,msg_len:u8,flags:u8,error_correct:bool)->Result<(MessageReadSummary, Self),<X::ReaderType as ReadDeserializer>::Error>
    where
        R: std::io::Read+std::io::Seek,
    ;
    fn load_data<R:std::io::Read+std::io::Seek>(&mut self, mut reader:R,summary:&MessageReadSummary)->Result<(),<X::ReaderType as ReadDeserializer>::Error>{
        let MessageReadSummary { data ,..} = summary;
        assert!(data.is_some());
        let (start,len,flag) = data.unwrap();
        let mut data = vec![0;len as usize];
        reader.seek(std::io::SeekFrom::Start(start))?;
        reader.read_exact(&mut data)?;
        if flag & X::DATA_COMP_FLAG == X::DATA_COMP_FLAG {
            let mut v = Vec::with_capacity((len+(len/4)) as usize);
            X::CompressorType::decompress_into(&mut v, &data)?;
            data = v;
        }
        self.set_data(data);
        Ok(())
    }
}

///Reads Message, but not it's data from given reader.
/// Reader = | msg |?msg_ecc | data_len(u32_le) | sys_data_tag(1) | data_bytes |? data_ecc_data |
pub fn read_msg<X,R,T>(reader: &mut R,msg_len:u8,flags:u8,error_correct:bool)->Result<(MessageReadSummary,T),<X::ReaderType as ReadDeserializer>::Error>
where
    X: ConcreteTypeProvider + SystemConsts,
    R: std::io::Read+std::io::Seek,
    T: DocuFortMsg + for<'de>serde::Deserialize<'de>,
{
    let mut msg_len = msg_len as usize;
    let mut msg_and_meta_len = msg_len + 2;
    let message_start = reader.seek(std::io::SeekFrom::Current(0))? - 2;

    let has_msg_ecc = flags & X::ECC_FLAG == X::ECC_FLAG;
    let has_msg_data = flags & X::MSG_DATA_FLAG == X::MSG_DATA_FLAG;
    
    let msg_tag = flags & X::CLEAR_MSG_FLAGS;
    assert!(msg_tag == *T::MSG_TAG);

    let mut ecc_len = if has_msg_ecc {X::EccType::calc_ecc_data_len(msg_and_meta_len)}else{0};
    let data_info_len = if has_msg_data {DATA_META_LEN as usize}else{0};
    let mut msg_buf = vec![0u8;msg_and_meta_len +ecc_len+data_info_len];
    msg_buf[0] = msg_len as u8;
    msg_buf[1] = flags as u8;
    reader.read_exact(&mut msg_buf[2..])?;

    let mut errors_corrected = if error_correct && has_msg_ecc {
        let errors = X::EccType::apply_ecc(&mut msg_buf[..msg_and_meta_len+ecc_len])?;
        errors
    }else{0};
    
    let message: T = X::ReaderType::read_from(&msg_buf[2..msg_len])?;

    if has_msg_data {
        let data_start = msg_buf.len();
        let sys_data_flag = *msg_buf.last().unwrap();
        let slice = &msg_buf[msg_buf.len()-5..msg_buf.len()-1];
        let data_len = u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]);
        let errors = if errors_corrected > 0 {Some((errors_corrected,msg_buf))}else{None};
        return Ok((MessageReadSummary{message_start,errors,data:Some((data_start as u64,data_len,sys_data_flag))},message))
    }else{
        let errors = if errors_corrected > 0 {Some((errors_corrected,msg_buf))}else{None};
        return Ok((MessageReadSummary{message_start,errors,data:None},message))
    }
}


///Writes message and any data to given writer
/// Writes = msg_len | msg_tag | msg |?msg_ecc | ?data_len(u32_le) | ?sys_data_tag(1) | ?data_bytes |? data_ecc_data |
pub fn write_doc<X,W,T>(writer: &mut W,message: T,try_compress: Option<CompressionLevel>,calc_ecc:bool)->Result<(),<X::WriterType as WriteSerializer>::Error>
where
    X: ConcreteTypeProvider+SystemConsts,
    W: std::io::Write + std::io::Seek,
    T: DocuFortMsg + serde::Serialize,
{
    let mut msg_tag = *T::MSG_TAG;
    
    let msg_size = X::WriterType::serialized_size(&message)?;
    assert!(msg_size < u8::MAX as usize);
    let msg_and_meta_size = msg_size+ 2;//+1 for msg_len byte +1 for msg_tag

    // See note where msg_ecc is applied
    // let mut msg_ecc_len = calc_ecc.and_then(|ecc_len|Some(calc_ecc_data_len(msg_size, ecc_len)));
    let msg_ecc_len = if calc_ecc {Some(X::EccType::calc_ecc_data_len(msg_and_meta_size))}else{None};

    let has_data = message.has_data();
    if has_data.is_some() {
        msg_tag |= X::MSG_DATA_FLAG;
    }
    
    let data = if let Some(ecc_data_len) = msg_ecc_len {
        let mut msg_bytes = vec![0u8;msg_and_meta_size + ecc_data_len];
        //we include our metadata in the ecc
        msg_bytes[0] = msg_size as u8;
        msg_bytes[1] = msg_tag as u8;
        X::WriterType::serialize_into(&mut msg_bytes, &message)?;
        {
            let (msg,mut ecc) = msg_bytes.split_at_mut(msg_size);
            X::EccType::calc_ecc_into(&mut ecc, msg)?;
        }
        writer.write_all(&msg_bytes)?;
        message.take_data()
    }else{
        //msg_meta
        writer.write_all(&[msg_size as u8,msg_tag as u8])?;
        X::WriterType::serialize_into(writer, &message)?;
        message.take_data()
    };

    if data.is_none() {
        assert!(has_data.is_none(),"Error in DocuFortMsg trait Impl");
        return Ok(())
    }
    let data = data.unwrap();
    let mut sys_data_tag = if calc_ecc {X::ECC_FLAG}else{0};
    
    let mut data_len = data.len();
    let data_ecc_len = if calc_ecc {Some(X::EccType::calc_ecc_data_len(data_len))}else{None};
    assert!(data_len == has_data.unwrap());
    //write the len as u32, this might change but we will advance the writer
    writer.write_all((data_len as u32).to_le_bytes().as_slice())?;
    writer.write_all(&[sys_data_tag])?;//temp write the tag
    let start_pos = writer.seek(std::io::SeekFrom::Current(0))?;
    let mut end_pos = start_pos  + data_len as u64;



    //try compresssion, and THEN apply ECC
    if try_compress.is_some() && data_len >= X::MIN_LEN_TRY_COMP{
        //if we are here, we are mostly certain that the compressed data will be smaller than the original
        //if this is true, then it might not have to re-allocate our Vec, so we should just write directly to the writer
        X::CompressorType::compress_into(writer, &data, try_compress)?;
        let cur_pos = writer.seek(std::io::SeekFrom::Current(0))?;

        if cur_pos != end_pos {
            assert!(cur_pos<end_pos, "Call to compress_into should result in the same length or less data written!");
            data_len = (cur_pos - start_pos) as usize;
            writer.seek(std::io::SeekFrom::Start(start_pos-DATA_META_LEN as u64))?;
            writer.write_all((data_len as u32).to_le_bytes().as_slice())?;
            //mark the sys_data_tag
            sys_data_tag |= X::DATA_COMP_FLAG;
            writer.write_all(&[sys_data_tag])?;//update tag, the ecc flag should already be set
            writer.seek(std::io::SeekFrom::Start(cur_pos))?;//skip back to end of data

        }//else our tag and len are correct
    }
    if let Some(data_ecc_len) = data_ecc_len {
        let mut ecc_bytes = vec![0u8;data_ecc_len];
        X::EccType::calc_ecc_into(&mut ecc_bytes, &data)?;
        writer.write_all(&data)?;
        writer.write_all(&ecc_bytes)?;
    }

    Ok(())
}