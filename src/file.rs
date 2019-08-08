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
