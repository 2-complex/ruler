use std::boxed::Box;

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

pub enum OpenError
{
    NotThere,
    CacheDirectoryMissing,
    SystemError(SystemError)
}

#[derive(Clone)]
pub struct SysCache<SystemType : System>
{
    system_box : Box<SystemType>,
    path : String,
}

impl<SystemType : System> SysCache<SystemType>
{
    pub fn new(system : SystemType, path : &str)
    -> SysCache<SystemType>
    {
        SysCache
        {
            system_box : Box::new(system),
            path : path.to_string(),
        }
    }

    pub fn restore_file(
        &mut self,
        ticket : &Ticket,
        target_path : &str
    ) -> RestoreResult
    {
        let system = &mut (*self.system_box);
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

    pub fn open<SystemType : System>(
        &self,
        system : &mut SystemType,
        ticket : &Ticket
    ) -> Result<SystemType::File, OpenError>
    {
        if system.is_dir(&self.path)
        {
            let cache_path = format!("{}/{}", self.path, ticket.base64());
            if system.is_file(&cache_path)
            {
                match system.open(&cache_path)
                {
                    Ok(file) => Ok(file),
                    Err(system_error) => Err(OpenError::SystemError(system_error)),
                }
            }
            else
            {
                Err(OpenError::NotThere)
            }
        }
        else
        {
            Err(OpenError::CacheDirectoryMissing)
        }
    }

    /*  Creates a file with the given ticket (convertd to base64) as a name, and
        moves the file into that place. */
    pub fn back_up_file_with_ticket
    (
        &mut self,
        ticket : &Ticket,
        target_path : &str
    )
    ->
    Result<(), ReadWriteError>
    {
        let system = &mut (*self.system_box);
        let cache_path = format!("{}/{}", self.path, ticket.base64());
        match system.rename(&target_path, &cache_path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(ReadWriteError::SystemError(error)),
        }
    }

    pub fn back_up_file
    (
        &mut self,
        target_path : &str
    )
    ->
    Result<(), ReadWriteError>
    {
        let system = &mut (*self.system_box);
        match TicketFactory::from_file(system, target_path)
        {
            Ok(mut factory) =>
            {
                self.back_up_file_with_ticket(&mut factory.result(), target_path)
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
    use crate::cache::
    {
        SysCache,
        RestoreResult
    };
    use crate::system::util::
    {
        write_str_to_file,
        read_file_to_string
    };

    fn make_fake_system_and_cache() -> (FakeSystem, SysCache<FakeSystem>)
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir("files")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let cache = SysCache::new(system.clone(), "files");
        (system, cache)
    }

    #[test]
    fn back_up_and_restore()
    {
        let (mut system, mut cache) = make_fake_system_and_cache();

        match write_str_to_file(&mut system, "apples.txt", "apples\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.back_up_file("apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!system.is_file("apples.txt"));

        match cache.restore_file(&TicketFactory::from_str("apples\n").result(), "apples.txt")
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
        let (system, mut cache) = make_fake_system_and_cache();
        assert!(!system.is_file("apples.txt"));

        match cache.back_up_file("apples.txt")
        {
            Ok(()) => panic!("Unexpected successful backup when file not present"),
            Err(_error) => {},
        }
    }

    #[test]
    fn restore_nonexistent_file()
    {
        let (mut system, mut cache) = make_fake_system_and_cache();

        match write_str_to_file(&mut system, "apples.txt", "wrong content\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.back_up_file("apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!system.is_file("apples.txt"));

        match cache.restore_file(&TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            RestoreResult::Done => panic!("Restore reported success when no backup was made"),
            RestoreResult::NotThere => {},
            RestoreResult::CacheDirectoryMissing => panic!("Cache directory missing, but we just made it"),
            RestoreResult::SystemError(_error) => panic!("File error in the middle of legit restore"),
        }

        assert!(!system.is_file("apples.txt"));
    }

    #[test]
    fn back_up_twice_and_restore()
    {
        let (mut system, mut cache) = make_fake_system_and_cache();

        match write_str_to_file(&mut system, "apples.txt", "apples\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.back_up_file("apples.txt")
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

        match cache.back_up_file("apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!system.is_file("apples.txt"));

        match cache.restore_file(&TicketFactory::from_str("apples\n").result(), "apples.txt")
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
