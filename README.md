# Docufort
A crash consistent append only file toolbox

# Overview
This is the thinnest wrapper possible to write the file recovery routines.  
Everything is treated as binary, even if you are storing text. 
This is **not** a 'batteries included' library. 


## Error Correcting Codes
The system requires Error Correcting Codes to function. It is setup for Reed-Solomon, where the 'correction' encoding for each block is concatenated and (pre- or) appended to the 'message'. This allows the data to be read directly and leave the correction processing as optional. The 'system' uses the ECC as a form of checksum and integrity insurance on the system messages. The 'Content' written to the file can be optionally ECC'd.

## Compression
I want to add compression, but to avoid complication I have avoided that here. The 'Content' would need to be wrapped one more time if someone wanted to optionally compress the 'Content' before writing and ECCing.

I don't know how large the content someone might be writing, and I didn't want to allocate a massive Vec in order to hook in compression in this system. So I thought it best to just leave it out.

# Version 2 Goals
- Switch from Reed-Solomon ECC to BCH, as we really want bit-rot protection (random instead of burst errors).
    - Currently don't see a BCH lib in Rust. There is [this one](https://kchmck.github.io/doc/p25/coding/bch/index.html), but it does more error correction than I think we probably need for bit rot.
- Add Compression as a crate feature.
- Write a hash recovery routine. Currently if the BlockEnd Hash bytes are corrupted beyond the ECC correction ability, and the rest of the block is fine, it will end up showing the block as corrupted. This is very unlikely so I didn't waste the time for V1.

I think to increase the write speed significantly we need to build a streaming function that will take in raw bytes (optionally compress them) and write both the 'content' as well as 'ecc data' to the file. We would need to add a flag to indicate the content is compressed, just as we do now to indicate the presence of ECC.

Since compression is always going to be faster than ECC, I think we need to do either Async or Threads for the ECC calculations, since these will be in 'small' chunks that we can parallel. I think we would need to set the BCH 'chunk size' to something that justifies the overhead of either a task or a thread. I think tasks would be better as they are lighter weight and generally BCH chunks are relatively small (lots of tasks). More research needed.

# Warning
This is less than a lib, it is a toolbox. You can take these primitives and make a non-working system. I tried to comment and document enough so it should make sense how it might flow.