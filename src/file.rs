extern crate file_objects_rs;
use file_objects_rs::FileSystem;

#[cfg(test)]
use std::str::from_utf8;

#[cfg(test)]
use std::fmt;

#[cfg(test)]
use std::io::Read;

use std::io::Write;

/*  Takes a FileSystem, a path a a str and a vector of binary data.  Supplants the file at the given path in the
    filesystem with the binary content.  If file-opening fails, this function echoes the std::io error. */
pub fn write_file
<
    FileSystemType : FileSystem,
>
(
    file_system : &mut FileSystemType,
    file_path : &str,
    content : &[u8]
)
-> Result<(), std::io::Error>
{
    match file_system.create(file_path)
    {
        Ok(mut file) =>
        {
            match file.write_all(&content)
            {
                Ok(_) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        Err(error) => return Err(error),
    }
}

/*  Takes a FileSystem, a path as a &str and content, also a &str writes the content to the file.  If file-io fails,
    forwards the std::io::Error. */
#[cfg(test)]
pub fn write_str_to_file
<
    FileSystemType : FileSystem,
>
(
    file_system : &mut FileSystemType,
    file_path : &str,
    content : &str
)
-> Result<(), std::io::Error>
{
    match file_system.create(file_path)
    {
        Ok(mut file) =>
        {
            match file.write_all(content.as_bytes())
            {
                Ok(_) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        Err(error) => return Err(error),
    }
}

/*  Reads binary data from a file in a FileSystem into a Vec<u8>.  If file-io fails, forwards the std::io::Error */
#[cfg(test)]
pub fn read_file
<
    FileSystemType : FileSystem,
>
(
    file_system : &FileSystemType,
    path : &str
)
-> Result<Vec<u8>, std::io::Error>
{
    match file_system.open(path)
    {
        Ok(mut file) =>
        {
            let mut content = Vec::new();
            match file.read_to_end(&mut content)
            {
                Ok(_size) =>
                {
                    return Ok(content);
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
pub enum ReadFileToStringError
{
    IOError(String, std::io::Error),
    NotUTF8(String)
}

/*  Display a ReadFileToStringError by printing a reasonable error message. */
#[cfg(test)]
impl fmt::Display for ReadFileToStringError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ReadFileToStringError::IOError(path, error) =>
                write!(formatter, "Error opening {} for read: {}", path, error),

            ReadFileToStringError::NotUTF8(path) =>
                write!(formatter, "Cannot interpret as UTF8: {}", path),
        }
    }
}

/*  Takes a FileSystem and a path as a str, opens the path in the filesystem, reads in the conent assuming that the
    content is a utf8-encoded string and returns the result as a String.  Two types of error can occur, an error
    opening the file, or an error reading utf8.  Therefore, this function has its own error type. */
#[cfg(test)]
pub fn read_file_to_string
<
    FileSystemType : FileSystem,
>
(
    file_system : &mut FileSystemType,
    path : &str
)
-> Result<String, ReadFileToStringError>
{
    match file_system.open(path)
    {
        Ok(mut file) =>
        {
            let mut content = Vec::new();
            match file.read_to_end(&mut content)
            {
                Ok(_size) =>
                {
                    match from_utf8(&content)
                    {
                        Ok(rule_text) => Ok(rule_text.to_owned()),
                        Err(_) => return Err(ReadFileToStringError::NotUTF8(path.to_string())),
                    }
                },
                Err(error) => Err(ReadFileToStringError::IOError(path.to_string(), error)),
            }
        },
        Err(error) => Err(ReadFileToStringError::IOError(path.to_string(), error)),
    }
}

