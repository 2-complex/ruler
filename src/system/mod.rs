use std::io::
{
    self,
    Write
};
use std::fmt;
use crate::system::language::
{
    OutDestination,
    ErrDestination
};

#[cfg(test)]
pub mod fake;

pub mod util;
pub mod real;
pub mod language;

#[derive(Debug, PartialEq)]
pub struct Standard
{
    pub out : Vec<u8>,
    pub err : Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct CommandResult
{
    pub standard: Standard,
    pub code: Option<i32>,
}

#[derive(Debug, PartialEq)]
pub struct CommandScriptLineResult
{
    pub standard: Standard,
    pub code: Option<i32>,
}

impl CommandScriptLineResult
{
    fn new() -> Self
    {
        Self
        {
            standard: Standard
            {
                out: vec![],
                err: vec![],
            },
            code: Some(0)
        }
    }

    pub fn is_success(self: &Self) -> bool
    {
        match self.code
        {
            Some(i) => i==0,
            None => false
        }
    }
}

/*  For example, the c++ compiler puts errors and warnings both in std-err.
    If it generates an executable, it returns error code 0, otherwise 1 */
#[derive(Debug, PartialEq)]
pub struct CommandScriptResult
{
    pub outputs: Vec<Standard>,
    pub code: Option<i32>,
}

impl CommandScriptResult
{
    pub fn is_success(self: &Self) -> bool
    {
        match self.code
        {
            Some(i) => i==0,
            None => false
        }
    }

    pub fn new() -> CommandScriptResult
    {
        CommandScriptResult
        {
            outputs: vec![], // TODO maybe change the name?
            code: Some(0),
        }
    }

    pub fn push(self: &mut Self, line_result: CommandScriptLineResult)
    {
        self.outputs.push(line_result.standard);
        self.code = line_result.code; // overwrite the one that's there
    }
}

fn bytes_to_string(buf: &[u8]) -> String
{
    match std::str::from_utf8(buf)
    {
        Ok(string) => string.to_string(),
        Err(_) => "<invalid utf8>".to_string(),
    }
}

impl fmt::Display for CommandScriptResult
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        for outputs in self.outputs.iter()
        {
            write!(formatter, "{}", bytes_to_string(&outputs.out))?;
            write!(formatter, "{}", bytes_to_string(&outputs.err))?;
        }
        if ! self.is_success()
        {
            write!(formatter, "Script ended in error:")?;
            match self.code
            {
                Some(i) => write!(formatter, "status code: {}", i),
                None => write!(formatter, "no status code"),
            }
        }
        else
        {
            write!(formatter, "Done.")
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Clone)]
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
    IOError(String),
    Weird(String),
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

            SystemError::IOError(io_error_message)
                => write!(formatter, "I/O Error: {}", io_error_message),

            SystemError::Weird(message)
                => write!(formatter, "Weird error, this happens when internal logic fails in a way the programmer didn't think was possible.  Message: {}", message),
        }
    }
}

pub struct Variables
{
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
        if self.is_file(path)
        {
            return Ok(timestamp);
        }
        for entry in self.list_dir(path)?
        {
            timestamp = std::cmp::max(timestamp, self.get_timestamp_recursive(&format!("{}/{}", path, entry))?);
        }
        Ok(timestamp)
    }

    fn is_executable(&self, path: &str) -> Result<bool, SystemError>;
    fn set_is_executable(&mut self, path: &str, executable : bool) -> Result<(), SystemError>;

    fn execute_command(&mut self, exec: String, args: Vec<String>, input: Vec<u8>) -> CommandResult;

    fn write_to_file(&mut self, path_string: &str, content: Vec<u8>) -> Result<(), (Option<i32>, Vec<u8>)>
    {
        let mut file = match self.create_file(&path_string)
        {
            Ok(file) => file,
            Err(error) => return Err((Some(1), format!("System error: {}", error).into_bytes())),
        };

        match file.write_all(&content)
        {
            Ok(_) => {},
            Err(error) => return Err((Some(1), format!("System error: {}", error).into_bytes())),
        }

        Ok(())
    }

    fn execute_command_script_line(&mut self, command_script_line: language::CommandScriptLine, input: Vec<u8>) -> CommandScriptLineResult
    {
        let command_result = self.execute_command(command_script_line.exec, command_script_line.args, input);
        let mut line_result = CommandScriptLineResult::new();
        line_result.code = command_result.code;

        match command_script_line.err_destination
        {
            ErrDestination::StdErr =>
            {
                line_result.standard.err = command_result.standard.err;
            },
            ErrDestination::File(path_string) =>
            {
                match self.write_to_file(&path_string, command_result.standard.err)
                {
                    Ok(_) => {},
                    Err((code, message)) =>
                    {
                        line_result.standard.err = message;
                        line_result.code = code;
                    }
                }
            }
        }

        match command_script_line.out_destination
        {
            OutDestination::StdOut =>
            {
                line_result.standard.out = command_result.standard.out;
            },
            OutDestination::File(path_string) =>
            {
                match self.write_to_file(&path_string, command_result.standard.out)
                {
                    Ok(_) => {},
                    Err((code, message)) =>
                    {
                        line_result.standard.err = message;
                        line_result.code = code;
                    }
                }
            },
            OutDestination::Command(script_line_box) =>
                return self.execute_command_script_line(*script_line_box, command_result.standard.out),
        }

        line_result
    }

    fn execute_command_script(&mut self, variables: &Variables, command_script : language::CommandScript) -> CommandScriptResult
    {
        let mut result = CommandScriptResult::new();
        for line in command_script.lines.into_iter()
        {
            let line_result = self.execute_command_script_line(line, vec![]);
            let is_success = line_result.is_success();

            result.push(line_result);
            if ! is_success
            {
                break;
            }
        }
        result
    }
}
