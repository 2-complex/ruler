use crate::system::
{
    System,
    SystemError,
    CommandLineOutput
};
use crate::system::util::
{
    read_file,
    write_str_to_file,
    timestamp_to_system_time,
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
use std::time::SystemTime;
use std::str::from_utf8;


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
    modified : SystemTime,
    executable : bool,
}

impl Metadata
{
    fn new(timestamp: u64) -> Self
    {
        Metadata
        {
            modified : timestamp_to_system_time(timestamp),
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
enum Node
{
    File(FileInfo),
    Dir(HashMap<String, Node>)
}

#[derive(Debug)]
enum NodeError
{
    FileInPlaceOfDirectory(String),
    DirectoryInPlaceOfFile(String),
    DirectoryNotFound(String),
    PathEmpty,
    RemoveFileFoundDir,
    ExpectedDirFoundFile,
    RemoveNonExistentFile,
    RemoveNonExistentDir,
    RenameFromNonExistent,
    RenameToNonExistent,
    GetModifiedOnDirectory,
    IsExecutableOnDirectory,
    Weird,
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

            NodeError::PathEmpty
                => write!(formatter, "Invalid arguments: found empty path"),

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

            NodeError::GetModifiedOnDirectory
                => write!(formatter, "Attempt to get modified time for a directory (that is not implemented)"),

            NodeError::IsExecutableOnDirectory
                => write!(formatter, "Attempt to ask whether a directory is an executable"),

            NodeError::Weird
                => write!(formatter, "Weird error, this happens when internal logic fails in a way the programmer didn't think was possible"),
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

fn get_dir_path_and_name(dir_path: &str) -> Result<(Vec<&str>, &str), NodeError>
{
    if dir_path == ""
    {
        return Err(NodeError::PathEmpty);
    }

    let v : Vec<&str> = dir_path.split('/').collect();
    if v.len() == 0
    {
        return Err(NodeError::PathEmpty);
    }

    return Ok((v[..v.len()-1].to_vec(), v[v.len()-1]))
}

impl Node
{
    pub fn empty_dir() -> Self
    {
        Node::Dir(HashMap::new())
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
                }
            },
            Err(_) =>
                false
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
                Node::Dir(name_to_node) =>
                {
                    match name_to_node.get(&component.to_string())
                    {
                        Some(n) => n,
                        None => return Err(NodeError::DirectoryNotFound(component.to_string())),
                    }
                }
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
                Node::Dir(name_to_node) =>
                {
                    match name_to_node.get_mut(&component.to_string())
                    {
                        Some(n) => n,
                        None => return Err(NodeError::DirectoryNotFound(component.to_string())),
                    }
                }
            }
        }

        return Ok(node)
    }

    fn get_dir_map_mut(&mut self, dir_components : &Vec<&str>) -> Result<&mut HashMap<String, Node>, NodeError>
    {
        match self.get_node_mut(dir_components)?
        {
            Node::File(_) => Err(NodeError::Weird),
            Node::Dir(name_to_node) => Ok(name_to_node),
        }
    }

    fn get_dir_map(&self, dir_components : &Vec<&str>) -> Result<&HashMap<String, Node>, NodeError>
    {
        match self.get_node(dir_components)?
        {
            Node::File(_) => Err(NodeError::Weird),
            Node::Dir(name_to_node) => Ok(name_to_node),
        }
    }

    fn insert(&mut self, dir_components : Vec<&str>, name : &str, node : Node) -> Result<(), NodeError>
    {
        self.get_dir_map_mut(&dir_components)?.insert(name.to_string(), node);
        Ok(())
    }

    pub fn create_file(&mut self, path: &str, content : Content, timestamp : u64) -> Result<Content, NodeError>
    {
        let (dir_components, name) = get_dir_path_and_name(path)?;

        self.insert(dir_components, name, Node::File(FileInfo::new(
            Metadata::new(timestamp),
            content.clone())))?;

        Ok(content)
    }

    pub fn create_dir(&mut self, path: &str) -> Result<(), NodeError>
    {
        let (dir_components, name) = get_dir_path_and_name(path)?;
        self.insert(dir_components, name, Node::Dir(HashMap::new()))?;
        Ok(())
    }

    pub fn remove_file(&mut self, path: &str) -> Result<(), NodeError>
    {
        let (dir_components, name) = get_dir_path_and_name(path)?;

        match self.get_node_mut(&dir_components)?
        {
            Node::File(_) => match dir_components.last()
            {
                Some(last) => return Err(NodeError::FileInPlaceOfDirectory(last.to_string())),
                None => return Err(NodeError::Weird),
            },
            Node::Dir(name_to_node) => match name_to_node.remove(name)
            {
                Some(node) => match node
                {
                    Node::File(_) => Ok(()),
                    Node::Dir(_) => 
                    {
                        name_to_node.insert(name.to_string(), node);
                        Err(NodeError::RemoveFileFoundDir)
                    }
                },
                None => Err(NodeError::RemoveNonExistentFile)
            }
        }
    }

    pub fn remove_dir(&mut self, path: &str) -> Result<(), NodeError>
    {
        let (dir_components, name) = get_dir_path_and_name(path)?;

        let name_to_node = self.get_dir_map_mut(&dir_components)?;
        match name_to_node.remove(name)
        {
            Some(node) => match node
            {
                Node::File(_) => 
                {
                    name_to_node.insert(name.to_string(), node);
                    Err(NodeError::ExpectedDirFoundFile)
                }
                Node::Dir(_) => Ok(()),
            },
            None => Err(NodeError::RemoveNonExistentDir)
        }
    }

    pub fn list_dir(self, path: &str) -> Result<Vec<String>, NodeError>
    {
        let mut result : Vec<String> =
            self.get_dir_map(&get_components(path))?.clone().into_keys().collect();
        result.sort();
        Ok(result)
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), NodeError>
    {
        let (from_dir_components, from_name) = get_dir_path_and_name(from)?;
        let (to_dir_components, to_name) = get_dir_path_and_name(to)?;

        let from_name_to_node = self.get_dir_map_mut(&from_dir_components)?;

        match from_name_to_node.remove(from_name)
        {
            Some(moving_node) =>
            {
                match self.get_dir_map_mut(&to_dir_components)
                {
                    Ok(to_name_to_node) =>
                    {
                        to_name_to_node.insert(to_name.to_string(), moving_node);
                        Ok(())
                    }

                    Err(_) =>
                    {
                        let from_name_to_node = self.get_dir_map_mut(&from_dir_components)?;
                        from_name_to_node.insert(from_name.to_string(), moving_node);
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
                        return Err(NodeError::PathEmpty)
                }
            }
        }
    }

    pub fn get_modified(&self, path: &str) -> Result<SystemTime, NodeError>
    {
        let components = get_components(path);
        match self.get_node(&components)?
        {
            Node::File(info) => Ok(info.metadata.modified.clone()),
            Node::Dir(_) => Err(NodeError::GetModifiedOnDirectory),
        }
    }

    pub fn is_executable(&self, path: &str) -> Result<bool, NodeError>
    {
        let components = get_components(path);
        match self.get_node(&components)?
        {
            Node::File(info) => Ok(info.metadata.executable),
            Node::Dir(_) => Err(NodeError::IsExecutableOnDirectory),
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

        NodeError::PathEmpty
            => SystemError::PathEmpty,

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

        NodeError::GetModifiedOnDirectory
            => SystemError::NotImplemented,

        NodeError::IsExecutableOnDirectory
            => SystemError::NotImplemented,

        NodeError::Weird
            => SystemError::Weird,
    }
}

impl FakeSystem
{
    pub fn new(start : u64) -> Self
    {
        FakeSystem
        {
            root : Arc::new(Mutex::new(Node::empty_dir())),

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
        match self.get_root_node_mut().create_dir(path)
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
        match self.get_root_node_mut().remove_file(path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn remove_dir(&mut self, path: &str) -> Result<(), SystemError>
    {
        match self.get_root_node_mut().remove_dir(path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), SystemError>
    {
        match self.get_root_node_mut().rename(from, to)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn get_modified(&self, path: &str) -> Result<SystemTime, SystemError>
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

    fn execute_command(&mut self, command_list: Vec<String>) -> Result<CommandLineOutput, SystemError>
    {
        self.get_command_log_mut().push( command_list.join(" "));

        let n = command_list.len();
        if n <= 0
        {
            return Ok(CommandLineOutput::error(format!("Wrong number of arguments")));
        }

        let mut output = String::new();
        match command_list[0].as_str()
        {
            "error" =>
            {
                Ok(CommandLineOutput::error("Failed".to_string()))
            },

            "mycat" =>
            {
                for file in command_list[1..(n-1)].iter()
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
                                Err(_) => return Ok(CommandLineOutput::error(format!("File contained non utf8 bytes: {}", file))),
                            }
                        }
                        Err(_) =>
                        {
                            return Ok(CommandLineOutput::error(format!("File failed to open: {}", file)));
                        }
                    }
                }

                match write_str_to_file(self, &command_list[n-1], &output)
                {
                    Ok(_) => Ok(CommandLineOutput::new()),
                    Err(why) =>
                    {
                        Ok(CommandLineOutput::error(format!("Failed to cat into file: {} : {}", command_list[n-1], why)))
                    }
                }
            },

            /*  Takes source files followed by two targets, concats the sources and puts the result in both the
                targets.  For instance:

                mycat2 in1.txt in2.txt out1.txt out2.txt

                concatinates in1.txt in2.txt  puts a copy in out1.txt and out2.txt.*/
            "mycat2" =>
            {
                for file in command_list[1..(n-2)].iter()
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
                                Err(_) => return Ok(CommandLineOutput::error(
                                    format!("mycat2: file contained non utf8 bytes: {}", file))),
                            }
                        }
                        Err(_) =>
                        {
                            return Ok(CommandLineOutput::error(
                                format!("mycat2: file failed to open: {}", file)));
                        }
                    }
                }

                match write_str_to_file(self, &command_list[n-2], &output)
                {
                    Ok(_) => {},
                    Err(why) => return Ok(CommandLineOutput::error(
                        format!("mycat2: failed to cat into file: {}: {}", command_list[n-2], why)))
                }

                match write_str_to_file(self, &command_list[n-1], &output)
                {
                    Ok(_) => Ok(CommandLineOutput::new()),
                    Err(why) => return Ok(CommandLineOutput::error(
                        format!("mycat2: failed to cat into file: {}: {}", command_list[n-1], why)))
                }
            },

            "rm" =>
            {
                for file in command_list[1..n].iter()
                {
                    match self.remove_file(file)
                    {
                        Ok(()) => {}
                        Err(_) =>
                        {
                            return Ok(CommandLineOutput::error(format!("File failed to delete: {}", file)));
                        }
                    }
                }

                Ok(CommandLineOutput::new())
            },
            _=> Ok(CommandLineOutput::error(format!("Invalid command given: {}", command_list[0])))
        }
    }
}

#[cfg(test)]
mod test
{
    use crate::system::
    {
        System,
        ReadWriteError,
    };

    use crate::system::fake::
    {
        Content,
        FileInfo,
        Node,
        NodeError,
        get_components,
        get_dir_path_and_name,
        FakeSystem,
    };

    use crate::system::util::
    {
        write_str_to_file,
        read_file,
        get_timestamp,
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

    fn empty_string_vec() -> Vec<&'static str>
    {
        Vec::new()
    }

    #[test]
    fn get_components_general()
    {
        assert_eq!(get_components(""), empty_string_vec());
        assert_eq!(get_components("apples"), vec!["apples"]);
        assert_eq!(get_components("apples/bananas"), vec!["apples", "bananas"]);
    }

    #[test]
    fn get_dir_path_and_name_three()
    {
        match get_dir_path_and_name("fruit/apples/arkansas red")
        {
            Ok((components, name)) =>
            {
                assert_eq!(components, vec!["fruit", "apples"]);
                assert_eq!(name, "arkansas red");
            },
            Err(_) => panic!("Error splitting ordinary path"),
        }
    }

    #[test]
    fn get_dir_path_and_name_two()
    {
        match get_dir_path_and_name("apples/arkansas red")
        {
            Ok((components, name)) =>
            {
                assert_eq!(components, vec!["apples"]);
                assert_eq!(name, "arkansas red");
            },
            Err(_) => panic!("Error splitting ordinary path"),
        }
    }

    #[test]
    fn get_dir_path_and_name_one()
    {
        match get_dir_path_and_name("apples")
        {
            Ok((components, name)) =>
            {
                assert_eq!(components, empty_string_vec());
                assert_eq!(name, "apples");
            },
            Err(_) => panic!("Error splitting ordinary path"),
        }
    }

    #[test]
    fn get_dir_path_and_name_zero()
    {
        match get_dir_path_and_name("")
        {
            Ok((_components, _name)) => panic!("Unexpected success getting dir and path-name from empty path"),
            Err(error) =>
                match error
                {
                    NodeError::PathEmpty => {},
                    _ => panic!("Unexpected error type.  Expected PathEmpty"),
                },
        }
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
        let node = Node::empty_dir();
        assert!(!node.is_file(""));
        assert!(node.is_dir(""));
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
        let mut node = Node::empty_dir();
        match node.create_file("file.txt", Content::new(b"some text".to_vec()), 0)
        {
            Ok(_) => {},
            Err(error) => panic!("create_file in empty root failed with error: {}", error),
        }
        assert!(node.is_file("file.txt"));
        match node.remove_file("file.txt")
        {
            Ok(_) => {},
            Err(error) => panic!("remove_file in empty root failed with error: {}", error),
        }
        assert!(!node.is_file("file.txt"));
        assert!(!node.is_dir("file.txt"));
    }

    #[test]
    fn add_remove_dir()
    {
        let mut node = Node::empty_dir();
        match node.create_dir("images")
        {
            Ok(_) => {},
            Err(error) => panic!("create_dir in empty root failed with error: {}", error),
        }
        assert!(node.is_dir("images"));
        match node.remove_dir("images")
        {
            Ok(_) => {},
            Err(error) => panic!("remove_dir we just created in empty root failed with error: {}", error),
        }
        assert!(!node.is_file("images"));
        assert!(!node.is_dir("images"));
    }

    #[test]
    fn add_and_list_dir_empty()
    {
        let mut node = Node::empty_dir();
        node.create_dir("images").unwrap();
        let list = node.list_dir("images").unwrap();
        assert!(list.len() == 0);
    }

    #[test]
    fn add_and_list_dir_dir()
    {
        let mut node = Node::empty_dir();
        node.create_dir("images").unwrap();
        node.create_dir("images/more_images").unwrap();
        let list = node.list_dir("images").unwrap();
        assert_eq!(list, vec!["more_images".to_string()]);
    }

    #[test]
    fn add_and_list_dir_file()
    {
        let mut node = Node::empty_dir();
        node.create_dir("images").unwrap();
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
        let mut node = Node::empty_dir();
        node.create_dir("images").unwrap();
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
        let mut node = Node::empty_dir();
        match node.remove_file("file-not-there.txt")
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
        let mut node = Node::empty_dir();
        match node.remove_dir("dir-not-there")
        {
            Ok(_) => panic!("Unexpected sucess removing non-existent file"),
            Err(error) => match error
            {
                NodeError::RemoveNonExistentDir => {},
                _ => panic!("Attempt to remove non-existent dir resulted in wrong error.")
            }
        }
        assert!(!node.is_file("some text"));
    }

    #[test]
    fn rename_file()
    {
        let mut node = Node::empty_dir();
        match node.create_file("kitten.jpg", Content::new(b"jpg-content".to_vec()), 0)
        {
            Ok(_) => {},
            Err(error) => panic!("create_file in empty root failed with error: {}", error),
        }

        match node.create_dir("images")
        {
            Ok(_) => {},
            Err(error) => panic!("create_dir in almost empty root failed with error: {}", error),
        }

        assert!(node.is_file("kitten.jpg"));
        assert!(node.is_dir("images"));

        match node.rename("kitten.jpg", "images/kitten.jpg")
        {
            Ok(_) => {},
            Err(error) => panic!("rename failed with error: {}", error),
        }

        assert!(!node.is_file("kitten.jpg"));
        assert!(node.is_dir("images"));
        assert!(node.is_file("images/kitten.jpg"));
    }

    #[test]
    fn rename_directory()
    {
        let mut node = Node::empty_dir();
        match node.create_dir("images")
        {
            Ok(_) => {},
            Err(error) => panic!("create_dir in empty root failed with error: {}", error),
        }

        match node.create_file("images/kitten.jpg", Content::new(b"jpg-content".to_vec()), 0)
        {
            Ok(_) => {},
            Err(error) => panic!("create_file failed with error: {}", error),
        }

        assert!(node.is_file("images/kitten.jpg"));
        assert!(node.is_dir("images"));

        match node.rename("images", "images2")
        {
            Ok(_) => {},
            Err(error) => panic!("rename failed with error: {}", error),
        }

        assert!(node.is_dir("images2"));
        assert!(!node.is_dir("images"));
        assert!(node.is_file("images2/kitten.jpg"));
        assert!(!node.is_file("images/kitten.jpg"));
    }

    #[test]
    fn system_add_remove_file()
    {
        let mut system = FakeSystem::new(10);
        match system.create_file("file.txt")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file in FakeSystem failed with error: {}", error),
        }
        assert!(system.is_file("file.txt"));
        match system.remove_file("file.txt")
        {
            Ok(_) => {},
            Err(error) => panic!("remove_file in FakeSystem failed with error: {}", error),
        }
        assert!(!system.is_file("file.txt"));
        assert!(!system.is_dir("file.txt"));
    }

    #[test]
    fn system_add_remove_dir()
    {
        let mut system = FakeSystem::new(10);
        match system.create_dir("images")
        {
            Ok(_) => {},
            Err(error) => panic!("create_dir in FakeSystem failed with error: {}", error),
        }
        assert!(system.is_dir("images"));
        match system.remove_dir("images")
        {
            Ok(_) => {},
            Err(error) => panic!("remove_file in FakeSystem failed with error: {}", error),
        }
        assert!(!system.is_file("images"));
        assert!(!system.is_dir("images"));
    }

    #[test]
    fn system_create_file_write_read_round_trip()
    {
        let mut system = FakeSystem::new(10);
        match write_str_to_file(&mut system, "fruit_file.txt", "cantaloupe")
        {
            Ok(_) => {},
            Err(error) =>
            {
                match error
                {
                    ReadWriteError::SystemError(error) =>
                        panic!("SystemError in write: {}", error),

                    ReadWriteError::IOError(error) =>
                        panic!("IOError in write: {}", error),
                }
            }
        }
        match read_file(&system, "fruit_file.txt")
        {
            Ok(content) => assert_eq!(content, b"cantaloupe"),
            Err(error) =>
            {
                match error
                {
                    ReadWriteError::SystemError(error) =>
                        panic!("SystemError in read: {}", error),

                    ReadWriteError::IOError(error) =>
                        panic!("IOError in read: {}", error),
                }
            }
        }
    }

    #[test]
    fn system_create_file_write_read_round_trip_with_cloning()
    {
        let mut system1 = FakeSystem::new(10);
        let system2 = system1.clone();
        match write_str_to_file(&mut system1, "fruit_file.txt", "cantaloupe")
        {
            Ok(_) => {},
            Err(error) =>
            {
                match error
                {
                    ReadWriteError::SystemError(error) =>
                        panic!("SystemError in write: {}", error),

                    ReadWriteError::IOError(error) =>
                        panic!("IOError in write: {}", error),
                }
            }
        }

        match read_file(&system2, "fruit_file.txt")
        {
            Ok(content) => assert_eq!(content, b"cantaloupe"),
            Err(error) =>
            {
                match error
                {
                    ReadWriteError::SystemError(error) =>
                        panic!("SystemError in read: {}", error),

                    ReadWriteError::IOError(error) =>
                        panic!("IOError in read: {}", error),
                }
            }
        }
    }

    #[test]
    fn system_create_file_write_read_round_trip_read_twice()
    {
        let mut system = FakeSystem::new(10);
        match write_str_to_file(&mut system, "fruit_file.txt", "cantaloupe")
        {
            Ok(_) => {},
            Err(error) =>
            {
                match error
                {
                    ReadWriteError::SystemError(error) =>
                        panic!("SystemError in write: {}", error),

                    ReadWriteError::IOError(error) =>
                        panic!("IOError in write: {}", error),
                }
            }
        }
        for _ in 0..2
        {
            match read_file(&system, "fruit_file.txt")
            {
                Ok(content) => assert_eq!(content, b"cantaloupe"),
                Err(error) =>
                {
                    match error
                    {
                        ReadWriteError::SystemError(error) =>
                            panic!("SystemError in read: {}", error),

                        ReadWriteError::IOError(error) =>
                            panic!("IOError in read: {}", error),
                    }
                }
            }
        }
    }

    #[test]
    fn system_rename_file()
    {
        let mut system = FakeSystem::new(10);
        match system.create_file("star.png")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file SystemError: {}", error),
        }

        assert!(system.is_file("star.png"));
        assert!(!system.is_file("heart.png"));

        match system.rename("star.png", "heart.png")
        {
            Ok(_) => {},
            Err(error) => panic!("rename SystemError: {}", error),
        }

        assert!(!system.is_file("star.png"));
        assert!(system.is_file("heart.png"));
    }

    #[test]
    fn modified_timestamps()
    {
        let mut system = FakeSystem::new(17);
        match system.create_file("star.png")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file SystemError: {}", error),
        }

        system.time_passes(17);
        match system.create_file("heart.png")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file SystemError: {}", error),
        }

        match system.get_modified("star.png")
        {
            Ok(system_time) => match get_timestamp(system_time)
            {
                Ok(timestamp) => assert_eq!(timestamp, 17),
                Err(error) => panic!("get_modified SystemTimeError: {}", error),
            },
            Err(error) => panic!("get_modified SystemError: {}", error),
        }

        match system.get_modified("heart.png")
        {
            Ok(system_time) => match get_timestamp(system_time)
            {
                Ok(timestamp) => assert_eq!(timestamp, 34),
                Err(error) => panic!("get_modified SystemTimeError: {}", error),
            },
            Err(error) => panic!("get_modified SystemError: {}", error),
        }
    }

    #[test]
    fn writing_updates_modified_timestamp()
    {
        let mut system = FakeSystem::new(0);

        system.time_passes(5);

        match system.create_file("cars.txt")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file SystemError: {}", error),
        }

        system.time_passes(6);

        match write_str_to_file(&mut system, "cars.txt", "cantaloupe")
        {
            Ok(_) => {},
            Err(error) =>
            {
                match error
                {
                    ReadWriteError::SystemError(error) =>
                        panic!("SystemError in write: {}", error),

                    ReadWriteError::IOError(error) =>
                        panic!("IOError in write: {}", error),
                }
            }
        }

        match system.get_modified("cars.txt")
        {
            Ok(system_time) => match get_timestamp(system_time)
            {
                Ok(timestamp) => assert_eq!(timestamp, 11),
                Err(error) => panic!("get_modified SystemTimeError: {}", error),
            },
            Err(error) => panic!("get_modified SystemError: {}", error),
        }
    }


    #[test]
    fn executing_error_gives_error_output()
    {
        let mut system = FakeSystem::new(10);
        match system.execute_command(vec!["error".to_string()])
        {
            Ok(output) =>
            {
                assert_eq!(output.out, "".to_string());
                assert_eq!(output.err, "Failed".to_string());
                assert_eq!(output.code, Some(1));
                assert_eq!(output.success, false);
            },
            Err(error) => panic!("Excpected successful command invocation got error: {}", error),
        }
    }

    #[test]
    fn executing_mycat_concatinates()
    {
        let mut system = FakeSystem::new(10);
        match system.create_file("line1.txt")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file SystemError: {}", error),
        }

        match write_str_to_file(&mut system, "line1.txt", "Ants\n")
        {
            Ok(_) => {},
            Err(error) => panic!("Error writing line1.txt: {}", error),
        }

        match system.create_file("line2.txt")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file SystemError: {}", error),
        }

        match write_str_to_file(&mut system, "line2.txt", "Love to dance\n")
        {
            Ok(_) => {},
            Err(error) => panic!("Error writing line2.txt: {}", error),
        }

        match system.execute_command(
            vec![
                "mycat".to_string(),
                "line1.txt".to_string(),
                "line2.txt".to_string(),
                "poem.txt".to_string()])
        {
            Ok(output) =>
            {
                assert_eq!(output.out, "".to_string());
                assert_eq!(output.err, "".to_string());
                assert_eq!(output.code, Some(0));
                assert_eq!(output.success, true);
            },
            Err(error) => panic!("Excpected successful command invocation got error: {}", error),
        }

        match read_file(&system, "poem.txt")
        {
            Ok(content) => assert_eq!(content, b"Ants\nLove to dance\n"),
            Err(error) => panic!("{}", error),
        }
    }


    #[test]
    fn executing_mycat2_concatinates_and_dupes()
    {
        let mut system = FakeSystem::new(10);
        match system.create_file("line1.txt")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file SystemError: {}", error),
        }

        match write_str_to_file(&mut system, "line1.txt", "Ants\n")
        {
            Ok(_) => {},
            Err(error) => panic!("Error writing line1.txt: {}", error),
        }

        match system.create_file("line2.txt")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file SystemError: {}", error),
        }

        match write_str_to_file(&mut system, "line2.txt", "Love to dance\n")
        {
            Ok(_) => {},
            Err(error) => panic!("Error writing line2.txt: {}", error),
        }

        match system.execute_command(
            vec![
                "mycat2".to_string(),
                "line1.txt".to_string(),
                "line2.txt".to_string(),
                "poem.txt".to_string(),
                "poem-backup.txt".to_string()])
        {
            Ok(output) =>
            {
                assert_eq!(output.out, "".to_string());
                assert_eq!(output.err, "".to_string());
                assert_eq!(output.code, Some(0));
                assert_eq!(output.success, true);
            },
            Err(error) => panic!("Excpected successful command invocation got error: {}", error),
        }

        match read_file(&system, "poem.txt")
        {
            Ok(content) => assert_eq!(content, b"Ants\nLove to dance\n"),
            Err(error) => panic!("{}", error),
        }

        match read_file(&system, "poem-backup.txt")
        {
            Ok(content) => assert_eq!(content, b"Ants\nLove to dance\n"),
            Err(error) => panic!("{}", error),
        }
    }


    #[test]
    fn use_commandline_to_remove()
    {
        let mut system = FakeSystem::new(10);
        match system.create_file("terrible-file.txt")
        {
            Ok(_) => {},
            Err(error) => panic!("create_file SystemError: {}", error),
        }

        assert!(system.is_file("terrible-file.txt"));

        match system.execute_command(
            vec![
                "rm".to_string(),
                "terrible-file.txt".to_string()
            ])
        {
            Ok(output) =>
            {
                assert_eq!(output.out, "".to_string());
                assert_eq!(output.err, "".to_string());
                assert_eq!(output.code, Some(0));
                assert_eq!(output.success, true);
            },
            Err(error) => panic!("Expected smooth commandline invocation, got error: {}", error),
        }

        assert!(!system.is_file("terrible-file.txt"));

    }
}
