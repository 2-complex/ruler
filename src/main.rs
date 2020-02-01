extern crate clap;
extern crate filesystem;

use clap::{Arg, App, SubCommand};
use filesystem::OsFileSystem;

mod cache;
mod build;
mod rule;
mod ticket;
mod work;
mod memory;
mod executor;
mod packet;
mod metadata;
mod internet;

use self::build::build;
use self::executor::OsExecutor;
use self::metadata::OsMetadataGetter;
use self::ticket::TicketFactory;

fn main()
{
    let big_matches = App::new("Ruler")
        .version("0.1.0")
        .author("Peterson Trethewey <ptrethewey@roblox.com>")
        .about("You know when you have files that depend on other files?  This is for that situation.")
        .setting(clap::AppSettings::ArgRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("clean")
            .help("Removes all files and directories specificed as targets in the rules file")
            .arg(Arg::with_name("target")
                .help("The path to the target file (or directory) to be cleaned.  Clean command will remove all files which are:\n - listed as targets in the rule\n - dependencies of the target specified ")
                .required(true)
                .index(1)
            )
        )
        .subcommand(
            SubCommand::with_name("upload")
            .help("Uploads all intermediates")
            .arg(Arg::with_name("target")
                .help("The path to the target file (or directory) to be uploaded.  Upload command will upload the target and all its intermeidate:\n")
                .required(true)
                .index(1)
            )
            .arg(Arg::with_name("server")
                .help("The upload url")
                .required(true)
                .index(2)
            )
        )
        .subcommand(
            SubCommand::with_name("build")
            .help("Builds the given target.\nThe target must be a file listed in the target section of the current rules file.\nThe rules file is either a file in the current working directory called \"build.rules\" or it can be specificed using --rules=<path>")
            .arg(Arg::with_name("target")
                .help("The path to the target file (or directory) to be built")
                .required(true)
                .index(1)
            )
        )
        .subcommand(
            SubCommand::with_name("hash")
            .help("Prints out the hash of a file.")
            .arg(Arg::with_name("path")
                .help("The path to the file to be hashed")
                .required(true)
                .index(1)
            )
        )
        .subcommand(
            SubCommand::with_name("memory")
            .help("Shows the content of ruler memory.  This includes rule histores and target histories.")
        )
        .get_matches();


    if let Some(matches) = big_matches.subcommand_matches("hash")
    {
        let path =
        match matches.value_of("path")
        {
            Some(value) => value,
            None => panic!("No path!"),
        };

        match TicketFactory::from_file(&OsFileSystem::new(), path)
        {
            Ok(mut factory) =>
            {
                println!("{}", factory.result().base64());
            },
            Err(error) => eprintln!("{}", error),
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("upload")
    {
        let rulefile = "build.rules";

        let target =
        match matches.value_of("target")
        {
            Some(value) => value,
            None => panic!("No target!"),
        };

        let server_url =
        match matches.value_of("server")
        {
            Some(value) => value,
            None => panic!("No server!"),
        };

        match build::upload(
            OsFileSystem::new(),
            rulefile,
            target,
            server_url)
        {
            Ok(()) => {},
            Err(error) => eprintln!("{}", error),
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("clean")
    {
        let rulefile =
        match matches.value_of("rules")
        {
            Some(value) => value,
            None => "build.rules",
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
            Some(value) => value,
            None => panic!("No target!"),
        };

        match build::clean(
            OsFileSystem::new(),
            OsMetadataGetter::new(),
            directory, rulefile, target)
        {
            Ok(()) => {},
            Err(error) => eprintln!("{}", error),
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("memory")
    {
        let directory =
        match matches.value_of("directory")
        {
            Some(value) => value,
            None => ".ruler",
        };

        let mut file_system = OsFileSystem::new();

        match build::init_directory(&mut file_system, directory)
        {
            Ok((memory, _, _)) => println!("{}", memory),
            Err(error) => eprintln!("{}", error),
        }
    }

    if let Some(matches) = big_matches.subcommand_matches("build")
    {
        let rulefile =
        match matches.value_of("rules")
        {
            Some(value) => value,
            None => "build.rules",
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
            Some(value) => value,
            None => panic!("No target!"),
        };

        match build(
            OsFileSystem::new(),
            OsExecutor::new(),
            OsMetadataGetter::new(),
            directory, rulefile, target)
        {
            Ok(()) => {},
            Err(error) => eprintln!("{}", error),
        }
    }
}
