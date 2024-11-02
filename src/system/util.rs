use crate::system::SystemError;
use std::io;

#[cfg(test)]
use crate::system::ReadWriteError;

use crate::system::System;

#[cfg(test)]
use std::io::Read;

#[cfg(test)]
use std::io::Write;

#[cfg(test)]
use std::time::Duration;

use std::time::
{
    SystemTime,
    SystemTimeError
};
use std::str::from_utf8;
use std::fmt;

#[cfg(test)]
pub fn timestamp_to_system_time(timestamp: u64) -> SystemTime
{
    SystemTime::UNIX_EPOCH
        + Duration::from_secs(timestamp / 1_000_000u64)
        + Duration::from_micros(timestamp % 1_000_000u64)
}

pub fn get_timestamp(system_time : SystemTime) -> Result<u64, SystemTimeError>
{
    match system_time.duration_since(SystemTime::UNIX_EPOCH)
    {
        Ok(duration) => Ok(1_000_000u64 * duration.as_secs() + u64::from(duration.subsec_micros())),
        Err(e) => Err(e),
    }
}

/*  Takes a System, a path as a &str and content, and content as a &str.  Writes content to the file.
    If system fails, forwards the system error.  If file-io fails, forwards the std::io::Error. */
#[cfg(test)]
pub fn write_str_to_file
<
    SystemType : System,
>
(
    system : &mut SystemType,
    file_path : &str,
    content : &str
)
-> Result<(), ReadWriteError>
{
    match system.create_file(file_path)
    {
        Ok(mut file) =>
        {
            match file.write_all(content.as_bytes())
            {
                Ok(_) => Ok(()),
                Err(error) => Err(ReadWriteError::IOError(format!("{}", error))),
            }
        }
        Err(error) => Err(ReadWriteError::SystemError(error))
    }
}

/*  Reads binary data from a file in a file-system into a Vec<u8>.
    If system fails, forwards the system error.  If file-io fails,
    forwards the std::io::Error. */
#[cfg(test)]
pub fn read_file
<
    F : System,
>
(
    system : &F,
    path : &str
)
-> Result<Vec<u8>, ReadWriteError>
{
    match system.open(path)
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
                Err(error) => Err(ReadWriteError::IOError(format!("{}", error))),
            }
        }
        Err(error) => Err(ReadWriteError::SystemError(error)),
    }
}

#[derive(Debug)]
pub enum FileToStringError
{
    IOError(io::Error),
    NotUTF8,
}

impl fmt::Display for FileToStringError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            FileToStringError::IOError(error) =>
                write!(formatter, "I/O Error reading file to string: {}", error),

            FileToStringError::NotUTF8 =>
                write!(formatter, "Cannot interpret as UTF8"),
        }
    }
}

pub fn file_to_string
<
    FileType : io::Read
>
(file : &mut FileType)
-> Result<String, FileToStringError>
{
    let mut content = Vec::new();
    match file.read_to_end(&mut content)
    {
        Ok(_size) =>
        {
            match from_utf8(&content)
            {
                Ok(rule_text) => Ok(rule_text.to_owned()),
                Err(_) => return Err(FileToStringError::NotUTF8),
            }
        },
        Err(error) => Err(FileToStringError::IOError(error)),
    }
}

#[derive(Debug)]
pub enum ReadFileToStringError
{
    IOError(String, io::Error),
    SystemError(String, SystemError),
    NotUTF8(String),
}

impl fmt::Display for ReadFileToStringError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ReadFileToStringError::IOError(path, error) =>
                write!(formatter, "I/O Error opening {} for read: {}", path, error),

            ReadFileToStringError::SystemError(path, error) =>
                write!(formatter, "System Error opening {} for read: {}", path, error),

            ReadFileToStringError::NotUTF8(path) =>
                write!(formatter, "Cannot interpret as UTF8: {}", path),
        }
    }
}

/*  Takes a System and a path as a str, opens the path in the filesystem, reads in the conent assuming that the
    content is a utf8-encoded string and returns the result as a String.  Two types of error can occur, an error
    opening the file, or an error reading utf8.  Therefore, this function has its own error type. */
pub fn read_file_to_string
<
    SystemType : System,
>
(
    system : &SystemType,
    path : &str
)
-> Result<String, ReadFileToStringError>
{
    match system.open(path)
    {
        Ok(mut file) =>
        {
            match file_to_string(&mut file)
            {
                Ok(result) => Ok(result),
                Err(FileToStringError::IOError(ioerror)) => Err(ReadFileToStringError::IOError(path.to_string(), ioerror)),
                Err(FileToStringError::NotUTF8) => Err(ReadFileToStringError::NotUTF8(path.to_string())),
            }
        },
        Err(error) => Err(ReadFileToStringError::SystemError(path.to_string(), error)),
    }
}

