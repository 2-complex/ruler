use std::str::from_utf8;
use std::process::Output;
use std::io;
use std::fmt;

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
                => write!(formatter, "I/O Error: {}", io_error_message),

            ReadWriteError::SystemError(error)
                => write!(formatter, "Read/Write Error: {}", error),
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

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
#[derive(Clone)]
pub enum SystemError
{
    NotFound,
    FileInPlaceOfDirectory(String),
    DirectoryInPlaceOfFile(String),
    PathInvalid,
    PathNotUnicode,
    RemoveFileFoundDir,
    ExpectedDirFoundFile,
    RemoveNonExistentFile,
    RemoveNonExistentDir,
    RenameFromNonExistent,
    RenameToNonExistent,
    MetadataNotFound,
    ModifiedNotFound,
    ModifiedInvalid,
    CreateOverExisting,
    CommandExecutationFailed(String),
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

            SystemError::PathInvalid
                => write!(formatter, "Invalid arguments: invalid path"),

            SystemError::PathNotUnicode
                => write!(formatter, "Path encountered which does not convert to unicode string"),

            SystemError::RemoveFileFoundDir
                => write!(formatter, "Attempt to remove file, found directory"),

            SystemError::ExpectedDirFoundFile
                => write!(formatter, "Attempt to access path as directory, found file"),

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

            SystemError::ModifiedInvalid
                => write!(formatter, "Attempt to access modified time failed becase system time did not convert to number"),

            SystemError::MetadataNotFound
                => write!(formatter, "Attempt to access metadate failed"),

            SystemError::CreateOverExisting
                => write!(formatter, "Attempt to create a filesystem entity where another already exists with different type"),

            SystemError::CommandExecutationFailed(message)
                => write!(formatter, "{}", message),

            SystemError::Weird
                => write!(formatter, "Weird error, this happens when internal logic fails in a way the programmer didn't think was possible"),
        }
    }
}

/*  System abstracts the filesystem and command-line executor.  An implementation can
    use the real computer's file-system and command-line, or it can fake it for testing. */
pub trait System: Clone + Send + Sync
{
    type File: io::Read + io::Write + fmt::Debug + Send;

    fn open(&self, path: &str) -> Result<Self::File, SystemError>;
    fn create_file(&mut self, path: &str) -> Result<Self::File, SystemError>;
    fn create_dir(&mut self, path: &str) -> Result<(), SystemError>;
    fn is_dir(&self, path: &str) -> bool;
    fn is_file(&self, path: &str) -> bool;
    fn exists(&self, path: &str) -> bool
    {
        return self.is_dir(path) || self.is_file(path);
    }

    #[cfg(test)]
    fn remove_file(&mut self, path: &str) -> Result<(), SystemError>;

    #[cfg(test)]
    fn remove_dir(&mut self, path: &str) -> Result<(), SystemError>;

    fn list_dir(&self, path: &str) -> Result<Vec<String>, SystemError>;
    fn rename(&mut self, from: &str, to: &str) -> Result<(), SystemError>;

    fn get_modified(&self, path: &str) -> Result<u64, SystemError>;
    fn get_timestamp_recursive(&self, path: &str) -> Result<u64, SystemError>
    {
        let mut timestamp = self.get_modified(path)?;
        for name in self.list_dir(path)?
        {
            timestamp = std::cmp::max(timestamp,
                self.get_timestamp_recursive(&format!("{}/{}", path, name))?);
        }
        Ok(timestamp)
    }

    fn is_executable(&self, path: &str) -> Result<bool, SystemError>;
    fn set_is_executable(&mut self, path: &str, executable : bool) -> Result<(), SystemError>;
    fn execute_command(&mut self, command_script: CommandScript) -> Vec<Result<CommandLineOutput, SystemError>>;
}
