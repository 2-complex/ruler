use crate::system::
{
    System,
    SystemError,
    ReadWriteError,
};

use crate::cache::
{
    SysCache,
    DownloaderCache,
    RestoreResult,
    DownloadResult,
};

use crate::system::util::get_timestamp;

use crate::ticket::
{
    TicketFactory,
    Ticket,
};
use serde::
{
    Serialize,
    Deserialize,
};
use std::fmt;
use std::time::
{
    SystemTimeError
};

pub enum FileResolution
{
    AlreadyCorrect,
    Recovered,
    Downloaded,
    NeedsRebuild,
}

pub struct TargetFileInfo
{
    pub path : String,
    pub history : TargetHistory,
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
struct TargetContentInfo
{
    pub ticket : Ticket,
    executable : bool,
}

pub enum BlobError
{
    Contradiction(Vec<usize>),
    TargetSizesDifferWeird,
}

/*  The target of a rule can be more than one file, and maybe one day, it can be a directory
    or a combination of those things.  A RuleHistory contains a map from source-ticket to this struct.
    This struct represents: whatever tickets we need to recover the target files. */
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct TargetTickets
{
    infos : Vec<TargetContentInfo>,
}

impl TargetTickets
{
    pub fn from_vec(tickets : Vec<Ticket>) -> TargetTickets
    {
        let mut infos = vec![];
        for ticket in tickets
        {
            infos.push(
                TargetContentInfo
                {
                    ticket : ticket,
                    executable : false,
                }
            );
        }

        TargetTickets{infos : infos}
    }

    /*  Takes a TargetTickets and looks at how the lists differ.

        Returns Ok if they're idendical, otherwise returns an error
        enum that indicates the way in which they differ.

        If they differ in length, that's weird, and this function returns
        BlobError::TargetSizesDifferWeird

        If they have the same length, but contain tickets that differ, a
        vector containing the indices of those tickets is returned inside a
        BlobError::Contradiction */
    pub fn compare(
        &self,
        other : TargetTickets)
    ->
    Result<(), BlobError>
    {
        let elen : usize = self.infos.len();

        if elen != other.infos.len()
        {
            Err(BlobError::TargetSizesDifferWeird)
        }
        else
        {
            let mut contradicting_indices = Vec::new();
            for i in 0..elen
            {
                if self.infos[i].ticket != other.infos[i].ticket
                {
                    contradicting_indices.push(i);
                }
            }

            if contradicting_indices.len() == 0
            {
                Ok(())
            }
            else
            {
                Err(BlobError::Contradiction(contradicting_indices))
            }
        }
    }

    fn get(
        &self,
        i : usize)
    -> Ticket
    {
        self.infos[i].ticket.clone()
    }

    /*  Currently used by a display function, hence the formatting. */
    pub fn base64(&self)
    -> String
    {
        let mut out = String::new();
        for info in self.infos.iter()
        {
            out.push_str("    ");
            out.push_str(&info.ticket.base64());
            out.push_str("\n");
        }
        out
    }

    /*  Currently used by a display function, hence the formatting. */
    pub fn download_string(&self)
    -> String
    {
        let mut out = String::new();
        for info in self.infos.iter()
        {
            out.push_str(&info.ticket.base64());
            out.push_str("\n");
        }
        out
    }
}

/*  Takes a System and a filepath as a string.

    If the file exists, returns a ticket.
    If the file does not exist, returns Ok, but with no Ticket inside
    If the file exists but does not open or some other error occurs when generating
    the ticket, returns an error. */
pub fn get_file_ticket_from_path<SystemType: System>
(
    system : &SystemType,
    path : &str
)
-> Result<Option<Ticket>, ReadWriteError>
{
    if system.is_file(&path) || system.is_dir(&path)
    {
        match TicketFactory::from_file(system, &path)
        {
            Ok(mut factory) => Ok(Some(factory.result())),
            Err(error) => Err(error),
        }
    }
    else
    {
        Ok(None)
    }
}

/*  There are two steps to checking if a target file is up-to-date.  First: check the rule-history to see what the target
    hash should be.  Second: compare the hash it should be to the hash it actually is.

    TargetHistory is a small struct meant to be the type of a value in the map 'target_histories' whose purpose is to
    help ruler tell if a target is up-to-date */
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct TargetHistory
{
    pub ticket : Ticket,
    pub timestamp : u64,
}

impl TargetHistory
{
    /*  Create a new empty TargetHistory */
    pub fn empty() -> TargetHistory
    {
        TargetHistory
        {
            ticket : TicketFactory::new().result(),
            timestamp : 0,
        }
    }

    pub fn new(
        ticket : Ticket,
        timestamp : u64) -> TargetHistory
    {
        TargetHistory
        {
            ticket : ticket,
            timestamp : timestamp,
        }
    }
}

/*  Takes a system and a TargetFileInfo, and obtains a ticket for the file described.
    If the modified date of the file matches the one in TargetHistory exactly. */
pub fn get_file_ticket<SystemType: System>
(
    system : &SystemType,
    target_info : &TargetFileInfo
)
-> Result<Option<Ticket>, ReadWriteError>
{
    /*  The body of this match looks like it has unhandled errors.  What's happening is:
        if any error occurs with the timestamp optimization, we skip the optimization. */
    match system.get_modified(&target_info.path)
    {
        Ok(system_time) =>
        {
            match get_timestamp(system_time)
            {
                Ok(timestamp) =>
                {
                    if timestamp == target_info.history.timestamp
                    {
                        return Ok(Some(target_info.history.ticket.clone()))
                    }
                },
                Err(_) => {},
            }
        },
        Err(_) => {},
    }

    get_file_ticket_from_path(system, &target_info.path)
}

pub enum GetFileTicketAndTimestampError
{
    ErrorConveratingModifiedDateToNumber(String, SystemTimeError),
    ErrorGettingTicketForFile(String, ReadWriteError),
    TargetFileNotFound(String, SystemError),
}

impl fmt::Display for GetFileTicketAndTimestampError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            GetFileTicketAndTimestampError::ErrorConveratingModifiedDateToNumber(path, error) =>
                write!(formatter, "Error converting from system time to number. File: {} Error: {}", path, error),

            GetFileTicketAndTimestampError::ErrorGettingTicketForFile(path, error) =>
                write!(formatter, "Read/write error while hashing file contents: File: {} Error: {}", path, error),

            GetFileTicketAndTimestampError::TargetFileNotFound(path, error) =>
                write!(formatter, "System error while attempting to read file: {} Error: {}", path, error),
        }
    }
}

/*  Takes a system and a TargetFileInfo, and obtains a ticket for the file described,
    and also a timestamp.

    If the modified date of the file matches the one in TargetHistory exactly, it
    doesn't bother recomputing the ticket, instead it clones the ticket from the
    target_info's history.
*/
pub fn get_file_ticket_and_timestamp<SystemType: System>
(
    system : &SystemType,
    target_info : &TargetFileInfo
)
-> Result<(Ticket, u64), GetFileTicketAndTimestampError>
{
    match system.get_modified(&target_info.path)
    {
        Ok(system_time) =>
        {
            match get_timestamp(system_time)
            {
                Ok(timestamp) =>
                {
                    if timestamp == target_info.history.timestamp
                    {
                        Ok((target_info.history.ticket.clone(), timestamp))
                    }
                    else
                    {
                        match TicketFactory::from_file(system, &target_info.path)
                        {
                            Ok(mut factory) => Ok((factory.result(), timestamp)),
                            Err(read_write_error) => Err(GetFileTicketAndTimestampError::ErrorGettingTicketForFile(
                                target_info.path.clone(),
                                read_write_error)),
                        }
                    }
                },
                Err(error) => Err(GetFileTicketAndTimestampError::ErrorConveratingModifiedDateToNumber(
                    target_info.path.clone(), error)),
            }
        },

        // Note: possibly there are other ways get_modified can fail than the file being absent.
        // Maybe this logic should change.
        Err(system_error) => Err(GetFileTicketAndTimestampError::TargetFileNotFound(target_info.path.clone(), system_error)),
    }
}

pub enum ResolutionError
{
    FileNotAvailableToCache(String, ReadWriteError),
    CacheDirectoryMissing,
    CacheMalfunction(SystemError),
    TicketAlignmentError(ReadWriteError),
}

impl fmt::Display for ResolutionError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ResolutionError::FileNotAvailableToCache(path, error) =>
                write!(formatter, "Read/write error when attempting to read file from local cache. File: {} Error: {}", path, error),

            ResolutionError::CacheDirectoryMissing =>
                write!(formatter, "Cache directory missing."),

            ResolutionError::CacheMalfunction(error) =>
                write!(formatter, "System error while attempting to use cache.  Error: {}", error),

            ResolutionError::TicketAlignmentError(error) =>
                write!(formatter, "Ticket alignment error: {}", error),
        }
    }
}

fn restore_or_download<SystemType : System>
(
    system : &mut SystemType,
    cache : &mut SysCache<SystemType>,
    downloader_cache : &DownloaderCache,
    remembered_ticket : &Ticket,
    path : &str,
)
-> Result<FileResolution, ResolutionError>
{
    match cache.restore_file(
        &remembered_ticket,
        path)
    {
        RestoreResult::Done =>
            return Ok(FileResolution::Recovered),

        RestoreResult::NotThere => {},

        RestoreResult::CacheDirectoryMissing =>
            return Err(ResolutionError::CacheDirectoryMissing),

        RestoreResult::SystemError(error) =>
            return Err(ResolutionError::CacheMalfunction(error)),
    }

    match downloader_cache.restore_file(
        &remembered_ticket,
        system,
        path)
    {
        DownloadResult::Done =>
            Ok(FileResolution::Downloaded),

        DownloadResult::NotThere =>
            Ok(FileResolution::NeedsRebuild),
    }
}

/*  Given a target-info and a remembered ticket for that target file, check the current
    ticket, and if it matches, return AlreadyCorrect.  If it doesn't match, back up the current
    file, and then attempt to restore the remembered file from cache, if the cache doesn't have it
    attempt to download.  If no recovery or download works, shrug and return NeedsRebuild */
pub fn resolve_single_target<SystemType : System>
(
    system : &mut SystemType,
    cache : &mut SysCache<SystemType>,
    downloader_cache : &DownloaderCache,
    remembered_ticket : &Ticket,
    target_info : &TargetFileInfo
)
->
Result<FileResolution, ResolutionError>
{
    match get_file_ticket(system, target_info)
    {
        Ok(Some(current_target_ticket)) =>
        {
            if *remembered_ticket == current_target_ticket
            {
                return Ok(FileResolution::AlreadyCorrect);
            }

            match cache.back_up_file_with_ticket(
                &current_target_ticket,
                &target_info.path)
            {
                Ok(_) => {},
                Err(error) =>
                {
                    return Err(ResolutionError::FileNotAvailableToCache(
                        target_info.path.clone(), error));
                },
            }

            restore_or_download(
                system,
                cache,
                downloader_cache,
                remembered_ticket,
                &target_info.path)
        },

        // None means the file is not there, in which case, we just try to restore/download, and then go home.
        Ok(None) =>
            restore_or_download(
                system,
                cache,
                downloader_cache,
                remembered_ticket,
                &target_info.path),

        Err(error) =>
            Err(ResolutionError::TicketAlignmentError(error)),
    }
}

pub fn resolve_remembered_target_tickets<SystemType : System>
(
    system : &mut SystemType,
    cache : &mut SysCache<SystemType>,
    downloader_cache : &DownloaderCache,
    target_infos : &Vec<TargetFileInfo>,
    remembered_tickets : &TargetTickets,
)
->
Result<Vec<FileResolution>, ResolutionError>
{
    let mut resolutions = vec![];
    for (i, target_info) in target_infos.iter().enumerate()
    {
        match resolve_single_target(
            system,
            cache,
            downloader_cache,
            &remembered_tickets.get(i),
            target_info)
        {
            Ok(resolution) => resolutions.push(resolution),
            Err(error) => return Err(error),
        }
    }

    Ok(resolutions)
}

pub fn resolve_with_no_memory<SystemType : System>
(
    system : &mut SystemType,
    cache : &mut SysCache<SystemType>,
    target_infos : &Vec<TargetFileInfo>,
)
->
Result<Vec<FileResolution>, ResolutionError>
{
    let mut resolutions = vec![];
    for target_info in target_infos.iter()
    {
        match get_file_ticket(system, target_info)
        {
            Ok(Some(current_target_ticket)) =>
            {
                match cache.back_up_file_with_ticket(
                    &current_target_ticket,
                    &target_info.path)
                {
                    Ok(_) =>
                    {
                        // TODO: Maybe encode whether it was cached in the FileResoluton
                        resolutions.push(FileResolution::NeedsRebuild);
                    },
                    Err(error) =>
                    {
                        return Err(
                            ResolutionError::FileNotAvailableToCache(
                                target_info.path.clone(), error));
                    }
                }
            },

            Ok(None) => resolutions.push(FileResolution::NeedsRebuild),

            Err(error) =>
                return Err(ResolutionError::TicketAlignmentError(error)),
        }
    }

    Ok(resolutions)
}


#[cfg(test)]
mod test
{
    use crate::ticket::
    {
        TicketFactory,
    };
    use crate::blob::
    {
        TargetHistory,
        TargetTickets,
        BlobError,
        TargetFileInfo,
        get_file_ticket
    };
    use crate::system::
    {
        fake::FakeSystem,
    };
    use crate::system::util::
    {
        write_str_to_file,
    };
    use crate::blob::
    {
        get_file_ticket_from_path,
    };

    /*  Use a fake system to create a file, and write a string to it.  Then use
        get_file_ticket_from_path to obtain a ticket for that file, and compare
        that against a ticket made directly from the string. */
    #[test]
    fn blob_get_file_ticket_from_path()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "quine.sh", "cat $0")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        match get_file_ticket_from_path(
            &system,
            "quine.sh")
        {
            Ok(ticket_opt) => match ticket_opt
            {
                Some(ticket) => assert_eq!(ticket, TicketFactory::from_str("cat $0").result()),
                None => panic!("Could not get ticket"),
            }
            Err(err) => panic!("Could not get ticket: {}", err),
        }
    }

    #[test]
    fn blob_compare_identical()
    {
        let a = TargetTickets::from_vec(
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result(),
                TicketFactory::from_str("Sugar is sweet\nThis is a poem\n").result(),
            ]
        );

        let b = TargetTickets::from_vec(
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result(),
                TicketFactory::from_str("Sugar is sweet\nThis is a poem\n").result(),
            ]
        );

        match a.compare(b)
        {
            Ok(_) => {},
            Err(_) => panic!("Unexpected error when comparing identical blobs"),
        }
    }

    #[test]
    fn blob_compare_mismatched_sizes()
    {
        let a = TargetTickets::from_vec(
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result(),
                TicketFactory::from_str("Sugar is sweet\nThis is a poem\n").result(),
            ]
        );

        let b = TargetTickets::from_vec(
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result(),
            ]
        );

        match a.compare(b)
        {
            Ok(_) => panic!("Unexpected success"),
            Err(BlobError::TargetSizesDifferWeird) => {},
            Err(_) => panic!("Wrong error when comparing blobs of different shapes"),
        }
    }

    #[test]
    fn blob_compare_contradiction()
    {
        let a = TargetTickets::from_vec(
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result(),
                TicketFactory::from_str("Sugar is sweet\nThis is a poem\n").result(),
            ]
        );

        let b = TargetTickets::from_vec(
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result(),
                TicketFactory::from_str("Sugar is sweet\nChicken soup\n").result(),
            ]
        );

        match a.compare(b)
        {
            Ok(_) => panic!("Unexpected success"),
            Err(BlobError::Contradiction(index_vec)) =>
            {
                assert_eq!(index_vec, vec![1]);
            },
            Err(_) => panic!("Unexpected error when comparing non-identical blobs of the same shape"),
        }
    }

    /*  Use the system to create a file, and write a string to it.  Then use get_file_ticket
        to obtain a ticket for that file, and compare that against a ticket made directly
        from the string. */
    #[test]
    fn blob_get_tickets_from_filesystem()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "quine.sh", "cat $0")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        match get_file_ticket(
            &system,
            &TargetFileInfo
            {
                path : "quine.sh".to_string(),
                history : TargetHistory
                {
                    ticket : TicketFactory::new().result(),
                    timestamp : 0,
                }
            })
        {
            Ok(ticket_opt) => match ticket_opt
            {
                Some(ticket) => assert_eq!(ticket, TicketFactory::from_str("cat $0").result()),
                None => panic!("Could not get ticket"),
            }
            Err(err) => panic!("Could not get ticket: {}", err),
        }
    }

    /*  Create a file and a TargetFileInfo for that file with matching timestamp.  Then fill the file
        with some other data.  Make sure that when we get_file_ticket, we get the one from the history
        instead of the one from the file. */
    #[test]
    fn blob_test_timestamp_optimization()
    {
        // Set the clock to 11
        let mut system = FakeSystem::new(11);

        let content = "int main(){printf(\"my game\"); return 0;}";
        let content_ticket = TicketFactory::from_str(content).result();

        // Doctor a TargetFileInfo to indicate the game.cpp was written at time 11
        let target_file_info = TargetFileInfo
        {
            path : "game.cpp".to_string(),
            history : TargetHistory
            {
                ticket : content_ticket.clone(),
                timestamp : 11,
            }
        };

        // Meanwhile, in the filesystem put some incorrect rubbish in game.cpp
        match write_str_to_file(&mut system, "game.cpp", "some rubbish")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        // Then get the ticket for the current target file, passing the TargetFileInfo
        // with timestamp 11.  Check that it gives the ticket for the C++ code.
        match get_file_ticket(
            &system,
            &target_file_info)
        {
            Ok(ticket_opt) =>
            {
                match ticket_opt
                {
                    Some(ticket) => assert_eq!(ticket, content_ticket),
                    None => panic!("Failed to generate ticket"),
                }
            },
            Err(_) => panic!("Unexpected error getting file ticket"),
        }
    }

    /*  Create a file and a TargetFileInfo for that file with not-matching timestamp.  Fill the file
        with new and improved code.  Make sure that when we get_file_ticket, we get the one from the
        file because the history doesn't match. */
    #[test]
    fn blob_test_timestamp_mismatch()
    {
        // Set the clock to 11
        let mut system = FakeSystem::new(11);

        let previous_content = "int main(){printf(\"my game\"); return 0;}";
        let previous_ticket = TicketFactory::from_str(previous_content).result();

        let current_content = "int main(){printf(\"my better game\"); return 0;}";
        let current_ticket = TicketFactory::from_str(current_content).result();

        // Doctor a TargetFileInfo to indicate the game.cpp was written at time 9
        let target_file_info = TargetFileInfo
        {
            path : "game.cpp".to_string(),
            history : TargetHistory
            {
                ticket : previous_ticket.clone(),
                timestamp : 9,
            }
        };

        // Meanwhile, in the filesystem, put new and improved game.cpp
        match write_str_to_file(&mut system, "game.cpp", current_content)
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        // Then get the ticket for the current target file, passing the TargetFileInfo
        // with timestamp 11.  Check that it gives the ticket for the C++ code.
        match get_file_ticket(
            &system,
            &target_file_info)
        {
            Ok(ticket_opt) =>
            {
                match ticket_opt
                {
                    Some(ticket) => assert_eq!(ticket, current_ticket),
                    None => panic!("Failed to generate ticket"),
                }
            },
            Err(_) => panic!("Unexpected error getting file ticket"),
        }
    }
}