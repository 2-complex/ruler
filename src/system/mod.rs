use std::io;
use std::io::Write;
use std::fmt;
use std::collections::HashMap;
use crate::system::language::
{
    OutDestination,
    ErrDestination,
    CommandScript,
    CommandScriptLine,
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
    pub command_script_line: CommandScriptLine,
    pub standard: Standard,
    pub code: Option<i32>,
}

/*  For example, the c++ compiler puts errors and warnings both in std-err.
    If it generates an executable, it returns error code 0, otherwise 1 */
#[derive(Debug, PartialEq)]
pub struct CommandScriptResult
{
    pub command_script_lines: Vec<CommandScriptLine>,
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
            if !outputs.out.is_empty()
            {
                write!(formatter, "OUT: {}\n", bytes_to_string(&outputs.out))?;
            }

            if !outputs.err.is_empty()
            {
                write!(formatter, "ERR: {}\n", bytes_to_string(&outputs.err))?;
            }
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
    map: HashMap<String, String>
}

impl Variables
{
    pub fn new() -> Variables
    {
        Variables
        {
            map: HashMap::new()
        }
    }

    fn add(self: &mut Self, key: &str, value: &str)
    {
        self.map.insert(key.to_string(), value.to_string());
    }

    fn apply(self: &Self, string: &str) -> String
    {
        match self.map.get(string)
        {
            Some(rep) => rep.clone(),
            None => string.to_string(),
        }
    }

    fn apply_vec(self: &Self, strings: &Vec<String>) -> Vec<String>
    {
        strings.iter().map(|s|{self.apply(s)}).collect()
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

    fn execute_command_script_line(&mut self, variables: &Variables, command_script_line: language::CommandScriptLine, input: Vec<u8>) -> CommandScriptLineResult
    {
        let command_result = self.execute_command(variables.apply(&command_script_line.exec), variables.apply_vec(&command_script_line.args), input);

        let mut standard = Standard
        {
            out: vec![],
            err: vec![],
        };
        let mut code = command_result.code;

        match command_script_line.err_destination
        {
            ErrDestination::StdErr =>
            {
                standard.err = command_result.standard.err;
            },
            ErrDestination::File(ref path_string) =>
            {
                match self.write_to_file(&path_string, command_result.standard.err)
                {
                    Ok(_) => {},
                    Err((in_code, message)) =>
                    {
                        standard.err = message;
                        code = in_code;
                    }
                }
            }
        }

        match command_script_line.out_destination
        {
            OutDestination::StdOut =>
            {
                standard.out = command_result.standard.out;
            },
            OutDestination::File(ref path_string) =>
            {
                match self.write_to_file(&path_string, command_result.standard.out)
                {
                    Ok(_) => {},
                    Err((in_code, message)) =>
                    {
                        standard.err = message;
                        code = in_code;
                    }
                }
            },
            OutDestination::Command(script_line_box) =>
                return self.execute_command_script_line(variables, *script_line_box, command_result.standard.out),
        }

        CommandScriptLineResult
        {
            command_script_line: command_script_line,
            standard: standard,
            code: code,
        }
    }

    fn execute_command_script(&mut self, variables: &Variables, command_script : CommandScript) -> CommandScriptResult
    {
        let mut outputs = vec![];
        let mut code = Some(0);
        let mut command_script_lines = vec![];

        for line in command_script.lines.into_iter()
        {
            let line_result = self.execute_command_script_line(variables, line, vec![]);
            outputs.push(line_result.standard);
            command_script_lines.push(line_result.command_script_line);
            code = line_result.code;
        }
        CommandScriptResult
        {
            command_script_lines: command_script_lines,
            outputs: outputs,
            code: code,
        }
    }
}
