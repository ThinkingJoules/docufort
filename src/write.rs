/*!
The format for a Docufort file is simple, consisting of three distinct message types. The primary point of interaction for users is the 'Content' message.

# Docufort File Layout

| Bytes | Description |
| --- | --- |
| 0..8 | Magic Number (b"docufort") |
| 8..10 | Version |
| 10..11 | ECC_LEN value (Reed-Solomon encoding value) |
| 11 onwards | First block starts |

## Block Structure

Each block is structured into three components: BlockStart, Content, and BlockEnd.

Each Component has a leading 'Header' that has the same fields and length.
### Header
| Byte Range | Field | Type | Description |
| --- | --- | --- | --- |
| 0..1 | FLAG_TAG | bytes | How to read what follows this header |
| 1..9 | Timestamp | u64 | Time of component creation (implementer can use it for whatever, not used internally) |
| 9..13 | header data | u32 | Represents Length of the data field on given certain flags, unused on others |
| 13..13+ECC_LEN | ECC info for Header | bytes | The ECC data for integrity and recovery |

### 1. BlockStart
The block start is the only thing that is not preceded by another component.
Preceding this component and its header is the MAGIC_NUMBER (b'docufort') and its ECC data (ECC_LEN).
This is used in the first step of recovery. We find a matching position for a recoverable MAGIC_NUMBER and know we are at the start of a block.

There is nothing more to a BlockStart than the Header. Their might be different encodings of what follows this header.
- A FLAG_TAG of b'A' is an Atomic Block with no error correction on the contents of the block.
    - The header data field represents the number of bytes for the atomic write. These directly follow the header.
- A FLAG_TAG of b'Q' is an Atomic Block with error correction on the contents of the block.
    - This header is followed directly by the ECC Data for the content, then the content bytes.
    - Since ECC is fixed for the life of the file, we can deduce the length of the ECC Data, given the content len (header data u32)
- A FLAG_TAG of b'B' is a Best Effort Block. A series of 'Content' components follow this header.

### 2. Content

Is a Header followed by the same pattern as an Atomic Block, but with different leading tag identifiers.
- A FLAG_TAG of b'@' is a Content Block with no error correction on the contents of the block.
    - The header data field represents the number of bytes for the atomic write. These directly follow the header.
- A FLAG_TAG of b'P' is a Content Block with error correction on the contents of the block.
    - This header is followed directly by the ECC Data for the content, then the content bytes.

### 3. BlockEnd

Is a Header followed by a 160bit hash of the block contents (all bytes after the BlockStart Header to the start of this header).
The Tag for a BlockEnd is b'D'.
The hash is also ECC'd to ensure integrity to avoid unnecessary error correction decoding during recovery.

| Byte Range | Field | Type | Description |
| --- | --- | --- | --- |
| 0..20 | Hash of block | 160-bit | Hash of the entire block |
| 20..20+ECC_LEN | ECC Data | bytes | ECC for the end block |


## Block Type

There are two types of blocks:

- **Atomic (A) block**: Contains a single blob of Content between BlockStart and BlockEnd.
    - If this is not perfectly written during a crash, all content will revert.
    - It follows that any corruption within this block will cause it to logically *become* invalid, even if it is not at the tail of the file.
- **Best Effort (B) block**: Contains ```Vec<BlockContent>``` Messages between a BlockStart and BlockEnd.

Content within either block may skip ECC calculation if the extra storage and computation cost is considered unnecessary.

## Importance of ECC

ECC is utilized as both a checksum and an integrity insurance for the Start/End and Content (header portion) blocks to aid recovery.
The magic number requires its own ECC value, otherwise a single flipped bit could result in the loss of a whole block during recovery. 

To enhance robustness, the ECC data for the DATA should be *prepended* to avoid misinterpretation of content (b'docufort') (with ECC data) as a block's start during recovery.

By incorporating ECC, the system is more resilient to data corruption, making it robust for a wide range of applications.

## Hash
This hash is used to avoid checking ECC to find errors. If the hash checks out there is no need to do ECC on the Contents.
It is recommended to use a cryptographic hash.

*/


use crate::{core::{BlockInputs, ComponentHeader}, ECC_LEN, ecc::{calculate_ecc_chunk, calculate_ecc_for_chunks}, MN_ECC, MAGIC_NUMBER, HASH_LEN, BlockTag, ReadWriteError, HashAdapter};


/// Initializes a new DocuFort file at the specified path.
///
/// This function creates a new file and writes the initialization header data, which includes
/// the magic number, version, and ecc length value.
pub fn init_file<W:std::io::Write>(file: &mut W) -> std::io::Result<()> {
    file.write_all(&MAGIC_NUMBER)?;
    file.write_all(&[b'V',b'1'])?;
    file.write_all(&[ECC_LEN as u8])?;   
    Ok(())
}


/// Writer represents the append only file, with the writer position at the end of the file.
/// This only writes the magic number and its ecc data.
pub fn write_magic_number<W: std::io::Write>(writer: &mut W)->std::io::Result<()>{
    writer.write_all(&MAGIC_NUMBER)?;
    writer.write_all(&MN_ECC)?;
    Ok(())
}

///Calculates ECC and Writes the header to the given writer.
pub fn write_header<W: std::io::Write>(writer: &mut W,header:&ComponentHeader)->Result<(),ReadWriteError>{
    writer.write_all(header.as_slice())?;
    calculate_ecc_chunk(header.as_slice(), writer)?;
    Ok(())
}
///Calculates ECC and Writes the header to the given writer.
pub fn write_content_header<W: std::io::Write, B:BlockInputs>(writer: &mut W,data_len:u32,has_ecc:bool,time_stamp: Option<[u8;8]>,hasher:&mut B)->Result<(),ReadWriteError>{
    let tag = if has_ecc {BlockTag::CEComponent as u8}else{BlockTag::CComponent as u8};
    let time_stamp = if let Some(ts) = time_stamp {ts}else{B::current_timestamp()};
    let data = data_len.to_le_bytes();
    let content_header = ComponentHeader::new_from_parts(tag, time_stamp, Some(data));
    let mut ha = HashAdapter::new(writer, hasher);
    use std::io::Write;
    ha.write_all(content_header.as_slice())?;
    calculate_ecc_chunk(content_header.as_slice(), &mut ha)?;
    Ok(())
}

///Only use with Atomic Block. Does **NOT** write the header.
pub fn write_content<W: std::io::Write,B:BlockInputs>(writer: &mut W,content:&[u8],calc_ecc:bool,hasher:&mut B)->Result<(),ReadWriteError>{
    if calc_ecc {
        let mut hw = HashAdapter::new(writer, hasher);
        calculate_ecc_for_chunks(content, &mut hw)?;
    }
    hasher.update(content);
    writer.write_all(content)?;
    Ok(())
}
/// Writer represents the append only file, with the writer position at the end of the file.
pub fn write_block_end<W: std::io::Write>(writer: &mut W,header:&ComponentHeader,hash:&[u8;HASH_LEN])->Result<(),ReadWriteError>{
    write_header(writer, header)?;
    writer.write_all(hash)?;
    calculate_ecc_chunk(&hash, writer)?;
    Ok(())
}

///Writes Header + Content Component, optionally computes ECC
pub fn write_content_component<W: std::io::Write,B:BlockInputs>(writer: &mut W,calc_ecc:bool,time_stamp: Option<[u8;8]>,content:&[u8],hasher:&mut B)->Result<(),ReadWriteError>{
    let data_len = content.len() as u32;
    write_content_header(writer, data_len,calc_ecc,time_stamp,hasher)?;
    write_content(writer, content, calc_ecc, hasher)?;
    Ok(())
}

///Writes Header + Content Component, optionally computes ECC
pub fn write_atomic_block<W: std::io::Write,B:BlockInputs>(writer: &mut W,start_time_stamp: Option<[u8;8]>,content:&[u8],calc_ecc:bool,end_block:Option<&ComponentHeader>)->Result<(),ReadWriteError>{
    let mut h = B::new();
    let tag = if calc_ecc {BlockTag::StartAEBlock}else{BlockTag::StartABlock};
    let data = content.len() as u32;
    let time_stamp = start_time_stamp.unwrap_or_else(||B::current_timestamp());
    let header = ComponentHeader::new_from_parts(tag as u8,time_stamp , Some(data.to_le_bytes()));
    write_header(writer, &header)?;   
    write_content(writer, content, calc_ecc, &mut h)?;
    let hash = h.finalize();
    if let Some(header) = end_block {
        assert_eq!(header.tag(),BlockTag::EndBlock);
        write_block_end(writer, header, &hash)?;
    }else{
        let tag = BlockTag::EndBlock;
        let data = None;
        let time_stamp = B::current_timestamp();
        let header = ComponentHeader::new_from_parts(tag as u8,time_stamp , data);
        write_block_end(writer, &header, &hash)?;
    }
    Ok(())
}



#[cfg(test)]
mod test_super {
    use crate::BlockTag;
    use super::*;
    use std::io::Cursor;

    struct DummyHasher(blake3::Hasher);
    impl BlockInputs for DummyHasher {
        fn new() -> Self {
            Self(blake3::Hasher::new())
        }

        fn update(&mut self, data: &[u8]) {
            self.0.update(data);
        }

        fn finalize(&self) -> [u8; HASH_LEN] {
            self.0.finalize().as_bytes()[0..HASH_LEN].try_into().unwrap()
        }

        fn current_timestamp() -> [u8;8] {
            unimplemented!()
        }
    }
    #[test]
    fn test_write_magic_number() {
        let mut writer = Cursor::new(Vec::new());
        let result = write_magic_number(&mut writer);

        assert!(result.is_ok(), "write_magic_number returned an error: {:?}", result.err());

        let data = writer.into_inner();
        assert_eq!(&data[0..MAGIC_NUMBER.len()], &MAGIC_NUMBER, "The magic number wasn't written correctly");

        assert_eq!(data[MAGIC_NUMBER.len()..], MN_ECC, "The ECC data wasn't written correctly");
    }

    #[test]
    fn test_write_header() {
        let mut writer = Cursor::new(Vec::new());
        let time_stamp = [1u8;8];
        let header = ComponentHeader::new_from_parts(BlockTag::StartBBlock as u8, time_stamp, None);
        let result = write_header(&mut writer,&header);

        assert!(result.is_ok(), "write_header returned an error: {:?}", result.err());
        let data = writer.into_inner();

        assert_eq!(data[0],BlockTag::StartBBlock as u8);
        assert_eq!(&data[1..9],[1u8;8]);
        assert_eq!(&data[9..13],[0u8;4]);

    }

    #[test]
    fn test_write_content_no_ecc() {
        let mut writer = Cursor::new(Vec::new());
        let cont = &[1u8,2,3,4,5,6,7,8,9,0];
        let mut h = DummyHasher::new();
        let result = write_content(&mut writer,cont,false,&mut h);

        assert!(result.is_ok(), "write_content returned an error: {:?}", result.err());
        let data = writer.into_inner();

        assert_eq!(&data[0..3],&[1,2,3]);
    }

    #[test]
    fn test_write_content_ecc() {
        let mut writer = Cursor::new(Vec::new());
        let cont = &[1u8,2,3,4,5,6,7,8,9,0];
        let mut h = DummyHasher::new();
        let result = write_content(&mut writer,cont,true,&mut h);

        assert!(result.is_ok(), "write_content returned an error: {:?}", result.err());
        let data = writer.into_inner();

        assert_eq!(&data[0..3],&[166, 78, 63]);
    }

    #[test]
    fn test_write_block_end() {
        let mut writer = Cursor::new(Vec::new());
        let time_stamp = [1u8;8];
        let header = ComponentHeader::new_from_parts(BlockTag::EndBlock as u8, time_stamp, None);
        let hash = [2u8;HASH_LEN];
        let result = write_block_end(&mut writer,&header,&hash);

        assert!(result.is_ok(), "write_content returned an error: {:?}", result.err());
        let data = writer.into_inner();

        assert_eq!(data[0],BlockTag::EndBlock as u8);
        assert_eq!(&data[1..9],[1u8;8]);
        assert_eq!(&data[9..13],[0u8;4]);
        assert_eq!(&data[13+ECC_LEN..23+ECC_LEN],&hash[..10]);
    }

    #[test]
    fn test_write_a_block_no_ecc() {
        let mut writer = Cursor::new(Vec::new());
        let start_time_stamp = [1u8;8];
        let end_time_stamp = [2u8;8];
        let content = &[1u8,2,3,4,5,6,7,8,9,0];
        let end_block = ComponentHeader::new_from_parts(BlockTag::EndBlock as u8, end_time_stamp, None);
        let result = write_atomic_block::<_,DummyHasher>(&mut writer, Some(start_time_stamp), content, false, Some(&end_block));

        assert!(result.is_ok(), "write_content returned an error: {:?}", result.err());
        let data = writer.into_inner();

        assert_eq!(data[0],BlockTag::StartABlock as u8);
        assert_eq!(&data[1..9],[1u8;8]);
        assert_eq!(&data[9..13],[10,0,0,0]);
        assert_eq!(&data[13+ECC_LEN..23+ECC_LEN],&content[..10]);
        assert_eq!(data[13+ECC_LEN+content.len()..14+ECC_LEN+content.len()][0],BlockTag::EndBlock as u8);
    }

    #[test]
    fn test_write_a_block_ecc() {
        let mut writer = Cursor::new(Vec::new());
        let start_time_stamp = [1u8;8];
        let end_time_stamp = [2u8;8];
        let content = &[1u8,2,3,4,5,6,7,8,9,0];
        let end_block = ComponentHeader::new_from_parts(BlockTag::EndBlock as u8, end_time_stamp, None);
        let result = write_atomic_block::<_,DummyHasher>(&mut writer, Some(start_time_stamp), content, true, Some(&end_block));

        assert!(result.is_ok(), "write_content returned an error: {:?}", result.err());
        let data = writer.into_inner();

        assert_eq!(data[0],BlockTag::StartAEBlock as u8);
        assert_eq!(&data[1..9],[1u8;8]);
        assert_eq!(&data[9..13],[10,0,0,0]);
        assert_eq!(&data[13+ECC_LEN*2..23+ECC_LEN*2],&content[..10]);
        assert_eq!(&data[13+(ECC_LEN*2)+content.len()..14+(ECC_LEN*2)+content.len()],&[BlockTag::EndBlock as u8]);

    }

}
