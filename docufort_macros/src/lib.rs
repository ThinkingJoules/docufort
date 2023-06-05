extern crate proc_macro;

use proc_macro::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Attribute;
use syn::Data;
use syn::DeriveInput;
use syn::Fields;
use syn::FieldsNamed;
use syn::Ident;
use syn::Meta;
use syn::Token;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse_macro_input;

use syn::{Result, LitInt};
use syn::token::Comma;
use syn::{punctuated::Punctuated};


struct IdentList(Vec<Ident>);

impl Parse for IdentList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut idents = Vec::new();
        while !input.is_empty() {
            idents.push(input.parse()?);
            let _ = input.parse::<Token![,]>();
        }    
        Ok(IdentList(idents))
    }    
}  
struct ItemStructs(Vec<syn::ItemStruct>);

impl Parse for ItemStructs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut item_structs = Vec::new();

        while !input.is_empty() {
            item_structs.push(input.parse()?);
        }

        Ok(Self(item_structs))
    }
}

struct SystemParams {
    data_comp_flag: u8,
    ecc_flag: u8,
    msg_data_flag: u8,
    msg_and_data_ecc_len: u8,
    min_len_try_comp:usize,
    write_serializer: Ident,
    read_deserializer: Ident,
    compressor: Ident,
    eccer: Ident,
    writer_error: Ident,
    reader_error: Ident,
    structs: IdentList,
}

impl Parse for SystemParams {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        syn::braced!(content in input);

        let mut data_comp_flag = Some(0b01000000);
        let mut ecc_flag = Some(0b00100000);
        let mut msg_data_flag = Some(0b01000000);
        let mut msg_and_data_ecc_len = Some(5);
        let mut min_len_try_comp = Some(35);
        let mut write_serializer = None;
        let mut read_deserializer = None;
        let mut compressor = None;
        let mut eccer = None;
        let mut writer_error = None;
        let mut reader_error = None;


        while !content.is_empty() {
            let name: Ident = content.parse()?;
            content.parse::<Token![:]>()?;
            match name.to_string().as_str() {
                "write_serializer" => write_serializer = Some(content.parse()?),
                "read_deserializer" => read_deserializer = Some(content.parse()?),
                "compressor" => compressor = Some(content.parse()?),
                "eccer" => eccer = Some(content.parse()?),
                "writer_error" => writer_error = Some(content.parse()?),
                "reader_error" => reader_error = Some(content.parse()?),
                "data_comp_flag" => {
                    let flag: LitInt = content.parse()?;
                    data_comp_flag = Some(flag.base10_parse::<u8>()?);
                },
                "ecc_flag" => {
                    let flag: LitInt = content.parse()?;
                    ecc_flag = Some(flag.base10_parse::<u8>()?);
                },
                "msg_data_flag" => {
                    let flag: LitInt = content.parse()?;
                    msg_data_flag = Some(flag.base10_parse::<u8>()?);
                },
                "msg_and_data_ecc_len" => {
                    let len: LitInt = content.parse()?;
                    msg_and_data_ecc_len = Some(len.base10_parse::<u8>()?);
                },
                "min_len_try_comp" => {
                    let len: LitInt = content.parse()?;
                    min_len_try_comp = Some(len.base10_parse::<usize>()?);
                },
                _ => return Err(syn::Error::new(name.span(), "Unknown key")),
            }
            // Skip comma if present, but it's optional on the last field
            let _ = content.parse::<Comma>();
        }
        let structs:IdentList = input.parse()?;


        Ok(SystemParams {
            data_comp_flag: data_comp_flag.ok_or_else(|| input.error("Expected `data_comp_flag` field"))?,
            ecc_flag: ecc_flag.ok_or_else(|| input.error("Expected `ecc_flag` field"))?,
            msg_data_flag: msg_data_flag.ok_or_else(|| input.error("Expected `msg_data_flag` field"))?,
            msg_and_data_ecc_len: msg_and_data_ecc_len.ok_or_else(|| input.error("Expected `msg_and_data_ecc_len` field"))?,
            min_len_try_comp: min_len_try_comp.ok_or_else(|| input.error("Expected `min_len_try_comp` field"))?,
            write_serializer: write_serializer.ok_or_else(|| input.error("Expected `write_serializer` field"))?,
            read_deserializer: read_deserializer.ok_or_else(|| input.error("Expected `read_deserializer` field"))?,
            compressor: compressor.ok_or_else(|| input.error("Expected `compressor` field"))?,
            eccer: eccer.ok_or_else(|| input.error("Expected `eccer` field"))?,
            writer_error: writer_error.ok_or_else(|| input.error("Expected `writer_error` field"))?,
            reader_error: reader_error.ok_or_else(|| input.error("Expected `reader_error` field"))?,
            structs,
        })
    }
}





/// Macro to generate a system with a set of configurations and message structures.
///
/// `make_system!` takes a block of system configurations, followed by one or more message structures,
/// and generates an implementation of the system with these settings.
/// 
/// There are several functions and traits that will be generated. Some helper, some required.
/// generate_stub_structs macro is only for testing compilation.
///
/// # Example
///
/// ```text
/// use docufort::*;
/// generate_stub_structs!();
///
/// make_system!({
///     data_comp_flag:0b01000000,
///     ecc_flag:0b00100000,
///     msg_data_flag:0b01000000,
///     msg_and_data_ecc_len:5,
///     min_len_try_comp: 35,
///     write_serializer: WriterStruct,
///     read_deserializer: ReaderStruct,
///     compressor: CompressorStruct,
///     eccer: EccerStruct,
///     writer_error:AllError,
///     reader_error:AllError
/// }
///     #[derive(Debug,MsgCoder,MsgReadWrite)]
///     #[write_error(AllError)]
///     #[read_error(AllError)]
///     pub struct TestMessage{
///         field1:u8,
///         field2:u32,
///         field3:bool,
///         data:Vec<u8>,
///     }
///
///     #[derive(Debug,MsgCoder,MsgReadWrite)]
///     #[write_error(AllError)]
///     #[read_error(AllError)]
///     pub struct TestMessage1{
///         field1:u8,
///         field2:u32,
///         field3:bool,
///     }
///
/// );
/// ```
///
/// # Parameters
///
/// * `data_comp_flag` - The flag indicating that the data is compressed.
/// * `ecc_flag` - The ECC flag to indicate if there is ECC data following the message (or Data).
/// * `msg_data_flag` - The flag indicating when messages have an extended 'data' field.
/// * `msg_and_data_ecc_len` - The length of the ECC for the message and data. Meaning depends on how you implement it.
/// * `min_len_try_comp` - The minimum length to try to compress, above which it will try compress, writing uncompressed if it is not beneficial.
/// * `write_serializer` - The serializer for writing operations. Must implement WriteSerializer Trait.
/// * `read_deserializer` - The deserializer for reading operations. Must implement ReadDerializer Trait.
/// * `compressor` - The compressor for the system. Must implement Compressor Trait.
/// * `eecer` - The ECC calculator for the system. Must implement Eccer Trait.
/// * `writer_error` - The error type for writer operations. Error: From: io:Error, Compressor::Error, Eccer::Error, WriteSerializer::Error
/// * `reader_error` - The error type for reader operations. Error: ... ,ReadSerializer::Error>
///
/// Each structure declaration represents a type of message that can be used in this system. Each
/// message structure needs to implement the `DocuFortMsg` trait.
///
/// For more information on system capabilities and how to implement, see the README in the main DocuFort repo.
///
#[proc_macro]
pub fn make_system(input: TokenStream) -> TokenStream {
    let SystemParams { 
        data_comp_flag, 
        ecc_flag, 
        msg_data_flag, 
        msg_and_data_ecc_len, 
        min_len_try_comp, 
        write_serializer, 
        read_deserializer, 
        compressor, 
        eccer, 
        writer_error, 
        reader_error, 
        structs 
    } = syn::parse_macro_input!(input as SystemParams);

    
    

    // Convert magic_number to a token stream
    let magic_number_length = 8usize;
    let file_header_len = magic_number_length + 4;
    let block_start_len = magic_number_length + 8 + 2;//u64 ts + 2 for msg_len/tag
 
    let clear_msg_flags = !(ecc_flag | msg_data_flag);

    let trait_tokens = quote!{
        pub trait DocuFortMsgCoding: DocuFortMsg + serde::Serialize + for<'de>serde::Deserialize<'de> {
            fn write_to<W>(self,writer: &mut W,try_compress: Option<CompressionLevel>,calc_ecc:bool)->Result<(),#writer_error>
            where
                W: std::io::Write + std::io::Seek,
            ;
            fn read_from<R>(reader:&mut R,msg_len:u8,flags:u8,error_correct:bool)->Result<(MessageReadSummary, Self),#reader_error>
            where
                R: std::io::Read+std::io::Seek,
            ;
            fn load_data<R:std::io::Read+std::io::Seek>(&mut self, mut reader:R,summary:&MessageReadSummary)->Result<(),#reader_error>{
                let MessageReadSummary { data ,..} = summary;
                assert!(data.is_some());
                let (start,len,flag) = data.unwrap();
                let mut data = vec![0;len as usize];
                reader.seek(std::io::SeekFrom::Start(start))?;
                reader.read_exact(&mut data)?;
                if flag & DATA_COMP_FLAG == DATA_COMP_FLAG {
                    let mut v = Vec::with_capacity((len+(len/4)) as usize);
                    #compressor::decompress_into(&mut v, &data)?;
                    data = v;
                }
                self.set_data(data);
                Ok(())
            }
        }

    };

    let reader_tokens = quote!{
        ///Reads Message, but not it's data from given reader.
        /// Reader = | msg |?msg_ecc | data_len(u32_le) | sys_data_tag(1) | data_bytes |? data_ecc_data |
        pub fn read_msg<R,T>(reader: &mut R,msg_len:u8,flags:u8,error_correct:bool)->Result<(MessageReadSummary,T),#reader_error>
        where
            R: std::io::Read+std::io::Seek,
            T: DocuFortMsg + for<'de>serde::Deserialize<'de>,
        {
            let mut msg_len = msg_len as usize;
            let mut msg_and_meta_len = msg_len + 2;
            let message_start = reader.seek(std::io::SeekFrom::Current(0))? - 2;

            let has_msg_ecc = flags & ECC_FLAG == ECC_FLAG;
            let has_msg_data = flags & MSG_DATA_FLAG == MSG_DATA_FLAG;
            
            let msg_tag = flags & CLEAR_MSG_FLAGS;
            assert!(msg_tag == T::MSG_TAG);

            let mut ecc_len = if has_msg_ecc {#eccer::calc_ecc_data_len(msg_and_meta_len)}else{0};
            let data_info_len = if has_msg_data {DATA_META_LEN as usize}else{0};
            let mut msg_buf = vec![0u8;msg_and_meta_len +ecc_len+data_info_len];
            msg_buf[0] = msg_len as u8;
            msg_buf[1] = flags as u8;
            reader.read_exact(&mut msg_buf[2..])?;

            let mut errors_corrected = if error_correct && has_msg_ecc {
                let errors = #eccer::apply_ecc(&mut msg_buf[..msg_and_meta_len+ecc_len])?;
                errors
            }else{0};
            
            let message: T = #read_deserializer::read_from(&msg_buf[2..msg_len])?;

            if has_msg_data {
                let data_start = msg_buf.len() as u64 +message_start;
                let sys_data_flag = *msg_buf.last().unwrap();
                let slice = &msg_buf[msg_buf.len()-5..msg_buf.len()-1];
                let data_len = u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]);
                let errors = if errors_corrected > 0 {Some((errors_corrected,msg_buf))}else{None};
                return Ok((MessageReadSummary{message_start,errors,data:Some((data_start,data_len,sys_data_flag))},message))
            }else{
                let errors = if errors_corrected > 0 {Some((errors_corrected,msg_buf))}else{None};
                return Ok((MessageReadSummary{message_start,errors,data:None},message))
            }
        }
    };

    let writer_tokens = quote!{
        ///Writes message and any data to given writer
        /// Writes = msg_len | msg_tag | msg |?msg_ecc | ?data_len(u32_le) | ?sys_data_tag(1) | ?data_bytes |? data_ecc_data |
        pub fn write_doc<W,T>(writer: &mut W,message: T,try_compress: Option<CompressionLevel>,calc_ecc:bool)->Result<(),#writer_error>
        where
            W: std::io::Write + std::io::Seek,
            T: DocuFortMsg + serde::Serialize,
        {
            let mut msg_tag = T::MSG_TAG;
            
            let msg_size = #write_serializer::serialized_size(&message)?;
            assert!(msg_size < u8::MAX as usize);
            let msg_and_meta_size = msg_size+ 2;//+1 for msg_len byte +1 for msg_tag

            // See note where msg_ecc is applied
            // let mut msg_ecc_len = calc_ecc.and_then(|ecc_len|Some(calc_ecc_data_len(msg_size, ecc_len)));
            let msg_ecc_len = if calc_ecc {Some(#eccer::calc_ecc_data_len(msg_and_meta_size))}else{None};

            let has_data = message.has_data();
            if has_data.is_some() {
                msg_tag |= MSG_DATA_FLAG;
            }
            
            let data = if let Some(ecc_data_len) = msg_ecc_len {
                let mut msg_bytes = vec![0u8;msg_and_meta_size + ecc_data_len];
                //we include our metadata in the ecc
                msg_bytes[0] = msg_size as u8;
                msg_bytes[1] = msg_tag as u8;
                #write_serializer::serialize_into(&mut msg_bytes, &message)?;
                {
                    let (msg,mut ecc) = msg_bytes.split_at_mut(msg_size);
                    #eccer::calc_ecc_into(&mut ecc, msg)?;
                }
                writer.write_all(&msg_bytes)?;
                message.take_data()
            }else{
                //msg_meta
                writer.write_all(&[msg_size as u8,msg_tag as u8])?;
                #write_serializer::serialize_into(writer, &message)?;
                message.take_data()
            };

            if data.is_none() {
                assert!(has_data.is_none(),"Error in DocuFortMsg trait Impl");
                return Ok(())
            }
            let data = data.unwrap();
            let mut sys_data_tag = if calc_ecc {ECC_FLAG}else{0};
            
            let mut data_len = data.len();
            let data_ecc_len = if calc_ecc {Some(#eccer::calc_ecc_data_len(data_len))}else{None};
            assert!(data_len == has_data.unwrap());
            //write the len as u32, this might change but we will advance the writer
            writer.write_all((data_len as u32).to_le_bytes().as_slice())?;
            writer.write_all(&[sys_data_tag])?;//temp write the tag
            let start_pos = writer.seek(std::io::SeekFrom::Current(0))?;
            let mut end_pos = start_pos  + data_len as u64;



            //try compresssion, and THEN apply ECC
            if try_compress.is_some() && data_len >= MIN_LEN_TRY_COMP{
                //if we are here, we are mostly certain that the compressed data will be smaller than the original
                //if this is true, then it might not have to re-allocate our Vec, so we should just write directly to the writer
                #compressor::compress_into(writer, &data, try_compress)?;
                let cur_pos = writer.seek(std::io::SeekFrom::Current(0))?;

                if cur_pos != end_pos {
                    assert!(cur_pos<end_pos, "Call to compress_into should result in the same length or less data written!");
                    data_len = (cur_pos - start_pos) as usize;
                    writer.seek(std::io::SeekFrom::Start(start_pos-DATA_META_LEN as u64))?;
                    writer.write_all((data_len as u32).to_le_bytes().as_slice())?;
                    //mark the sys_data_tag
                    sys_data_tag |= DATA_COMP_FLAG;
                    writer.write_all(&[sys_data_tag])?;//update tag, the ecc flag should already be set
                    writer.seek(std::io::SeekFrom::Start(cur_pos))?;//skip back to end of data

                }//else our tag and len are correct
            }
            if let Some(data_ecc_len) = data_ecc_len {
                let mut ecc_bytes = vec![0u8;data_ecc_len];
                #eccer::calc_ecc_into(&mut ecc_bytes, &data)?;
                writer.write_all(&data)?;
                writer.write_all(&ecc_bytes)?;
            }

            Ok(())
        }
    };

    let mut struct_names_vec: Vec<Ident> = structs.0;//do we add blockstart/end here?
    struct_names_vec.push(format_ident!("DfBlockStart"));
    struct_names_vec.push(format_ident!("DfBlockEnd"));

    // let generated = structs.0.into_iter().map(|s| {
    //     let struct_name = s.ident.clone();
    //     //dbg!(&struct_name);
    //     struct_names_vec.push(struct_name);
        
    //     let has_data = has_data_field(&s.fields);
    //     // Inspect derive attributes
    //     //let attrs = s.attrs.clone();
    //     let serialize = check_derive_attrs(&s.attrs,"Serialize");
    //     let deserialize = check_derive_attrs(&s.attrs,"Deserialize");
    //     let msg_coder = check_derive_attrs(&s.attrs,"MsgCoder");
    //     let user_impl = check_derive_attrs(&s.attrs,"ManualMsgCoder");
    //     //dbg!(&serialize,&deserialize,&msg_coder,&user_impl);
    //     if user_impl {
    //         //attrs = remove_derive(attrs, format_ident!("ManualMsgCoder"));
    //         if has_data.is_ok() && *has_data.as_ref().unwrap() {
    //             println!("Warning: 'data' field is present, be sure to add the '#[serde(skip_serializing)]' above the 'data' field.");
    //         }
    //     }else if !msg_coder && serialize && deserialize && has_data.is_ok() && *has_data.as_ref().unwrap(){
    //         println!("Warning: Serialize and Deserialize are already derived for a struct with 'data' field. Consider if MsgCoder is appropriate. Otherwise be sure to add the '#[serde(skip_serializing)]' above the 'data' field.");

    //     }else if (serialize && !deserialize) || (!serialize && deserialize) && !user_impl{
    //        //panic!("Must derive Serialize && Deserialize together. If manually implemented add the derive tag 'ManualMsgCoder' to indicate that");
    //     }else if !msg_coder && !(serialize && deserialize){
    //         //attrs = add_derive(attrs,format_ident!("MsgCoder"));
    //     }
    //     //s.attrs = attrs;
        
    //     quote! {
    //         #s 
    //     }
    // }).collect::<Vec<_>>();
    let test_function = quote! {
        #[test]
        fn df_check_msg_tag_values() {
            let mut tag_values: std::collections::HashSet<u8> = std::collections::HashSet::new();
            #(
                {
                    let tag_value = <#struct_names_vec as DocuFortMsg>::MSG_TAG;
                    assert!(tag_value & ECC_FLAG == 0,"MSG_TAG value found for {} has the ECC_FLAG bit high!", stringify!(#struct_names_vec));
                    assert!(tag_value & MSG_DATA_FLAG == 0,"MSG_TAG value found for {} has the MSG_DATA_FLAG bit high!", stringify!(#struct_names_vec));
                    assert!(!tag_values.contains(&tag_value), "Duplicate MSG_TAG value found for {}", stringify!(#struct_names_vec));
                    tag_values.insert(tag_value);
                }
            )*
        }
    };

    let sys_impls = quote!{
        impl DocuFortMsgCoding for DfBlockStart{
            fn write_to<W: std::io::Write + std::io::Seek>(self,writer: &mut W,try_compress: Option<CompressionLevel>,calc_ecc:bool)->Result<(),#writer_error>{
                let mut tag = Self::MSG_TAG;
                tag |= ECC_FLAG;
                let ecc_len = #eccer::calc_ecc_data_len(#block_start_len);
                let mut ecc_buf = vec![0;ecc_len];
                let mut msg_bytes = [0u8;#block_start_len];
                msg_bytes[0] = #block_start_len as u8 - 2;
                msg_bytes[1] = tag;
                use std::io::Write;
                (&mut msg_bytes[2..#magic_number_length + 2]).write_all(&self.magic_number[..]).unwrap();
                (&mut msg_bytes[#magic_number_length + 2..]).write_all(&self.time_stamp.to_le_bytes()[..]).unwrap();
                #eccer::calc_ecc_into(&mut ecc_buf, &msg_bytes)?;
                writer.write_all(&msg_bytes)?;
                writer.write_all(&ecc_buf)?;
                Ok(())
            }
            fn read_from<R: std::io::Read + std::io::Seek>(reader:&mut R,msg_len:u8,flags:u8,error_correct:bool)->Result<(MessageReadSummary, Self),#reader_error>{
                let message_start = reader.seek(std::io::SeekFrom::Current(0))? - 2;
                
                let ecc_len = #eccer::calc_ecc_data_len(#block_start_len);
                let mut msg_bytes_and_ecc_bytes = vec![0; #block_start_len + ecc_len];
                msg_bytes_and_ecc_bytes[0] = msg_len;
                msg_bytes_and_ecc_bytes[1] = flags;
                reader.read_exact(&mut msg_bytes_and_ecc_bytes[2..])?;

                let errors = #eccer::apply_ecc(&mut msg_bytes_and_ecc_bytes[..])?;

                let mut magic_number = [0u8; #magic_number_length];
                magic_number.copy_from_slice(&msg_bytes_and_ecc_bytes[2..#magic_number_length + 2]);

                let time_stamp_bytes = &msg_bytes_and_ecc_bytes[#magic_number_length + 2..msg_bytes_and_ecc_bytes.len()-ecc_len];
                let time_stamp = u64::from_le_bytes(time_stamp_bytes.try_into().unwrap());

                let message = Self {
                    magic_number,
                    time_stamp,
                };

                let errors = if errors > 0 {Some((errors,msg_bytes_and_ecc_bytes))}else{None};
                return Ok((MessageReadSummary{message_start,errors,data:None},message))
            }
        }
        impl DocuFortMsgCoding for DfBlockEnd{
            fn write_to<W: std::io::Write + std::io::Seek>(self,writer: &mut W,try_compress: Option<CompressionLevel>,calc_ecc:bool)->Result<(),#writer_error>{
                let mut tag = Self::MSG_TAG;
                tag |= ECC_FLAG;
                let ecc_len = #eccer::calc_ecc_data_len(28+2);
                let mut ecc_buf = vec![0;ecc_len];
                let mut msg_bytes = [0u8;30];
                msg_bytes[0] = 28;
                msg_bytes[1] = tag;
                use std::io::Write;
                (&mut msg_bytes[2..10]).write_all(&self.time_stamp.to_le_bytes()[..]).unwrap();;
                (&mut msg_bytes[10..]).write_all(&self.hash[..]).unwrap();
                #eccer::calc_ecc_into(&mut ecc_buf, &msg_bytes)?;
                writer.write_all(&msg_bytes)?;
                writer.write_all(&ecc_buf)?;
                Ok(())
            }
            fn read_from<R: std::io::Read + std::io::Seek>(reader:&mut R,msg_len:u8,flags:u8,error_correct:bool)->Result<(MessageReadSummary, Self),#reader_error>{
                let message_start = reader.seek(std::io::SeekFrom::Current(0))? - 2;
                
                let ecc_len = #eccer::calc_ecc_data_len(30);
                let mut msg_bytes_and_ecc_bytes = vec![0; 30 + ecc_len];
                msg_bytes_and_ecc_bytes[0] = msg_len;
                msg_bytes_and_ecc_bytes[1] = flags;
                reader.read_exact(&mut msg_bytes_and_ecc_bytes[2..])?;

                let errors = #eccer::apply_ecc(&mut msg_bytes_and_ecc_bytes[..])?;

                let mut hash = [0u8; 20];
                hash.copy_from_slice(&msg_bytes_and_ecc_bytes[10..30]);

                let time_stamp_bytes = &msg_bytes_and_ecc_bytes[2..10];
                let time_stamp = u64::from_le_bytes(time_stamp_bytes.try_into().unwrap());

                let message = Self {
                    hash,
                    time_stamp,
                };

                let errors = if errors > 0 {Some((errors,msg_bytes_and_ecc_bytes))}else{None};
                return Ok((MessageReadSummary{message_start,errors,data:None},message))
            }
        }
    };
    let enum_name = format_ident!("DfMessage");
    let enum_tokens = quote! {
        #[derive(Debug)]
        pub enum #enum_name {
            #(
                #struct_names_vec(#struct_names_vec),
            )*
        }
    };
    let function_name = format_ident!("df_{}_decoder", enum_name.to_string().to_lowercase());

    let decoder_tokens = quote!{
        pub fn #function_name<R:std::io::Read+std::io::Seek>(reader:&mut R,error_correct:bool)->Result<(MessageReadSummary, #enum_name),#reader_error> {
            let mut len_tag = [0;2];
            reader.read_exact(&mut len_tag)?;
            let flags = len_tag[1];
            let tag = flags & CLEAR_MSG_FLAGS;
            match tag {
                #(
                    x if x == <#struct_names_vec>::MSG_TAG =>{
                        let (mrs,msg) = <#struct_names_vec>::read_from(reader,len_tag[0],flags,error_correct)?;
                        Ok((mrs,#enum_name::#struct_names_vec(msg)))
                    },
                )*
                _ => panic!("Unknown Message Tag!")
            }

        }

    };

    // Build the output token stream
    let output = quote! {
        ///This only exists on the sys_data_tag
        pub const DATA_COMP_FLAG: u8 = #data_comp_flag;
        ///This is used in both the MSG_TAG and the sys_data_tag
        pub const ECC_FLAG: u8 = #ecc_flag;
        ///This is only used in the MSG_TAG
        pub const MSG_DATA_FLAG: u8 = #msg_data_flag;
        pub const CLEAR_MSG_FLAGS: u8 = #clear_msg_flags;
        pub const ECC_LEN: u8 = #msg_and_data_ecc_len;
        ///Depends on how structured the data is in the messages.
        ///Pure Random breaks even around 45 (using best, zlib)
        ///u64 micro_unix_ts only need 20 bytes to break even (using best, zlib)
        const MIN_LEN_TRY_COMP:usize = #min_len_try_comp;
        
        

        /// Initializes a new DocuFort file at the specified path.
        ///
        /// This function creates a new file and writes the initialization header data, which includes
        /// the magic number and flags for error checking and correction (ECC), message data, and data compression.
        ///
        /// The DocuFort file is identified by a specific magic number, and the flags are used to customize
        /// the behavior of the file's operations. 
        /// These are read on startup to ensure they match the software configuration. As they cannot change for the life of the file.
        ///
        /// # Arguments
        ///
        /// - `path`: The path where the new DocuFort file will be created.
        ///
        /// # Errors
        ///
        /// Returns an `std::io::Error` if the file already exists at the given path, or if there's an error
        /// in the process of creating the file or writing to it.
        ///
        /// # Example
        ///
        /// ```no_run
        /// use std::path::Path;
        ///
        /// let path = Path::new("/path/to/myfile.docufort");
        /// df_init(path).unwrap();
        /// ```
        pub fn df_init(path: &std::path::Path) -> std::io::Result<()> {
            // check if file exists
            if path.exists() {
                return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "File already exists."));
            }
            let mut file = std::fs::File::create(path)?;
            let mut buf = MAGIC_NUMBER.to_vec();
            buf.extend_from_slice(&[ECC_LEN, ECC_FLAG, MSG_DATA_FLAG, DATA_COMP_FLAG]);
            use std::io::Write;
            file.write_all(&buf)?;        
            Ok(())
        }
        /// Verifies a DocuFort file at the specified path by comparing its header data with the compiled system constants.
        ///
        /// This function reads the header of the file, which includes the magic number and flags for error checking and correction (ECC),
        /// message data, and data compression, and compares these with the compiled constants in the system.
        ///
        /// # Arguments
        ///
        /// - `path`: The path to the DocuFort file to be verified.
        ///
        /// # Returns
        ///
        /// Returns the number of bytes in the file header if the file's magic number and flags match the system's constants.
        ///
        /// # Errors
        ///
        /// Returns an `std::io::Error` if:
        ///
        /// - The file does not exist at the given path (`std::io::ErrorKind::NotFound`).
        /// - The magic number in the file does not match the system's magic number (`std::io::ErrorKind::InvalidData`).
        /// - Any of the flags in the file do not match the system's corresponding flags (`std::io::ErrorKind::InvalidData`).
        ///
        /// # Example
        ///
        /// ```no_run
        /// use std::path::Path;
        ///
        /// let path = Path::new("/path/to/myfile.docufort");
        /// let header_length = df_verify(path).unwrap();
        /// println!("The header length is {}", header_length);
        /// ```
        pub fn df_verify(path: &std::path::Path) -> std::io::Result<usize> {
            // check if file exists
            if !path.exists() {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "File not found."));
            }
        
            // read file
            let mut file = std::fs::File::open(path)?;
            // Create a buffer large enough for all data
            let mut buffer = [0; #file_header_len];
            use std::io::Read;
            file.read_exact(&mut buffer)?;
        
            // Split the buffer into the magic number and the constants
            let (magic_number, constants) = buffer.split_at(#magic_number_length);
        
            // Convert the magic number slice to an array
            let magic_number_arr: [u8; #magic_number_length] = magic_number.try_into().expect("Wrong size for magic number");
        
            if magic_number_arr != MAGIC_NUMBER {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "MAGIC_NUMBER does not match."));
            }
        
            if constants[0] != ECC_LEN {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "ECC_LEN does not match."));
            }
        
            if constants[1] != ECC_FLAG {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "ECC_FLAG does not match."));
            }
        
            if constants[2] != MSG_DATA_FLAG {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "MSG_DATA_FLAG does not match."));
            }
        
            if constants[3] != DATA_COMP_FLAG {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "DATA_COMP_FLAG does not match."));
            }
        
            Ok(#file_header_len)
        }


        /// Verifies a DocuFort DfBlockStart in a given slice and attempts to correct any detected errors.
        ///
        /// This function checks a provided data slice for a valid BlockStart by applying Error Checking and Correction (ECC).
        /// ECC is performed on the message and its metadata to ensure data integrity and to attempt recovery from any errors.
        /// This also acts as a sort of checksum to ensure the magic number was not just random noise.
        ///
        /// # Arguments
        ///
        /// - `slice`: The data slice containing the BlockStart to be verified.
        ///
        /// # Returns
        ///
        /// Returns `Some(true)` if the DfBlockStart is valid and contains no errors.
        ///
        /// Returns `Some(false)` if the DfBlockStart had errors that were corrected by ECC.
        ///
        /// Returns `None` if ECC reported too many errors to correct, which could be due to extreme data corruption, but more likely the
        /// MAGIC_NUMBER match was accidental noise.
        ///
        /// # Example
        ///
        /// ```no_run
        /// let slice = &my_data[..];
        /// let block_start_status = df_verify_valid_block_start(slice);
        /// match block_start_status {
        ///     Some(true) => println!("The DfBlockStart is valid and error-free."),
        ///     Some(false) => println!("The DfBlockStart had errors but they were corrected."),
        ///     None => println!("The DfBlockStart had too many errors to correct."),
        /// }
        /// ```
        pub fn df_verify_valid_block_start(slice: &[u8]) -> Option<bool> {
            let mut msg_len = 16;
            let mut msg_and_meta_len = msg_len + 2;

            //since we can't know if something might be corrupted, we simply see if the ECC works
            
            let msg_len_byte = slice[0];
            let flags = slice[1];
            
            let mut ecc_len = #eccer::calc_ecc_data_len(msg_and_meta_len);
            let mut msg_buf = slice[..msg_and_meta_len+ecc_len].to_vec();
            let res = #eccer::apply_ecc(&mut msg_buf[..]);
            if res.is_err() {return None} //assume we are not THAT corrupted, if so keep going..
            let errors = res.unwrap();
            if errors>0 {
                println!("Found a valid BlockStart, but has {} errors",errors);
                return Some(false)
            }else{
                return Some(true)
            }

        }
        /// Attempts to find a DocuFort DfBlockStart in a memory-mapped file.
        ///
        /// This function scans the memory-mapped file in reverse order, searching for the MAGIC_NUMBER that marks a DfBlockStart.
        /// Upon finding a potential BlockStart, it verifies the BlockStart using `df_verify_valid_block_start()`.
        ///
        /// # Arguments
        ///
        /// - `mmap_file`: A memory-mapped file to search for a DfBlockStart.
        ///
        /// # Returns
        ///
        /// - `Ok(u64)`: A DfBlockStart was found at the specified offset in the file, and no errors were detected during verification.
        ///
        /// - `Err(Some(u64))`: A potential DfBlockStart was found at the specified offset, but errors were detected during verification.
        ///
        /// - `Err(None)`: No valid DfBlockStart was found in the file.
        ///
        /// Both `Err(Some(u64))` and `Err(None)` should probably trigger a full file integrity check, starting from the beginning of the file.
        ///
        /// # Example
        ///
        /// ```no_run
        /// let file = std::fs::File::open("my_file.dfort").expect("failed to open file");
        /// let mmap_file = unsafe { memmap2::Mmap::map(&file).expect("failed to map file") };
        ///
        /// match df_find_block_start(&mmap_file) {
        ///     Ok(offset) => println!("Found a valid DfBlockStart at offset {}", offset),
        ///     Err(Some(offset)) => println!("Found a DfBlockStart with errors at offset {}", offset),
        ///     Err(None) => println!("No valid DfBlockStart found in the file"),
        /// }
        /// ```
        pub fn df_find_block_start(mmap_file: &memmap2::Mmap) -> Result<u64,Option<u64>,> {
            // Determine the size of the magic number in bytes
            let magic_number_size = MAGIC_NUMBER.len();

            // Ensure the file is large enough to contain the magic number
            if mmap_file.len() < magic_number_size {
                return Err(None);
            }

            // Iterate over the file in reverse, one byte at a time
            for end_index in (magic_number_size..=mmap_file.len()).rev() {
                let start_index = end_index - magic_number_size;
                let slice = &mmap_file[start_index..end_index];

                if slice == MAGIC_NUMBER && end_index >= magic_number_size + 2{
                    // If the magic number is found and there are at least 2 bytes before it
                    match df_verify_valid_block_start(&mmap_file[start_index - 2..]){
                        Some(true) => return Ok((start_index-2) as u64),
                        Some(false) => return Err(Some((start_index-2) as u64)),
                        None => continue,
                    }
                }
            }
            Err(None)
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
        /// Verifies a block in a memory-mapped DocuFort file, starting from the specified offset.
        ///
        /// This function decodes messages from the block, tracking any errors encountered and their locations. The verification ends when a `DfBlockEnd` message is decoded, or when a decoding error occurs.
        ///
        /// # Arguments
        ///
        /// - `mmap_file`: A memory-mapped DocuFort file containing the block to verify.
        ///
        /// - `block_start_offset`: The offset in the file at which the block starts.
        ///
        /// # Returns
        ///
        /// A `DfBlockVerificationSummary` summarizing the results of the block verification.
        ///
        /// # Example
        ///
        /// ```no_run
        /// let file = std::fs::File::open("my_file.dfort").expect("failed to open file");
        /// let mmap_file = unsafe { memmap2::Mmap::map(&file).expect("failed to map file") };
        /// let block_start_offset = /* offset of the block start */;
        ///
        /// match df_check_block(&mmap_file, block_start_offset) {
        ///     DfBlockVerificationSummary::MaybeSuccess { errors, hash_start_index, hash_end_index, end_struct } =>
        ///         /* handle possible success */,
        ///     DfBlockVerificationSummary::OpenABlock { errors } =>
        ///         /* handle open Atomic block */,
        ///     DfBlockVerificationSummary::OpenBBlock { truncate_at_then_close_block, errors } =>
        ///         /* handle open Basic block */,
        ///     DfBlockVerificationSummary::BlockStartFailedDecoding =>
        ///         /* handle BlockStart decoding failure */,
        /// }
        /// ```
        pub fn df_check_block(mmap_file: &memmap2::Mmap,block_start_offset:u64)->DfBlockVerificationSummary{
            let mut tot_errors = 0;
            let mut patches: Vec<(u64, Vec<u8>)> = Vec::new();

            let mut reader = std::io::Cursor::new(&mmap_file[block_start_offset as usize..]);
            let bs = if let Ok((mrs,DfMessage::DfBlockStart(bs))) = df_dfmessage_decoder(&mut reader,true) {
                let MessageReadSummary {errors, message_start, .. } = mrs;
                if let Some((errs,patch)) = errors {
                    tot_errors += errs;
                    patches.push((message_start,patch));
                }
                bs
            }else{
                return DfBlockVerificationSummary::BlockStartFailedDecoding
            };
            //we have the block start message
            let is_atomic = bs.is_atomic();
            let mut be = None;
            let mut last_valid_message = reader.position();
            use std::io::Seek;
            loop{
                match df_dfmessage_decoder(&mut reader,true){
                    Ok((MessageReadSummary { errors, message_start, .. },DfMessage::DfBlockEnd(b))) => {
                        if let Some((errs,patch)) = errors {
                            tot_errors += errs;
                            patches.push((message_start,patch));
                        }
                        last_valid_message = reader.position();
                        be.replace((b,message_start));
                        break
                    },
                    Ok((MessageReadSummary { errors, message_start, data },msg)) => {
                        if let Some((errs,patch)) = errors {
                            tot_errors += errs;
                            patches.push((message_start,patch));
                        }
                        if let Some((_,len,flag)) = data {
                            //advance reader if flag has ECC_FLAG
                            let ecc_len = if flag & ECC_FLAG == ECC_FLAG{#eccer::calc_ecc_data_len(len as usize)as u32}else{0};
                            reader.seek(std::io::SeekFrom::Current((len+ecc_len) as i64)).unwrap();
                        }
                        last_valid_message = reader.position();

                    },
                    Err(_) => break,
                }
            };
            let end_of_block_pos = last_valid_message+block_start_offset;
            let errors = if tot_errors > 0 {Some((tot_errors,patches))}else{None};

            if be.is_none() && is_atomic {return DfBlockVerificationSummary::OpenABlock { errors }}
            else if be.is_none() {return DfBlockVerificationSummary::OpenBBlock { truncate_at_then_close_block:end_of_block_pos, errors }}
            else{
                let (be,be_msg_start) = be.unwrap();
                let hash_end_index = be_msg_start + 8 + 20; //u64 ts + 160bit hash
                return DfBlockVerificationSummary::MaybeSuccess { errors, hash_start_index: block_start_offset, hash_end_index, end_struct: be }
            }
        }
        
        #writer_tokens
        
        #reader_tokens

        #trait_tokens

        #sys_impls

        #enum_tokens

        #decoder_tokens

        #test_function
        
    };

     // Convert the output token stream into a `proc_macro::TokenStream`
     output.into()

}

// fn check_derive_attrs(attrs: &[Attribute],check:&str) -> bool {
//     for attr in attrs {
//         if attr.path().is_ident("derive") {
//             let nested = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated).unwrap();
//             for meta in nested {
//                 match meta {
//                     Meta::Path(path) if path.is_ident(check) => {
//                         return true
//                     }
//                     _ => {
//                         continue
//                     }
//                 }
//             }
//         }
//     }
//     false
// }
// fn has_data_field(fields: &Fields) -> std::result::Result<bool, ()> {
//     if let Fields::Named(named_fields) = fields {
//         let mut has_data = false;

//         for (i, field) in named_fields.named.iter().enumerate() {
//             let ident = field.ident.as_ref().unwrap();
            
//             if ident == "data" {
//                 has_data = true;
//             }

//             if has_data && i < named_fields.named.len() - 1 {
//                 return Err(());
//             }
//         }

//         return Ok(has_data);
//     }

//     Ok(false)
// }
/// `MsgReadWrite` is a custom derive macro that generates a default implementation for the `DocuFortMsgCoding` trait methods.
///
/// This macro enables struct-level control of error types associated with the methods `write_to` and `read_from`.
/// The read_from fn is what is called in the system-generated df_dfmessage_decoder function.
/// The write_to *should* be used by the docufort system designer to write to disk. NOT the helper write_doc fn.
/// The system messages do not use the default 'write_doc' fn, as the encoding is fixed for those messages.
/// 
/// # When not to use
/// You may not want to use this macro if you want to set a default for the compression level for a specific message, or if you want to force it to calc_ecc all the time.
///
/// # How to use
/// To use this macro, annotate the struct with `#[derive(MsgReadWrite)]` and add the `#[write_error]` and `#[read_error]`
/// attributes specifying the respective error types to use for the `write_to` and `read_from` methods.
///
/// # Example
/// ```text
/// #[derive(Debug, MsgReadWrite)]
/// #[write_error(WriteError)]
/// #[read_error(ReadError)]
/// pub struct TestStruct {
///     field1: u8,
///     field2: u32,
///     field3: bool,
/// }
/// ```
///
/// In this example, `TestStruct` implements `DocuFortMsgCoding` trait with the `write_to` method using `WriteError` type 
/// and `read_from` method using `ReadError` type for handling errors.
/// 
/// # Under the hood
/// This macro simply calls the default 'write_doc' or 'read_msg' helper functions that are generated from the make_system macro.
/// 
/// If you want to make a default value fixed for a particular message, it is suggested to still use the write_doc/read_msg functions:
/// ```text
/// impl DocuFortMsgCoding for #struct_name {
///     fn write_to<W>(self, writer: &mut W, _try_compress: Option<CompressionLevel>, _calc_ecc: bool) -> Result<(), #write_error>
///         where
///         W: std::io::Write + std::io::Seek,
///     {
///        write_doc::<W, Self>(writer, self, Some(CompressionLevel::Best), true)
///     }

///     fn read_from<R>(reader: &mut R, msg_len: u8, flags: u8, error_correct: bool) -> Result<(MessageReadSummary, Self), #read_error>
///         where
///         R: std::io::Read + std::io::Seek,
///     {
///         read_msg::<R, Self>(reader, msg_len, flags, error_correct)
///     }
/// }
/// 
/// ```
///
/// # Attributes
/// * `write_error`: Specifies the error type for the `write_to` method.
/// * `read_error`: Specifies the error type for the `read_from` method.
///
/// # Note
/// This macro expects the specified error types to be in scope. If they are defined elsewhere, ensure to import them.
///
/// # Limitations
/// The error types provided via `write_error` and `read_error` attributes must implement `std::error::Error`.
#[proc_macro_derive(MsgReadWrite, attributes(write_error, read_error))]
pub fn docu_fort_msg_coding(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);

    // Get the name of the struct implementing the trait
    let struct_name = &input.ident;

    // Get the error types from the attribute arguments
    let write_error = get_error_type(&input.attrs, "write_error");
    let read_error = get_error_type(&input.attrs, "read_error");

    // Generate the implementation code for the trait methods
    let output = quote! {
        impl DocuFortMsgCoding for #struct_name {
            fn write_to<W>(self, writer: &mut W, try_compress: Option<CompressionLevel>, calc_ecc: bool) -> Result<(), #write_error>
            where
                W: std::io::Write + std::io::Seek,
            {
                write_doc::<W, Self>(writer, self, try_compress, calc_ecc)
            }

            fn read_from<R>(reader: &mut R, msg_len: u8, flags: u8, error_correct: bool) -> Result<(MessageReadSummary, Self), #read_error>
            where
                R: std::io::Read + std::io::Seek,
            {
                read_msg::<R, Self>(reader, msg_len, flags, error_correct)
            }
        }
    };

    // Return the generated implementation as a TokenStream
    output.into()
}
fn get_error_type(attrs: &[Attribute], attr_name: &str) -> Ident {
    for attr in attrs {
        if attr.path().is_ident(attr_name) {
            let nested = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated).unwrap();
            for meta in nested {
                match meta {
                    Meta::Path(path) => {
                        if let Some(ident) = path.get_ident() {
                            return ident.clone();
                        }
                    }
                    _ => {
                        continue
                    }
                }
            }
        }
    }
    syn::Ident::new("AllError", proc_macro2::Span::call_site())
}
#[proc_macro]
///FOR TESTING ONLY
///Used to create structs with valid trait bounds to allow compilation, and only compilation.
///Use the struct `AllError` for all your Error structs
pub fn generate_stub_structs(_: TokenStream) -> TokenStream {
    let tokens = quote! {
        #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
        pub struct AllError;

        impl std::convert::From<std::io::Error> for AllError {
            fn from(_value: std::io::Error) -> Self {
                todo!()
            }
        }
        
        impl std::convert::From<<WriterStruct as WriteSerializer>::Error> for AllError {
            fn from(_value: <WriterStruct as WriteSerializer>::Error) -> Self {
                todo!()
            }
        }
        
        impl std::convert::From<<ReaderStruct as ReadDeserializer>::Error> for AllError {
            fn from(_value: <ReaderStruct as ReadDeserializer>::Error) -> Self {
                todo!()
            }
        }
        
        impl std::convert::From<<CompressorStruct as Compressor>::Error> for AllError {
            fn from(_value: <CompressorStruct as Compressor>::Error) -> Self {
                todo!()
            }
        }
        
        impl std::convert::From<<EccerStruct as Eccer>::Error> for AllError {
            fn from(_value: <EccerStruct as Eccer>::Error) -> Self {
                todo!()
            }
        }
        
        pub struct WError;
        
        impl std::convert::From<std::io::Error> for WError {
            fn from(_value: std::io::Error) -> Self {
                todo!()
            }
        }
        #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
        pub struct WriterStruct;
        
        impl WriteSerializer for WriterStruct {
            type Error = WError;
        
            fn serialize_into<W: std::io::Write, T: serde::Serialize + DocuFortMsg>(
                _writer: &mut W,
                _message: &T,
            ) -> Result<(), Self::Error> {
                todo!()
            }
        
            fn serialized_size<T: serde::Serialize + DocuFortMsg>(
                _message: &T,
            ) -> Result<usize, Self::Error> {
                todo!()
            }
        }
        #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
        pub struct RError;
        
        impl std::convert::From<std::io::Error> for RError {
            fn from(_value: std::io::Error) -> Self {
                todo!()
            }
        }
        #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
        pub struct ReaderStruct;
        
        impl ReadDeserializer for ReaderStruct {
            type Error = RError;
        
            fn read_from<'de, T: serde::Deserialize<'de> + DocuFortMsg>(
                _bytes: &[u8],
            ) -> Result<T, Self::Error> {
                todo!()
            }
        }
        #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
        pub struct CError;
        #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
        pub struct CompressorStruct;
        
        impl Compressor for CompressorStruct {
            type Error = CError;
        
            fn compress_into<W: std::io::Write + std::io::Seek>(
                _writer: &mut W,
                _data: &[u8],
                _try_compress: Option<CompressionLevel>,
            ) -> Result<(), Self::Error> {
                todo!()
            }
        
            fn decompress_into<W: std::io::Write>(
                _writer: &mut W,
                _data: &[u8],
            ) -> Result<(), Self::Error> {
                todo!()
            }
        }
        #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
        pub struct EError;
        #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
        pub struct EccerStruct;
        
        impl Eccer for EccerStruct {
            type Error = EError;
        
            fn calc_ecc_into<W: std::io::Write>(_writer: &mut W, _raw_data: &[u8]) -> Result<(), Self::Error> {
                todo!()
            }
        
            fn apply_ecc(_raw_data: &mut [u8]) -> Result<usize, Self::Error> {
                todo!()
            }
        
            fn calc_ecc_data_len(_raw_data_len: usize) -> usize {
                todo!()
            }
        }
    };

    tokens.into()
}

/// `MsgCoder` is a custom derive macro that simplifies Serde's `Serialize` and `Deserialize` trait implementations.
/// It caters specifically to structures that contain an optional `data` field which should be last in order.
///
/// When a struct is annotated with `#[derive(MsgCoder)]`, this macro takes care of several things:
///
/// * It ensures that the `data` field, if present, is the last field of the struct.
/// * It automatically skips serialization of the `data` field.
/// * Upon deserialization, it provides a default value for the `data` field.
///
/// This macro is designed to save you from the details of implementing Serde traits or manually adding Serde skip/default attribute tags to the `data` field.
/// It is recommended to ensure things work as expected.
/// # Example
/// ```text
/// #[derive(MsgCoder)]
/// pub struct TestStruct {
///     field1: u8,
///     field2: u32,
///     data: Option<Vec<u8>>,
/// }
/// ```
/// The same as:
/// ```text
/// #[derive(Serialize,Deserialize)]
/// pub struct TestStruct {
///     field1: u8,
///     field2: u32,
///     #[serde(skip_serializing,default)]
///     data: Option<Vec<u8>>,
/// }
/// ```
///
/// In the example above, `TestStruct` can be serialized/deserialized using Serde, but the `data` field is automatically skipped during serialization and defaults to `None` during deserialization.
///
/// # Important
/// This macro doesn't validate if the `data` field is set at runtime; it will only ensure that the `data` field, if present, is the last field during compile time. You must manage the `data` field.
///
/// # Note
/// This macro is a convenience tool, and it's not mandatory. If you want, you can manually derive or implement `Serialize` and `Deserialize` for your structs as shown above.
/// If you forget to skip serializing the data field, there is only runtime checks to ensure the message part (non-data) is 255 bytes or less.
#[proc_macro_derive(MsgCoder)]
pub fn msg_impls(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);

    // Struct name
    let struct_name = &input.ident;

    // Used to accumulate the field tokens and field names to output
    let mut field_names = Vec::new();


    // Check if the struct has named fields
    if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(FieldsNamed { named, .. }) = &data_struct.fields {
            // Iterate over each field
            let mut has_data = false;
            for field in named {
                if has_data {panic!("'data' must be the last field on the message struct!")}
                // Get the field name
                let ident = field.ident.as_ref().unwrap();
                if ident == "data" {
                    // If the field name is 'data', skip it during serialization
                    has_data = true;
                    continue;
                }

                // Add the field name to the field names
                let field_name = format_ident!("{}", ident);
                field_names.push(field_name);
            }
            let num_fields = field_names.len();
            let visitor_name = format_ident!("{}Visitor", struct_name);
            let field_indices: Vec<_> = (0..field_names.len()).collect();
            // The tokens for setting the 'data' field to its default value
            let data_field_tokens = if has_data {
                quote! { data: Default::default(), }
            } else {
                quote! {}
            };
            // Construct the output tokens
            let serialize_tokens = quote! {
                impl serde::Serialize for #struct_name {
                    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                    where
                        S: serde::Serializer,
                    {
                        use serde::ser::SerializeStruct;
                        let mut s = serializer.serialize_struct(stringify!(#struct_name), #num_fields)?;
                        #(s.serialize_field(stringify!(#field_names), &self.#field_names)?;)*
                        s.end()
                    }
                }

                struct #visitor_name;

                impl<'de> ::serde::de::Visitor<'de> for #visitor_name {
                    type Value = #struct_name;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str("struct ")
                    }

                    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                    where
                        A: ::serde::de::SeqAccess<'de>,
                    {
                        Ok(#struct_name {
                            #(
                                #field_names: seq.next_element()?.ok_or_else(|| ::serde::de::Error::invalid_length(#field_indices, &self))?,
                            )*
                            #data_field_tokens
                        })
                    }
                }

                impl<'de> ::serde::Deserialize<'de> for #struct_name {
                    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                    where
                        D: ::serde::Deserializer<'de>,
                    {
                        deserializer.deserialize_seq(#visitor_name)
                    }
                }
                
            };

            // Return the resulting token stream
            TokenStream::from(serialize_tokens)
        } else {
            panic!("This macro only supports named fields");
        }
    } else {
        panic!("This macro only supports structs with named fields");
    }
}



