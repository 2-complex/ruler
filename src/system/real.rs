use crate::system::
{
    System,
    SystemError,
    CommandLineOutput
};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
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

fn to_path_buf(path: &str) -> PathBuf
{
    Path::new(".").join(path.split("/").map(|s|{s.to_string()}).collect::<PathBuf>())
}

fn to_path_str(path : &Path) -> Result<String, SystemError>
{
    let mut result = Vec::new();
    for component in path.components()
    {
        result.push(
            match component.as_os_str().to_str()
            {
                Some(s) => s.to_string(),
                None => return Err(SystemError::PathNotUnicode),
            }
        )
    }

    match result.get(0)
    {
        Some(string) =>
        {
            if string == "."
            {
                Ok(result[1..].join("/"))
            }
            else
            {
                Err(SystemError::Weird)
            }
        }
        _ => Err(SystemError::Weird)
    }
}

impl System for RealSystem
{
    type File = fs::File;

    fn open(&self, path: &str) -> Result<Self::File, SystemError>
    {
        match fs::File::open(to_path_buf(path))
        {
            Ok(file) => Ok(file),
            Err(error) => Err(convert_io_error_to_system_error(error)),
        }
    }

    fn create_file(&mut self, path: &str) -> Result<Self::File, SystemError>
    {
        match fs::File::create(to_path_buf(path))
        {
            Ok(file) => Ok(file),
            Err(error) => Err(convert_io_error_to_system_error(error)),  
        }
    }

    fn create_dir(&mut self, path: &str) -> Result<(), SystemError>
    {
        match fs::create_dir(to_path_buf(path))
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_io_error_to_system_error(error)),  
        }
    }

    fn is_file(&self, path: &str) -> bool
    {
        Path::new(&to_path_buf(path)).is_file()
    }

    fn is_dir(&self, path: &str) -> bool
    {
        Path::new(&to_path_buf(path)).is_dir()
    }

    #[cfg(test)]
    fn remove_file(&mut self, path: &str) -> Result<(), SystemError>
    {
        match fs::remove_file(to_path_buf(path))
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_io_error_to_system_error(error)),  
        }
    }

    #[cfg(test)]
    fn remove_dir(&mut self, path: &str) -> Result<(), SystemError>
    {
        match fs::remove_dir(to_path_buf(path))
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_io_error_to_system_error(error)),
        }
    }

    fn list_dir(&self, path: &str) -> Result<Vec<String>, SystemError>
    {
        let mut result = Vec::new();
        for dir_entry_opt in match fs::read_dir(to_path_buf(path))
        {
            Ok(entries) => entries,
            Err(error) =>
            {
                return Err(convert_io_error_to_system_error(error));
            },
        }
        {
            result.push(
                match dir_entry_opt
                {
                    Ok(entry) => to_path_str(&entry.path())?,
                    Err(error) =>
                    {
                        return Err(convert_io_error_to_system_error(error));
                    },
                }
            );
        }

        result.sort();
        Ok(result)
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

