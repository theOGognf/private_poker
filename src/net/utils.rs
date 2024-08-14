use bincode::{deserialize, serialize, ErrorKind};
use serde::{de::DeserializeOwned, Serialize};
use std::io::{self, Read, Write};

pub fn read_prefixed<T: DeserializeOwned, R: Read>(reader: &mut R) -> io::Result<T> {
    // Read the size as a u32
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    // Read the remaining data
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;

    match deserialize(&buf) {
        Ok(value) => Ok(value),
        Err(error) => match *error {
            ErrorKind::Io(error) => Err(error),
            _ => Err(io::ErrorKind::InvalidData.into()),
        },
    }
}

pub fn write_prefixed<T: Serialize, W: Write>(writer: &mut W, value: &T) -> io::Result<()> {
    match serialize(&value) {
        Ok(serialized) => {
            // Write the size of the serialized data as a u32
            let size = serialized.len() as u32;
            writer.write_all(&size.to_le_bytes())?;

            // Write the serialized data
            writer.write_all(&serialized)?;

            Ok(())
        }
        Err(error) => match *error {
            ErrorKind::Io(error) => Err(error),
            _ => Err(io::ErrorKind::InvalidData.into()),
        },
    }
}
