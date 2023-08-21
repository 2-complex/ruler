use std::fmt;

use crate::current::
{
    CurrentFileStates,
    CurrentFileStatesError,
};
use crate::history::
{
    History,
};
use crate::cache::
{
    SysCache,
};

use crate::system::
{
    System,
    SystemError
};

#[derive(Debug)]
pub enum InitDirectoryError
{
    FailedToCreateDirectory(SystemError),
    FailedToCreateCacheDirectory(SystemError),
    FailedToCreateHistoryDirectory(SystemError),
    FailedToReadCurrentFileStates(CurrentFileStatesError),
}

impl fmt::Display for InitDirectoryError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            InitDirectoryError::FailedToCreateDirectory(error) =>
                write!(formatter, "Failed to create directory: {}", error),

            InitDirectoryError::FailedToCreateCacheDirectory(error) =>
                write!(formatter, "Failed to create cache directory: {}", error),

            InitDirectoryError::FailedToCreateHistoryDirectory(error) =>
                write!(formatter, "Failed to create history directory: {}", error),

            InitDirectoryError::FailedToReadCurrentFileStates(error) =>
                write!(formatter, "Failed to read current_file_states file: {}", error),
        }
    }
}

pub fn init<SystemType : System>
(
    system : &mut SystemType,
    directory : &str
)
-> Result<Elements<SystemType>, InitDirectoryError>
{
    if ! system.is_dir(directory)
    {
        match system.create_dir(directory)
        {
            Ok(_) => {},
            Err(error) => return Err(InitDirectoryError::FailedToCreateDirectory(error)),
        }
    }

    let cache_path = format!("{}/cache", directory);

    if ! system.is_dir(&cache_path)
    {
        match system.create_dir(&cache_path)
        {
            Ok(_) => {},
            Err(error) => return Err(InitDirectoryError::FailedToCreateCacheDirectory(error)),
        }
    }

    let history_path = format!("{}/history", directory);

    if ! system.is_dir(&history_path)
    {
        match system.create_dir(&history_path)
        {
            Ok(_) => {},
            Err(error) => return Err(InitDirectoryError::FailedToCreateHistoryDirectory(error)),
        }
    }

    let current_file_statesfile = format!("{}/current_file_states", directory);

    Ok(Elements
    {
        current_file_states : match CurrentFileStates::from_file(system.clone(), current_file_statesfile)
        {
            Ok(current_file_states) => current_file_states,
            Err(error) => return Err(InitDirectoryError::FailedToReadCurrentFileStates(error)),
        },
        cache : SysCache::new(system.clone(), &cache_path),
        history : History::new(system.clone(), &history_path),
    })
}

pub struct Elements<SystemType : System>
{
    pub current_file_states : CurrentFileStates<SystemType>,
    pub cache : SysCache<SystemType>,
    pub history : History<SystemType>,
}

#[cfg(test)]
mod test
{
    use crate::directory;
    use crate::system::
    {
        fake::FakeSystem
    };

    #[test]
    fn build_basic()
    {
        let mut system = FakeSystem::new(180);

        let _elements =
            match directory::init(&mut system, "ruler-directory")
            {
                Ok(elements) => elements,
                Err(error) => panic!("Failed to init directory error: {}", error)
            };
    }
}
