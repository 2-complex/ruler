use std::boxed::Box;
use std::fmt;

#[cfg(test)]
use rand::prelude::*;

use crate::ticket::Ticket;
use crate::ticket::TicketFactory;
use crate::system::
{
    System,
    SystemError,
    ReadWriteError,
};
use crate::downloader::
{
    download_file,
};

#[derive(Debug, PartialEq)]
pub enum RestoreResult
{
    Done,
    NotThere,
    CacheDirectoryMissing,
    SystemError(SystemError)
}

pub enum DownloadResult
{
    Done,
    NotThere
}

#[derive(Debug)]
pub enum OpenError
{
    NotThere,
    CacheDirectoryMissing,
    SystemError(SystemError)
}

impl fmt::Display for OpenError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            OpenError::NotThere =>
                write!(formatter, "File not found"),

            OpenError::CacheDirectoryMissing =>
                write!(formatter, "Cache directory missing"),

            OpenError::SystemError(error) =>
                write!(formatter, "Underlying System Error: {}", error),
        }
    }
}

#[derive(Clone)]
pub struct DownloaderCache
{
    base_urls : Vec<String>,
}

impl DownloaderCache
{
    pub fn new(
        base_urls : Vec<String>
    ) -> DownloaderCache
    {
        DownloaderCache
        {
            base_urls : base_urls,
        }
    }

    pub fn restore_file<SystemType : System>(
        &self,
        ticket : &Ticket,
        system : &mut SystemType,
        target_path : &str
    ) -> DownloadResult
    {
        for base_url in &self.base_urls
        {
            match download_file(
                system, &format!("{}/{}", base_url, ticket.human_readable()), target_path)
            {
                Ok(()) => return DownloadResult::Done,
                Err(_error) => {},
            }
        }

        DownloadResult::NotThere
    }
}

pub struct InboxFile<SystemType : System>
{
    pub cache : SysCache<SystemType>,
    pub inbox_file_path : String,
    pub file : SystemType::File,
    pub ticket_factory : TicketFactory,
}

impl<SystemType : System> std::io::Write for InboxFile<SystemType>
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize>
    {
        self.ticket_factory.input_bytes(buf);
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()>
    {
        self.file.flush()
    }
}

impl<SystemType : System> InboxFile<SystemType>
{
    #[cfg(test)]
    fn finish(mut self) -> Result<(), ReadWriteError>
    {
        drop(self.file);
        self.cache.back_up_file_with_ticket(
            &self.ticket_factory.result(),
            &self.inbox_file_path)
    }
}

#[derive(Clone)]
pub struct SysCache<SystemType : System>
{
    system_box : Box<SystemType>,
    path : String,
}

#[cfg(test)]
fn random_filename() -> String
{
    const ALPHABET : [u8; 62] = [
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57,
        97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122,
        65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90
    ];

    let mut rng = rand::thread_rng();
    std::str::from_utf8(&(0..20).map(
        |_i|{ALPHABET[rng.gen_range(0..62) as usize]}).collect::<Vec<u8>>()).unwrap().to_string()
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
            let cache_path = format!("{}/{}", self.path, ticket.human_readable());
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

    pub fn open(
        &self,
        ticket : &Ticket
    ) -> Result<impl std::io::Read, OpenError>
    {
        let system = &(*self.system_box);
        if ! system.is_dir(&self.path)
        {
            return Err(OpenError::CacheDirectoryMissing);
        }

        let cache_path = format!("{}/{}", self.path, ticket.human_readable());
        if ! system.is_file(&cache_path)
        {
            return Err(OpenError::NotThere);
        }

        match system.open(&cache_path)
        {
            Ok(file) => Ok(file),
            Err(system_error) => Err(OpenError::SystemError(system_error)),
        }
    }

    #[cfg(test)]
    pub fn open_inbox_file(&mut self) -> Result<InboxFile<SystemType>, OpenError>
    {
        let system = &mut (*self.system_box);
        if ! system.is_dir(&self.path)
        {
            return Err(OpenError::CacheDirectoryMissing);
        }

        match system.create_dir(&format!("{}/inbox", self.path))
        {
            Ok(()) => {},
            Err(system_error) => return Err(OpenError::SystemError(system_error)),
        }

        let inbox_file_path = loop
        {
            let inbox_file_path = format!("{}/inbox/{}", self.path, random_filename());
            if ! system.is_file(&inbox_file_path)
            {
                break inbox_file_path;
            }
        };

        let file = match system.create_file(&inbox_file_path)
        {
            Ok(file) => file,
            Err(system_error) => return Err(OpenError::SystemError(system_error)),
        };

        Ok(InboxFile
        {
            cache : self.clone(),
            inbox_file_path : inbox_file_path,
            file : file,
            ticket_factory : TicketFactory::new(),
        })
    }

    /*  Creates a file with the given ticket (convertd to human_readable) as a name, and
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
        let cache_path = format!("{}/{}", self.path, ticket.human_readable());
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
    Result<Ticket, ReadWriteError>
    {
        let system = &mut (*self.system_box);
        let ticket = TicketFactory::from_file(system, target_path)?.result();
        self.back_up_file_with_ticket(&ticket, target_path)?;
        Ok(ticket)
    }
}

#[cfg(test)]
mod test
{
    use crate::cache::
    {
        SysCache,
        RestoreResult,
        OpenError,
    };
    use crate::system::
    {
        System,
        fake::FakeSystem
    };
    use crate::ticket::TicketFactory;
    use crate::system::util::
    {
        write_str_to_file,
        read_file_to_string,
        file_to_string,
    };
    use std::io::Write;

    fn make_fake_system_and_cache() -> (FakeSystem, SysCache<FakeSystem>)
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("files").unwrap();

        let cache = SysCache::new(system.clone(), "files");
        (system, cache)
    }

    #[test]
    fn back_up_and_restore()
    {
        let (mut system, mut cache) = make_fake_system_and_cache();
        write_str_to_file(&mut system, "apples.txt", "apples\n").unwrap();

        match cache.back_up_file("apples.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Backup failed unexpectedly: {}", error),
        }

        assert!(!system.is_file("apples.txt"));
        assert_eq!(
            cache.restore_file(&TicketFactory::from_str("apples\n").result(), "apples.txt"),
            RestoreResult::Done);

        assert!(system.is_file("apples.txt"));

        assert_eq!(read_file_to_string(&mut system, "apples.txt").unwrap(), "apples\n");
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

    #[test]
    fn back_up_and_reopen()
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

        let mut file =
        match cache.open(&TicketFactory::from_str("apples\n").result())
        {
            Ok(file) => file,
            Err(OpenError::NotThere) => panic!("Back up not there when expected"),
            Err(OpenError::CacheDirectoryMissing) => panic!("Cache directory missing, but we just made it"),
            Err(OpenError::SystemError(error)) => panic!("File error in the middle of legit reopen: {}", error),
        };

        match file_to_string(&mut file)
        {
            Ok(content) => assert_eq!(content, "apples\n"),
            Err(error) => panic!("Reopened file failed to read into string: {}", error),
        }
    }

    #[test]
    fn open_nonexistent_file()
    {
        let (mut _system, cache) = make_fake_system_and_cache();
        match cache.open(&TicketFactory::from_str("apples\n").result())
        {
            Ok(_file) => panic!("File present when no file was expected"),
            Err(OpenError::NotThere) => {},
            Err(OpenError::CacheDirectoryMissing) => panic!("Cache directory missing, but we just made it"),
            Err(OpenError::SystemError(error)) => panic!("File error in the middle of legit reopen: {}", error),
        }
    }

    #[test]
    fn open_file_with_directory_not_there()
    {
        let cache = SysCache::new(FakeSystem::new(11), "files");
        match cache.open(&TicketFactory::from_str("apples\n").result())
        {
            Err(OpenError::CacheDirectoryMissing) => {},
            _=> panic!("unexpected result"),
        }
    }

    #[test]
    fn open_inbox_file_with_directory_not_there()
    {
        let mut cache = SysCache::new(FakeSystem::new(12), "files");
        match cache.open_inbox_file()
        {
            Err(OpenError::CacheDirectoryMissing) => {},
            _=> panic!("unexpected result"),
        }
    }

    #[test]
    fn open_inbox_file_with_errant_inbox_file()
    {
        let mut system = FakeSystem::new(13);
        system.create_dir("cache-dir").unwrap();
        system.create_file("cache-dir/inbox").unwrap();

        let mut cache = SysCache::new(system, "cache-dir");
        match cache.open_inbox_file()
        {
            Err(OpenError::SystemError(_system_error)) => {},
            _=> panic!("unexpected result"),
        }
    }

    #[test]
    fn open_inbox_file_directory_missing()
    {
        let system = FakeSystem::new(14);
        let mut cache = SysCache::new(system, "cache-dir");
        match cache.open_inbox_file()
        {
            Err(OpenError::CacheDirectoryMissing) => {},
            _ => panic!("unexpected result"),
        }
    }

    #[test]
    fn open_inbox_file_and_write()
    {
        let mut system = FakeSystem::new(14);
        system.create_dir("cache-dir").unwrap();
        let mut cache = SysCache::new(system, "cache-dir");
        let mut file = cache.open_inbox_file().unwrap();
        assert_eq!(file.write(&[1u8, 2, 3]).unwrap(), 3usize);
    }

    #[test]
    fn open_inbox_file_write_and_read()
    {
        let mut system = FakeSystem::new(14);
        system.create_dir("cache-dir").unwrap();
        let mut cache = SysCache::new(system, "cache-dir");
        let mut writing_file = cache.open_inbox_file().unwrap();
        assert_eq!(writing_file.write("abc".as_bytes()).unwrap(), 3usize);
        writing_file.finish().unwrap();

        let mut reading_file = cache.open(&TicketFactory::from_str("abc").result()).unwrap();
        assert_eq!(file_to_string(&mut reading_file).unwrap(), "abc".to_string());
    }
}
