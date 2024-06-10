# Docufort
A crash consistent append only file toolbox

# Overview
This is the thinnest wrapper possible to write the file recovery routines.
Everything is treated as binary, even if you are storing text.
This is **not** a 'batteries included' library.


## Error Correcting Codes
The system requires Error Correcting Codes to function. It is setup for Reed-Solomon, where the 'correction' encoding for each block is concatenated and (pre- or) appended to the 'message'. This allows the data to be read directly and leave the correction processing as optional. The 'system' uses the ECC as a form of checksum and integrity insurance on the system messages. The 'Content' written to the file can be optionally ECC'd.

## Compression
I added a fixed zstd compression option. I will be removing that and adding the required stuff to the BlockInputs Trait.

# Version 2 Goals
- Switch from Reed-Solomon ECC to BCH, as we really want bit-rot protection (random instead of burst errors).
    - Currently don't see a BCH lib in Rust. There is [this one](https://kchmck.github.io/doc/p25/coding/bch/index.html), but it does more error correction than I think we probably need for bit rot.
- Write a hash recovery routine. Currently if the BlockEnd Hash bytes are corrupted beyond the ECC correction ability, and the rest of the block is fine, it will end up showing the block as corrupted. This is very unlikely so I didn't waste the time for V1.
- Crate Feature: parallel -- to allow for parallel ecc calculation while writing content.


# Warning
This is less than a lib, it is a toolbox. You can take these primitives and make a non-working system. I tried to comment and document enough so it should make sense how it might flow.