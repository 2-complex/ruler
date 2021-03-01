extern crate clap;
extern crate toml;
extern crate serde;
extern crate execute;

use clap::
{
    Arg,
    App,
    SubCommand,
    AppSettings
};
use serde::
{
    Serialize,
    Deserialize
};
use std::fmt;
use crate::system::
{
    System,
    SystemError,
    ReadWriteError
};
use crate::system::util::
{
    read_file_to_string,
    write_str_to_file,
    ReadFileToStringError,
};
use crate::system::real::RealSystem;
use crate::printer::StandardPrinter;

mod cache;
mod build;
mod rule;
mod ticket;
mod work;
mod memory;
mod packet;
mod printer;
mod system;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct BuildInvocation
{
    rules: Option<Vec<String>>,
    target: Option<String>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Config
{
    again: Option<BuildInvocation>
}

impl Config
{
    fn new() -> Config
    {
        Config
        {
            again : None
        }
    }
}

pub enum ConfigError
{
    FailedToCreateDirectory(SystemError),
    FailedToCreateConfigFile(ReadWriteError),
    FailedToReadConfigFile(ReadFileToStringError),
    TomlDeError(toml::de::Error),
    TomlSerError(toml::ser::Error),
}

impl fmt::Display for ConfigError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ConfigError::FailedToCreateDirectory(error) =>
                write!(formatter, "Failed to create directory: {}", error),

            ConfigError::FailedToCreateConfigFile(error) =>
                write!(formatter, "Failed to create config file: {}", error),

            ConfigError::FailedToReadConfigFile(error) =>
                write!(formatter, "Failed to create cache directory: {}", error),

            ConfigError::TomlDeError(error) =>
                write!(formatter, "Config file opened, but failed to parse as toml: {}", error),

            ConfigError::TomlSerError(error) =>
                write!(formatter, "Config failed to encode as toml: {}", error),
        }
    }
}

/*  From the given .ruler directry read the config file and parse as toml to obtain a
    Config object.  If any part of that fails, forward the appropriate error. */
fn read_config<SystemType : System>
(
    system : &mut SystemType,
    directory : &str
)
->
Result<Config, ConfigError>
{
    if ! system.is_dir(directory)
    {
        match system.create_dir(directory)
        {
            Ok(_) => {},
            Err(error) => return Err(ConfigError::FailedToCreateDirectory(error)),
        }
    }

    let config_path = format!("{}/config.toml", directory);

    if system.is_file(&config_path)
    {
        match read_file_to_string(system, &config_path)
        {
            Ok(content_string) =>
            {
                return
                match toml::from_str(&content_string)
                {
                    Ok(config) => Ok(config),
                    Err(error) => Err(ConfigError::TomlDeError(error)),
                }
            },
            Err(error) => return Err(ConfigError::FailedToReadConfigFile(error)),
        }
    }
    else
    {
        let default_config = Config::new();
        match toml::to_string(&default_config)
        {
            Ok(config_toml) =>
            match write_str_to_file(system, &config_path, &config_toml)
            {
                Ok(_) => Ok(default_config),
                Err(error) => Err(ConfigError::FailedToCreateConfigFile(error)),
            },
            Err(error) => Err(ConfigError::TomlSerError(error)),
        }
    }
}

/*  In the given directory, write the config object to toml file.  If any part of that
    goes wrong, error. */
fn write_config<SystemType : System>
(
    system : &mut SystemType,
    directory : &str,
    config : &Config
)
->
Result<(), ConfigError>
{
    if ! system.is_dir(directory)
    {
        match system.create_dir(directory)
        {
            Ok(_) => {},
            Err(error) => return Err(ConfigError::FailedToCreateDirectory(error)),
        }
    }

    let config_path = format!("{}/config.toml", directory);

    match toml::to_string(config)
    {
        Ok(config_toml) =>
        match write_str_to_file(system, &config_path, &config_toml)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(ConfigError::FailedToCreateConfigFile(error)),
        },
        Err(error) => Err(ConfigError::TomlSerError(error)),
    }
}

fn main()
{
    let big_matches = App::new("Ruler")
        .version("0.1.5")
        .author("Peterson Trethewey <peterson@2-complex.com>")
        .about("
Ruler is a tool for managing a dependence graph of files.  It works with a
.rules file.  A .rules file contains newline-separated blocks called 'rules'.
Each rule looks like this:

path/to/target/file1
path/to/target/file2
:
path/to/source/file1
path/to/source/file2
path/to/source/file3
:
command
--option1=value1
--option2=value2
:

The command-line invocation is meant to update the target files using the
source files as input.

Ruler maintains a history of file-hashes to determine whether a target needs to
update.  When you type a build command such as:

ruler build

Ruler checks the current state of the targets against the hashes it has on
file, determines which ones need to update and in what order, and runs the
commands for those rules.

")
        .subcommand(
            SubCommand::with_name("clean")
            .about("Removes all targets")
            .help("
Removes all files and directories specificed as targets in the rules file.
If a target is specified, removes all that targets ancestors.

Note: clean does not delete the files, it moves them to a cache so they can be
recovered later if needed.
")
            .arg(Arg::with_name("target")
                .help("
The path to the clean-target.  The clean command removes all files listed as
targets in rules that the clean-target depends on.  If no clean-target is
specified, the clean command removes all files listed as targets in any rule.
")
                .required(false)
                .index(1)
            )
        )
        .subcommand(
            SubCommand::with_name("build")
            .about("Builds the given target or all targets")
            .help("
Builds the given target.  If no build-target is specified, builds all targets.
The target must be a file listed in the target section of the current rules
file.  The rules file is either a file in the current working directory called
\"build.rules\" or it can be specificed using --rules=<path>
")
            .arg(Arg::with_name("target")
                .help("
Path to a specific build-target to build.  Ruler will only build this target,
and its ancestors, as needed.")
                .required(false)
                .index(1)
            )
        )
        .subcommand(
            SubCommand::with_name("again")
            .about("Repeats the most recent build command")
            .help("
Repeats the most recent `ruler build` invocation.  To get started, type `ruler build`.
The next time you run `ruler again`, it will repeat that `ruler build` with the same options.
")
            .arg(Arg::with_name("target")
                .help("
Path to a specific build-target to build.  Ruler will only build this target, and its ancestors, as needed.")
                .required(false)
                .index(1)
            )
        )
        .arg(Arg::with_name("rules")
            .short("r")
            .long("rules")
            .value_name("rules")
            .help("Path to a custom rules file (default: build.rules)")
            .takes_value(true))
        .setting(AppSettings::ArgRequiredElseHelp)
        .get_matches();

    if let Some(matches) = big_matches.subcommand_matches("again")
    {
        let directory =
        match matches.value_of("directory")
        {
            Some(value) => value,
            None => ".ruler",
        };

        let mut system = RealSystem::new();
        match read_config(&mut system, &directory)
        {
            Ok(config) =>
                match config.again
                {
                    Some(again) => 
                    {
                        let rules =
                        match again.rules
                        {
                            Some(value) => value.clone(),
                            None => vec!["build.rules".to_string()],
                        };

                        let target =
                        match again.target
                        {
                            Some(value) => Some(value.to_string()),
                            None => None,
                        };

                        let mut printer = StandardPrinter::new();
                        match build::build(
                            system,
                            directory,
                            rules,
                            target,
                            &mut printer)
                        {
                            Ok(()) => {},
                            Err(error) => eprintln!("{}", error),
                        }
                    }
                    None =>
                    {
                        println!("Repeats the most recent `ruler build` invocation.  To get started, type `ruler build`.
The next time you run `ruler again`, it will repeat that `ruler build` with the same options.");
                    },
                }
            Err(config_error) => println!("Error reading config: {}", config_error),
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("clean")
    {
        let directory =
        match matches.value_of("directory")
        {
            Some(value) => value,
            None => ".ruler",
        };

        let rulefiles =
        match matches.values_of("rules")
        {
            Some(values) => values.map(|s| s.to_string()).collect(),
            None => vec!("build.rules".to_string()),
        };

        let target =
        match matches.value_of("target")
        {
            Some(value) => Some(value.to_string()),
            None => None,
        };

        match build::clean(
            RealSystem::new(), directory, rulefiles, target)
        {
            Ok(()) => {},
            Err(error) => eprintln!("{}", error),
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("build")
    {
        let rulefiles =
        match matches.values_of("rules")
        {
            Some(values) => values.map(|s| s.to_string()).collect(),
            None => vec!("build.rules".to_string()),
        };

        let directory =
        match matches.value_of("directory")
        {
            Some(value) => value,
            None => ".ruler",
        };

        let target =
        match matches.value_of("target")
        {
            Some(value) => Some(value.to_string()),
            None => None,
        };

        let config = Config
        {
            again : Some(
                BuildInvocation
                {
                    target : target.clone(),
                    rules : Some(rulefiles.clone()),
                }
            )
        };

        let mut system = RealSystem::new();
        let mut printer = StandardPrinter::new();

        match write_config(&mut system, &directory, &config)
        {
            Ok(()) =>
            {
                match build::build(
                    system,
                    directory,
                    rulefiles,
                    target,
                    &mut printer)
                {
                    Ok(()) => {},
                    Err(error) => eprintln!("{}", error),
                }
            },
            Err(error) =>
            {
                println!("Error writing config file: {}", error);
            }
        }

    }
}
