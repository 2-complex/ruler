use std::str::from_utf8;
use std::process::Output;
use std::io;
use std::fmt;
use std::time::SystemTime;

#[cfg(test)]
pub mod fake;

#[cfg(test)]
pub mod util;

pub mod real;

pub struct CommandLineOutput
{
    pub out : String,
    pub err : String,
    pub code : Option<i32>,
    pub success : bool,
}

pub enum ReadWriteError
{
    IOError(io::Error),
    SystemError(SystemError)
}

impl fmt::Display for ReadWriteError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ReadWriteError::IOError(error)
                => write!(formatter, "{}", error),

            ReadWriteError::SystemError(error)
                => write!(formatter, "{}", error),
        }
    }
}

impl CommandLineOutput
{
    pub fn new() -> CommandLineOutput
    {
        CommandLineOutput
        {
            out : "".to_string(),
            err : "".to_string(),
            code : Some(0),
            success : true,
        }
    }

    #[cfg(test)]
    pub fn error(message : String) -> CommandLineOutput
    {
        CommandLineOutput
        {
            out : "".to_string(),
            err : message,
            code : Some(1),
            success : false,
        }
    }

    pub fn from_output(output : Output) -> CommandLineOutput
    {
        CommandLineOutput
        {
            out : match from_utf8(&output.stdout)
            {
                Ok(text) => text,
                Err(_) => "<non-utf8 data>",
            }.to_string(),

            err : match from_utf8(&output.stderr)
            {
                Ok(text) => text,
                Err(_) => "<non-utf8 data>",
            }.to_string(),

            code : output.status.code(),
            success : output.status.success(),
        }
    }
}

/*  A lot of these are only contructed by the fake filesystem.
    In the future, maybe hone the list of errors down to something that real/fake
    system can agree on completely, but in the mean time, disabling the warning.
*/
#[allow(dead_code)]
pub enum SystemError
{
    NotFound,
    FileInPlaceOfDirectory(String),
    DirectoryInPlaceOfFile(String),
    PathEmpty,
    RemoveFileFoundDir,
    RemoveDirFoundFile,
    RemoveNonExistentFile,
    RemoveNonExistentDir,
    RenameFromNonExistent,
    RenameToNonExistent,
    MetadataNotFound,
    ModifiedNotFound,
    CommandExecutationFailed,
    NotImplemented,
    Weird,
}

impl fmt::Display for SystemError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            SystemError::NotFound
                => write!(formatter, "No such file or directory"),

            SystemError::FileInPlaceOfDirectory(component)
                => write!(formatter, "Expected directory, found file: {}", component),

            SystemError::DirectoryInPlaceOfFile(component)
                => write!(formatter, "Expected file, found directory: {}", component),

            SystemError::PathEmpty
                => write!(formatter, "Invalid arguments: found empty path"),

            SystemError::RemoveFileFoundDir
                => write!(formatter, "Attempt to remove file, found directory"),

            SystemError::RemoveDirFoundFile
                => write!(formatter, "Attempt to remove directory, found file"),

            SystemError::RemoveNonExistentFile
                => write!(formatter, "Attempt to remove non-existent file"),

            SystemError::RemoveNonExistentDir
                => write!(formatter, "Attempt to remove non-existent directory"),

            SystemError::RenameFromNonExistent
                => write!(formatter, "Attempt to rename a non-existent file or directory"),

            SystemError::RenameToNonExistent
                => write!(formatter, "Attempt to rename a file or directory with non-existent target directory"),

            SystemError::ModifiedNotFound
                => write!(formatter, "Attempt to access modified time for file failed"),

            SystemError::MetadataNotFound
                => write!(formatter, "Attempt to access metadate failed"),

            SystemError::CommandExecutationFailed
                => write!(formatter, "Underlying OS failed to execute command"),

            SystemError::NotImplemented
                => write!(formatter, "Attempt to perform an operation not currently implemented by fake system"),

            SystemError::Weird
                => write!(formatter, "Weird error, this happens when internal logic fails in a way the programmer didn't think was possible"),
        }
    }
}


pub trait System: Clone + Send + Sync
{
    type File: io::Read + io::Write + fmt::Debug;

    fn open(&self, path: &str) -> Result<Self::File, SystemError>;
    fn create_file(&mut self, path: &str) -> Result<Self::File, SystemError>;
    fn create_dir(&mut self, path: &str) -> Result<(), SystemError>;
    fn is_dir(&self, path: &str) -> bool;
    fn is_file(&self, path: &str) -> bool;
    fn remove_file(&mut self, path: &str) -> Result<(), SystemError>;
    fn remove_dir(&mut self, path: &str) -> Result<(), SystemError>;
    fn rename(&mut self, from: &str, to: &str) -> Result<(), SystemError>;

    fn get_modified(&self, path: &str) -> Result<SystemTime, SystemError>;
    fn execute_command(&mut self, command_list: Vec<String>) -> Result<CommandLineOutput, SystemError>;
}

