use crate::system::
{
    System,
    SystemError,
    ReadWriteError,
};
use std::io::
{
    self,
    Read,
    Write
};
use std::time::Duration;
use std::time::
{
    SystemTime,
    SystemTimeError
};

#[cfg(test)]
use std::str::from_utf8;

#[cfg(test)]
use std::fmt;


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

/*  Takes a System, a path a a str and a vector of binary data.  Supplants the file at the given path in the
    filesystem with the binary content.  If file-opening fails, this function echoes the std::io error. */
pub fn write_file
<
    SystemType : System,
>
(
    system : &mut SystemType,
    file_path : &str,
    content : &[u8]
)
-> Result<(), ReadWriteError>
{
    match system.create_file(file_path)
    {
        Ok(mut file) =>
        {
            match file.write_all(&content)
            {
                Ok(_) => return Ok(()),
                Err(error) => return Err(ReadWriteError::IOError(error)),
            }
        }
        Err(error) => return Err(ReadWriteError::SystemError(error)),
    }
}


/*  Takes a System, a path as a &str and content, and content as a &str.  Writes content to the file.
    If system fails, forwards the system error.  If file-io fails, forwards the std::io::Error. */
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
                Err(error) => Err(ReadWriteError::IOError(error)),
            }
        }
        Err(error) => Err(ReadWriteError::SystemError(error))
    }
}

/*  Reads binary data from a file in a file-system into a Vec<u8>.
    If system fails, forwards the system error.  If file-io fails, forwards the std::io::Error. */
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
                Err(error) => Err(ReadWriteError::IOError(error)),
            }
        }
        Err(error) => Err(ReadWriteError::SystemError(error)),
    }
}

#[cfg(test)]
pub enum ReadFileToStringError
{
    IOError(String, io::Error),
    SystemError(String, SystemError),
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
#[cfg(test)]
pub fn read_file_to_string
<
    SystemType : System,
>
(
    system : &mut SystemType,
    path : &str
)
-> Result<String, ReadFileToStringError>
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
                    match from_utf8(&content)
                    {
                        Ok(rule_text) => Ok(rule_text.to_owned()),
                        Err(_) => return Err(ReadFileToStringError::NotUTF8(path.to_string())),
                    }
                },
                Err(error) => Err(ReadFileToStringError::IOError(path.to_string(), error)),
            }
        },
        Err(error) => Err(ReadFileToStringError::SystemError(path.to_string(), error)),
    }
}

