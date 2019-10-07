extern crate filesystem;

use crate::ticket::Ticket;

#[cfg(test)]
use crate::ticket::TicketFactory;

use filesystem::FileSystem;
use std::io::Error;

pub enum RestoreResult
{
    Done,
    NotThere,
    CacheDirectoryMissing,
    FileSystemError(Error)
}

#[derive(Clone)]
pub struct LocalCache
{
    path : String,
}

impl LocalCache
{
    pub fn new(path : &str)
        -> LocalCache
    {
        LocalCache
        {
            path : path.to_string(),
        }
    }

    pub fn restore_file<FileSystemType : FileSystem>(&self, file_system : &FileSystemType, ticket : &Ticket, target_path : &str)
        -> RestoreResult
    {
        if file_system.is_dir(&self.path)
        {
            let cache_path = format!("{}/{}", self.path, ticket.base64());
            if file_system.is_file(&cache_path)
            {
                match file_system.rename(&cache_path, &target_path)
                {
                    Err(error) => RestoreResult::FileSystemError(error),
                    Ok(()) => RestoreResult::Done
                }
            }
            else
            {
                RestoreResult::NotThere
            }
        }
        else
        {
            RestoreResult::CacheDirectoryMissing
        }
    }

    pub fn back_up_file_with_ticket<FileSystemType : FileSystem>(&self, file_system : &FileSystemType, ticket : &Ticket, target_path : &str)
        -> Result<(), Error>
    {
        let cache_path = format!("{}/{}", self.path, ticket.base64());
        match file_system.rename(&target_path, &cache_path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }

    #[cfg(test)]
    fn back_up_file<FileSystemType : FileSystem>(&self, file_system : &FileSystemType, target_path : &str)
        -> Result<(), Error>
    {
        match TicketFactory::from_file(file_system, target_path)
        {
            Ok(mut factory) =>
            {
                self.back_up_file_with_ticket(file_system, &factory.result(), target_path)
            }
            Err(error) => Err(error)
        }
    }
}

#[cfg(test)]
mod test
{
    use filesystem::{FileSystem, FakeFileSystem};
    use crate::ticket::TicketFactory;
    use crate::cache::{LocalCache, RestoreResult};

    #[test]
    fn back_up_and_restore()
    {
        let file_system = FakeFileSystem::new();

        match file_system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        match file_system.write_file("apples.txt", "apples\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(file_system.is_file("apples.txt"));

        match cache.back_up_file(&file_system, "apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!file_system.is_file("apples.txt"));

        match cache.restore_file(&file_system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            RestoreResult::Done => {},
            RestoreResult::NotThere => panic!("Back up not there when expected"),
            RestoreResult::CacheDirectoryMissing => panic!("Cache directory missing, but we just made it"),
            RestoreResult::FileSystemError(_error) => panic!("File error in the middle of legit restore"),
        }

        assert!(file_system.is_file("apples.txt"));

        match file_system.read_file_to_string("apples.txt")
        {
            Ok(content) => assert_eq!(content, "apples\n"),
            Err(_) => panic!("Restored file was not there"),
        }
    }

    #[test]
    fn back_up_nonexistent_file()
    {
        let file_system = FakeFileSystem::new();

        match file_system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        assert!(!file_system.is_file("apples.txt"));

        match cache.back_up_file(&file_system, "apples.txt")
        {
            Ok(()) => panic!("Unexpected successful backup when file not present"),
            Err(_error) => {},
        }
    }

    #[test]
    fn restore_nonexistent_file()
    {
        let file_system = FakeFileSystem::new();

        match file_system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        match file_system.write_file("apples.txt", "wrong content\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(file_system.is_file("apples.txt"));

        match cache.back_up_file(&file_system, "apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!file_system.is_file("apples.txt"));

        match cache.restore_file(&file_system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            RestoreResult::Done => panic!("Restore reported success when no backup was made"),
            RestoreResult::NotThere => {},
            RestoreResult::CacheDirectoryMissing => panic!("Cache directory missing, but we just made it"),
            RestoreResult::FileSystemError(_error) => panic!("File error in the middle of legit restore"),
        }

        assert!(!file_system.is_file("apples.txt"));
    }

    #[test]
    fn restore_file_from_nonexistent_cache()
    {
        let file_system = FakeFileSystem::new();

        match file_system.create_dir(".wrong-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        match file_system.write_file("apples.txt", "wrong content\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(file_system.is_file("apples.txt"));

        match cache.restore_file(&file_system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            RestoreResult::Done => panic!("Restore reported success when no backup was made"),
            RestoreResult::NotThere => panic!("Backup successfully reported not to be present, when an error was expected"),
            RestoreResult::CacheDirectoryMissing => {},
            RestoreResult::FileSystemError(_error) => panic!("Error when cache was nonexistent"),
        }

        assert!(file_system.is_file("apples.txt"));
    }

    #[test]
    fn back_up_twice_and_restore()
    {
        let file_system = FakeFileSystem::new();

        match file_system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        match file_system.write_file("apples.txt", "apples\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(file_system.is_file("apples.txt"));

        match cache.back_up_file(&file_system, "apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!file_system.is_file("apples.txt"));

        match file_system.write_file("apples.txt", "apples\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(file_system.is_file("apples.txt"));

        match cache.back_up_file(&file_system, "apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!file_system.is_file("apples.txt"));

        match cache.restore_file(&file_system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            RestoreResult::Done => {},
            RestoreResult::NotThere => panic!("Back up not there when expected"),
            RestoreResult::CacheDirectoryMissing => panic!("Cache directory missing, but we just made it"),
            RestoreResult::FileSystemError(_error) => panic!("File error in the middle of legit restore"),
        }

        assert!(file_system.is_file("apples.txt"));

        match file_system.read_file_to_string("apples.txt")
        {
            Ok(content) => assert_eq!(content, "apples\n"),
            Err(_) => panic!("Restored file was not there"),
        }
    }
}
