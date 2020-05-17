extern crate filesystem;
use filesystem::FileSystem;
use std::path::Path;
use std::str::from_utf8;
use std::fmt;
use std::io::
{
    Read,
    Write,
};

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


pub enum ReadFileToStringError
{
    IOError(String, std::io::Error),
    NotUTF8(String)
}

/*  Display a ReadFileToStringError by printing a reasonable error message. */
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

