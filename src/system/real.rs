use crate::system::
{
    System,
    SystemError,
    CommandScript,
    CommandLineOutput,
};
use std::fs;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::fmt;

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
        ErrorKind::NotFound => SystemError::NotFound,
        _ => SystemError::Weird("Convert function expects io error NotFound, nothing else".to_string()),
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

fn to_file_name_str(os_string : OsString) -> Result<String, SystemError>
{
    match os_string.to_str()
    {
        Some(s) => Ok(s.to_string()),
        None => return Err(SystemError::PathNotUnicode),
    }
}

fn get_file_name(path : &Path) -> Result<String, SystemError>
{
    match path.components().last()
    {
        Some(filename_component) => to_file_name_str(filename_component.as_os_str().to_os_string()),
        None => Err(SystemError::Weird("get_file_name expected path to have at least one component".to_string())),
    }
}

#[derive(Debug, PartialEq)]
pub enum GetTimestampError
{
    Error(String),
}

impl fmt::Display for GetTimestampError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            GetTimestampError::Error(message) =>
                write!(formatter, "Error getting timestamp: {}", message),
        }
    }
}

fn get_timestamp(system_time : SystemTime) -> Result<u64, GetTimestampError>
{
    match system_time.duration_since(SystemTime::UNIX_EPOCH)
    {
        Ok(duration) => Ok(1_000_000u64 * duration.as_secs() + u64::from(duration.subsec_micros())),
        Err(error) => Err(GetTimestampError::Error(format!("{}", error))),
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
            Err(error) =>
            {
                if error.kind() == ErrorKind::AlreadyExists
                {
                    Ok(())
                }
                else
                {
                    Err(convert_io_error_to_system_error(error))
                }
            }
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
        let path_buf = to_path_buf(path);
        if !Path::new(&path_buf).is_dir()
        {
            if Path::new(&path_buf).is_file()
            {
                return Err(SystemError::ExpectedDirFoundFile)
            }
            else
            {
                return Err(SystemError::NotFound)
            }
        }

        let mut result = Vec::new();
        for dir_entry_opt in match fs::read_dir(path_buf)
        {
            Ok(entries) => entries,
            Err(error) => return Err(convert_io_error_to_system_error(error))
        }
        {
                match dir_entry_opt
            {
                Ok(entry) =>
                {
                    let name = get_file_name(&entry.path())?;
                    if ! name.starts_with(".")
                {
                        result.push(name);
                    }
                }
                    Err(error) =>
                    {
                        return Err(convert_io_error_to_system_error(error));
                    },
                }
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

    fn get_modified(&self, path: &str) -> Result<u64, SystemError>
    {
        match fs::metadata(path)
        {
            Ok(metadata) =>
            {
                match metadata.modified()
                {
                    Ok(system_time) => 
                    {
                        match get_timestamp(system_time)
                        {
                            Ok(timestamp) => Ok(timestamp),
                            Err(_) => Err(SystemError::ModifiedInvalid),
                        }
                    },
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

    fn execute_command(&mut self, command_script : CommandScript) ->
        Vec<Result<CommandLineOutput, SystemError>>
    {
        let mut result = vec![];
        for element in command_script.lines.into_iter()
        {
            match execute::shell(element).execute_output()
            {
                Ok(output) => result.push(Ok(CommandLineOutput::from_output(output))),
                Err(error) =>
                {
                    result.push(Err(SystemError::CommandExecutationFailed(format!("{}", error))));
                    return result;
                },
            }
        }
        result
    }
}
