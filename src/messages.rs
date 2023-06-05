
use serde::de::{Visitor, SeqAccess};
use serde::{Deserialize, Serialize, Deserializer};
use crate::coder::{DocuFortMsg, MsgTag};

use crate::{util::{DocID}};


#[derive(Debug,PartialEq,Serialize, Deserialize,Default)]
pub struct Create {
    pub doc_id: DocID,
}
impl Create {
    pub fn new(doc_id: DocID) -> Self { Self { doc_id } }
}

#[derive(Debug,PartialEq,Serialize, Deserialize,Default)]
pub struct Update {
    pub doc_id: DocID,
    pub time_stamp: u64,
    pub delta:Option<DocID>,
    #[serde(default, skip_serializing)]
    pub expects:Option<[u8;32]>//atomic update
}



impl Update {
    pub fn new(doc_id: DocID, time_stamp: u64, delta: Option<DocID>,expects:Option<[u8;32]>) -> Self { Self { doc_id, time_stamp, delta, expects } }
}

#[derive(Clone,Debug,PartialEq,Serialize, Default)]
pub struct Data {
    pub doc_id: DocID,
    pub time_stamp: u64,
    pub op_doc_id: Option<DocID>, //The operation document this data is continuing. The ie; 'time_stamp' on the update/archive, None = Create
    pub last:bool, //is this the 'last' piece of data for the operation? Data writes can span over multiple blocks, so we need this.
    #[serde(default)]
    pub data: Vec<u8>,
}

struct DataVisitor;

impl<'de> Visitor<'de> for DataVisitor {
    type Value = Data;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("struct Data")
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<Data, V::Error>
    where
        V: SeqAccess<'de>,
    {
        let doc_id = seq.next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
        let time_stamp = seq.next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
        let op_doc_id = seq.next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
        let last = seq.next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;
        let data = seq.next_element()?
            .unwrap_or_else(Vec::new);

        Ok(Data {
            doc_id,
            time_stamp,
            op_doc_id,
            last,
            data,
        })
    }
}

impl<'de> Deserialize<'de> for Data {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(DataVisitor)
    }
}


impl Data {
    pub fn new(doc_id: DocID, time_stamp: u64, op_doc_id: Option<DocID>,last:bool, data: Vec<u8>) -> Self { Self { doc_id, time_stamp, op_doc_id, last, data } }
}

#[derive(Debug,PartialEq,Serialize, Deserialize,Default)]
pub struct Delete {
    pub doc_id: DocID,
    pub time_stamp: u64,
}

impl Delete {
    pub fn new(doc_id: DocID, time_stamp: u64) -> Self { Self { doc_id, time_stamp } }
}

#[derive(Debug,PartialEq,Serialize, Deserialize,Default)]
pub struct Archive {
    pub doc_id: DocID,
    pub time_stamp: u64,
}

impl Archive {
    pub fn new(doc_id: DocID, time_stamp: u64) -> Self { Self { doc_id, time_stamp } }
}




#[cfg(test)]
mod tests {
    use std::io::{BufWriter, Write};


    use super::*;

    fn format_vec_as_hex(data: &[u8]) -> String {
        let hex_chars: Vec<String> = data.iter().map(|byte| format!("{:02X}", byte)).collect();
        hex_chars.join("")
    }

    const ECC_LEN:u8 = 5;
    #[tokio::test]
    async fn block_start_bytes() {
      
    }
    #[tokio::test]
    async fn block_end_bytes() {
        
    }
}
