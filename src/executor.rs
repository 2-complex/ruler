use std::str::from_utf8;
use std::collections::VecDeque;
use std::process::Command;
use crate::system::CommandLineOutput;
#[cfg(test)]
use crate::file::
{
    write_str_to_file,
    read_file
};

#[cfg(test)]
use file_objects_rs::{FileSystem, FakeFileSystem};

pub trait Executor
{
    fn execute_command(&mut self, command_list: Vec<String>) -> Result<CommandLineOutput, String>;
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
        OsExecutor
        {
        }
    }
}

impl Executor for OsExecutor
{
    fn execute_command(&mut self, command_list: Vec<String>) -> Result<CommandLineOutput, String>
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
    fn execute_command(&mut self, command_list : Vec<String>) -> Result<CommandLineOutput, String>
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
                        match read_file(&self.file_system, file)
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

                    match write_str_to_file(&mut self.file_system, &command_list[n-1], &output)
                    {
                        Ok(_) => Ok(CommandLineOutput::new()),
                        Err(why) =>
                        {
                            Err(format!("Filed to cat into file: {}: {}", command_list[n-1], why))
                        }
                    }
                },

                /*  Takes source files followed by two targets, concats the sources and puts the result in both the
                    targets.  For instance:

                    mycat2 in1.txt in2.txt out1.txt out2.txt

                    concatinates in1.txt in2.txt  puts a copy in out1.txt and out2.txt.*/
                "mycat2" =>
                {
                    for file in command_list[1..(n-2)].iter()
                    {
                        match read_file(&self.file_system, file)
                        {
                            Ok(content) =>
                            {
                                match from_utf8(&content)
                                {
                                    Ok(content_string) =>
                                    {
                                        output.push_str(content_string);
                                    }
                                    Err(_) => return Err(format!("mycat2: file contained non utf8 bytes: {}", file)),
                                }
                            }
                            Err(_) =>
                            {
                                return Err(format!("mycat2: file failed to open: {}", file));
                            }
                        }
                    }

                    match write_str_to_file(&mut self.file_system, &command_list[n-2], &output)
                    {
                        Ok(_) => {},
                        Err(why) =>
                        {
                            return Err(format!("mycat2: failed to cat into file: {}: {}", command_list[n-2], why));
                        }
                    }

                    match write_str_to_file(&mut self.file_system, &command_list[n-1], &output)
                    {
                        Ok(_) => Ok(CommandLineOutput::new()),
                        Err(why) =>
                        {
                            Err(format!("mycat2: failed to cat into file: {}: {}", command_list[n-1], why))
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
