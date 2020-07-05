use crate::system::
{
    System,
    SystemError,
    CommandLineOutput
};
use std::collections::VecDeque;
use std::process::Command;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::SystemTime;


#[derive(Debug, Clone)]
pub struct RealSystem
{
}

impl RealSystem
{
    pub fn new() -> Self
    {
        RealSystem{}
    }
}

fn convert_io_error_to_system_error(error : std::io::Error) -> SystemError
{
    match error.kind()
    {
        ErrorKind::NotFound
            => SystemError::NotFound,

        _ => SystemError::Weird,
    }
}


impl System for RealSystem
{
    type File = fs::File;

    fn open(&self, path: &str) -> Result<Self::File, SystemError>
    {
        match fs::File::open(path)
        {
            Ok(file) => Ok(file),
            Err(error) => Err(convert_io_error_to_system_error(error)),
        }
    }

    fn create_file(&mut self, path: &str) -> Result<Self::File, SystemError>
    {
        match fs::File::create(path)
        {
            Ok(file) => Ok(file),
            Err(error) => Err(convert_io_error_to_system_error(error)),  
        }
    }

    fn create_dir(&mut self, path: &str) -> Result<(), SystemError>
    {
        match fs::create_dir(path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_io_error_to_system_error(error)),  
        }
    }

    fn is_file(&self, path: &str) -> bool
    {
        Path::new(path).is_file()
    }

    fn is_dir(&self, path: &str) -> bool
    {
        Path::new(path).is_dir()
    }

    fn remove_file(&mut self, path: &str) -> Result<(), SystemError>
    {
        match fs::remove_file(path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_io_error_to_system_error(error)),  
        }
    }

    fn remove_dir(&mut self, path: &str) -> Result<(), SystemError>
    {
        match fs::remove_dir(path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_io_error_to_system_error(error)),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), SystemError>
    {
        match fs::rename(from, to)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_io_error_to_system_error(error)),
        }
    }

    fn get_modified(&self, path: &str) -> Result<SystemTime, SystemError>
    {
        match fs::metadata(path)
        {
            Ok(metadata) =>
            {
                match metadata.modified()
                {
                    Ok(timestamp) => Ok(timestamp),
                    Err(_) => Err(SystemError::ModifiedNotFound)
                }
            },
            Err(_) => Err(SystemError::MetadataNotFound)
        }
    }

    fn execute_command(&mut self, command_list: Vec<String>) ->
        Result<CommandLineOutput, SystemError>
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
                    Err(_error) => Err(SystemError::CommandExecutationFailed),
                }
            },
            None => Ok(CommandLineOutput::new()),
        }
    }
}