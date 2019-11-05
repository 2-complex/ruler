use std::str::from_utf8;
use std::process::Output;
use std::collections::VecDeque;
use std::process::Command;

#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use filesystem::{FileSystem, FakeFileSystem};

pub struct CommandLineOutput
{
    pub out : String,
    pub err : String,
    pub code : Option<i32>,
    pub success : bool,
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

pub trait Executor
{
    fn execute_command(&self, command_list: Vec<String>) -> Result<CommandLineOutput, String>;
}

#[derive(Clone)]
#[cfg(test)]
pub struct FakeExecutor
{
    file_system: FakeFileSystem
}

#[cfg(test)]
impl FakeExecutor
{
    pub fn new(file_system: FakeFileSystem) -> FakeExecutor
    {
        FakeExecutor
        {
            file_system: file_system
        }
    }
}

#[derive(Clone)]
pub struct OsExecutor
{
}

impl OsExecutor
{
    pub fn new() -> OsExecutor
    {
        OsExecutor{}
    }
}

impl Executor for OsExecutor
{
    fn execute_command(&self, command_list: Vec<String>) -> Result<CommandLineOutput, String>
    {
        let mut command_queue = VecDeque::from(command_list);
        let command_opt = match command_queue.pop_front()
        {
            Some(first) =>
            {
                let mut command = Command::new(first);
                while let Some(argument) = command_queue.pop_front()
                {
                    command.arg(argument);
                }
                Some(command)
            },
            None => None
        };

        match command_opt
        {
            Some(mut command) =>
            {
                match command.output()
                {
                    Ok(out) => Ok(CommandLineOutput::from_output(out)),
                    Err(why) => Err(why.to_string()),
                }
            },
            None => Ok(CommandLineOutput::new()),
        }
    }
}

#[cfg(test)]
impl Executor for FakeExecutor
{
    fn execute_command(&self, command_list : Vec<String>) -> Result<CommandLineOutput, String>
    {
        let n = command_list.len();
        let mut output = String::new();

        if n > 0
        {
            match command_list[0].as_str()
            {
                "error" =>
                {
                    Ok(CommandLineOutput::error("Failed".to_string()))
                },

                "mycat" =>
                {
                    for file in command_list[1..(n-1)].iter()
                    {
                        match self.file_system.read_file(file)
                        {
                            Ok(content) =>
                            {
                                match from_utf8(&content)
                                {
                                    Ok(content_string) =>
                                    {
                                        output.push_str(content_string);
                                    }
                                    Err(_) => return Err(format!("File contained non utf8 bytes: {}", file)),
                                }
                            }
                            Err(_) =>
                            {
                                return Err(format!("File failed to open: {}", file));
                            }
                        }
                    }

                    match self.file_system.write_file(Path::new(&command_list[n-1]), output)
                    {
                        Ok(_) => Ok(CommandLineOutput::new()),
                        Err(why) =>
                        {
                            Err(format!("Filed to cat into file: {}: {}", command_list[n-1], why))
                        }
                    }
                },

                "rm" =>
                {
                    for file in command_list[1..n].iter()
                    {
                        match self.file_system.remove_file(file)
                        {
                            Ok(()) => {}
                            Err(_) =>
                            {
                                return Err(format!("File failed to delete: {}", file));
                            }
                        }
                    }

                    Ok(CommandLineOutput::new())
                },
                _=> Err(format!("Non command given: {}", command_list[0]))
            }
        }
        else
        {
            Ok(CommandLineOutput::new())
        }
    }
}
