use std::str::from_utf8;
use std::process::Output;

pub struct CommandLineOutput
{
    pub out : String,
    pub err : String,
    pub code : Option<i32>,
    pub success : bool,
}

impl CommandLineOutput
{
    pub fn new() -> CommandLineOutput
    {
        CommandLineOutput
        {
            out : "".to_string(),
            err : "".to_string(),
            code : Some(0),
            success : true,
        }
    }

    pub fn from_output(output: Output) -> CommandLineOutput
    {
        CommandLineOutput
        {
            out : match from_utf8(&output.stdout)
            {
                Ok(text) => text,
                Err(_) => "<non-utf8 data>",
            }.to_string(),

            err : match from_utf8(&output.stderr)
            {
                Ok(text) => text,
                Err(_) => "<non-utf8 data>",
            }.to_string(),

            code : output.status.code(),
            success : output.status.success(),
        }
    }
}

pub trait Executor
{
    fn execute_command(&self, command_list: Vec<String>) -> Result<CommandLineOutput, String>;
}
