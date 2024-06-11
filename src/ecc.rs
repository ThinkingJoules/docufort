//! Error correction code (ECC) functions for encoding and decoding data.
use crate::DATA_SIZE;
use crate::ECC_LEN;
use reed_solomon::{Encoder,Decoder, DecoderError};




pub fn ceiling_division(numerator: usize, denominator: usize) -> usize {
    if numerator % denominator == 0 {
        numerator / denominator
    } else {
        (numerator / denominator) + 1
    }
}
pub fn calc_ecc_data_len(raw_data_len:usize)->usize{
    ceiling_division(raw_data_len, DATA_SIZE)*ECC_LEN
}
pub fn calculate_ecc_chunk<W: std::io::Write>(data: &[u8],writer:&mut W) -> std::io::Result<()> {
    //let bytes: &[u8] = data.as_ref();
    let encoder = Encoder::new(ECC_LEN);
    let ecc_data = encoder.encode(data);
    writer.write_all(ecc_data.ecc())?;
    Ok(())
}
#[cfg(feature = "parallel")]
pub fn calculate_ecc_chunk_par(data: &[u8]) -> [u8;ECC_LEN] {
    //let bytes: &[u8] = data.as_ref();
    let encoder = Encoder::new(ECC_LEN);
    let ecc_data = encoder.encode(data);
    ecc_data.ecc().try_into().unwrap()
}
#[cfg(not(feature = "parallel"))]
pub fn calculate_ecc_for_chunks<W: std::io::Write>(data: &[u8], writer: &mut W) -> std::io::Result<()> {
    data.chunks(DATA_SIZE).try_for_each(|chunk_data| {
        calculate_ecc_chunk(chunk_data, writer)
    })
}

#[cfg(feature = "parallel")]
pub fn calculate_ecc_for_chunks<W: std::io::Write>(data: &[u8], writer: &mut W) -> std::io::Result<()> {
    use rayon::prelude::*;

    let par_results= data
        .par_chunks(DATA_SIZE)
        .map(|chunk_data| calculate_ecc_chunk_par(chunk_data)) // Assuming calculate_ecc_chunk returns [u8; ECC_SIZE]
        .collect_vec_list();

    for thread_result in par_results.into_iter().flatten() {
        writer.write_all(&thread_result)?;
    }

    Ok(())
}


pub fn apply_ecc(ecc_data: &mut[u8]) -> Result<usize,DecoderError> {
    let decoder = Decoder::new(ECC_LEN);
    if decoder.is_corrupted(&ecc_data) {
        let (buffer,errors) = decoder.correct_err_count(&ecc_data,None)?;
        {
            let (data,ecc) = ecc_data.split_at_mut(buffer.data().len());
            data.copy_from_slice(buffer.data());
            ecc.copy_from_slice(buffer.ecc());
        }
        Ok(errors)
    }else{
        Ok(0)
    }
}
///This assumes the ecc_data is before the msg_data, as the case for the 'content'
#[cfg(not(feature = "parallel"))]
pub fn apply_ecc_for_chunks(raw_data: &mut [u8]) -> Result<usize, DecoderError> {
    let len = raw_data.len();
    let msg_len = calculate_msg_len(len);
    let ecc_len = len - msg_len;
    let num_chunks = (len - msg_len) / ECC_LEN;
    assert_eq!((len - msg_len) % ECC_LEN, 0);
    let mut tot_errors = 0;
    let mut chunk_data = [0u8;255];
    for i in 0..num_chunks {
        let data_start = (i * DATA_SIZE) + ecc_len;
        let data_end = (((i + 1) * DATA_SIZE) + ecc_len).min(len);
        let chunk_data_len = data_end-data_start;
        let ecc_start = i * ECC_LEN;
        let chunk_len = chunk_data_len+ECC_LEN;
        chunk_data[..chunk_data_len].copy_from_slice(&raw_data[data_start..data_end]);
        chunk_data[chunk_data_len..chunk_len].copy_from_slice(&raw_data[ecc_start..ecc_start+ECC_LEN]);
        //dbg!(data_start,data_end,chunk_data_len,ecc_start,chunk_len);

        let errors = apply_ecc(&mut chunk_data[..chunk_len])?;
        if errors > 0{
            // split out and copy the chunk and ecc back to the raw_data if there is an error
            let (chunk, ecc) = chunk_data.split_at(chunk_data_len);
            raw_data[data_start..data_end].copy_from_slice(chunk);
            raw_data[ecc_start..ecc_start+ECC_LEN].copy_from_slice(ecc);
        }
        tot_errors += errors;
    }

    Ok(tot_errors)
}



#[cfg(feature = "parallel")]
pub fn apply_ecc_par(ecc_data: &[u8]) -> Result<Result<(),([u8;255],usize,usize)>,DecoderError> {
    let decoder = Decoder::new(ECC_LEN);
    if decoder.is_corrupted(&ecc_data) {
        let (buffer,errors) = decoder.correct_err_count(&ecc_data,None)?;
        if errors > 0 {
            let mut ret = [0u8;255];
            {
                let (data,ecc) = ret.split_at_mut(buffer.data().len());
                data.copy_from_slice(buffer.data());
                ecc.copy_from_slice(buffer.ecc());
            }
            Ok(Err((ret,ecc_data.len(),errors)))
        }else{
            Ok(Ok(()))
        }
    }else{
        Ok(Ok(()))
    }
}
///This assumes the ecc_data is before the msg_data, as the case for the 'content'
#[cfg(feature = "parallel")]
pub fn apply_ecc_for_chunks(raw_data: &mut [u8]) -> Result<usize, DecoderError> {
    use rayon::prelude::*;

    let len = raw_data.len();
    let msg_len = calculate_msg_len(len);
    let ecc_len = len - msg_len;
    let num_chunks = (len - msg_len) / ECC_LEN;
    assert_eq!((len - msg_len) % ECC_LEN, 0);
    let chunk_pairs: Vec<(usize,[u8; 255], usize)> = (0..num_chunks)
        .into_par_iter()
        .map(|i| {
            let mut chunk_data = [0u8; 255];
            let data_start = (i * DATA_SIZE) + ecc_len;
            let data_end = (((i + 1) * DATA_SIZE) + ecc_len).min(len);
            let chunk_data_len = data_end-data_start;
            let ecc_start = i * ECC_LEN;
            let chunk_len = chunk_data_len+ECC_LEN;
            chunk_data[..chunk_data_len].copy_from_slice(&raw_data[data_start..data_end]);
            chunk_data[chunk_data_len..chunk_len].copy_from_slice(&raw_data[ecc_start..ecc_start+ECC_LEN]);
            (i,chunk_data, chunk_len)
        })
        .collect();

    let results = chunk_pairs
        .into_par_iter()
        .map(|(i,chunk, chunk_len)| (i,apply_ecc_par(&chunk[..chunk_len]))) // Apply ECC correction to each chunk
        .collect_vec_list();

    let mut tot_errors = 0;
    for (i,res) in results.into_iter().flatten() {
        let inner_res = res?;
        if let Err((chunk, chunk_len,errors)) = inner_res {
            debug_assert!(errors > 0);
            let data_start = (i * DATA_SIZE) + ecc_len;
            let data_end = (((i + 1) * DATA_SIZE) + ecc_len).min(len);
            let chunk_data_len = data_end-data_start;
            let ecc_start = i * ECC_LEN;
            debug_assert_eq!(chunk_data_len+ECC_LEN,chunk_len,"Chunk: {} Data Start: {} Data End: {} ECC Start: {} Chunk Len: {}",i,data_start,data_end,ecc_start,chunk_len);
            let (chunk, ecc) = &chunk[..chunk_len].split_at(chunk_data_len);
            raw_data[data_start..data_end].copy_from_slice(chunk);
            raw_data[ecc_start..ecc_start+ECC_LEN].copy_from_slice(ecc);
            tot_errors += errors;
        }
    }
    Ok(tot_errors)
}

pub fn calculate_msg_len(total_len: usize) -> usize {
    const C_SIZE:usize = DATA_SIZE + ECC_LEN;
    let num_complete_chunks = total_len / C_SIZE;
    let total_ecc_len = ECC_LEN * (num_complete_chunks + (total_len % C_SIZE > 0) as usize);
    total_len - total_ecc_len
}


#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    #[test]
    fn test_calculate_msg_len() {
        let total_len = DATA_SIZE + ECC_LEN;
        let calculated_msg_len = calculate_msg_len(total_len);
        assert_eq!(calculated_msg_len, DATA_SIZE);

        let total_len = DATA_SIZE + 1 + ECC_LEN*2 ;
        let calculated_msg_len = calculate_msg_len(total_len);
        assert_eq!(calculated_msg_len, DATA_SIZE+1);
    }

    #[test]
    fn test_calculate_ecc_chunk() {
        let data = vec![128;DATA_SIZE];
        let mut writer = Cursor::new(Vec::new());

        calculate_ecc_chunk(&data, &mut writer).unwrap();

        // Verify the writer contains the expected ECC data
        let expected_ecc_data = vec![214, 227, 17, 164,];
        assert_eq!(writer.into_inner(), expected_ecc_data);
    }

    #[test]
    fn test_apply_ecc(){
        let data = vec![128;DATA_SIZE];
        let mut writer = Cursor::new(Vec::new());

        calculate_ecc_chunk(&data, &mut writer).unwrap();

        // Verify the writer contains the expected ECC data
        let expected_ecc_data = vec![214, 227, 17, 164,];
        let ecc_data = writer.into_inner();
        assert_eq!(ecc_data, expected_ecc_data);

        let mut combined = data.clone();
        combined.extend_from_slice(&ecc_data);
        let mut corrupted = combined.clone();
        //corrupt a byte
        corrupted[0] = 255;
        let errors = apply_ecc(&mut corrupted).unwrap();
        assert_eq!(errors,1);

        // Verify that the expected number of errors were corrected
        assert_eq!(combined, corrupted);
    }

    #[test]
    fn test_calculate_ecc_for_chunks() {
        let data: Vec<u8> = vec![128;500]; // Two chunks
        let mut output = Cursor::new(Vec::new());
        assert_eq!(ECC_LEN, 4);
        let ecc = calculate_ecc_for_chunks(data.as_slice(),&mut output);
        assert!(ecc.is_ok());
        assert_eq!(output.into_inner().as_slice(),&[214, 227, 17, 164, 30, 173, 161, 146]);
    }

    #[test]
    fn test_apply_ecc_for_chunks() {
        let val = 128u8;
        let len = 500;
        let data: Vec<u8> = vec![val;len]; // Two chunks
        let mut ecc = Cursor::new(Vec::new());
        assert_eq!(ECC_LEN, 4);
        let res = calculate_ecc_for_chunks(data.as_slice(),&mut ecc);
        assert!(res.is_ok());
        let ecc = ecc.into_inner();
        let mut all_data = ecc.clone();
        all_data[0] = 255;
        all_data.extend_from_slice(data.as_slice());
        assert_eq!(calculate_msg_len(all_data.len()),len);
        let result = apply_ecc_for_chunks(&mut all_data);
        // Check if result is the original data.
        match result {
            Ok(errors) => {
                assert_eq!(errors,1);
                assert!(&all_data[all_data.len()-500..].iter().all(|a|*a==val))
            },
            Err(_) => panic!("DecoderError"),
        }
    }
}