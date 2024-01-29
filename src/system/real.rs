use crate::system::
{
    System,
    SystemError,
    CommandLineOutput
};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::SystemTime;

use execute::Execute;

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

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
pub fn is_executable(path: &str) -> Result<bool, SystemError>
{
    match fs::metadata(path)
    {
        Ok(metadata) => Ok(metadata.permissions().mode() & 0o111 != 0),
        Err(_) => Err(SystemError::MetadataNotFound),
    }
}

#[cfg(unix)]
pub fn set_is_executable(path: &str, executable : bool) -> Result<(), SystemError>
{
    match fs::metadata(path)
    {
        Ok(metadata) =>
        {
            let m = metadata.permissions().mode();
            if executable
            {
                fs::set_permissions(path, fs::Permissions::from_mode(m | 0o111)).unwrap();
            }
            else
            {
                fs::set_permissions(path, fs::Permissions::from_mode(m - (m & 0o111))).unwrap();
            }
            Ok(())
        }
        Err(_) => Err(SystemError::MetadataNotFound),
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

    fn list_dir(&mut self, path: &str) -> Result<Vec<String>, SystemError>
    {
        vec![]
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

    fn is_executable(&self, path: &str) -> Result<bool, SystemError>
    {
        is_executable(path)
    }

    fn set_is_executable(&mut self, path: &str, executable : bool) -> Result<(), SystemError>
    {
        set_is_executable(path, executable)
    }

    fn execute_command(&mut self, mut all_lines: Vec<String>) ->
        Result<CommandLineOutput, SystemError>
    {
        let mut command_lines = vec![];
        let mut result = Err(SystemError::CommandExecutationFailed("".to_string()));

        for line in all_lines.drain(..)
        {
            match line.as_ref()
            {
                ";" =>
                {
                    let mut cmd = execute::shell(command_lines.join(" "));
                    match cmd.execute_output()
                    {
                        Ok(output) =>
                        {
                            result = Ok(CommandLineOutput::from_output(output))
                        },

                        Err(error) => return Err(SystemError::CommandExecutationFailed(format!("{}", error))),
                    }
                    command_lines = vec![];
                }
                _ =>
                {
                    command_lines.push(line);
                }
            }
        }

        if command_lines.len() != 0
        {
            let mut cmd = execute::shell(command_lines.join(" "));
            match cmd.execute_output()
            {
                Ok(output) =>
                {
                    result = Ok(CommandLineOutput::from_output(output))
                },

                Err(error) => return Err(SystemError::CommandExecutationFailed(format!("{}", error))),
            }
        }

        result
    }
}

