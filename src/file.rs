use std::io::prelude::*;
use std::path::Path;
use std::fs::File;

pub fn read(path_as_string: &str) -> Result<String, String>
{
    let path = Path::new(&path_as_string);
    match File::open(&path)
    {
        Err(_why) => Err(
            format!("Could not open file: {}", path.display())),

        Ok(mut file) =>
        {
            let mut content = String::new();
            match file.read_to_string(&mut content)
            {
                Err(_why) => Err(format!("File content not text: {}",
                    path.display())),
                Ok(_) => Ok(content),
            }
        },
    }
}

#[cfg(test)]
mod test
{
    use filesystem::{FileSystem, OsFileSystem, FakeFileSystem};
    use std::path::Path;
    use std::thread::{self, JoinHandle};

    #[test]
    fn test_os_files()
    {
        let fs = OsFileSystem::new();

        match fs.read_file_to_string(&Path::new("A.txt"))
        {
            Ok(_) => {},
            Err(_) => panic!("File not found"),
        }
    }

    fn do_test_is_there<T: FileSystem+Send+'static>(fs: T) -> JoinHandle<Result<(), String>>
    {
        thread::spawn(
            move || -> Result<(), String>
            {
                match fs.read_file_to_string(&Path::new("A.txt"))
                {
                    Ok(_content) => Ok(()),
                    Err(_) => Err(format!("No file found")),
                }
            }
        )
    }

    #[test]
    fn test_fake_files()
    {
        let fs = FakeFileSystem::new();
        let newfs = fs.clone();

        match fs.write_file(&Path::new("somefile_1.txt"), "stuff")
        {
            Ok(_) => {},
            Err(_) => panic!("Couldn't write file"),
        }

        match fs.read_file_to_string(&Path::new("somefile_1.txt"))
        {
            Ok(stuff) => assert_eq!("stuff", stuff),
            Err(_) => panic!("File not found"),
        }

        match newfs.read_file_to_string(&Path::new("somefile_1.txt"))
        {
            Ok(stuff) => assert_eq!("stuff", stuff),
            Err(_) => panic!("File not found"),
        }
    }

    #[test]
    fn test_fake_filesystem_trait()
    {
        let fs = FakeFileSystem::new();
        match do_test_is_there(fs).join()
        {
            Ok(_) => {},
            Err(_) => panic!("Thread ran and returned error"),
        }
    }
}
