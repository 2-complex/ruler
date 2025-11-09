use crate::system::
{
    System,
    SystemError,
    CommandScriptResult,
    StandardOutputs,
};
use crate::system::language::
{
    CommandScript,
    CommandScriptLine,
    OutDestination,
};
use crate::system::util::
{
    read_file,
    write_str_to_file,
    get_dir_path_and_name,
    PathError
};
use std::collections::HashMap;
use std::sync::
{
    Arc,
    Mutex
};
use std::ops::
{
    Deref,
    DerefMut
};
use std::io::
{
    Error,
    ErrorKind,
    Read,
    Write
};
use std::cmp::min;
use std::fmt;
use std::str::from_utf8;
use std::io;

#[derive(Debug, Clone)]
pub struct Content(Arc<Mutex<Vec<u8>>>);

impl Content
{
    fn new(content: Vec<u8>) -> Self
    {
        Content(Arc::new(Mutex::new(content)))
    }

    fn empty() -> Self
    {
        Content(Arc::new(Mutex::new(vec![])))
    }

    pub fn borrow(&self) -> impl Deref<Target=Vec<u8>> + '_
    {
        self.0.lock().unwrap()
    }

    pub fn borrow_mut(&self) -> impl DerefMut<Target=Vec<u8>> + '_
    {
        self.0.lock().unwrap()
    }
}

#[derive(Debug, Clone)]
struct Metadata
{
    modified : u64,
    executable : bool,
}

impl Metadata
{
    fn new(timestamp: u64) -> Self
    {
        Metadata
        {
            modified : timestamp,
            executable : false,
        }
    }
}

#[derive(Debug, Clone)]
struct FileInfo
{
    metadata : Metadata,
    content : Content
}

impl FileInfo
{
    fn new(metadata : Metadata, content : Content) -> Self
    {
        FileInfo
        {
            metadata : metadata,
            content : content
        }
    }

    fn from_content(content : Content) -> Self
    {
        FileInfo
        {
            metadata : Metadata::new(0),
            content : content
        }
    }
}

#[derive(Debug, Clone)]
struct DirInfo
{
    timestamp : u64,
    name_to_node : HashMap<String, Node>,
}

impl DirInfo
{
    fn new(timestamp : u64, name_to_node : HashMap<String, Node>) -> Self
    {
        Self
        {
            timestamp : timestamp,
            name_to_node : name_to_node
        }
    }
}

#[derive(Debug, Clone)]
enum Node
{
    File(FileInfo),
    Dir(DirInfo),
    ErrorFile(SystemError),
}

#[derive(Debug, PartialEq)]
enum NodeError
{
    FileInPlaceOfDirectory(String),
    DirectoryInPlaceOfFile(String),
    DirectoryNotFound(String),
    PathInvalid,
    RemoveFileFoundDir,
    ExpectedDirFoundFile,
    RemoveNonExistentFile,
    RemoveNonExistentDir,
    RenameFromNonExistent,
    RenameToNonExistent,
    CreateOverExisting,
    IsExecutableOnDirectory,
    SystemError(SystemError),
}


impl fmt::Display for NodeError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            NodeError::DirectoryNotFound(name)
                => write!(formatter, "Directory not found: {}", name),

            NodeError::FileInPlaceOfDirectory(component)
                => write!(formatter, "Expected directory, found file: {}", component),

            NodeError::DirectoryInPlaceOfFile(component)
                => write!(formatter, "Expected file, found directory: {}", component),

            NodeError::PathInvalid
                => write!(formatter, "Invalid arguments: empty path or empty path components"),

            NodeError::RemoveFileFoundDir
                => write!(formatter, "Attempt to remove file, found directory"),

            NodeError::ExpectedDirFoundFile
                => write!(formatter, "Attempt to remove directory, found file"),

            NodeError::RemoveNonExistentFile
                => write!(formatter, "Attempt to remove non-existent file"),

            NodeError::RemoveNonExistentDir
                => write!(formatter, "Attempt to remove non-existent directory"),

            NodeError::RenameFromNonExistent
                => write!(formatter, "Attempt to rename a non-existent file or directory"),

            NodeError::RenameToNonExistent
                => write!(formatter, "Attempt to rename a file or directory with non-existent target directory"),

            NodeError::CreateOverExisting
                => write!(formatter, "Attempt to create a filesystem entity where another already exists with different type"),

            NodeError::IsExecutableOnDirectory
                => write!(formatter, "Attempt to ask whether a directory is an executable"),

            NodeError::SystemError(error)
                => write!(formatter, "Intensional error for testing: {}", error),
        }
    }
}

fn get_components(dir_path: &str) -> Vec<&str>
{
    if dir_path == ""
    {
        vec![]
    }
    else
    {
        dir_path.split('/').collect()
    }
}

fn to_node_error<'a>(result : Result<(Vec<&'a str>, &'a str), PathError>) -> Result<(Vec<&'a str>, &'a str), NodeError>
{
    match result
    {
        Ok(p) => Ok(p),
        Err(PathError::PathEmpty) => Err(NodeError::PathInvalid),
        Err(PathError::PathComponentEmpty) => Err(NodeError::PathInvalid),
    }
}

impl Node
{
    pub fn empty_dir(modified_timestamp : u64) -> Self
    {
        Node::Dir(DirInfo::new(modified_timestamp, HashMap::new()))
    }

    pub fn is_file(&self, path : &str) -> bool
    {
        match self.get_node(&get_components(path))
        {
            Ok(node) =>
            {
                match node
                {
                    Node::Dir(_) => false,
                    Node::File(_) => true,
                    Node::ErrorFile(_) => true,
                }
            },
            Err(_) => false
        }
    }

    pub fn is_dir(&self, path : &str) -> bool
    {
        match self.get_node(&get_components(path))
        {
            Ok(node) =>
            {
                match node
                {
                    Node::Dir(_) => true,
                    Node::File(_) => false,
                    Node::ErrorFile(_) => false,
                }
            },
            Err(_) => false
        }
    }

    pub fn get_node(&self, dir_components : &Vec<&str>)
        -> Result<&Node, NodeError>
    {
        let mut node = self;
        for component in dir_components.iter()
        {
            node = match node
            {
                Node::File(_) => return Err(NodeError::FileInPlaceOfDirectory(component.to_string())),
                Node::Dir(dir_info) =>
                {
                    match dir_info.name_to_node.get(&component.to_string())
                    {
                        Some(n) => n,
                        None => return Err(NodeError::DirectoryNotFound(component.to_string())),
                    }
                },
                Node::ErrorFile(error) => return Err(NodeError::SystemError(error.clone())),
            }
        }
        return Ok(node)
    }

    pub fn get_node_mut(&mut self, dir_components : &Vec<&str>) -> Result<&mut Node, NodeError>
    {
        let mut node = self;
        for component in dir_components.iter()
        {
            node = match node
            {
                Node::File(_) => return Err(NodeError::FileInPlaceOfDirectory(component.to_string())),
                Node::Dir(dir_info) =>
                {
                    match dir_info.name_to_node.get_mut(&component.to_string())
                    {
                        Some(n) => n,
                        None => return Err(NodeError::DirectoryNotFound(component.to_string())),
                    }
                },
                Node::ErrorFile(error) => return Err(NodeError::SystemError(error.clone())),
            }
        }
        return Ok(node)
    }

    fn get_dir_map(&self, dir_components : &Vec<&str>) -> Result<&HashMap<String, Node>, NodeError>
    {
        match self.get_node(dir_components)?
        {
            Node::File(_) => panic!("Attmept to get_dir_map for file"),
            Node::Dir(dir_info) => Ok(&dir_info.name_to_node),
            Node::ErrorFile(error) => Err(NodeError::SystemError(error.clone())),
        }
    }

    fn get_dir_info_mut(&mut self, dir_components : &Vec<&str>) -> Result<&mut DirInfo, NodeError>
    {
        match self.get_node_mut(dir_components)?
        {
            Node::File(_) => panic!("Attmept to get_dir_info_mut for file"),
            Node::Dir(dir_info) => Ok(dir_info),
            Node::ErrorFile(error) => Err(NodeError::SystemError(error.clone())),
        }
    }

    pub fn create_file(&mut self, path: &str, content : Content, timestamp : u64) -> Result<Content, NodeError>
    {
        let (dir_components, name) = to_node_error(get_dir_path_and_name(path))?;
        let dir_info = self.get_dir_info_mut(&dir_components)?;

        match dir_info.name_to_node.get(name)
        {
            None =>
            {
                dir_info.timestamp = timestamp;
            },
            Some(Node::File(_)) => {},
            _ => return Err(NodeError::CreateOverExisting),
        }

        dir_info.name_to_node.insert(name.to_string(), Node::File(
            FileInfo::new(Metadata::new(timestamp), content.clone())));

        Ok(content)
    }

    pub fn create_dir(&mut self, path: &str, timestamp : u64) -> Result<(), NodeError>
    {
        let (dir_components, name) = to_node_error(get_dir_path_and_name(path))?;
        let dir_info = self.get_dir_info_mut(&dir_components)?;

        match dir_info.name_to_node.get(name)
        {
            None => {},
            Some(Node::Dir(_)) => {},
            _ => return Err(NodeError::CreateOverExisting),
        }

        dir_info.timestamp = timestamp;
        dir_info.name_to_node.insert(name.to_string(), Node::Dir(DirInfo::new(timestamp, HashMap::new())));
        Ok(())
    }

    pub fn create_error_file(&mut self, path: &str, error: SystemError) -> Result<(), NodeError>
    {
        let (dir_components, name) = to_node_error(get_dir_path_and_name(path))?;
        let dir_info = self.get_dir_info_mut(&dir_components)?;

        match dir_info.name_to_node.get(name)
        {
            None => {},
            Some(Node::ErrorFile(_)) => {},
            _ => return Err(NodeError::CreateOverExisting),
        }

        dir_info.name_to_node.insert(name.to_string(), Node::ErrorFile(error));
        Ok(())
    }

    pub fn remove_file(&mut self, path: &str, timestamp : u64) -> Result<(), NodeError>
    {
        let (dir_components, name) = to_node_error(get_dir_path_and_name(path))?;
        match self.get_node_mut(&dir_components)?
        {
            Node::File(_) => match dir_components.last()
            {
                Some(last) => return Err(NodeError::FileInPlaceOfDirectory(last.to_string())),
                None =>
                {
                    panic!("In remove_file, a File node was found at the root path?");
                },
            },
            Node::Dir(dir_info) => {match dir_info.name_to_node.remove(name)
            {
                Some(node) => match node
                {
                    Node::File(_) => 
                    {
                        dir_info.timestamp = timestamp;
                        Ok(())
                    },
                    Node::Dir(_) => 
                    {
                        dir_info.name_to_node.insert(name.to_string(), node);
                        Err(NodeError::RemoveFileFoundDir)
                    },
                    Node::ErrorFile(error) => Err(NodeError::SystemError(error.clone()))
                },
                None => Err(NodeError::RemoveNonExistentFile)
            }},
            Node::ErrorFile(error) => Err(NodeError::SystemError(error.clone()))
        }
    }

    pub fn remove_dir(&mut self, path : &str, timestamp : u64) -> Result<(), NodeError>
    {
        let (dir_components, name) = to_node_error(get_dir_path_and_name(path))?;

        let dir_info = self.get_dir_info_mut(&dir_components)?;
        match dir_info.name_to_node.remove(name)
        {
            Some(node) => match node
            {
                Node::File(_) => 
                {
                    dir_info.name_to_node.insert(name.to_string(), node);
                    Err(NodeError::ExpectedDirFoundFile)
                }
                Node::Dir(_) => 
                {
                    dir_info.timestamp = timestamp;
                    Ok(())
                },
                Node::ErrorFile(error) => Err(NodeError::SystemError(error.clone())),
            },
            None => Err(NodeError::RemoveNonExistentDir)
        }
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<String>, NodeError>
    {
        let mut result : Vec<String> =
            self.get_dir_map(&get_components(path))?.clone().into_keys().collect();
        result.sort();
        Ok(result)
    }

    pub fn rename(&mut self, from: &str, to: &str, timestamp : u64) -> Result<(), NodeError>
    {
        let (from_dir_components, from_name) = to_node_error(get_dir_path_and_name(from))?;
        let (to_dir_components, to_name) = to_node_error(get_dir_path_and_name(to))?;

        let from_dir_info = self.get_dir_info_mut(&from_dir_components)?;

        match from_dir_info.name_to_node.remove(from_name)
        {
            Some(moving_node) =>
            {
                match self.get_dir_info_mut(&to_dir_components)
                {
                    Ok(to_dir_info) =>
                    {
                        to_dir_info.timestamp = timestamp;
                        to_dir_info.name_to_node.insert(to_name.to_string(), moving_node);
                        Ok(())
                    }

                    Err(_) =>
                    {
                        let dir_info = self.get_dir_info_mut(&from_dir_components)?;
                        dir_info.name_to_node.insert(from_name.to_string(), moving_node);
                        Err(NodeError::RenameToNonExistent)
                    }
                }
            },
            None => Err(NodeError::RenameFromNonExistent),
        }
    }

    pub fn open_file(&self, path: &str) -> Result<&Content, NodeError>
    {
        let components = get_components(path);

        match self.get_node(&components)?
        {
            Node::File(info) => Ok(&info.content),
            Node::Dir(_) =>
            {
                match components.last()
                {
                    Some(last) =>
                        return Err(NodeError::DirectoryInPlaceOfFile(last.to_string())),
                    None =>
                        return Err(NodeError::PathInvalid),
                }
            },
            Node::ErrorFile(error) => Err(NodeError::SystemError(error.clone())),
        }
    }

    pub fn get_modified(&self, path: &str) -> Result<u64, NodeError>
    {
        let components = get_components(path);
        match self.get_node(&components)?
        {
            Node::File(info) => Ok(info.metadata.modified),
            Node::Dir(dir_info) => Ok(dir_info.timestamp),
            Node::ErrorFile(error) => Err(NodeError::SystemError(error.clone())),
        }
    }

    pub fn is_executable(&self, path: &str) -> Result<bool, NodeError>
    {
        let components = get_components(path);
        match self.get_node(&components)?
        {
            Node::File(info) => Ok(info.metadata.executable),
            Node::Dir(_) => Ok(false),
            Node::ErrorFile(error) => Err(NodeError::SystemError(error.clone())),
        }
    }

    pub fn set_is_executable(&mut self, path: &str, executable : bool) -> Result<(), NodeError>
    {
        let components = get_components(path);
        match self.get_node_mut(&components)?
        {
            Node::File(info) =>
            {
                info.metadata.executable = executable;
                Ok(())
            },
            Node::Dir(_) => Err(NodeError::IsExecutableOnDirectory),
            Node::ErrorFile(error) => Err(NodeError::SystemError(error.clone())),
        }
    }
}

#[derive(Debug, PartialEq)]
enum AccessMode
{
    Read,
    Write,
}

#[derive(Debug)]
pub struct FakeOpenFile
{
    content : Content,
    pos : usize,
    access_mode : AccessMode,
}

impl FakeOpenFile
{
    fn new(content: &Content, access_mode: AccessMode) -> Self
    {
        FakeOpenFile
        {
            content: content.clone(),
            pos: 0,
            access_mode,
        }
    }

    fn verify_access(&self, access_mode: AccessMode) -> std::io::Result<()>
    {
        if access_mode != self.access_mode
        {
            Err(Error::new(ErrorKind::Other, "Attempt to read/write the wrong way"))
        }
        else
        {
            Ok(())
        }
    }
}

impl Read for FakeOpenFile
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>
    {
        self.verify_access(AccessMode::Read)?;
        let content = self.content.borrow();
        let pos = self.pos;

        // If the underlying file has shrunk, the offset could
        // point to beyond eof.
        let len =
        if pos < content.len()
        {
            min(content.len() - pos, buf.len())
        }
        else
        {
            0
        };

        if len > 0
        {
            buf[..len].copy_from_slice(&content[pos..pos+len]);
            self.pos += len;
        }
        Ok(len)
    }
}

impl Write for FakeOpenFile
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize>
    {
        self.verify_access(AccessMode::Write)?;
        let mut content = self.content.borrow_mut();
        let pos = self.pos;
        // if pos points beyond eof, resize content to pos and pad with zeros
        if pos > content.len()
        {
            content.resize(pos, 0);
        }

        let copy_len = min(buf.len(), content.len() - pos);
        content[pos..pos+copy_len].copy_from_slice(&buf[..copy_len]);
        content.extend_from_slice(&buf[copy_len..]);
        self.pos += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()>
    {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FakeSystem
{
    root: Arc<Mutex<Node>>,
    current_timestamp: u64,
    command_log: Arc<Mutex<Vec<String>>>
}

fn convert_node_error_to_system_error(error : NodeError) -> SystemError
{
    match error
    {
        NodeError::FileInPlaceOfDirectory(component)
            => SystemError::FileInPlaceOfDirectory(component),

        NodeError::DirectoryInPlaceOfFile(component)
            => SystemError::DirectoryInPlaceOfFile(component),

        NodeError::DirectoryNotFound(_component)
            => SystemError::NotFound,

        NodeError::PathInvalid
            => SystemError::PathInvalid,

        NodeError::RemoveFileFoundDir
            => SystemError::RemoveFileFoundDir,

        NodeError::ExpectedDirFoundFile
            => SystemError::ExpectedDirFoundFile,

        NodeError::RemoveNonExistentFile
            => SystemError::RemoveNonExistentFile,

        NodeError::RemoveNonExistentDir
            => SystemError::RemoveNonExistentDir,

        NodeError::RenameFromNonExistent
            => SystemError::RenameFromNonExistent,

        NodeError::RenameToNonExistent
            => SystemError::RenameToNonExistent,

        NodeError::CreateOverExisting
            => SystemError::CreateOverExisting,

        NodeError::IsExecutableOnDirectory
            => panic!("Attempt to ask is executable on directory"),

        NodeError::SystemError(error)
            => error,
    }
}

fn error_message(message: String) -> (Option<i32>, StandardOutputs)
{
    (Some(1), StandardOutputs::error(message.into_bytes()))
}

fn empty_output() -> StandardOutputs
{
    StandardOutputs{ out : vec![], err : vec![] }
}

impl FakeSystem
{
    pub fn new(start : u64) -> Self
    {
        FakeSystem
        {
            root : Arc::new(Mutex::new(Node::empty_dir(start))),

            /*  When too many timestamps are 0 by default it triggers the
                timestamp optimization at the wrong time */
            current_timestamp : start,

            command_log : Arc::new(Mutex::new(vec![])),
        }
    }

    pub fn time_passes(&mut self, increment : u64)
    {
        self.current_timestamp += increment;
    }

    fn get_root_node(&self) -> impl Deref<Target=Node> + '_
    {
        self.root.lock().unwrap()
    }

    fn get_root_node_mut(&self) -> impl DerefMut<Target=Node> + '_
    {
        self.root.lock().unwrap()
    }

    fn get_command_log_mut(&self) -> impl DerefMut<Target=Vec<String>> + '_
    {
        self.command_log.lock().unwrap()
    }

    pub fn get_command_log(&self) -> Vec<String>
    {
        self.command_log.lock().unwrap().clone()
    }

    pub fn create_error_file(&mut self, path: &str, error: SystemError) -> Result<(), SystemError>
    {
        match self.get_root_node_mut().create_error_file(path, error)
        {
            Ok(()) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn execute_script_line(&mut self, line : CommandScriptLine)
        -> (Option<i32>, StandardOutputs)
    {
        match line.exec.as_str()
        {
            "error" =>
            {
                return error_message("Failed".to_string())
            },

            "cat" =>
            {
                let mut output = String::new();
                for file in line.args.iter()
                {
                    match read_file(self, file)
                    {
                        Ok(content) =>
                        {
                            match from_utf8(&content)
                            {
                                Ok(content_string) =>
                                {
                                    output.push_str(content_string);
                                }
                                Err(_) => return error_message(
                                    format!("File contained non utf8 bytes: {}", file)),
                            }
                        }
                        Err(error) =>
                        {
                            return error_message(
                                format!("File failed to open: {} with error: {}", file, error));
                        }
                    }
                }

                match line.out
                {
                    OutDestination::StdOut =>
                    {
                        // TODO
                        panic!("What do I put here!?!?!");
                    },
                    OutDestination::File(path_string) =>
                    {
                        match write_str_to_file(self, &path_string, &output)
                        {
                            Ok(_) => (Some(0), empty_output()),
                            Err(error) =>
                                error_message(format!("Failed to cat into file: {} : {}",
                                    path_string, error)),
                        }
                    },
                    OutDestination::Command(command_box) =>
                    {
                        self.execute_script_line(*command_box)
                    }
                }
            },

            "rm" =>
            {
                for file in line.args.iter()
                {
                    match self.remove_file(file)
                    {
                        Ok(()) => {}
                        Err(_) =>
                            return error_message(format!("File failed to delete: {}", file)),
                    }
                }

                return (Some(0), empty_output())
            },

            "cp" =>
            {
                let mut iter = line.args.iter();
                let src = match iter.next()
                {
                    Some(src) => src.as_str(),
                    None => return error_message("cp: wrong number of arguments".to_string()),
                };

                let dst = match iter.next()
                {
                    Some(dst) => dst.as_str(),
                    None => return error_message("cp: wrong number of arguments".to_string()),
                };

                match io::copy(
                    &mut match self.open(src)
                    {
                        Ok(mut file) => file,
                        Err(error) => return error_message(
                            format!("cp: source file failed to open: {} with error: {}", src, error)),
                    },
                    &mut match self.create_file(dst)
                    {
                        Ok(mut file) => file,
                        Err(error) => return error_message(
                            format!("cp: source file failed to open: {} with error: {}", dst, error)),
                    })
                {
                    Ok(_) => (Some(0), empty_output()),
                    Err(error) => error_message(format!("cp: stream failed: {}", error)),
                }
            },
            _=> return error_message(format!("Invalid command given: {}", line.exec))
        }
    }
}

impl System for FakeSystem
{
    type File = FakeOpenFile;

    fn open(&self, path: &str) -> Result<Self::File, SystemError>
    {
        match self.get_root_node().open_file(path)
        {
            Ok(content) =>
                Ok(FakeOpenFile::new(content, AccessMode::Read)),

            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn create_file(&mut self, path: &str) -> Result<Self::File, SystemError>
    {
        match self.get_root_node_mut().create_file(path, Content::empty(), self.current_timestamp)
        {
            Ok(content) => Ok(FakeOpenFile::new(&content, AccessMode::Write)),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn create_dir(&mut self, path: &str) -> Result<(), SystemError>
    {
        match self.get_root_node_mut().create_dir(path, self.current_timestamp)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn is_file(&self, path: &str) -> bool
    {
        self.get_root_node().is_file(path)
    }

    fn is_dir(&self, path: &str) -> bool
    {
        self.get_root_node().is_dir(path)
    }

    fn remove_file(&mut self, path: &str) -> Result<(), SystemError>
    {
        match self.get_root_node_mut().remove_file(path, self.current_timestamp)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn remove_dir(&mut self, path: &str) -> Result<(), SystemError>
    {
        match self.get_root_node_mut().remove_dir(path, self.current_timestamp)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn list_dir(&self, path: &str) -> Result<Vec<String>, SystemError>
    {
        match self.get_root_node_mut().list_dir(path)
        {
            Ok(result) => Ok(result),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), SystemError>
    {
        match self.get_root_node_mut().rename(from, to, self.current_timestamp)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn get_modified(&self, path: &str) -> Result<u64, SystemError>
    {
        match self.get_root_node().get_modified(path)
        {
            Ok(system_time) => Ok(system_time),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn is_executable(&self, path: &str) -> Result<bool, SystemError>
    {
        match self.get_root_node().is_executable(path)
        {
            Ok(executable) => Ok(executable),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn set_is_executable(&mut self, path: &str, executable : bool) -> Result<(), SystemError>
    {
        match self.get_root_node_mut().set_is_executable(path, executable)
        {
            Ok(()) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn execute_command_script(&mut self, command_script: CommandScript) -> CommandScriptResult
    {
        let mut output = CommandScriptResult::new();
        self.get_command_log_mut().push(format!("{}", command_script));
        for line in command_script.lines
        {
            output.push(self.execute_script_line(line));
        }
        output
    }
}

#[cfg(test)]
mod test
{
    use crate::system::
    {
        System,
        SystemError,
        StandardOutputs,
        CommandScriptResult,
    };

    use crate::system::language::CommandScript;

    use crate::system::fake::
    {
        Content,
        FileInfo,
        Node,
        NodeError,
        get_components,
        FakeSystem,
        empty_output
    };

    use crate::system::util::
    {
        write_str_to_file,
        read_file,
    };

    #[test]
    fn content_borrows_with_star()
    {
        let content = Content::new(b"content1".to_vec());
        assert_eq!(*content.borrow(), b"content1".to_vec());
    }

    #[test]
    fn content_borrows_mut_and_change()
    {
        let file_content = Content::new(b"content1".to_vec());
        file_content.borrow_mut()[7] = b'2';
        assert_eq!(*file_content.borrow(), b"content2".to_vec());
    }

    #[test]
    fn content_clone_points_to_same_data()
    {
        let file_content = Content::new(b"content1".to_vec());
        let file_content_clone = file_content.clone();
        file_content.borrow_mut()[7] = b'2';
        assert_eq!(*file_content_clone.borrow(), b"content2".to_vec());
    }

    #[test]
    fn get_components_general()
    {
        assert_eq!(get_components(""), Vec::<&str>::new());
        assert_eq!(get_components("apples"), vec!["apples"]);
        assert_eq!(get_components("apples/bananas"), vec!["apples", "bananas"]);
    }

    #[test]
    fn file_is_file()
    {
        let node = Node::File(FileInfo::from_content(Content::new(b"things".to_vec())));
        assert!(node.is_file(""));
        assert!(!node.is_dir(""));
    }

    #[test]
    fn new_empty_file_is_file()
    {
        let node = Node::File(FileInfo::from_content(Content::new(b"".to_vec())));
        assert!(node.is_file(""));
        assert!(!node.is_dir(""));
    }

    #[test]
    fn dir_is_dir()
    {
        let node = Node::empty_dir(1);
        assert!(!node.is_file(""));
        assert!(node.is_dir(""));
    }

    #[test]
    fn error_file_is_file_or_directory()
    {
        let node = Node::ErrorFile(SystemError::NotFound);
        assert!(node.is_file(""));
        assert!(!node.is_dir(""));
    }

    #[test]
    fn non_existent_child_is_not_file_or_dir()
    {
        let node = Node::File(FileInfo::from_content(Content::new(b"stuff".to_vec())));
        assert!(!node.is_file("stuf-not-there"));
        assert!(!node.is_dir("stuf-not-there"));
    }

    #[test]
    fn add_remove_file()
    {
        let mut node = Node::empty_dir(2);
        node.create_file("file.txt", Content::new(b"some text".to_vec()), 0).unwrap();
        assert!(node.is_file("file.txt"));
        node.remove_file("file.txt", 3).unwrap();
        assert!(!node.is_file("file.txt"));
        assert!(!node.is_dir("file.txt"));
    }

    #[test]
    fn add_remove_dir()
    {
        let mut node = Node::empty_dir(2);
        node.create_dir("images", 2).unwrap();
        assert!(node.is_dir("images"));
        node.remove_dir("images", 3).unwrap();
        assert!(!node.is_file("images"));
        assert!(!node.is_dir("images"));
    }

    #[test]
    fn add_and_attempt_to_remove_error_node()
    {
        let mut node = Node::empty_dir(3);
        node.create_error_file("photos", SystemError::PathNotUnicode).unwrap();
        assert!(!node.is_dir("photos"));
        assert_eq!(node.remove_dir("photos", 4), Err(NodeError::SystemError(SystemError::PathNotUnicode)));
        assert!(!node.is_file("photos"));
        assert!(!node.is_dir("photos"));
    }

    #[test]
    fn add_list_dir()
    {
        let mut node = Node::empty_dir(5);
        node.create_error_file("photos", SystemError::PathNotUnicode).unwrap();
        assert!(!node.is_dir("photos"));
        assert_eq!(node.remove_dir("photos", 6), Err(NodeError::SystemError(SystemError::PathNotUnicode)));
        assert!(!node.is_file("photos"));
        assert!(!node.is_dir("photos"));
    }

    #[test]
    fn add_and_list_dir_empty()
    {
        let mut node = Node::empty_dir(7);
        node.create_dir("images", 7).unwrap();
        let list = node.list_dir("images").unwrap();
        assert!(list.len() == 0);
    }

    #[test]
    fn add_and_list_dir_dir()
    {
        let mut node = Node::empty_dir(10);
        node.create_dir("images", 10).unwrap();
        node.create_dir("images/more_images", 10).unwrap();
        let list = node.list_dir("images").unwrap();
        assert_eq!(list, vec!["more_images".to_string()]);
    }

    /*  Create a directory, plant an errorfile inside, then list the directory.
        That should succeed.  Only accessing the content of an error file triggers the error. */
    #[test]
    fn add_and_list_dir_containing_error_node()
    {
        let mut node = Node::empty_dir(11);
        node.create_dir("photos", 11).unwrap();
        node.create_error_file("photos/more_photos", SystemError::NotFound).unwrap();

        // Merely listing the name of the error node is not an error.
        let list = node.list_dir("photos").unwrap();
        assert_eq!(list, vec!["more_photos".to_string()]);
    }

    #[test]
    fn attempt_to_list_dir_on_error_node()
    {
        let mut node = Node::empty_dir(14);
        node.create_error_file("photos", SystemError::MetadataNotFound).unwrap();
        assert_eq!(node.list_dir("photos"), Err(NodeError::SystemError(SystemError::MetadataNotFound)));
    }

    #[test]
    fn create_file_with_directory_already_present()
    {
        let mut node = Node::empty_dir(12);
        node.create_dir("images", 12).unwrap();
        node.create_dir("images/more_images", 12).unwrap();
        match node.create_file("images/more_images", Content::new(b"content".to_vec()), 0)
        {
            Err(NodeError::CreateOverExisting) => {},
            _ => panic!("unexpected result"),
        }
    }

    #[test]
    fn create_directory_with_file_already_present()
    {
        let mut node = Node::empty_dir(11);
        node.create_dir("images", 12).unwrap();
        node.create_dir("images/more_images", 13).unwrap();
        match node.create_file("images/more_images", Content::new(b"content".to_vec()), 0)
        {
            Err(NodeError::CreateOverExisting) => {},
            _ => panic!("unexpected result"),
        }
    }

    #[test]
    fn create_error_file_node_with_file_already_present()
    {
        let mut node = Node::empty_dir(12);
        node.create_dir("images", 13).unwrap();
        node.create_dir("images/more_images", 14).unwrap();
        assert_eq!(
            node.create_error_file("images/more_images", SystemError::MetadataNotFound),
            Err(NodeError::CreateOverExisting));
    }

    #[test]
    fn add_and_list_dir_file()
    {
        let mut node = Node::empty_dir(0);
        node.create_dir("images", 0).unwrap();
        node.create_file("images/mydog.jpg", Content::new(b"jpeginternals".to_vec()), 0).unwrap();
        let list = node.list_dir("images").unwrap();
        assert_eq!(list, vec!["mydog.jpg".to_string()]);
    }

    /*  This test is supposed to check whether the retured list of paths is sorted.  It does this by
        creating the files in an arbitrary, non-sorted order.  Of course there is a small probability
        that the implementation sorts them by chance, but with enough files in the list, that probability
        is very low */
    #[test]
    fn list_dir_sorted()
    {
        let mut node = Node::empty_dir(1);
        node.create_dir("images", 1).unwrap();
        node.create_file("images/B.txt", Content::new(b"B".to_vec()), 0).unwrap();
        node.create_file("images/G.txt", Content::new(b"G".to_vec()), 0).unwrap();
        node.create_file("images/D.txt", Content::new(b"D".to_vec()), 0).unwrap();
        node.create_file("images/C.txt", Content::new(b"C".to_vec()), 0).unwrap();
        node.create_file("images/E.txt", Content::new(b"E".to_vec()), 0).unwrap();
        node.create_file("images/F.txt", Content::new(b"F".to_vec()), 0).unwrap();
        node.create_file("images/A.txt", Content::new(b"A".to_vec()), 0).unwrap();
        let list = node.list_dir("images").unwrap();
        assert_eq!(list, vec![
            "A.txt".to_string(),
            "B.txt".to_string(),
            "C.txt".to_string(),
            "D.txt".to_string(),
            "E.txt".to_string(),
            "F.txt".to_string(),
            "G.txt".to_string()]);
    }

    #[test]
    fn remove_non_existent_file_errors()
    {
        let mut node = Node::empty_dir(2);
        match node.remove_file("file-not-there.txt", 3)
        {
            Ok(_) => panic!("Unexpected sucess removing non-existent file"),
            Err(error) => match error
            {
                NodeError::RemoveNonExistentFile => {},
                _ => panic!("Attempt to remove non-existent file resulted in wrong error.")
            }
        }
        assert!(!node.is_file("some text"));
    }

    #[test]
    fn remove_non_existent_dir_errors()
    {
        let mut node = Node::empty_dir(1);
        assert_eq!(node.remove_dir("dir-not-there", 2), Err(NodeError::RemoveNonExistentDir));
        assert!(!node.is_file("some text"));
    }

    #[test]
    fn rename_file()
    {
        let mut node = Node::empty_dir(1);
        node.create_file("kitten.jpg", Content::new(b"jpg-content".to_vec()), 0).unwrap();
        node.create_dir("images", 1).unwrap();
        assert!(node.is_file("kitten.jpg"));
        assert!(node.is_dir("images"));
        node.rename("kitten.jpg", "images/kitten.jpg", 2).unwrap();
        assert!(!node.is_file("kitten.jpg"));
        assert!(node.is_dir("images"));
        assert!(node.is_file("images/kitten.jpg"));
    }

    #[test]
    fn rename_directory()
    {
        let mut node = Node::empty_dir(2);
        node.create_dir("images", 1).unwrap();
        node.create_file("images/kitten.jpg", Content::new(b"jpg-content".to_vec()), 0).unwrap();
        assert!(node.is_dir("images"));
        assert!(node.is_file("images/kitten.jpg"));
        assert!(!node.is_dir("images2"));
        assert!(!node.is_file("images2/kitten.jpg"));
        node.rename("images", "images2", 2).unwrap();
        assert!(!node.is_dir("images"));
        assert!(!node.is_file("images/kitten.jpg"));
        assert!(node.is_dir("images2"));
        assert!(node.is_file("images2/kitten.jpg"));
    }

    #[test]
    fn rename_error_file()
    {
        let mut node = Node::empty_dir(3);
        node.create_error_file("kitten.jpg", SystemError::ExpectedDirFoundFile).unwrap();
        node.create_dir("images", 3).unwrap();
        assert!(node.is_file("kitten.jpg"));
        assert!(node.is_dir("images"));
        assert!(!node.is_file("images/kitten.jpg"));
        node.rename("kitten.jpg", "images/kitten.jpg", 4).unwrap();
        assert!(!node.is_file("kitten.jpg"));
        assert!(node.is_dir("images"));
        assert!(node.is_file("images/kitten.jpg"));
    }

    #[test]
    fn system_add_remove_file_basic()
    {
        let mut system = FakeSystem::new(10);
        system.create_file("file.txt").unwrap();
        assert!(system.is_file("file.txt"));
        system.remove_file("file.txt").unwrap();
        assert!(!system.is_file("file.txt"));
        assert!(!system.is_dir("file.txt"));
    }

    #[test]
    fn system_add_remove_file_using_command()
    {
        let mut system = FakeSystem::new(10);
        system.create_file("file.txt").unwrap();
        assert!(system.is_file("file.txt"));
        system.execute_command_script(CommandScript::parse("rm file.txt").unwrap());
        assert!(!system.is_file("file.txt"));
        assert!(!system.exists("file.txt"));
        assert!(!system.is_dir("file.txt"));
    }

    #[test]
    fn system_add_remove_dir()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("images").unwrap();
        assert!(system.is_dir("images"));
        system.remove_dir("images").unwrap();
        assert!(!system.is_file("images"));
        assert!(!system.is_dir("images"));
    }

    #[test]
    fn system_add_remove_error()
    {
        let mut system = FakeSystem::new(10);
        system.create_error_file("photos", SystemError::PathNotUnicode).unwrap();
        assert!(system.is_file("photos"));
        assert!(!system.is_dir("photos"));
        assert_eq!(system.remove_dir("photos"), Err(SystemError::PathNotUnicode));
        assert!(!system.is_file("photos"));
        assert!(!system.is_dir("photos"));
    }

    #[test]
    fn system_create_file_write_read_round_trip()
    {
        let mut system = FakeSystem::new(10);
        write_str_to_file(&mut system, "fruit_file.txt", "cantaloupe").unwrap();
        read_file(&system, "fruit_file.txt").unwrap();
    }

    #[test]
    fn system_create_file_write_read_round_trip_with_cloning()
    {
        let mut system1 = FakeSystem::new(10);
        let system2 = system1.clone();
        write_str_to_file(&mut system1, "fruit_file.txt", "cantaloupe").unwrap();
        assert_eq!(read_file(&system2, "fruit_file.txt").unwrap(), b"cantaloupe");
    }

    #[test]
    fn system_create_file_write_read_round_trip_read_twice()
    {
        let mut system = FakeSystem::new(10);
        write_str_to_file(&mut system, "fruit_file.txt", "cantaloupe").unwrap();
        assert_eq!(read_file(&system, "fruit_file.txt").unwrap(), b"cantaloupe");
        assert_eq!(read_file(&system, "fruit_file.txt").unwrap(), b"cantaloupe");
    }

    #[test]
    fn system_error_node_write_errors()
    {
        let mut system = FakeSystem::new(10);
        system.create_error_file("fruit_file.txt", SystemError::PathNotUnicode).unwrap();
        assert_eq!(write_str_to_file(&mut system, "fruit_file.txt", "cantaloupe"), Err(SystemError::CreateOverExisting));
    }

    #[test]
    fn system_error_node_read_errors()
    {
        let mut system = FakeSystem::new(10);
        system.create_error_file("fruit_file.txt", SystemError::PathNotUnicode).unwrap();
        assert_eq!(read_file(&system, "fruit_file.txt"), Err(SystemError::PathNotUnicode));
    }

    #[test]
    fn system_rename_file()
    {
        let mut system = FakeSystem::new(10);
        system.create_file("star.png").unwrap();
        assert!(system.is_file("star.png"));
        assert!(!system.is_file("heart.png"));
        system.rename("star.png", "heart.png").unwrap();
        assert!(!system.is_file("star.png"));
        assert!(system.is_file("heart.png"));
    }

    #[test]
    fn system_rename_directory()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("hearts").unwrap();
        system.create_file("hearts/star1.png").unwrap();
        system.create_file("hearts/star2.png").unwrap();
        assert!(system.is_dir("hearts"));
        assert!(system.is_file("hearts/star1.png"));
        assert!(system.is_file("hearts/star2.png"));
        assert!(!system.is_dir("stars"));
        assert!(!system.is_file("stars/star1.png"));
        assert!(!system.is_file("stars/star2.png"));
        system.rename("hearts", "stars").unwrap();
        assert!(!system.is_dir("hearts"));
        assert!(!system.is_file("hearts/star1.png"));
        assert!(!system.is_file("hearts/star2.png"));
        assert!(system.is_dir("stars"));
        assert!(system.is_file("stars/star1.png"));
        assert!(system.is_file("stars/star2.png"));
    }

    #[test]
    fn system_rename_error()
    {
        let mut system = FakeSystem::new(10);
        system.create_error_file("star.png", SystemError::PathNotUnicode).unwrap();
        assert!(system.is_file("star.png"));
        assert!(!system.is_file("heart.png"));
        system.rename("star.png", "heart.png").unwrap();
        assert!(!system.is_file("star.png"));
        assert!(system.is_file("heart.png"));
    }

    #[test]
    fn modified_timestamps()
    {
        let mut system = FakeSystem::new(17);
        system.create_file("star.png").unwrap();
        system.time_passes(17);
        system.create_file("heart.png").unwrap();

        assert_eq!(system.get_modified("star.png").unwrap(), 17);
        assert_eq!(system.get_modified("heart.png").unwrap(), 34);
    }

    #[test]
    fn writing_updates_modified_timestamp()
    {
        let mut system = FakeSystem::new(0);
        system.time_passes(5);
        system.create_file("cars.txt").unwrap();
        system.time_passes(6);
        write_str_to_file(&mut system, "cars.txt", "cantaloupe").unwrap();
        assert_eq!(system.get_modified("cars.txt").unwrap(), 11);
    }

    #[test]
    fn renaming_keeps_modified_timestamp()
    {
        let mut system = FakeSystem::new(0);
        system.time_passes(5);
        system.create_file("cars.txt").unwrap();
        system.time_passes(6);
        system.rename("cars.txt", "cars2.txt").unwrap();
        assert_eq!(system.get_modified("cars2.txt").unwrap(), 5);
    }

    #[test]
    fn get_modified_on_directory_basic()
    {
        let mut system = FakeSystem::new(32);
        system.create_dir("stuff").unwrap();
        assert_eq!(system.get_modified("stuff").unwrap(), 32);
    }

    #[test]
    fn get_modified_on_directory_after_adding_file()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("stuff").unwrap();
        system.time_passes(1);
        system.create_file("stuff/thing").unwrap();
        assert_eq!(system.get_modified("stuff").unwrap(), 11);
    }

    #[test]
    fn get_modified_on_directory_after_removing_file()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("stuff").unwrap();
        system.create_file("stuff/thing").unwrap();
        system.time_passes(1);
        system.remove_file("stuff/thing").unwrap();
        assert_eq!(system.get_modified("stuff").unwrap(), 11);
    }

    #[test]
    fn get_modified_on_directory_after_renaming_file()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("stuff").unwrap();
        system.create_file("stuff/thing").unwrap();
        system.time_passes(1);
        system.rename("stuff/thing", "stuff/object").unwrap();
        assert_eq!(system.get_modified("stuff").unwrap(), 11);
    }

    #[test]
    fn get_modified_on_directory_after_adding_subdirectory()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("stuff").unwrap();
        system.time_passes(1);
        system.create_dir("stuff/thing").unwrap();
        assert_eq!(system.get_modified("stuff").unwrap(), 11);
    }

    #[test]
    fn get_modified_on_directory_after_removing_subdirectory()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("stuff").unwrap();
        system.create_dir("stuff/thing").unwrap();
        system.time_passes(1);
        system.remove_dir("stuff/thing").unwrap();
        assert_eq!(system.get_modified("stuff").unwrap(), 11);
    }

    #[test]
    fn get_modified_on_directory_after_renaming_subdirectory()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("stuff").unwrap();
        system.create_dir("stuff/thing").unwrap();
        system.time_passes(1);
        system.rename("stuff/thing", "stuff/things").unwrap();
        assert_eq!(system.get_modified("stuff").unwrap(), 11);
    }

    #[test]
    fn get_timestamp_recursive_on_file()
    {
        let mut system = FakeSystem::new(32);
        system.create_file("data").unwrap();
        let recursive_timestamp = system.get_timestamp_recursive("data").unwrap();
        let timestamp = system.get_timestamp_recursive("data").unwrap();

        assert_eq!(recursive_timestamp, 32);
        assert_eq!(timestamp, recursive_timestamp);
    }

    #[test]
    fn get_timetstamp_recursive_on_empty_directory()
    {
        let mut system = FakeSystem::new(14);
        system.create_dir("images").unwrap();
        let recursive_timestamp = system.get_timestamp_recursive("images").unwrap();
        let timestamp = system.get_modified("images").unwrap();

        assert_eq!(recursive_timestamp, 14);
        assert_eq!(timestamp, recursive_timestamp);
    }

    #[test]
    fn get_timetstamp_recursive_on_directory_with_one_file()
    {
        let mut system = FakeSystem::new(14);
        system.create_dir("images").unwrap();
        system.time_passes(1);
        system.create_file("images/kitten.jpg").unwrap();
        let recursive_timestamp = system.get_timestamp_recursive("images").unwrap();
        assert_eq!(recursive_timestamp, 15);
    }

    /*  Create a file inside a directory, let some time pass, then write to that file.
        Check that the timestamp of the file up to date, but the timestamp of the directory
        remains the same. */
    #[test]
    fn get_timetstamp_recursive_on_directory_with_one_file_then_write()
    {
        let mut system = FakeSystem::new(14);
        system.create_dir("images").unwrap();
        system.create_file("images/kitten.jpg").unwrap();
        system.time_passes(1);
        write_str_to_file(&mut system, "images/kitten.jpg", "image content").unwrap();

        let directory_timestamp = system.get_modified("images").unwrap();
        let recursive_timestamp = system.get_timestamp_recursive("images").unwrap();

        assert_eq!(directory_timestamp, 14);
        assert_eq!(recursive_timestamp, 15);
    }

    #[test]
    fn get_timetstamp_non_existent_error_basic()
    {
        let mut system = FakeSystem::new(14);
        system.create_dir("images").unwrap();
        system.create_dir("images/cats").unwrap();
        assert_eq!(
            system.get_modified("images/cats/monorailcat.jpg"),
            Err(SystemError::NotFound));
    }

    #[test]
    fn get_timetstamp_non_existent_error_deep()
    {
        let mut system = FakeSystem::new(14);
        system.create_dir("images").unwrap();
        assert_eq!(
            system.get_modified("images/cats/monorailcat.jpg"),
            Err(SystemError::NotFound));
    }

    #[test]
    fn get_timetstamp_recursive_deep()
    {
        let mut system = FakeSystem::new(14);
        system.create_dir("images").unwrap();
        system.time_passes(1);
        system.create_dir("images/cats").unwrap();
        system.time_passes(1);
        system.create_file("images/cats/monorailcat.jpg").unwrap();
        system.time_passes(1);
        write_str_to_file(&mut system, "images/cats/monorailcat.jpg", "image content").unwrap();

        let directory_timestamp = system.get_modified("images").unwrap();
        let subdirectory_timestamp = system.get_modified("images/cats").unwrap();
        let recursive_timestamp = system.get_timestamp_recursive("images/cats/monorailcat.jpg").unwrap();

        assert_eq!(directory_timestamp, 15);
        assert_eq!(subdirectory_timestamp, 16);
        assert_eq!(recursive_timestamp, 17);
    }

    #[test]
    fn executing_error_gives_error_output()
    {
        let mut system = FakeSystem::new(10);
        assert_eq!(
            system.execute_command_script(CommandScript::parse("error").unwrap()),
            CommandScriptResult
            {
                outputs: vec![
                    StandardOutputs::error("Failed".as_bytes().to_vec()),
                ],
                code: Some(1)
            }
        );
    }

    #[test]
    fn executing_cat_concatinates()
    {
        let mut system = FakeSystem::new(10);
        system.create_file("line1.txt").unwrap();
        write_str_to_file(&mut system, "line1.txt", "Ants\n").unwrap();
        system.create_file("line2.txt").unwrap();
        write_str_to_file(&mut system, "line2.txt", "Love to dance\n").unwrap();

        assert_eq!(
            system.execute_command_script(CommandScript::parse(
                "cat line1.txt line2.txt > poem.txt").unwrap()
            ),
            CommandScriptResult
            {
                outputs: vec![
                    empty_output(),
                ],
                code: Some(0)
            }
        );

        assert_eq!(read_file(&system, "poem.txt"), Ok(b"Ants\nLove to dance\n".to_vec()));
    }

    #[test]
    fn executing_cat_concatinates_and_dupes()
    {
        let mut system = FakeSystem::new(10);
        system.create_file("line1.txt").unwrap();
        write_str_to_file(&mut system, "line1.txt", "Ants\n").unwrap();
        system.create_file("line2.txt").unwrap();
        write_str_to_file(&mut system, "line2.txt", "Love to dance\n").unwrap();
        assert_eq!(system.execute_command_script(CommandScript::parse(
            "cat line1.txt line2.txt > poem.txt; cp poem.txt poem-backup.txt").unwrap()),
            CommandScriptResult
            {
                outputs: vec![
                    empty_output(),
                    empty_output(),
                ],
                code: Some(0)
            }
        );

        assert_eq!(read_file(&system, "poem.txt").unwrap(), b"Ants\nLove to dance\n");
        assert_eq!(read_file(&system, "poem-backup.txt").unwrap(), b"Ants\nLove to dance\n");
    }

    #[test]
    fn use_commandline_to_remove()
    {
        let mut system = FakeSystem::new(10);
        system.create_file("terrible-file.txt").unwrap();
        assert!(system.is_file("terrible-file.txt"));
        assert_eq!(
            system.execute_command_script(CommandScript::parse("rm terrible-file.txt").unwrap()),
            CommandScriptResult
            {
                outputs: vec![
                    empty_output(),
                ],
                code: Some(0)
            }
        );

        assert!(!system.is_file("terrible-file.txt"));
    }
}
