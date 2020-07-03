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
use std::time::SystemTime;
use crate::system::
{
    System,
    SystemError,
    CommandLineOutput
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
enum Node
{
    File(Content),
    Dir(HashMap<String, Node>)
}

enum NodeError
{
    FileInPlaceOfDirectory(String),
    DirectoryInPlaceOfFile(String),
    DirectoryNotFound(String),
    PathEmpty,
    RemoveFileFoundDir,
    RemoveDirFoundFile,
    RemoveNonExistentFile,
    RemoveNonExistentDir,
    RenameFromNonExistent,
    RenameToNonExistent,
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

            NodeError::RemoveDirFoundFile
                => write!(formatter, "Attempt to remove directory, found file"),

            NodeError::RemoveNonExistentFile
                => write!(formatter, "Attempt to remove non-existent file"),

            NodeError::RemoveNonExistentDir
                => write!(formatter, "Attempt to remove non-existent directory"),

            NodeError::RenameFromNonExistent
                => write!(formatter, "Attempt to rename a non-existent file or directory"),

            NodeError::RenameToNonExistent
                => write!(formatter, "Attempt to rename a file or directory with non-existent target directory"),

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
            Err(_) =>
                false
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

    pub fn get_dir_map_mut(&mut self, dir_components : &Vec<&str>) -> Result<&mut HashMap<String, Node>, NodeError>
    {
        match self.get_node_mut(dir_components)?
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

    pub fn create_file(&mut self, path: &str, content : Content) -> Result<Content, NodeError>
    {
        let (dir_components, name) = get_dir_path_and_name(path)?;
        self.insert(dir_components, name, Node::File(content.clone()))?;
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
                    Err(NodeError::RemoveDirFoundFile)
                }
                Node::Dir(_) => Ok(()),
            },
            None => Err(NodeError::RemoveNonExistentDir)
        }
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
                drop(from_name_to_node);
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
            Node::File(content) => Ok(content),
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
    root: Node,
}

fn convert_node_error_to_system_error(error : NodeError) -> SystemError
{
    match error
    {
        NodeError::FileInPlaceOfDirectory(component)
            => SystemError::FileInPlaceOfDirectory(component),

        NodeError::DirectoryInPlaceOfFile(component)
            => SystemError::DirectoryInPlaceOfFile(component),

        NodeError::DirectoryNotFound(component)
            => SystemError::DirectoryNotFound(component),

        NodeError::PathEmpty
            => SystemError::PathEmpty,

        NodeError::RemoveFileFoundDir
            => SystemError::RemoveFileFoundDir,

        NodeError::RemoveDirFoundFile
            => SystemError::RemoveDirFoundFile,

        NodeError::RemoveNonExistentFile
            => SystemError::RemoveNonExistentFile,

        NodeError::RemoveNonExistentDir
            => SystemError::RemoveNonExistentDir,

        NodeError::RenameFromNonExistent
            => SystemError::RenameFromNonExistent,

        NodeError::RenameToNonExistent
            => SystemError::RenameToNonExistent,

        NodeError::Weird
            => SystemError::Weird,
    }
}

impl FakeSystem
{
    fn new() -> Self
    {
        FakeSystem
        {
            root : Node::empty_dir()
        }
    }
}

impl System for FakeSystem
{
    type File = FakeOpenFile;

    fn open(&self, path: &str) -> Result<Self::File, SystemError>
    {
        match self.root.open_file(path)
        {
            Ok(content) =>
                Ok(FakeOpenFile::new(content, AccessMode::Read)),

            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn create_file(&mut self, path: &str) -> Result<Self::File, SystemError>
    {
        match self.root.create_file(path, Content::empty())
        {
            Ok(content) => Ok(FakeOpenFile::new(&content, AccessMode::Write)),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn create_dir(&mut self, path: &str) -> Result<(), SystemError>
    {
        match self.root.create_dir(path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn is_file(&self, path: &str) -> bool
    {
        self.root.is_file(path)
    }

    fn is_dir(&self, path: &str) -> bool
    {
        self.root.is_dir(path)
    }

    fn remove_file(&mut self, path: &str) -> Result<(), SystemError>
    {
        match self.root.remove_file(path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn remove_dir(&mut self, path: &str) -> Result<(), SystemError>
    {
        match self.root.remove_dir(path)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), SystemError>
    {
        match self.root.rename(from, to)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(convert_node_error_to_system_error(error)),
        }
    }

    fn get_modified(&self, _path: &str) -> Result<SystemTime, SystemError>
    {
        Ok(SystemTime::UNIX_EPOCH)
    }

    fn execute_command(&mut self, _command_list: Vec<String>) -> Result<CommandLineOutput, SystemError>
    {
        Ok(CommandLineOutput::new())
    }
}

#[cfg(test)]
mod test
{
    use std::io::
    {
        Error,
        Read,
        Write
    };

    use crate::system::
    {
        System,
        SystemError,
        // CommandLineOutput
    };

    use crate::system::fake::
    {
        Content,
        Node,
        NodeError,
        get_components,
        get_dir_path_and_name,
        FakeSystem
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
                    _ => panic!("Unexpected error type.  Expected PathEmtpy"),
                },
        }
    }

    #[test]
    fn file_is_file()
    {
        let node = Node::File(Content::new(b"things".to_vec()));
        assert!(node.is_file(""));
        assert!(!node.is_dir(""));
    }

    #[test]
    fn new_empty_file_is_file()
    {
        let node = Node::File(Content::new(b"".to_vec()));
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
        let node = Node::File(Content::new(b"stuff".to_vec()));
        assert!(!node.is_file("stuf-not-there"));
        assert!(!node.is_dir("stuf-not-there"));
    }

    #[test]
    fn add_remove_file()
    {
        let mut node = Node::empty_dir();
        match node.create_file("file.txt", Content::new(b"some text".to_vec()))
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
        match node.create_file("kitten.jpg", Content::new(b"jpg-content".to_vec()))
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

        match node.create_file("images/kitten.jpg", Content::new(b"jpg-content".to_vec()))
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

    enum ReadWriteError
    {
        IOError(Error),
        SystemError(SystemError)
    }

    /*  Takes a FileSystem, a path as a &str and content, also a &str writes the content to the file.
        If system fails, forwards the system error.  If file-io fails, forwards the std::io::Error. */
    fn write_str_to_file
    <
        SystemType : System,
    >
    (
        system : &mut SystemType,
        file_path : &str,
        content : &str
    )
    -> Result<(), ReadWriteError>
    {
        match system.create_file(file_path)
        {
            Ok(mut file) =>
            {
                match file.write_all(content.as_bytes())
                {
                    Ok(_) => Ok(()),
                    Err(error) => Err(ReadWriteError::IOError(error)),
                }
            }
            Err(error) => Err(ReadWriteError::SystemError(error))
        }
    }

    /*  Reads binary data from a file in a FileSystem into a Vec<u8>.
        If system fails, forwards the system error.  If file-io fails, forwards the std::io::Error. */
    fn read_file
    <
        F : System,
    >
    (
        system : &F,
        path : &str
    )
    -> Result<Vec<u8>, ReadWriteError>
    {
        match system.open(path)
        {
            Ok(mut file) =>
            {
                let mut content = Vec::new();
                match file.read_to_end(&mut content)
                {
                    Ok(_size) =>
                    {
                        return Ok(content);
                    }
                    Err(error) => Err(ReadWriteError::IOError(error)),
                }
            }
            Err(error) => Err(ReadWriteError::SystemError(error)),
        }
    }

    #[test]
    fn system_add_remove_file()
    {
        let mut system = FakeSystem::new();
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
        let mut system = FakeSystem::new();
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
        let mut system = FakeSystem::new();
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
    fn system_rename_file()
    {
        let mut system = FakeSystem::new();
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
}
