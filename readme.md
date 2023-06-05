# Docufort
A crash consistent append only file toolbox

# Overview
Everything is treated as binary, even if you are storing text.
Each thing is a Message.
A message can have a 'data' field.
The non-data portion of the message can be a maximum of 255bytes.
The data portion can be at max u32::MAX, but practically limitation will probably see far less than that as a max size.

The system does not specify an encoding syntax, however Bincode was envisioned.
The system does not specify a compression type. It allows for optional compression and a configurable min_size before attempting compression. Something like Zlib was envisioned.
The system includes an Error Correcting Code feature. The 'system' uses the ECC as a form of checksum on the system messages. I don't know if my traits allow for much flexibility, but I envisioned using Reed-Solomon.

# Warning
This is pretty low level yet. See the examples on what needs to be implemented.
I tried to keep things as generic as possible and used some wrapper traits to allow different impl choices.
This is less than a lib, it is a toolbox.