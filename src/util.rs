
use std::{time::{SystemTime, UNIX_EPOCH}, ops::Deref, sync::Arc, io::Read};

use crc::{Crc, CRC_32_CKSUM};

use serde::{Deserialize, Serialize};

use reed_solomon::{Encoder, Decoder, DecoderError};
use tokio::task;

use std::sync::{RwLock};

pub(crate) struct SharedOnce<T> {
    value: RwLock<Option<T>>,
    local:Option<T>
}

impl<T> SharedOnce<T> {
    pub(crate) fn new() -> Self {
        SharedOnce {
            value: RwLock::new(None),
            local:None
       }
    }

    pub(crate) fn set(&self, value: T) {
        let mut locked_value = self.value.write().unwrap();
        *locked_value = Some(value);
    }

    pub(crate) fn get(&self) -> T
    where
        T: Copy + Default,
    {
        if let Some(t) = self.local {
            return t
        }
        while self.value.read().unwrap().is_none() { // Wait for the value to be set
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        self.value.read().unwrap().unwrap()
    }
}


/// Microsecond Unix Timestamp
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize,PartialOrd, Ord, Hash)]
pub struct DocID(u64);

impl DocID {
    pub fn new() -> Self {
        Self(microseconds_since_epoch())
    }
}

impl Deref for DocID {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_CKSUM);

pub fn microseconds_since_epoch() -> u64 {
    let now = std::time::SystemTime::now();
    match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => {
            let seconds = duration.as_secs();
            let microseconds = duration.subsec_micros();
            seconds * 1_000_000 + u64::from(microseconds)
        },
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    }
}

pub fn ceiling_division(numerator: usize, denominator: usize) -> usize {
    if numerator % denominator == 0 {
        numerator / denominator
    } else {
        (numerator / denominator) + 1
    }
}
pub fn calc_ecc_data_len(raw_data_len:usize,ecc_len:u8)->usize{
    ceiling_division(raw_data_len, (255-ecc_len) as usize)*ecc_len as usize
}

pub struct ArcSlice{
    arc: Arc<Vec<u8>>,
    start: usize,
    end: usize,
    position: usize
}

impl Read for ArcSlice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Check if we have reached the end
        if self.position >= self.end {
            return Ok(0);
        }

        // Determine how much we can read
        let available = self.end - self.position;
        let to_read = buf.len().min(available);

        // Copy the bytes from our slice to the buffer
        let start = self.position;
        let end = start + to_read;
        buf[..to_read].copy_from_slice(&self.arc[start..end]);

        // Update our position
        self.position = end;

        Ok(to_read)
    }
}
impl Deref for ArcSlice {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.arc[self.start..self.end]
    }
}
impl AsRef<[u8]> for ArcSlice {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl ArcSlice{
    pub fn new(arc: &Arc<Vec<u8>>, start: usize, end: usize) -> Self { Self { arc:arc.clone(), start, end,position:start } }
}

pub fn calculate_ecc_chunk<T: AsRef<[u8]>>(data: T,ecc_len:u8) -> Vec<u8> {
    let bytes: &[u8] = data.as_ref();
    let encoder = Encoder::new(ecc_len as usize);
    let ecc_data = encoder.encode(bytes);
    ecc_data.ecc().to_vec()
}
pub fn calculate_ecc_for_chunks(data: Vec<u8>, data_start: usize, data_end: usize, ecc_len: u8) -> (Vec<u8>, Vec<u8>) {
    let chunk_size = (255 - ecc_len) as usize;
    let len = data_end - data_start;
    let ecc_data_len = calc_ecc_data_len(len, ecc_len);
    let num_chunks = ecc_data_len / chunk_size;
    let mut ecc_data = Vec::with_capacity(ecc_data_len);

    for i in 0..num_chunks {
        let start = i * chunk_size;
        let end = ((i + 1) * chunk_size).min(len);
        let chunk_data = &data[start..end];

        let ecc = calculate_ecc_chunk(chunk_data, ecc_len);
        ecc_data.extend(ecc);
    }

    (data, ecc_data)
}

///takes the data buffer splits it up, makes calcs the ECC for each chunk, then concats all all the ECC data together -> (data,ecc_data)
pub async fn calculate_ecc_for_chunks_async(data: Vec<u8>,data_start:usize,data_end:usize, ecc_len: u8) -> (Vec<u8>, Vec<u8>){
    let chunk_size = (255 - ecc_len) as usize;
    let len = data_start-data_end;
    let ecc_data_len = calc_ecc_data_len(len, ecc_len);
    let num_chunks = ecc_data_len / chunk_size;
    let mut ecc_data = Vec::with_capacity(ecc_data_len);
    let arc = Arc::new(data);
    let data_section = ArcSlice::new(&arc, data_start, data_end);
    let tasks: Vec<_> = (0..num_chunks).map(|i| {
        let start = i * chunk_size;
        let end = ((i + 1) * chunk_size).min(len);
        let chunk_data = ArcSlice::new(&arc,start,end);

        task::spawn(async move {
            calculate_ecc_chunk(chunk_data, ecc_len)
        })
    }).collect();

    for task in tasks {
        let ecc = task.await.expect("Failed to await task");
        ecc_data.extend(ecc);
    }
    
    (Arc::try_unwrap(arc).unwrap(),ecc_data)
}

pub fn apply_ecc<T: AsRef<[u8]>>(ecc_data: T,ecc_len:usize) -> Result<Option<(usize,Vec<u8>)>,DecoderError> {
    let bytes: &[u8] = ecc_data.as_ref();
    let decoder = Decoder::new(ecc_len);
    if decoder.is_corrupted(&bytes) {
        let (buffer,errors) = decoder.correct_err_count(&bytes,None)?;
        let mut reader = buffer.data().chain(buffer.ecc());
        //should be the same length, overwrite what was given
        let mut out = Vec::with_capacity(bytes.len());
        reader.read_to_end(&mut out).expect("Buffert to short!");
        Ok(Some((errors,out)))
    }else{
        Ok(None)
    }
}

pub fn apply_ecc_for_chunks(mut raw_data: Vec<u8>, msg_len: usize, ecc_len: u8) -> Result<(usize, Vec<u8>), DecoderError> {
    let ecc_len = ecc_len as usize;
    let len = raw_data.len();
    let num_chunks = (len - msg_len) / ecc_len;
    assert_eq!((len - msg_len) % ecc_len, 0);
    let chunk_size = 255 - ecc_len;

    let mut tot_errors = 0;
    for i in 0..num_chunks {
        let mut chunk_data = Vec::with_capacity(chunk_size + ecc_len);
        let start = i * chunk_size;
        let end = (i + 1) * chunk_size;
        chunk_data.extend_from_slice(&raw_data[start..end]);
        let start = msg_len + i * ecc_len;
        let end = start + ecc_len;
        chunk_data.extend_from_slice(&raw_data[start..end]);

        match apply_ecc(chunk_data, ecc_len) {
            Ok(Some((errors, fixed_bytes))) => {
                // split out and copy the chunk and ecc back to the raw_data if there is an error
                let (chunk, ecc) = fixed_bytes.split_at(chunk_size);
                let start = i * chunk_size;
                let end = start + chunk_size;
                raw_data[start..end].copy_from_slice(chunk);
                let start = msg_len + i * ecc_len;
                let end = start + ecc_len;
                raw_data[start..end].copy_from_slice(ecc);

                tot_errors += errors;
            },
            Ok(None) => (),
            Err(e) => return Err(e),
        }
    }

    Ok((tot_errors, raw_data))
}


///takes the data+ecc_data and return (errors corrected,data(minus ecc))
pub async fn apply_ecc_for_chunks_async(mut raw_data:Vec<u8>,msg_len:usize,ecc_len:u8) -> Result<(usize,Vec<u8>),DecoderError> {
    let ecc_len = ecc_len as usize;
    let len = raw_data.len();
    let num_chunks = (len-msg_len) / ecc_len;
    assert_eq!((len-msg_len) % ecc_len, 0);
    let chunk_size = 255-ecc_len;
    let tasks: Vec<_> = (0..num_chunks).map(|i| {
        let mut chunk_data = Vec::with_capacity(chunk_size+ecc_len);
        let start = i * chunk_size;
        let end = (i + 1) * chunk_size;
        chunk_data.extend_from_slice(&raw_data[start..end]);
        let start = msg_len+i*ecc_len;
        let end = start+ecc_len;
        chunk_data.extend_from_slice(&raw_data[start..end]);

        task::spawn(async move {
            apply_ecc(chunk_data,ecc_len)
        })
    }).collect();
    let mut tot_errors = 0;
    for (i, task) in tasks.into_iter().enumerate() {
        match task.await.expect("Failed to await task")? {
            Some((errors,fixed_bytes)) => {
                // split out and copy the chunk and ecc back to the raw_data if there is an error
                let (chunk, ecc) = fixed_bytes.split_at(chunk_size);
                let start = i * chunk_size;
                let end = start + chunk_size;
                raw_data[start..end].copy_from_slice(chunk);
                let start = msg_len + i * ecc_len;
                let end = start + ecc_len;
                raw_data[start..end].copy_from_slice(ecc);

                tot_errors += errors;
            },
            None => (),
        }
    }

    Ok((tot_errors,raw_data))
}



// pub async fn apply_ecc(mut ecc_data: Vec<u8>,ecc_len:usize) -> Result<(usize,Vec<u8>),DecoderError> {
//     let bytes: &[u8] = ecc_data.as_ref();
//     let decoder = Decoder::new(ecc_len);
//     if decoder.is_corrupted(&bytes) {
//         let (buffer,errors) = decoder.correct_err_count(&bytes,None)?;
//         let mut reader = buffer.data().chain(buffer.ecc());
//         //should be the same length, overwrite what was given
//         reader.read_to_end(&mut ecc_data).expect("Buffert to short!");
//         Ok((errors,ecc_data))
//     }else{
//         Ok((0,ecc_data))
//     }
// }
// ///takes the data+ecc_data and return (errors corrected,data(minus ecc))
// pub async fn apply_ecc_for_chunks(mut raw_data:Vec<u8>,msg_len:usize,ecc_len:u8,pos:u64) -> Result<(usize,Vec<u8>),DecoderError> {
//     let ecc_len = ecc_len as usize;
//     let len = raw_data.len();
//     let num_chunks = (len-msg_len) / ecc_len;
//     assert_eq!((len-msg_len) % ecc_len, 0);
//     let chunk_size = 255-ecc_len;
//     let tasks: Vec<_> = (0..num_chunks).map(|i| {
//         let mut chunk_data = Vec::with_capacity(chunk_size+ecc_len);
//         let start = i * chunk_size;
//         let end = (i + 1) * chunk_size;
//         chunk_data.extend_from_slice(&raw_data[start..end]);
//         let start = msg_len+i*ecc_len;
//         let end = start+ecc_len;
//         chunk_data.extend_from_slice(&raw_data[start..end]);

//         task::spawn(async move {
//             apply_ecc(chunk_data,ecc_len).await
//         })
//     }).collect();
//     let mut tot_errors = 0;
//     for (i, task) in tasks.into_iter().enumerate() {
//         let (errors, chunk_data) = task.await.expect("Failed to await task")?;
//         if errors == 0 {continue;}

//         // split out and copy the chunk and ecc back to the raw_data if there is an error
//         let (chunk, ecc) = chunk_data.split_at(chunk_size);
//         let start = i * chunk_size;
//         let end = start + chunk_size;
//         raw_data[start..end].copy_from_slice(chunk);
//         let start = msg_len + i * ecc_len;
//         let end = start + ecc_len;
//         raw_data[start..end].copy_from_slice(ecc);

//         tot_errors += errors;
//     }

//     Ok((tot_errors,raw_data))
// }







#[cfg(test)]
mod tests {
    use super::*;

    const ECC_LEN:u8 = 5;
    
    #[tokio::test]
    async fn test_calculate_ecc_for_chunks() {
        let data: Vec<u8> = vec![128;500]; // Two chunks

        let (data,ecc) = calculate_ecc_for_chunks_async(data,0,500, ECC_LEN).await;

        // Check if result is correct. This will depend on the specifics of your ECC
        // algorithm and encoder, so replace this with your own check.
        assert!(data.len() > 0);  // A very basic check.
    }

    #[tokio::test]
    async fn test_apply_ecc_for_chunks() {

        let data: Vec<u8> = vec![128;500]; // Two chunks
        let (data, ecc_data) = calculate_ecc_for_chunks_async(data.into(),0,500,ECC_LEN).await;
        let result = apply_ecc_for_chunks_async(data.clone(), 500, ECC_LEN).await;

        // Check if result is the original data.
        match result {
            Ok((_, result_data)) => assert_eq!(data, result_data),
            Err(_) => panic!("DecoderError"),
        }
    }
}
