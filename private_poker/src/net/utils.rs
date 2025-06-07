use bincode::{deserialize, serialize, ErrorKind};
use serde::{de::DeserializeOwned, Serialize};
use std::io::{self, Read, Write};

use super::super::constants::MAX_USER_INPUT_LENGTH;

#[must_use]
pub fn preprocess_username(username: &str) -> String {
    let mut username: String = username
        .chars()
        .map(|c| if c.is_ascii_whitespace() { '_' } else { c })
        .collect();
    username.truncate(MAX_USER_INPUT_LENGTH / 2);
    username
}

pub fn read_prefixed<T: DeserializeOwned, R: Read>(reader: &mut R) -> io::Result<T> {
    // Read the size as a u32
    let mut len_bytes = [0; 4];
    reader.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    // Read the remaining data. If we get a would block error,
    // then it's very likely that the sender doesn't follow the
    // prefix protocol. Return an invalid data error to let
    // the readers determine how to handle such senders. It is
    // possible for the would block error to be something that
    // isn't as sketchy, but that should be pretty rare.
    let mut buf = vec![0; len];
    if let Err(error) = reader.read_exact(&mut buf) {
        let kind = match error.kind() {
            io::ErrorKind::WouldBlock => io::ErrorKind::InvalidData,
            error => error,
        };
        return Err(kind.into());
    }

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
            // Write the size of the serialized data and the serialized data
            // all in one chunk to prevent read-side EOF race conditions.
            let size = serialized.len() as u32;
            let mut buf = Vec::from(size.to_le_bytes());
            buf.extend(serialized);
            writer.write_all(&buf)?;
            Ok(())
        }
        Err(error) => match *error {
            ErrorKind::Io(error) => Err(error),
            _ => Err(io::ErrorKind::InvalidData.into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use mio::net::{TcpListener, TcpStream};

    use super::{read_prefixed, write_prefixed};

    fn setup() -> (TcpStream, TcpStream) {
        let random_port_addr = "127.0.0.1:0".parse().unwrap();
        let server = TcpListener::bind(random_port_addr).unwrap();
        let addr = server.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (stream, _) = server.accept().unwrap();
        (client, stream)
    }

    #[test]
    fn write_and_read() {
        let (mut client, mut stream) = setup();
        let value = "Hello, World!".to_string();
        assert!(write_prefixed(&mut stream, &value).is_ok());
        assert!(read_prefixed::<String, TcpStream>(&mut client).is_ok_and(|v| v == value));
    }

    #[test]
    fn write_and_read_invalid_data() {
        let (mut client, mut stream) = setup();

        // Writing a size but not having the data to follow it up
        // results in invalid data.
        assert!(stream.write_all(&1u32.to_le_bytes()).is_ok());
        assert_eq!(
            read_prefixed::<String, TcpStream>(&mut client).map_err(|e| e.kind()),
            Err(io::ErrorKind::InvalidData)
        );
    }

    #[test]
    fn write_and_read_unexpected_eof() {
        let (mut client, mut stream) = setup();
        let value = "Hello, World!".to_string();
        let buf = value.as_bytes();
        let incorrect_size = buf.len() as u32 - 2;
        assert!(stream.write_all(&incorrect_size.to_le_bytes()).is_ok());
        assert!(stream.write_all(buf).is_ok());
        assert_eq!(
            read_prefixed::<String, TcpStream>(&mut client).map_err(|e| e.kind()),
            Err(io::ErrorKind::UnexpectedEof)
        );
    }
}
