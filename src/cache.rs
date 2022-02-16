use crate::ticket::Ticket;
use crate::ticket::TicketFactory;

use crate::system::
{
    System,
    SystemError,
    ReadWriteError,
};

use std::collections::HashMap;
use std::fmt;

pub enum CacheError
{
    NotThere,
    CacheDirectoryMissing,
    SystemError(SystemError)
}

impl fmt::Display for CacheError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            CacheError::NotThere =>
                write!(formatter, "File not there"),

            CacheError::CacheDirectoryMissing =>
                write!(formatter, "Cache directory missing"),

            CacheError::SystemError(error) =>
                write!(formatter, "System error while accessing cache: {}", error),
        }
    }
}

#[derive(Clone)]
pub struct LocalCache
{
    /*  In the filesystem, the path to the directory full of cached files */
    cache_directory_path : String,

    /*  If a file is in the filesystem, this map keeps track of where, so we can
        still retrieve it from the ticket. */
    ticket_to_path : HashMap<Ticket, String>,
}

impl LocalCache
{
    pub fn new(path : &str)
    ->
    LocalCache
    {
        LocalCache
        {
            cache_directory_path : path.to_string(),
            ticket_to_path : HashMap::new(),
        }
    }

    pub fn restore_file<SystemType : System>
    (
        &mut self,
        system : &mut SystemType,
        ticket : &Ticket,
        target_path : &str
    )
    ->
    Result<(), CacheError>
    {
        if system.is_dir(&self.cache_directory_path)
        {
            let cache_path = format!("{}/{}", self.cache_directory_path, ticket.base64());
            if system.is_file(&cache_path)
            {
                match system.rename(&cache_path, &target_path)
                {
                    Err(error) => Err(CacheError::SystemError(error)),
                    Ok(_) =>
                    {
                        self.ticket_to_path.insert(ticket.clone(), target_path.to_string());
                        Ok(())
                    }
                }
            }
            else
            {
                Err(CacheError::NotThere)
            }
        }
        else
        {
            Err(CacheError::CacheDirectoryMissing)
        }
    }

    pub fn open<SystemType : System>(
        &self,
        system : &mut SystemType,
        ticket : &Ticket
    )
    ->
    Result<SystemType::File, CacheError>
    {
        println!("opening {}", ticket);

        if system.is_dir(&self.cache_directory_path)
        {
            println!("cache_directory_path is directory");
            match self.ticket_to_path.get(ticket)
            {
                Some(path) =>
                {
                    println!("path found: {}", &path);
                    return match system.open(&path)
                    {
                        Ok(file) => Ok(file),
                        Err(system_error) => Err(CacheError::SystemError(system_error)),
                    }
                },
                None => {
                    println!("ticket to path returned none");
                },
            }

            let cache_path = format!("{}/{}", self.cache_directory_path, ticket.base64());
            if system.is_file(&cache_path)
            {
                return match system.open(&cache_path)
                {
                    Ok(file) => Ok(file),
                    Err(system_error) => Err(CacheError::SystemError(system_error)),
                }
            }

            Err(CacheError::NotThere)
        }
        else
        {
            Err(CacheError::CacheDirectoryMissing)
        }
    }

    /*  Creates a file with the given ticket (convertd to base64) as a name, and
        moves the file into that place. */
    pub fn back_up_file_with_ticket<SystemType : System>
    (
        &mut self,
        system : &mut SystemType,
        ticket : &Ticket,
        target_path : &str
    )
    ->
    Result<(), ReadWriteError>
    {
        let cache_path = format!("{}/{}", self.cache_directory_path, ticket.base64());
        match system.rename(&target_path, &cache_path)
        {
            Ok(_) =>
            {
                self.ticket_to_path.remove(ticket);
                Ok(())
            }
            Err(error) => Err(ReadWriteError::SystemError(error)),
        }
    }

    pub fn back_up_file<SystemType : System>
    (
        &mut self,
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
    use crate::cache::{LocalCache, CacheError};
    use crate::system::util::
    {
        write_str_to_file,
        read_file_to_string
    };
    use std::str::from_utf8;
    use std::io::Read;

    #[test]
    fn back_up_and_restore()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let mut cache = LocalCache::new(".ruler-cache");

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
            Ok(()) => {},
            Err(CacheError::NotThere) => panic!("Back up not there when expected"),
            Err(CacheError::CacheDirectoryMissing) => panic!("Cache directory missing, but we just made it"),
            Err(CacheError::SystemError(_error)) => panic!("File error in the middle of legit restore"),
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

        let mut cache = LocalCache::new(".ruler-cache");

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

        let mut cache = LocalCache::new(".ruler-cache");

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
            Ok(()) => panic!("Restore reported success when no backup was made"),
            Err(CacheError::NotThere) => {},
            Err(CacheError::CacheDirectoryMissing) => panic!("Cache directory missing, but we just made it"),
            Err(CacheError::SystemError(_error)) => panic!("File error in the middle of legit restore"),
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

        let mut cache = LocalCache::new(".ruler-cache");

        match write_str_to_file(&mut system, "apples.txt", "wrong content\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.restore_file(&mut system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            Ok(()) => panic!("Restore reported success when no backup was made"),
            Err(CacheError::NotThere) => panic!("Backup successfully reported not to be present, when an error was expected"),
            Err(CacheError::CacheDirectoryMissing) => {},
            Err(CacheError::SystemError(_error)) => panic!("Error when cache was nonexistent"),
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

        let mut cache = LocalCache::new(".ruler-cache");

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
            Ok(()) => {},
            Err(CacheError::NotThere) => panic!("Back up not there when expected"),
            Err(CacheError::CacheDirectoryMissing) => panic!("Cache directory missing, but we just made it"),
            Err(CacheError::SystemError(_error)) => panic!("File error in the middle of legit restore"),
        }

        assert!(system.is_file("apples.txt"));

        match read_file_to_string(&mut system, "apples.txt")
        {
            Ok(content) => assert_eq!(content, "apples\n"),
            Err(_) => panic!("Restored file was not there"),
        }
    }

    /*  Create a cache in a fake file-system.  Then create a file, and call
        cache.back_up_file().  Construct a ticket based on the known contents
        of the file, and use that ticket as the argument to cache.open().
        Check that the contents read from open() are euqal to the original
        file contents.  Then try calling open() with a phony, nonsense ticket,
        and check that it fails.

        Restore the file and then use cache.open() again with the same ticket.
        Check that open reveals the same data even though the file is now in
        the file system. */
    #[test]
    fn back_up_restore_and_open()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        let mut cache = LocalCache::new(".ruler-cache");

        match write_str_to_file(&mut system, "apples.txt", "apples\n")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }

        assert!(system.is_file("apples.txt"));

        match cache.open(&mut system, &TicketFactory::from_str("apples\n").result())
        {
            Ok(mut _file) => panic!("cache open unexpected success"),
            Err(CacheError::NotThere) => {},
            Err(cache_error) => panic!("Expeced file not there, got : {}", cache_error),
        }

        match cache.back_up_file(&mut system, "apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        match cache.open(&mut system, &TicketFactory::from_str("apples\n").result())
        {
            Ok(mut file) =>
            {
                let mut content = Vec::new();
                match file.read_to_end(&mut content)
                {
                    Ok(_size) =>
                    {
                        match from_utf8(&content)
                        {
                            Ok(text) => assert_eq!(text, "apples\n"),
                            Err(_) => panic!("File contents when read back were not even utf8"),
                        }
                    },
                    Err(error) => panic!("File didn't read error: {}", error),
                }
            },

            Err(cache_error) =>
            {
                panic!("Open failed at a time when it was supposed to succeed error: {}", cache_error)
            },
        }

        match cache.open(&mut system, &TicketFactory::from_str("pear\n").result())
        {
            Ok(_file) =>
            {
                panic!("Open succeeded when contents were wrong")
            },

            Err(_cache_error) =>
            {
            },
        }

        assert!(!system.is_file("apples.txt"));

        match cache.restore_file(&mut system, &TicketFactory::from_str("apples\n").result(), "apples.txt")
        {
            Ok(()) => {},
            Err(CacheError::NotThere) => panic!("Back up not there when expected"),
            Err(CacheError::CacheDirectoryMissing) => panic!("Cache directory missing, but we just made it"),
            Err(CacheError::SystemError(_error)) => panic!("File error in the middle of legit restore"),
        }

        match cache.open(&mut system, &TicketFactory::from_str("apples\n").result())
        {
            Ok(mut file) =>
            {
                let mut content = Vec::new();
                match file.read_to_end(&mut content)
                {
                    Ok(_size) =>
                    {
                        match from_utf8(&content)
                        {
                            Ok(text) => assert_eq!(text, "apples\n"),
                            Err(_) => panic!("File contents when read back were not even utf8"),
                        }
                    },
                    Err(error) => panic!("File didn't read error: {}", error),
                }
            },

            Err(cache_error) =>
            {
                panic!("Open failed at a time when it was supposed to succeed error: {}", cache_error)
            },
        }

        match cache.open(&mut system, &TicketFactory::from_str("pear\n").result())
        {
            Ok(_file) =>
            {
                panic!("Open succeeded when contents were wrong")
            },

            Err(_cache_error) =>
            {
            },
        }

        assert!(system.is_file("apples.txt"));

        match read_file_to_string(&mut system, "apples.txt")
        {
            Ok(content) => assert_eq!(content, "apples\n"),
            Err(_) => panic!("Restored file was not there"),
        }
    }
}
