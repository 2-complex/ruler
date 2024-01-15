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

#[derive(Debug)]
pub enum FileResolution
{
    AlreadyCorrect,
    Recovered,
    Downloaded,
    NeedsRebuild,
}

/*  The data in FileState are things which would follow the file if it were renamed/moved.  There's a ticket
    representing the file's contents, a timestamp (modifed date), and a bool for whether the file is executable.
    Those things would follow the file in a rename/move operation. */
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct FileState
{
    pub ticket : Ticket,
    pub timestamp : u64,
    pub executable : bool,
}

impl FileState
{
    /*  Create a new empty FileState */
    pub fn empty() -> FileState
    {
        FileState
        {
            ticket : TicketFactory::new().result(),
            timestamp : 0,
            executable : false,
        }
    }

    #[cfg(test)]
    pub fn new(
        ticket : Ticket,
        timestamp : u64) -> FileState
    {
        FileState
        {
            ticket : ticket,
            timestamp : timestamp,
            executable : false,
        }
    }

    #[cfg(test)]
    pub fn new_with_ticket(
        ticket : Ticket) -> FileState
    {
        FileState
        {
            ticket : ticket,
            timestamp : 0,
            executable : false,
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct FileInfo
{
    pub path : String,
    pub file_state : FileState,
}

#[derive(Debug)]
pub enum GetTicketsError
{
    FileNotFound(String),
    ReadWriteError(String, ReadWriteError),
}

#[derive(Debug)]
pub enum BlobError
{
    Contradiction(Vec<usize>),
    TargetSizesDifferWeird,
}

#[derive(Debug, PartialEq)]
pub struct Blob
{
    file_infos : Vec<FileInfo>
}

impl Blob
{
    pub fn get_paths
    (
        self : &Self
    )
    -> Vec<String>
    {
        return self.file_infos.iter().map(|f|{return f.path.clone()}).collect();
    }

    pub fn empty()
    -> Blob
    {
        Blob
        { 
            file_infos : vec![]
        }
    }

    pub fn get_current_file_state_vec<SystemType: System>
    (
        self : &Self,
        system : &SystemType,
    )
    -> Result<FileStateVec, GetTicketsError>
    {
        let mut tickets = vec![];
        for target_info in self.file_infos.iter()
        {
            match get_file_ticket(system, &target_info.path, &target_info.file_state)
            {
                Ok(ticket_opt) =>
                {
                    match ticket_opt
                    {
                        Some(ticket) => tickets.push(ticket),
                        None => return Err(GetTicketsError::FileNotFound(target_info.path.clone())),
                    }
                },
                Err(error) => return Err(GetTicketsError::ReadWriteError(target_info.path.clone(), error)),
            }
        }

        return Ok(
            FileStateVec::from_ticket_vec(tickets.iter().map(|ticket| ticket.clone()).collect())
        );
    }

    /*  Takes a system, and updates the file contents in the blob to reflect the files in the system.
        Returns a vector of FileStates FileState object which is current according to the file system. */
    pub fn update_to_match_system_file_state<SystemType: System>
    (
        self : &mut Self,
        system : &SystemType
    )
    -> Result<FileStateVec, GetCurrentFileInfoError>
    {
        let mut infos = vec![];
        for target_info in self.file_infos.iter_mut()
        {
            match get_actual_file_state(system, &target_info.path, &target_info.file_state)
            {
                Ok(current_info) =>
                {
                    target_info.file_state = current_info.clone();
                    infos.push(
                        FileState
                        {
                            ticket : current_info.ticket,
                            timestamp : 0,
                            executable : current_info.executable,
                        });
                },
                Err(error) => return Err(error),
            }
        }

        return Ok(
            FileStateVec::from_ticket_vec(infos.iter().map(|info| info.ticket.clone()).collect())
        );
    }

    pub fn get_file_infos
    (
        self : &Self
    )
    -> Vec<FileInfo>
    {
        return self.file_infos.clone();
    }

    /*  Takes a vector of paths, and a function mapping path to FileState.  Populates
        the Blob with the paths in the vector, with FileStates returnd by the function.
        The format of this function might be unusual, but it covers all the use-cases. */
    pub fn from_paths
    (
        paths : Vec<String>,
        mut get_state : impl FnMut(&str) -> FileState
    ) -> Self
    {
        Blob{file_infos : paths.into_iter().map(|path|
            {
                FileInfo
                {
                    file_state : get_state(&path),
                    path : path,
                }
            }
        ).collect()}
    }

    pub fn resolve_remembered_target_tickets<SystemType : System>
    (
        self : &Self,
        system : &mut SystemType,
        cache : &mut SysCache<SystemType>,
        downloader_cache_opt : &Option<DownloaderCache>,
        remembered_tickets : &FileStateVec,
    )
    ->
    Result<Vec<FileResolution>, ResolutionError>
    {
        let mut resolutions = vec![];
        for (i, info) in self.file_infos.iter().enumerate()
        {
            match resolve_single_target(
                system,
                cache,
                downloader_cache_opt,
                &remembered_tickets.get_info(i),
                info)
            {
                Ok(resolution) => resolutions.push(resolution),
                Err(error) => return Err(error),
            }
        }

        Ok(resolutions)
    }

    pub fn resolve_with_no_current_file_states<SystemType : System>
    (
        self : &Blob,
        system : &mut SystemType,
        cache : &mut SysCache<SystemType>,
    )
    ->
    Result<Vec<FileResolution>, ResolutionError>
    {
        let mut resolutions = vec![];
        for file_info in self.file_infos.iter()
        {
            match get_file_ticket(system, &file_info.path, &file_info.file_state)
            {
                Ok(Some(current_target_ticket)) =>
                {
                    match cache.back_up_file_with_ticket(
                        &current_target_ticket,
                        &file_info.path)
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
                                    file_info.path.clone(), error));
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

}

#[derive(Debug)]
pub enum FileStateVecParseError
{
    NotProperBase64,
}

/*  The target of a rule can be more than one file, and maybe one day, it can be a directory
    or a combination of those things.  A RuleHistory contains a map from source-ticket to this struct.
    This struct represents: whatever tickets we need to recover the target files. */
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct FileStateVec
{
    infos : Vec<FileState>,
}

impl FileStateVec
{
    pub fn from_ticket_vec(tickets : Vec<Ticket>) -> FileStateVec
    {
        let mut infos = vec![];
        for ticket in tickets
        {
            infos.push(
                FileState
                {
                    ticket : ticket,
                    timestamp : 0,
                    executable : false,
                }
            );
        }

        FileStateVec{infos : infos}
    }

    pub fn from_download_string(download_string : &str)
        -> Result<FileStateVec, FileStateVecParseError>
    {
        let mut tickets = vec![];
        for part in download_string.split("\n")
        {
            tickets.push(match Ticket::from_base64(part)
            {
                Ok(ticket) => ticket,
                Err(_) => return Err(
                    FileStateVecParseError::NotProperBase64),
            });
        }
        Ok(FileStateVec::from_ticket_vec(tickets))
    }

    /*  Takes a FileStateVec and looks at how the lists differ.

        Returns Ok if they're idendical, otherwise returns an error
        enum that indicates the way in which they differ.

        If they differ in length, that's weird, and this function returns
        BlobError::TargetSizesDifferWeird

        If they have the same length, but contain tickets that differ, a
        vector containing the indices of those tickets is returned inside a
        BlobError::Contradiction */
    pub fn compare(
        &self,
        other : FileStateVec)
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

    fn get_info(
        &self,
        i : usize)
    -> FileState
    {
        self.infos[i].clone()
    }

    pub fn get_ticket(&self, sub_index : usize) -> Ticket
    {
        self.infos[sub_index].ticket.clone()
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
        self.infos.iter().map(|info|{info.ticket.base64()}).collect::<Vec<String>>().join("\n")
    }
}

/*  Takes a System and a filepath as a string.

    If the file exists, returns a ticket.
    If the file does not exist, returns Ok, but with no Ticket inside
    If the file exists but does not open or some other error occurs when generating
    the ticket, returns an error. */
fn get_file_ticket_from_path<SystemType: System>
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

/*  Takes a system, a path, and an assumed FileState, obtains a ticket for the file described.
    If the modified date of the file matches the one in FileState exactly, this function
    assumes the ticket matches.  This is part of the timestamp optimization. */
pub fn get_file_ticket<SystemType: System>
(
    system : &SystemType,
    path : &str,
    assumed_file_state : &FileState,
)
-> Result<Option<Ticket>, ReadWriteError>
{
    /*  The body of this match looks like it has unhandled errors.  What's happening is:
        if any error occurs with the timestamp optimization, we skip the optimization. */
    match system.get_modified(&path)
    {
        Ok(system_time) =>
        {
            match get_timestamp(system_time)
            {
                Ok(timestamp) =>
                {
                    if timestamp == assumed_file_state.timestamp
                    {
                        return Ok(Some(assumed_file_state.ticket.clone()))
                    }
                },
                Err(_) => {},
            }
        },
        Err(_) => {},
    }

    get_file_ticket_from_path(system, path)
}

#[derive(Debug)]
pub enum GetCurrentFileInfoError
{
    ErrorConveratingModifiedDateToNumber(String, SystemTimeError),
    ErrorGettingFilePermissions(String, SystemError),
    ErrorGettingTicketForFile(String, ReadWriteError),
    TargetFileNotFound(String, SystemError),
}

impl fmt::Display for GetCurrentFileInfoError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            GetCurrentFileInfoError::ErrorConveratingModifiedDateToNumber(path, error) =>
                write!(formatter, "Error converting from system time to number. File: {} Error: {}", path, error),

            GetCurrentFileInfoError::ErrorGettingFilePermissions(path, error) =>
                write!(formatter, "Error getting executable permission from file. File: {} Error: {}", path, error),

            GetCurrentFileInfoError::ErrorGettingTicketForFile(path, error) =>
                write!(formatter, "Read/write error while hashing file contents: File: {} Error: {}", path, error),

            GetCurrentFileInfoError::TargetFileNotFound(path, error) =>
                write!(formatter, "System error while attempting to read file: {} Error: {}", path, error),
        }
    }
}

/*  Takes a system, a path and an assumed FileState.
    Returns a FileState object which is current according to the file system.

    Why does the function take the assumed FileState at all?  Why doens't it just take system
    and path?  Because it does the following optimization:

    If the modified date of the file matches the one in FileState exactly, it
    doesn't bother recomputing the ticket, instead it takes the ticket from the
    target_info's history.
*/
pub fn get_actual_file_state<SystemType: System>
(
    system : &SystemType,
    path : &str,
    assumed_file_state : &FileState,
)
-> Result<FileState, GetCurrentFileInfoError>
{
    let system_time =
    match system.get_modified(path)
    {
        Ok(system_time) => system_time,

        // Note: possibly there are other ways get_modified can fail than the file being absent.
        // Maybe this logic should change.
        Err(system_error) => return Err(
            GetCurrentFileInfoError::TargetFileNotFound(path.to_string(), system_error)),
    };

    let timestamp =
    match get_timestamp(system_time)
    {
        Ok(timestamp) => timestamp,
        Err(error) => return Err(GetCurrentFileInfoError::ErrorConveratingModifiedDateToNumber(
            path.to_string(), error)),
    };

    let executable =
    match system.is_executable(path)
    {
        Ok(executable) => executable,
        Err(system_error) => return Err(GetCurrentFileInfoError::ErrorGettingFilePermissions(
            path.to_string(), system_error))
    };

    if timestamp == assumed_file_state.timestamp
    {
        return Ok(
            FileState
            {
                ticket : assumed_file_state.ticket.clone(),
                timestamp : timestamp,
                executable : executable
            }
        )
    }

    match TicketFactory::from_file(system, &path)
    {
        Ok(mut factory) => Ok(
            FileState
            {
                ticket : factory.result(),
                timestamp : timestamp,
                executable : executable
            }),
        Err(read_write_error) => Err(GetCurrentFileInfoError::ErrorGettingTicketForFile(
            path.to_string(),
            read_write_error)),
    }
}

#[derive(Debug)]
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
    downloader_cache_opt : &Option<DownloaderCache>,
    remembered_target_content_info : &FileState,
    target_info : &FileInfo
)
-> Result<FileResolution, ResolutionError>
{
    match cache.restore_file(
        &remembered_target_content_info.ticket,
        &target_info.path)
    {
        RestoreResult::Done =>
            return Ok(FileResolution::Recovered),

        RestoreResult::NotThere => {},

        RestoreResult::CacheDirectoryMissing =>
            return Err(ResolutionError::CacheDirectoryMissing),

        RestoreResult::SystemError(error) =>
            return Err(ResolutionError::CacheMalfunction(error)),
    }

    match downloader_cache_opt
    {
        Some(downloader_cache) =>
        {
            match downloader_cache.restore_file(
                &remembered_target_content_info.ticket,
                system,
                &target_info.path)
            {
                DownloadResult::Done => {}
                DownloadResult::NotThere =>
                    return Ok(FileResolution::NeedsRebuild),
            }

            return match system.set_is_executable(&target_info.path, remembered_target_content_info.executable)
            {
                Err(_) =>
                {
                    println!("Warning: failed to set executable");
                    Ok(FileResolution::Downloaded)
                },
                Ok(_) => Ok(FileResolution::Downloaded)
            };
        },

        None => {}
    }

    Ok(FileResolution::NeedsRebuild)
}

/*  Given a target-info and a remembered ticket for that target file, check the current
    ticket, and if it matches, return AlreadyCorrect.  If it doesn't match, back up the current
    file, and then attempt to restore the remembered file from cache, if the cache doesn't have it,
    attempt to download.  If no recovery or download works, shrug and return NeedsRebuild */
pub fn resolve_single_target<SystemType : System>
(
    system : &mut SystemType,
    cache : &mut SysCache<SystemType>,
    downloader_cache_opt : &Option<DownloaderCache>,
    remembered_target_content_info : &FileState,
    target_info : &FileInfo
)
->
Result<FileResolution, ResolutionError>
{
    match get_file_ticket(system, &target_info.path, &target_info.file_state)
    {
        Ok(Some(current_target_ticket)) =>
        {
            if remembered_target_content_info.ticket == current_target_ticket
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
                downloader_cache_opt,
                remembered_target_content_info,
                target_info)
        },

        // None means the file is not there, in which case, we just try to restore/download, and go home.
        Ok(None) =>
        {
            restore_or_download(
                system,
                cache,
                downloader_cache_opt,
                remembered_target_content_info,
                target_info)
        },

        Err(error) =>
        {
            Err(ResolutionError::TicketAlignmentError(error))
        },
    }
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
        FileState,
        FileStateVec,
        BlobError,
        get_file_ticket
    };
    use crate::system::
    {
        fake::FakeSystem,
        System
    };
    use crate::system::util::
    {
        write_str_to_file,
    };
    use crate::blob::
    {
        get_file_ticket_from_path,
        get_actual_file_state,
        GetCurrentFileInfoError,
    };

    /*  Create a file, and make FileInfo that matches the reality of that file.
        Call get_actual_file_state and check that the returned data matches. */
    #[test]
    fn blob_get_actual_file_state_complete_match()
    {
        let mut system = FakeSystem::new(23);

        write_str_to_file(&mut system, "quine.sh", "cat $0").unwrap();

        let file_state = get_actual_file_state(&system,
            "quine.sh",
            &FileState
            {
                ticket : TicketFactory::from_str("cat $0").result(),
                timestamp : 23,
                executable : false,
            }).unwrap();

        assert_eq!(file_state.ticket, TicketFactory::from_str("cat $0").result());
        assert_eq!(file_state.timestamp, 23);
        assert_eq!(file_state.executable, false);
    }

    /*  Create a file, and make target_info that matches the reality of that file,
        except for one detail: executable is different.

        Call get_actual_file_state and check that the returned data matches, except
        executable. */
    #[test]
    fn blob_get_actual_file_state_executable_contradicts()
    {
        let mut system = FakeSystem::new(23);

        write_str_to_file(&mut system, "quine.sh", "cat $0").unwrap();
        system.set_is_executable("quine.sh", true).unwrap();

        let file_state = get_actual_file_state(&system,
            "quine.sh",
            &FileState
            {
                ticket : TicketFactory::from_str("cat $0").result(),
                timestamp : 23,
                executable : false,
            }).unwrap();

        assert_eq!(file_state.ticket, TicketFactory::from_str("cat $0").result());
        assert_eq!(file_state.timestamp, 23);
        assert_eq!(file_state.executable, true);
    }

    /*  Create a file, and make target_info that matches the reality of that file,
        except for one detail: the timestamp is different.

        Call get_actual_file_state and check that the returned data matches, except
        the timestamp which should be up-to-date. */
    #[test]
    fn blob_get_actual_file_state_old_timestamp()
    {
        let mut system = FakeSystem::new(24);

        write_str_to_file(&mut system, "quine.sh", "cat $0").unwrap();

        let file_state = get_actual_file_state(&system,
            "quine.sh",
            &FileState
            {
                ticket : TicketFactory::from_str("cat $0").result(),
                timestamp : 11,
                executable : false,
            }).unwrap();

        assert_eq!(file_state.ticket, TicketFactory::from_str("cat $0").result());
        assert_eq!(file_state.timestamp, 24);
        assert_eq!(file_state.executable, false);
    }

    /*  Create a file, and simulate a reasonable out-of-date FileState for the
        input to get_actual_file_state, one where the timestamp is out of date, and
        so is the content.

        Call get_actual_file_state and check that the returned data matches the
        current file. */
    #[test]
    fn blob_get_actual_file_state_rough_draft_final_draft()
    {
        let mut system = FakeSystem::new(25);
        write_str_to_file(&mut system, "story.txt", "final draft").unwrap();

        let file_state = get_actual_file_state(&system,
            "story.txt",
            &FileState
            {
                ticket : TicketFactory::from_str("rough draft").result(),
                timestamp : 11,
                executable : false,
            }).unwrap();

        assert_eq!(file_state.ticket, TicketFactory::from_str("final draft").result());
        assert_eq!(file_state.timestamp, 25);
        assert_eq!(file_state.executable, false);
    }

    /*  Create a file, and simulate a very unlikely out-of-date FileState for
        the input to get_actual_file_state, one in which content is out of date, but
        somehow the timestamp matches.

        In this scenario, get_actual_file_state should actually give the wrong
        answer, because it does the optimization where if the timestamp matches
        what's in the filesystem, it doesn't bother looking at the file's actual
        contents to compute a new ticket.  Instead, it just repeats back the assumed
        ticket. */
    #[test]
    fn blob_get_actual_file_state_subvert_the_timestamp_optimization()
    {
        let mut system = FakeSystem::new(25);
        write_str_to_file(&mut system, "story.txt", "final draft").unwrap();

        let file_state = get_actual_file_state(&system,
            "story.txt",
            &FileState
            {
                ticket : TicketFactory::from_str("rough draft").result(),
                timestamp : 25,
                executable : false,
            }).unwrap();
        assert_eq!(file_state.ticket, TicketFactory::from_str("rough draft").result());
        assert_eq!(file_state.timestamp, 25);
        assert_eq!(file_state.executable, false);
    }

    /*  Create a FileInfo for a file that does not exist.
        Check that get_actual_file_state returns an appropriate error. */
    #[test]
    fn blob_get_actual_file_state_file_not_found()
    {
        let system = FakeSystem::new(25);

        match get_actual_file_state(&system,
            "story.txt",
            &FileState
            {
                ticket : TicketFactory::from_str("final draft").result(),
                timestamp : 10,
                executable : false,
            })
        {
            Ok(_) => panic!("Unexpected success"),
            Err(GetCurrentFileInfoError::TargetFileNotFound(path, _system_error)) =>
            {
                assert_eq!(path, "story.txt");
            },
            _ => panic!("Unexpected error"),
        }
    }

    /*  Use a fake system to create a file, and write a string to it.  Then use
        get_file_ticket_from_path to obtain a ticket for that file, and compare
        that against a ticket made directly from the string. */
    #[test]
    fn blob_get_file_ticket_from_path()
    {
        let mut system = FakeSystem::new(10);

        write_str_to_file(&mut system, "quine.sh", "cat $0").unwrap();

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
        let a = FileStateVec::from_ticket_vec(
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result(),
                TicketFactory::from_str("Sugar is sweet\nThis is a poem\n").result(),
            ]
        );

        let b = FileStateVec::from_ticket_vec(
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
        let a = FileStateVec::from_ticket_vec(
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result(),
                TicketFactory::from_str("Sugar is sweet\nThis is a poem\n").result(),
            ]
        );

        let b = FileStateVec::from_ticket_vec(
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
        let a = FileStateVec::from_ticket_vec(
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result(),
                TicketFactory::from_str("Sugar is sweet\nThis is a poem\n").result(),
            ]
        );

        let b = FileStateVec::from_ticket_vec(
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
            "quine.sh",
            &FileState::new_with_ticket(TicketFactory::new().result()))
        {
            Ok(ticket_opt) => match ticket_opt
            {
                Some(ticket) => assert_eq!(ticket, TicketFactory::from_str("cat $0").result()),
                None => panic!("Could not get ticket"),
            }
            Err(err) => panic!("Could not get ticket: {}", err),
        }
    }

    /*  Create a file and a FileInfo for that file with matching timestamp.  Then fill the file
        with some other data.  Make sure that when we get_file_ticket, we get the one from the history
        instead of the one from the file. */
    #[test]
    fn blob_test_timestamp_optimization()
    {
        // Set the clock to 11
        let mut system = FakeSystem::new(11);

        let content = "int main(){printf(\"my game\"); return 0;}";
        let content_ticket = TicketFactory::from_str(content).result();

        // Meanwhile, in the filesystem put some incorrect rubbish in game.cpp
        match write_str_to_file(&mut system, "game.cpp", "some rubbish")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        // Then get the ticket for the current target file, passing the FileInfo
        // with timestamp 11.  Check that it gives the ticket for the C++ code.
        match get_file_ticket(
            &system,
            "game.cpp",
            &FileState::new(content_ticket.clone(), 11))
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

    /*  Create a file and a FileInfo for that file with not-matching timestamp.  Fill the file
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

        // Meanwhile, in the filesystem, put new and improved game.cpp
        match write_str_to_file(&mut system, "game.cpp", current_content)
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        // Then get the ticket for the current target file, passing the FileInfo
        // with timestamp 11.  Check that it gives the ticket for the C++ code.
        match get_file_ticket(
            &system,
            "game.cpp",
            &FileState::new(previous_ticket.clone(), 9))
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

    #[test]
    fn blob_test_download_string_round_trip()
    {
        let file_state_vec = FileStateVec::from_ticket_vec(vec![
            TicketFactory::from_str("Alabaster\n").result(),
            TicketFactory::from_str("Banana\n").result()]);

        assert_eq!(file_state_vec, FileStateVec::from_download_string(
            &file_state_vec.download_string()).unwrap());
    }
}