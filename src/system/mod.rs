use std::str::from_utf8;
use std::process::Output;
use std::io;
use std::fmt;
use std::time::SystemTime;

#[cfg(test)]
pub mod fake;
pub mod util;
pub mod real;

#[derive(Debug, PartialEq)]
pub struct CommandLineOutput
{
    pub out : String,
    pub err : String,
    pub code : Option<i32>,
    pub success : bool,
}

#[derive(Debug, PartialEq)]
pub enum ReadWriteError
{
    IOError(String),
    SystemError(SystemError)
}

impl fmt::Display for ReadWriteError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ReadWriteError::IOError(io_error_message)
                => write!(formatter, "{}", io_error_message),

            ReadWriteError::SystemError(error)
                => write!(formatter, "{}", error),
        }
    }
}

pub struct CommandScript
{
    pub lines : Vec<String>
}

impl fmt::Display for CommandScript
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        write!(formatter, "{}", self.lines.join("; "))
    }
}

pub fn to_command_script(mut all_lines : Vec<String>) -> CommandScript
{
    let mut command_script = CommandScript{lines:vec![]};
    let mut command_lines : Vec<String> = vec![];

    for line in all_lines.drain(..)
    {
        match line.as_ref()
        {
            ";" =>
            {
                command_script.lines.push(command_lines.join(" "));
                command_lines = vec![];
            },
            _ =>
            {
                command_lines.push(line);
            }
        }
    }

    if command_lines.len() != 0
    {
        command_script.lines.push(command_lines.join(" "));
    }

    command_script
}


impl CommandLineOutput
{
    #[cfg(test)]
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
#[derive(Debug, PartialEq)]
pub enum SystemError
{
    NotFound,
    FileInPlaceOfDirectory(String),
    DirectoryInPlaceOfFile(String),
    PathEmpty,
    PathNotUnicode,
    RemoveFileFoundDir,
    ExpectedDirFoundFile,
    RemoveNonExistentFile,
    RemoveNonExistentDir,
    RenameFromNonExistent,
    RenameToNonExistent,
    MetadataNotFound,
    ModifiedNotFound,
    CreateFileOverExistingDirectory,
    CreateDirectoryOverExistingFile,
    CommandExecutationFailed(String),
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

            SystemError::PathNotUnicode
                => write!(formatter, "Path encountered which does not convert to unicode string"),

            SystemError::RemoveFileFoundDir
                => write!(formatter, "Attempt to remove file, found directory"),

            SystemError::ExpectedDirFoundFile
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

            SystemError::CreateFileOverExistingDirectory
                => write!(formatter, "Attempt to create a file where a directory already exists"),

            SystemError::CreateDirectoryOverExistingFile
                => write!(formatter, "Attempt to create a directory where a file already exists"),

            SystemError::CommandExecutationFailed(message)
                => write!(formatter, "{}", message),

            SystemError::NotImplemented
                => write!(formatter, "Attempt to perform an operation not currently implemented by fake system"),

            SystemError::Weird
                => write!(formatter, "Weird error, this happens when internal logic fails in a way the programmer didn't think was possible"),
        }
    }
}

/*  System abstracts the filesystem and command-line executor.  An implementation can appeal to the
    real computer's file-system and command-line, or it can fake it for testing. */
pub trait System: Clone + Send + Sync
{
    type File: io::Read + io::Write + fmt::Debug + Send;

    fn open(&self, path: &str) -> Result<Self::File, SystemError>;
    fn create_file(&mut self, path: &str) -> Result<Self::File, SystemError>;
    fn create_dir(&mut self, path: &str) -> Result<(), SystemError>;
    fn is_dir(&self, path: &str) -> bool;
    fn is_file(&self, path: &str) -> bool;

    #[cfg(test)]
    fn remove_file(&mut self, path: &str) -> Result<(), SystemError>;

    #[cfg(test)]
    fn remove_dir(&mut self, path: &str) -> Result<(), SystemError>;

    fn list_dir(&self, path: &str) -> Result<Vec<String>, SystemError>;
    fn rename(&mut self, from: &str, to: &str) -> Result<(), SystemError>;

    fn get_modified(&self, path: &str) -> Result<SystemTime, SystemError>;
    fn is_executable(&self, path: &str) -> Result<bool, SystemError>;
    fn set_is_executable(&mut self, path: &str, executable : bool) -> Result<(), SystemError>;
    fn execute_command(&mut self, command_script: CommandScript) -> Vec<Result<CommandLineOutput, SystemError>>;
}
