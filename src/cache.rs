use crate::ticket::Ticket;
use crate::ticket::TicketFactory;

use crate::system::
{
    System,
    SystemError,
    ReadWriteError,
};

pub enum RestoreResult
{
    Done,
    NotThere,
    CacheDirectoryMissing,
    SystemError(SystemError)
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

    pub fn restore_file<SystemType : System>(
        &self,
        system : &mut SystemType,
        ticket : &Ticket,
        target_path : &str
    ) -> RestoreResult
    {
        if system.is_dir(&self.path)
        {
            let cache_path = format!("{}/{}", self.path, ticket.base64());
            if system.is_file(&cache_path)
            {
                match system.rename(&cache_path, &target_path)
                {
                    Err(error) => RestoreResult::SystemError(error),
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

    /*  Creates a file with the given ticket (convertd to base64) as a name, and
        moves the file into that place. */
    pub fn back_up_file_with_ticket<SystemType : System>
    (
        &self,
        system : &mut SystemType,
        ticket : &Ticket,
        target_path : &str
    )
    ->
    Result<(), ReadWriteError>
    {
        let cache_path = format!("{}/{}", self.path, ticket.base64());
        match system.rename(&target_path, &cache_path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(ReadWriteError::SystemError(error)),
        }
    }

    pub fn back_up_file<SystemType : System>
    (
        &self,
        system : &mut SystemType,
        target_path : &str
    )
    ->
    Result<(), ReadWriteError>
    {
        match TicketFactory::from_file(system, target_path)
        {
            Ok(mut factory) =>
            {
                self.back_up_file_with_ticket(system, &factory.result(), target_path)
            }
            Err(error) => Err(error)
        }
    }
}

#[cfg(test)]
mod test
{
    use crate::system::
    {
        System,
        fake::FakeSystem
    };
    use crate::ticket::TicketFactory;
    use crate::cache::{LocalCache, RestoreResult};
    use crate::system::util::
    {
        write_str_to_file,
        read_file_to_string
    };

    #[test]
    fn back_up_and_restore()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        match write_str_to_file(&mut system, "apples.txt", "apples\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.back_up_file(&mut system, "apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!system.is_file("apples.txt"));

        match cache.restore_file(&mut system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            RestoreResult::Done => {},
            RestoreResult::NotThere => panic!("Back up not there when expected"),
            RestoreResult::CacheDirectoryMissing => panic!("Cache directory missing, but we just made it"),
            RestoreResult::SystemError(_error) => panic!("File error in the middle of legit restore"),
        }

        assert!(system.is_file("apples.txt"));

        match read_file_to_string(&mut system, "apples.txt")
        {
            Ok(content) => assert_eq!(content, "apples\n"),
            Err(_) => panic!("Restored file was not there"),
        }
    }

    #[test]
    fn back_up_nonexistent_file()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        assert!(!system.is_file("apples.txt"));

        match cache.back_up_file(&mut system, "apples.txt")
        {
            Ok(()) => panic!("Unexpected successful backup when file not present"),
            Err(_error) => {},
        }
    }

    #[test]
    fn restore_nonexistent_file()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        match write_str_to_file(&mut system, "apples.txt", "wrong content\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.back_up_file(&mut system, "apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!system.is_file("apples.txt"));

        match cache.restore_file(&mut system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            RestoreResult::Done => panic!("Restore reported success when no backup was made"),
            RestoreResult::NotThere => {},
            RestoreResult::CacheDirectoryMissing => panic!("Cache directory missing, but we just made it"),
            RestoreResult::SystemError(_error) => panic!("File error in the middle of legit restore"),
        }

        assert!(!system.is_file("apples.txt"));
    }

    #[test]
    fn restore_file_from_nonexistent_cache()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".wrong-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        match write_str_to_file(&mut system, "apples.txt", "wrong content\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.restore_file(&mut system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            RestoreResult::Done => panic!("Restore reported success when no backup was made"),
            RestoreResult::NotThere => panic!("Backup successfully reported not to be present, when an error was expected"),
            RestoreResult::CacheDirectoryMissing => {},
            RestoreResult::SystemError(_error) => panic!("Error when cache was nonexistent"),
        }

        assert!(system.is_file("apples.txt"));
    }

    #[test]
    fn back_up_twice_and_restore()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = LocalCache::new(".ruler-cache");

        match write_str_to_file(&mut system, "apples.txt", "apples\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.back_up_file(&mut system, "apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!system.is_file("apples.txt"));

        match write_str_to_file(&mut system, "apples.txt", "apples\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.back_up_file(&mut system, "apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!system.is_file("apples.txt"));

        match cache.restore_file(&mut system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            RestoreResult::Done => {},
            RestoreResult::NotThere => panic!("Back up not there when expected"),
            RestoreResult::CacheDirectoryMissing => panic!("Cache directory missing, but we just made it"),
            RestoreResult::SystemError(_error) => panic!("File error in the middle of legit restore"),
        }

        assert!(system.is_file("apples.txt"));

        match read_file_to_string(&mut system, "apples.txt")
        {
            Ok(content) => assert_eq!(content, "apples\n"),
            Err(_) => panic!("Restored file was not there"),
        }
    }
}
