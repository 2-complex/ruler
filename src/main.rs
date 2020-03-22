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

use self::executor::OsExecutor;
use self::metadata::OsMetadataGetter;

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
                .help("The path to the clean-target file to be cleaned.  Clean command removes all files which are listed as targets in rules that the clean-target depends on.\nIf no clean-target is specified, clean command removes all files listed as targets in any rule.")
                .required(false)
                .index(1)
            )
        )
        .subcommand(
            SubCommand::with_name("build")
            .help("Builds the given target.\nThe target must be a file listed in the target section of the current rules file.\nThe rules file is either a file in the current working directory called \"build.rules\" or it can be specificed using --rules=<path>")
            .arg(Arg::with_name("target")
                .help("The path to the target file (or directory) to be built")
                .required(false)
                .index(1)
            )
        )
        .get_matches();


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
            Some(value) => Some(value.to_string()),
            None => None,
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
            Some(value) => Some(value.to_string()),
            None => None,
        };

        match build::build(
            OsFileSystem::new(),
            OsExecutor::new(),
            OsMetadataGetter::new(),
            directory,
            rulefile,
            target)
        {
            Ok(()) => {},
            Err(error) => eprintln!("{}", error),
        }
    }
}
