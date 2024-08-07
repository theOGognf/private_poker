use anyhow::Error;
use bincode::{deserialize, serialize};
use serde::{de::DeserializeOwned, Serialize};
use std::io::{Read, Write};

pub fn read_value_prefixed<T: DeserializeOwned, R: Read>(reader: &mut R) -> Result<T, Error> {
    // Read the size as a u32
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    // Read the remaining data
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;

    let value = deserialize(&buf)?;
    Ok(value)
}

pub fn write_serialized_prefixed<W: Write>(writer: &mut W, serialized: &[u8]) -> Result<(), Error> {
    // Write the size of the serialized data as a u32
    let size = serialized.len() as u32;
    writer.write_all(&size.to_le_bytes())?;

    // Write the serialized data
    writer.write_all(serialized)?;

    Ok(())
}

pub fn write_value_prefixed<T: Serialize, W: Write>(
    writer: &mut W,
    value: &T,
) -> Result<(), Error> {
    let serialized = serialize(&value)?;

    // Write the size of the serialized data as a u32
    let size = serialized.len() as u32;
    writer.write_all(&size.to_le_bytes())?;

    // Write the serialized data
    writer.write_all(&serialized)?;

    Ok(())
}
