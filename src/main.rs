extern crate clap;
extern crate filesystem;

use clap::{Arg, App, SubCommand};
use filesystem::OsFileSystem;

mod build;
mod rule;
mod ticket;
mod work;
mod memory;
mod station;
mod executor;
mod packet;
mod metadata;

use self::build::build;
use self::work::OsExecutor;
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
            .help("Removes all files and directories specificed as targets in the rules file"))
        .subcommand(
            SubCommand::with_name("build")
            .help("Builds the given target.\nThe target must be a file listed in the target section of the current rules file.\nThe rules file is either a file in the current working directory called \"build.rules\" or it can be specificed using --rules=<path>")
            .arg(Arg::with_name("target")
                .help("The path to the target file (or directory) to be built")
                .required(true)
                .index(1)))
        .arg(Arg::from_usage("-r --rules=[RULES] 'Sets a rule file to use.  If not provided, the app will look for a file in the current working directory called \"build.rules\"'"))
        .arg(Arg::from_usage("-m --history=[HISTORY] 'Where to read/write cached file content data'"))
        .get_matches();

    if let Some(_matches) = big_matches.subcommand_matches("clean")
    {
        println!("here's where we would clean");
    }

    if let Some(matches) = big_matches.subcommand_matches("build")
    {
        let historyfile =
        match matches.value_of("history")
        {
            Some(value) => value,
            None => ".ruler-history",
        };

        let rulefile =
        match matches.value_of("rules")
        {
            Some(value) => value,
            None => "build.rules",
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
            historyfile, rulefile, target)
        {
            Ok(()) => {},
            Err(error) => eprintln!("{}", error),
        }
    }
}
