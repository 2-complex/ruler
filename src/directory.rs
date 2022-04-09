use std::fmt;

use crate::memory::{Memory, MemoryError};
use crate::cache::LocalCache;

use crate::system::
{
    System,
    SystemError
};

pub enum InitDirectoryError
{
    FailedToCreateDirectory(SystemError),
    FailedToCreateCacheDirectory(SystemError),
    FailedToReadMemoryFile(MemoryError),
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

            InitDirectoryError::FailedToReadMemoryFile(error) =>
                write!(formatter, "Failed to read memory file: {}", error),
        }
    }
}

pub fn init<SystemType : System + Clone + Send + 'static>
(
    system : &mut SystemType,
    directory : &str
)
->
Result<(Memory, LocalCache, String), InitDirectoryError>
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

    let memoryfile = format!("{}/memory", directory);

    Ok((
        match Memory::from_file(system, &memoryfile)
        {
            Ok(memory) => memory,
            Err(error) => return Err(InitDirectoryError::FailedToReadMemoryFile(error)),
        },
        LocalCache::new(&cache_path),
        memoryfile
    ))
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

        let (mut _memory, _cache, _memoryfile) =
            match directory::init(&mut system, "ruler-directory")
            {
                Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
                Err(error) => panic!("Failed to init directory error: {}", error)
            };
    }
}
