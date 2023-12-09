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
use crate::downloader::
{
    download_file
};

mod blob;
mod build;
mod cache;
mod directory;
mod current;
mod history;
mod packet;
mod printer;
mod rule;
mod server;
mod system;
mod ticket;
mod work;
mod downloader;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct BuildInvocation
{
    rules: Option<Vec<String>>,
    target: Option<String>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct PersistentInfo
{
    again: Option<BuildInvocation>,
}

impl PersistentInfo
{
    fn new() -> PersistentInfo
    {
        PersistentInfo
        {
            again : None
        }
    }
}

pub enum PersistentInfoError
{
    FailedToCreateDirectory(SystemError),
    FailedToCreatePersistentInfoFile(ReadWriteError),
    FailedToReadPersistentInfoFile(ReadFileToStringError),
    TomlDeError(toml::de::Error),
    TomlSerError(toml::ser::Error),
}

impl fmt::Display for PersistentInfoError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            PersistentInfoError::FailedToCreateDirectory(error) =>
                write!(formatter, "Failed to create directory: {}", error),

            PersistentInfoError::FailedToCreatePersistentInfoFile(error) =>
                write!(formatter, "Failed to create persistent_info file: {}", error),

            PersistentInfoError::FailedToReadPersistentInfoFile(error) =>
                write!(formatter, "Failed to create cache directory: {}", error),

            PersistentInfoError::TomlDeError(error) =>
                write!(formatter, "PersistentInfo file opened, but failed to parse as toml: {}", error),

            PersistentInfoError::TomlSerError(error) =>
                write!(formatter, "PersistentInfo failed to encode as toml: {}", error),
        }
    }
}

/*  From the given .ruler directry read the persistent_info.toml file and parse as toml to
    obtain a PersistentInfo object.  If any part of that fails, forward the appropriate error. */
fn read_persistent_info<SystemType : System>
(
    system : &mut SystemType,
    directory : &str
)
->
Result<PersistentInfo, PersistentInfoError>
{
    if ! system.is_dir(directory)
    {
        match system.create_dir(directory)
        {
            Ok(_) => {},
            Err(error) => return Err(PersistentInfoError::FailedToCreateDirectory(error)),
        }
    }

    let persistent_info_path = format!("{}/persistent_info.toml", directory);

    if system.is_file(&persistent_info_path)
    {
        match read_file_to_string(system, &persistent_info_path)
        {
            Ok(content_string) =>
            {
                return
                match toml::from_str(&content_string)
                {
                    Ok(persistent_info) => Ok(persistent_info),
                    Err(error) => Err(PersistentInfoError::TomlDeError(error)),
                }
            },
            Err(error) => return Err(PersistentInfoError::FailedToReadPersistentInfoFile(error)),
        }
    }
    else
    {
        let default_persistent_info = PersistentInfo::new();
        match toml::to_string(&default_persistent_info)
        {
            Ok(persistent_info_toml) =>
            match write_str_to_file(system, &persistent_info_path, &persistent_info_toml)
            {
                Ok(_) => Ok(default_persistent_info),
                Err(error) => Err(PersistentInfoError::FailedToCreatePersistentInfoFile(error)),
            },
            Err(error) => Err(PersistentInfoError::TomlSerError(error)),
        }
    }
}

/*  In the given directory, write the persistent info object to toml file.  If any part of that
    goes wrong, error. */
fn write_persistent_info<SystemType : System>
(
    system : &mut SystemType,
    directory : &str,
    persistent_info : &PersistentInfo
)
->
Result<(), PersistentInfoError>
{
    if ! system.is_dir(directory)
    {
        match system.create_dir(directory)
        {
            Ok(_) => {},
            Err(error) => return Err(PersistentInfoError::FailedToCreateDirectory(error)),
        }
    }

    let persistent_info_path = format!("{}/persistent_info.toml", directory);

    match toml::to_string(persistent_info)
    {
        Ok(persistent_info_toml) =>
        match write_str_to_file(system, &persistent_info_path, &persistent_info_toml)
        {
            Ok(_) => Ok(()),
            Err(error) => Err(PersistentInfoError::FailedToCreatePersistentInfoFile(error)),
        },
        Err(error) => Err(PersistentInfoError::TomlSerError(error)),
    }
}


#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Config
{
    download_urls: Option<Vec<String>>,
}

pub enum ConfigError
{
    FileNotFound,
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
            ConfigError::FileNotFound =>
                write!(formatter, "Config file not found"),

            ConfigError::FailedToReadConfigFile(error) =>
                write!(formatter, "Failed to create cache directory: {}", error),

            ConfigError::TomlDeError(error) =>
                write!(formatter, "Config file opened, but failed to parse as toml: {}", error),

            ConfigError::TomlSerError(error) =>
                write!(formatter, "Config failed to encode as toml: {}", error),
        }
    }
}

/*  Open ruler.toml if it exists, load config data */
fn read_config<SystemType : System>
(
    system : &mut SystemType
)
->
Result<Config, ConfigError>
{
    let config_path = "ruler.toml";
    if ! system.is_file(config_path)
    {
        return Err(ConfigError::FileNotFound);
    }

    match read_file_to_string(system, config_path)
    {
        Ok(content_string) =>
        {
            match toml::from_str(&content_string)
            {
                Ok(config) => Ok(config),
                Err(error) => Err(ConfigError::TomlDeError(error)),
            }
        },
        Err(error) => return Err(ConfigError::FailedToReadConfigFile(error)),
    }
}

fn main()
{
    let big_matches = App::new("Ruler")
        .version("0.1.6")
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
            .arg(Arg::with_name("rules")
                .short("r")
                .long("rules")
                .value_name("rules")
                .multiple(true)
                .help("Path to a custom rules file (default: build.rules)")
                .takes_value(true))
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
            .arg(Arg::with_name("rules")
                .short("r")
                .long("rules")
                .value_name("rules")
                .multiple(true)
                .help("Path to a custom rules file (default: build.rules)")
                .takes_value(true))
        )
        .subcommand(
            SubCommand::with_name("hash")
            .about("Outputs the hash of a file")
            .help("
Takes a path to a file, returns the url-safe-base64-encoded sha256 of the file.
")
            .arg(Arg::with_name("path")
                .help("
Path to any file.
")
                .required(true)
                .index(1)
            )
        )
        .setting(AppSettings::ArgRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("nodes")
            .about("Displays info on current build-nodes along with their
current rule-hash")
            .help("
Reads the rules files the same way as when invoking ruler build, except instead
of running the build process, prints information about each node.  This command
is read only.  It is useful for troubleshooting and understanding how ruler
works.
")
        )
        .setting(AppSettings::ArgRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("again")
            .about("Repeats the most recent build command")
            .help("
Repeats the most recent `ruler build` invocation.  To get started, type
`ruler build`.  The next time you run `ruler again`, it will repeat that
`ruler build` with the same options.
")
            .arg(Arg::with_name("target")
                .help("
Path to a specific build-target to build.  Ruler will only build this target,
and its ancestors, as needed.")
                .required(false)
                .index(1)
            )
            .arg(Arg::with_name("rules")
                .short("r")
                .long("rules")
                .value_name("rules")
                .multiple(true)
                .help("Path to a custom rules file (default: build.rules)")
                .takes_value(true))
        )
        .subcommand(
            SubCommand::with_name("download")
            .about("Downloads a file from a ruler server to the target path")
            .help("Downloads a file from a ruler server to the target path")
            .arg(Arg::with_name("hash")
                .help("")
                .required(true)
                .index(1)
            )
            .arg(Arg::with_name("path")
                .help("")
                .required(true)
                .index(2)
            )
        )
        .subcommand(
            SubCommand::with_name("serve")
            .about("Starts a server to provide other instances of ruler on the
network access to the files in the cache.")
            .help("Starts a server to provide other instances of ruler on the
network access to the files in the cache.")
            .arg(Arg::with_name("address")
                .short("a")
                .long("address")
                .value_name("address")
                .help("The address upon which to serve")
                .takes_value(true))
        )
        .setting(AppSettings::ArgRequiredElseHelp)
        .get_matches();

    if let Some(matches) = big_matches.subcommand_matches("download")
    {
        let mut system = RealSystem::new();

        let download_urls =
        match read_config(&mut system)
        {
            Ok(config) =>
            {
                match config.download_urls
                {
                    Some(download_urls) =>
                    {
                        if download_urls.len() == 0
                        {
                            println!("Download urls list empty");
                            return;
                        }
                        download_urls
                    },
                    None =>
                    {
                        println!("No download_urls in config file");
                        return;
                    }
                }
            },
            Err(config_error) =>
            {
                println!("{}", config_error);
                return;
            }
        };

        let hash =
        match matches.value_of("hash")
        {
            Some(value) => value,
            None => panic!("Agument name mismatch"),
        };

        let path =
        match matches.value_of("path")
        {
            Some(value) => value,
            None => panic!("Agument name mismatch"),
        };

        let mut success = false;
        for url in download_urls
        {
            match download_file(&mut system, &format!("{}/files/{}", url, hash), &path)
            {
                Ok(_) => {success = true},
                Err(_error) => {},
            }
        }
        if ! success
        {
            println!("Download failed");
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("again")
    {
        let directory =
        match matches.value_of("directory")
        {
            Some(value) => value,
            None => ".ruler",
        };

        let mut system = RealSystem::new();
        match read_persistent_info(&mut system, &directory)
        {
            Ok(persistent_info) =>
                match persistent_info.again
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
                            None,
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
            Err(persistent_info_error) => println!("Error reading persistent_info: {}", persistent_info_error),
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
            Some(values) =>
            {
                values.map(|s| s.to_string()).collect()
            },
            None =>
            {
                vec!("build.rules".to_string())
            },
        };

        for f in rulefiles.iter()
        {
            println!("{}", f);
        }

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

        let persistent_info = PersistentInfo
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

        match write_persistent_info(&mut system, &directory, &persistent_info)
        {
            Ok(()) =>
            {
                match build::build(
                    system,
                    directory,
                    rulefiles,
                    None,
                    target,
                    &mut printer)
                {
                    Ok(()) => {},
                    Err(error) => eprintln!("{}", error),
                }
            },
            Err(error) =>
            {
println!("Error writing persistent_info.toml: {}", error);
            }
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("serve")
    {
        let directory =
        match matches.value_of("directory")
        {
            Some(value) => value,
            None => ".ruler",
        };

        match server::serve(
            RealSystem::new(),
            directory)
        {
            Ok(()) => {},
            Err(error) => eprintln!("{}", error),
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("hash")
    {
        match matches.value_of("path")
        {
            Some(path) =>
            {
                let system = RealSystem::new();
                match blob::get_file_ticket_from_path(&system, &path)
                {
                    Ok(Some(file_ticket)) =>
                    {
                        println!("{}", file_ticket);
                    },
                    Ok(None) => eprintln!("File not found: {}", path),
                    Err(error) => eprintln!("{}", error),
                }
            },
            None =>
            {
                eprintln!("Internal error");
            }
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("nodes")
    {
        let rulefiles =
        match matches.values_of("rules")
        {
            Some(values) =>
            {
                values.map(|s| s.to_string()).collect()
            },
            None =>
            {
                vec!("build.rules".to_string())
            },
        };

        let target =
        match matches.value_of("target")
        {
            Some(value) => Some(value.to_string()),
            None => None,
        };

        let system = RealSystem::new();

        match build::get_nodes(
            &system,
            rulefiles,
            target)
        {
            Ok(nodes) =>
            {
                for node in nodes.iter()
                {
                    print!("{}", node);
                }
            },
            Err(error) => eprintln!("{}", error),
        }
    }
}
