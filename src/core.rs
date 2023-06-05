use docufort_macros::MsgCoder;
use serde::*;


pub const MAGIC_NUMBER: [u8; 8] = [0x64, 0x6F, 0x63, 0x75, 0x66, 0x6F, 0x72, 0x74]; //b"docufort"

/// A structure representing the start of a block in the data storage.
///
/// This block start message is important for the crash recovery process. When set as `atomic`, all writes will be rolled back 
/// if the system crashes before this block is properly closed off with a block end message. This guarantees the atomicity of the
/// operations within the block, i.e., either all operations succeed, or none do.
///
/// If `atomic` is set to false, the recovery process will attempt to recover as many trailing messages as possible and disregard
/// the last, incomplete message.
///
/// # Fields
///
/// - `magic_number`: An array of bytes representing the magic number for block start.
/// - `time_stamp`: A timestamp marking the start of the block. The most significant bit of the timestamp is used to mark if the block is atomic.
///
/// # Example
///
/// ```
/// let block_start = DfBlockStart::new(1622558943, true);
/// assert_eq!(block_start.is_atomic(), true);
/// assert_eq!(block_start.get_ts(), 1622558943);
/// ```
#[derive(Debug,PartialEq,Eq,PartialOrd,Ord,MsgCoder)]
pub struct DfBlockStart {
    pub magic_number: [u8;8],
    pub time_stamp:u64,
}
impl DfBlockStart {
    /// Constructs a new `DfBlockStart` with the provided timestamp and atomicity.
    ///
    /// The `time_stamp` should be a valid UNIX timestamp (number of seconds since 1970-01-01 00:00:00 UTC).
    /// The `atomic` parameter determines whether the block should be considered atomic or not.
    ///
    /// # Example
    ///
    /// ```
    /// let block_start = DfBlockStart::new(1622558943, true);
    /// ```
    pub fn new(time_stamp:u64,atomic:bool) -> Self {
        let mut ts = time_stamp;
        if atomic {ts |= 1<<63}
        Self {
            time_stamp: ts,
            magic_number: MAGIC_NUMBER,
        }
    }
    /// Returns the timestamp of the block start.
    ///
    /// # Example
    ///
    /// ```
    /// let block_start = DfBlockStart::new(1622558943, true);
    /// assert_eq!(block_start.get_ts(), 1622558943);
    /// ```
    pub fn get_ts(&self)->u64{
        self.time_stamp & !(1 << 63)
    }
    /// Returns `true` if the block is atomic, and `false` otherwise.
    ///
    /// # Example
    ///
    /// ```
    /// let block_start = DfBlockStart::new(1622558943, true);
    /// assert_eq!(block_start.is_atomic(), true);
    /// ```
    pub fn is_atomic(&self)->bool{
        self.time_stamp & (1 << 63) == 1<<63
    }
}
impl DocuFortMsg for DfBlockStart{
    const MSG_TAG: u8 = 0;
    const FIXED_INTS: bool = true;
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
/// A structure representing the end of a block in the data storage.
///
/// This block end marker contains a timestamp and a hash of the block content.
///
/// The choice of the hash function is left to the implementer. It is perfectly acceptable to use the first 160 bits 
/// of a longer hash function output if a specific function that does not have a shorter output is preferred.
///
/// # Fields
///
/// - `time_stamp`: A timestamp marking the end of the block.
/// - `hash`: A 160-bit (20-byte) hash representing the block content.
///
/// # Example
///
/// ```
/// let block_end = DfBlockEnd::new(1622558943, [0u8; 20]);
/// ```
#[derive(Debug,PartialEq,Eq,PartialOrd,Ord,MsgCoder)]
pub struct DfBlockEnd {
    pub time_stamp: u64,
    pub hash: [u8;20],
}
impl DfBlockEnd {
    /// Constructs a new `DfBlockEnd` with the provided timestamp and hash.
    ///
    /// The `time_stamp` should be a valid UNIX timestamp (number of seconds since 1970-01-01 00:00:00 UTC).
    /// The `hash` should be a 160-bit (20-byte) value representing the hash of the block content.
    ///
    /// # Example
    ///
    /// ```
    /// let block_end = DfBlockEnd::new(1622558943, [0u8; 20]);
    /// ```
    pub fn new(time_stamp:u64,hash: [u8;20]) -> Self {
        Self {
            time_stamp,
            hash,
        }
    }
}
impl DocuFortMsg for DfBlockEnd{
    const MSG_TAG: u8 = 1;

    const FIXED_INTS: bool = true;

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

/// A structure representing the compression level to be used in compression operations.
/// Lower values mean less compression (faster, but larger result), 
/// while higher values mean more compression (slower, but smaller result).
///
/// # Example
///
/// ```
/// let level = CompressionLevel::from(3);
/// // Now `level` can be used in the compression functions that require a compression level.
/// ```
#[derive(Copy, Clone, Debug, PartialOrd, Ord,PartialEq, Eq)]
pub struct CompressionLevel(pub u8);
impl std::ops::Deref for CompressionLevel {
    type Target = u8;

    /// Dereferences the value.
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u8> for CompressionLevel {
    /// Constructs a `CompressionLevel` from a `u8`.
    ///
    /// # Example
    ///
    /// ```
    /// use docufort::CompressionLevel;
    /// let level = CompressionLevel::from(3);
    /// ```
    fn from(level: u8) -> Self {
        CompressionLevel(level)
    }
}

/// `Compressor` is a trait for compressing and decompressing data. It's critical to implement this trait correctly for the proper functioning of data compression and decompression in your application.
///
/// # Associated Types
///
/// * `Error`: The type of error that methods of this trait should return upon failure.
///
/// # Methods
///
/// * `compress_into`: Attempts to compress data and write it into a provided writer. If compression isn't beneficial (i.e., the compressed data is the same length or longer), the original, uncompressed data must be written instead. The method returns `Ok(None)` if the data was not compressed but written as is. It returns `Ok(Some(compressed_length))` if the data was successfully compressed. If the writer runs out of space during the operation (e.g., because the compressed data ends up being larger than the original data), an `EoF` error should not be returned, the uncompressed data should be written.
///
/// * `decompress_into`: Decompresses the provided data and writes the uncompressed data into a provided writer. This method will only be called if the data is known to be compressed.
///
/// # Example
///
/// Implementations of this trait could include various compression algorithms, such as gzip or zlib.
///
/// # Important
///
/// It is crucial to handle the `EoF` error correctly in the `compress_into` method. This error should be returned *only* if it occurs when writing the uncompressed data, or compressed data shorter than the uncompressed.
///
/// The `compress_into` method should ensure that if the compressed data is shorter than the uncompressed data, else it writes the uncompressed data. This check ensures that compression does not unnecessarily increase the data size.
pub trait Compressor {
    type Error;
    /// Tries to compress data and writes it into the provided writer.
    /// If the compression is not beneficial (compressed data is the same length or longer), the original data is written.
    /// If an EoF error occurs during the writing it should only be returned from writing the uncompressed or compressed data shorter than the uncompressed.
    /// In other words, if compression is not beneficial and causes an EoF error, this should be caught inside the implementation.
    /// The method returns `Ok(Some(compressed_length))` if the data was successfully compressed, or `Ok(None)` if the data was not compressed but written as is.
    fn compress_into<W: std::io::Write+std::io::Seek>(writer: &mut W, data: &[u8], try_compress: Option<CompressionLevel>) -> Result<(), Self::Error>;
    // Decompresses provided data and writes the uncompressed data into the provided writer.
    /// This method will only be called if the data is known to be compressed.
    fn decompress_into<W: std::io::Write>(writer: &mut W, data: &[u8]) -> Result<(), Self::Error>;
}

/// `Eccer` is a trait for error checking and correction (ECC). Correct implementation of this trait is essential for the correct handling of error correction in your application.
///
/// # Associated Types
///
/// * `Error`: The type of error that methods of this trait should return upon failure.
///
/// # Methods
///
/// * `calc_ecc_into`: Calculates the error correction code (ECC) for the given raw data and writes it into the provided writer.
///
/// * `apply_ecc`: Attempts to correct any errors in the given mutable raw data slice using ECC. It returns `Ok(number_of_errors_corrected)` upon successful error correction. Note that the input raw data may be modified by this function.
///
/// * `calc_ecc_data_len`: Determines the length of the ECC data based on the length of the raw data. The implementation should respect the ECC_LEN constant, but may adjust the length based on the raw data length. 
/// Note: Any variable length encoding schemes cannot be changed once the first message is written to disk. Any change will result in incorrect decoding offsets for existing data since the ECC data length is not written to disk - only the raw data length is. This function is crucial in determining how bytes are read.
///
/// # Example
///
/// Implementations of this trait could include various ECC algorithms, such as Hamming codes or Reed-Solomon codes.
///
/// # Important
///
/// If you decide to adjust the length of the ECC data based on the raw data length in `calc_ecc_data_len`, remember that this decision is permanent for that implementation. Any subsequent change will disrupt the decoding offsets of existing data, leading to incorrect data interpretation.
/// If you need to make a change you will need to start a new file and decode from the old file and encode the new info to the new file.
pub trait Eccer {
    type Error;
    /// Calculates the error correction code (ECC) for the given raw data and writes it into the provided writer.
    fn calc_ecc_into<W: std::io::Write>(writer: &mut W, raw_data: &[u8]) -> Result<(), Self::Error>;
    /// Attempts to correct any errors in the given mutable raw data slice. Returns the number of errors corrected upon successful operation. 
    /// Note that the input raw data may be modified by this function.
    /// The raw_data should be: | msg_len_u8 | msg_tag_u8 | msg_bytes | ecc_data |
    fn apply_ecc(raw_data: &mut[u8]) -> Result<usize, Self::Error>;
    /// Determines the length of the ECC data based on the length of the raw data. It's crucial to keep the output of this function consistent 
    /// across the lifespan of an implementation. Changes can result in incorrect decoding offsets for existing data.
    fn calc_ecc_data_len(raw_data_len:usize)->usize;
}

/// `correct_errors` is a function that attempts to correct errors identified during a message read operation.
///
/// Given a `MessageReadSummary` and a writer that implements `std::io::Write` and `std::io::Seek`, this function will seek to the start of the message and overwrite the original content with the fixed content, if any errors were reported.
///
/// # Parameters
///
/// * `writer`: A mutable reference to a writer that implements `std::io::Write` and `std::io::Seek`.
/// * `summary`: A `MessageReadSummary` instance, summarizing the outcome of a previous message read operation.
///
/// # Returns
///
/// If successful, this function returns a `Result` with the number of corrected errors. If no errors were reported in the `MessageReadSummary`, the function returns `Ok(0)`. If an error occurs during the error correction process, it returns `Err` with the corresponding `std::io::Error`.
///
/// # Example
///
/// ```
/// use docufort::*;
/// let summary = MessageReadSummary{
///     errors: Some((1, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08])),
///     message_start: 10,
///     data: Some((23, 5, 0x20)),
/// };
///
/// //writer should be the main append only file.
/// let mut writer = std::io::Cursor::new(vec![0;20]);
///
/// let corrected = correct_errors(&mut writer, summary).unwrap();
/// assert_eq!(corrected, 1);
/// ```
///
/// In the example above, `correct_errors` attempts to correct three errors (indicated by the `errors` field in `summary`) by overwriting the original content in `writer` starting at position 22.
pub fn correct_errors<W: std::io::Write + std::io::Seek>(writer: &mut W,summary:MessageReadSummary)->Result<usize,std::io::Error>{
    let MessageReadSummary { errors, message_start, .. } = summary;
    if errors.is_none() {return Ok(0)}
    let (num_errors,fixed) = errors.unwrap();
    writer.seek(std::io::SeekFrom::Start(message_start))?;
    writer.write_all(&fixed)?;
    Ok(num_errors)
}

/// `MessageReadSummary` is a struct that encapsulates the outcome of a message read operation.
///
/// It provides an organized way to summarize the result of a message reading operation, including error correction information, the start of the message, and optional data that was not read from disk.
///
/// # Fields
///
/// * `errors`: Some((error_corrected, correct_msg_bytes)) indicates there was at least one error corrected, and the included bytes *should* be written back to disk.
///
/// * `message_start`: The start position (offset) of the message document in the data source. The start position is the meta serialization start point, not the start of the Message struct.
///
/// * `data`: An optional tuple indicating the presence of associated data. The tuple includes the start position of the data, the length of the data, and a flag byte. If this field is `None`, it means that the message doesn't have associated data.
///
/// # Example
///
/// ```
/// let summary = MessageReadSummary{
///     errors: Some((1, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08])),
///     message_start: 10,
///     data: Some((23, 5, 0x20)),
/// };
/// ```
///
/// In the example above, `summary` indicates that there was one error corrected during the message read operation, the message starts at position 10, and there is associated data starting at position 23 with a length of 5 bytes and a flag byte of 0x20.
/// If you don't want to use the supplied helper function `correct_errors`. Then simply write all the correct_msg_bytes to the file starting at offset message_start
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageReadSummary{
    pub errors:Option<(usize,Vec<u8>)>,
    pub message_start: u64, //if errors is_some() write the whole vec starting at message_start
    ///(Start,Len,FlagByte)
    pub data: Option<(u64,u32,u8)>,
}

/// A trait for serializing a DocuFortMsg into a writer.
pub trait WriteSerializer {
    /// The type of error that can occur during serialization.
    type Error;
    
    /// Serializes a value into a writer.
    ///
    /// # Arguments
    ///
    /// * `writer` - The mutable reference to the writer to serialize into.
    /// * `message` - The reference to the DocuFortMsg to be serialized.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if serialization is successful, otherwise returns an error of type `Self::Error`.
    ///
    fn serialize_into<W: std::io::Write, T: Serialize + DocuFortMsg>(writer: &mut W, message: &T) -> Result<(), Self::Error>;
    
    /// Returns the serialized size of a Message.
    ///
    /// # Arguments
    ///
    /// * `message` - The reference to the DocuFortMsg to determine the serialized size of.
    ///
    /// # Returns
    ///
    /// Returns the size of the serialized value as a `usize`, or an error of type `Self::Error`.
    ///
    fn serialized_size<T: Serialize + DocuFortMsg>(message: &T) -> Result<usize, Self::Error>;

    ///Inteded to be microseconds, but doesn't have to be.
    fn current_timestamp()->u64;

    ///For hashing the data blocks for integrity checks.
    fn hash(bytes:&[u8])->[u8;20];

}

/// A trait for deserializing a DocufortMsg from bytes.
pub trait ReadDeserializer {
    /// The type of error that can occur during deserialization.
    type Error;
    
    /// Deserializes a value from bytes.
    ///
    /// # Arguments
    ///
    /// * `bytes` - The byte slice containing the serialized data.
    ///
    /// # Returns
    ///
    /// Returns the deserialized DocuFortMsg, or an error of type `Self::Error`.
    ///
    fn read_from<T: de::DeserializeOwned + DocuFortMsg>(bytes: &[u8]) -> Result<T, Self::Error>;
}

/// An enum summarizing the results of a DocuFort block verification.
///
/// Each variant indicates a different outcome from the block verification process, and contains relevant data for handling that outcome.
///
/// - `MaybeSuccess`: The block was potentially successfully verified. If errors were encountered, they are included along with their locations and suggested patches. 
/// The starting and ending file offsets to hash are included to allow implementer to check the integrity, along with the `DfBlockEnd` struct are also included. 
/// To confirm success, these patches should be applied and the hash of the block should be recomputed and compared with the hash in the `DfBlockEnd` struct.
///
/// - `OpenABlock`: The block is an Atomic block that is currently open, indicating an unexpected termination during block writing. 
/// If any errors were encountered, they are included along with their locations and suggested patches. 
/// In this case, the file should be truncated at the block start and a new block should be attempted.
///
/// - `OpenBBlock`: The block is a Basic block that is currently open, indicating an unexpected termination during block writing. 
/// If any errors were encountered, they are included along with their locations and suggested patches. 
/// The file should be truncated at the specified offset, then a DfBlockEnd calculated and written.
///
/// - `BlockStartFailedDecoding`: The DfBlockStart struct failed to decode, implying serious corruption. 
/// The file should be truncated at the block start the implementer should try searching backward for the next magic number.
pub enum DfBlockVerificationSummary{
    ///If there are patches they should be written to the file (file_offset,corrected_bytes), then hash the start..end range of the file to verify hash
    MaybeSuccess{errors:Option<(usize,Vec<(u64, Vec<u8>)>)>,hash_start_index:u64,hash_end_index:u64,end_struct:DfBlockEnd},
    ///If this is returned truncate file at block_start_offset and try finding another block
    OpenABlock{errors:Option<(usize,Vec<(u64, Vec<u8>)>)>},
    OpenBBlock{truncate_at_then_close_block:u64,errors:Option<(usize,Vec<(u64, Vec<u8>)>)>},
    ///Treat this the same as the OpenABlock case. Truncate at start and try again.
    BlockStartFailedDecoding,
}

pub trait DocuFortMsg {
    const MSG_TAG: u8;
    const FIXED_INTS: bool;
    fn take_data(self)->Option<Vec<u8>>;
    fn has_data(&self)->Option<usize>;
    fn set_data(&mut self, data:Vec<u8>);
}

///u32_le + 1 sys_data_tag byte
pub const DATA_META_LEN: u8 = 5;
